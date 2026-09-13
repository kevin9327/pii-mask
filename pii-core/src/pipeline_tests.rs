//! End-to-end tests that drive `process_file` (the shipped entry point).

use crate::checksum::{biz_check_digit, corp_check_digit, luhn_check_digit, rrn_check_digit};
use crate::parse::ooxml::{xml_escape, write_zip};
use crate::report::{csv_report, html_report, json_report};
use crate::types::Confidence;
use crate::{process_file, MaskMode, ProcessConfig, RuleSet};
use docagent_model::{Block, Document, Paragraph, Section, Table};

fn cfg(mode: MaskMode, do_mask: bool) -> ProcessConfig {
    ProcessConfig {
        rules: RuleSet::builtin().expect("builtin rules"),
        mask_mode: mode,
        do_mask,
    }
}

fn rrn_string(front6: [u8; 6], gender: u8, serial5: [u8; 5]) -> String {
    let mut d = Vec::with_capacity(13);
    d.extend_from_slice(&front6);
    d.push(gender);
    d.extend_from_slice(&serial5);
    let check = rrn_check_digit(&d);
    d.push(check);
    format!(
        "{}{}{}{}{}{}-{}{}{}{}{}{}{}",
        d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11], d[12]
    )
}

fn biz_string(first9: [u8; 9]) -> String {
    let c = biz_check_digit(&first9);
    format!(
        "{}{}{}-{}{}-{}{}{}{}{}",
        first9[0], first9[1], first9[2], first9[3], first9[4], first9[5], first9[6], first9[7],
        first9[8], c
    )
}

fn corp_string(first12: [u8; 12]) -> String {
    let c = corp_check_digit(&first12);
    format!(
        "{}{}{}{}{}{}-{}{}{}{}{}{}{}",
        first12[0], first12[1], first12[2], first12[3], first12[4], first12[5], first12[6],
        first12[7], first12[8], first12[9], first12[10], first12[11], c
    )
}

fn card_string() -> String {
    let payload = [4u8, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
    let c = luhn_check_digit(&payload);
    format!("4111-1111-1111-111{c}")
}

fn pdf_with_text(text: &str) -> Vec<u8> {
    let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = format!("BT /F1 12 Tf 72 720 Td ({escaped}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len() as u32);
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

fn empty_pdf() -> Vec<u8> {
    pdf_with_text("")
}

fn pdf_with_form_xobject(page_text: &str, header: &str) -> Vec<u8> {
    let escaped_page = page_text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let escaped_header = header
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let page_stream = format!("q /Fm1 Do Q BT /F1 12 Tf 72 400 Td ({escaped_page}) Tj ET");
    let form_stream = format!("BT /F1 10 Tf 0 8 Td ({escaped_header}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 6 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{page_stream}\nendstream", page_stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 400 24] /Resources << /Font << /F1 5 0 R >> >> /Length {} >>\nstream\n{form_stream}\nendstream",
            form_stream.len()
        ),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len() as u32);
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

fn pdf_with_info_subject(subject: &str) -> Vec<u8> {
    let escaped = subject.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = b"BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), std::str::from_utf8(stream).unwrap()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Title (report) /Subject ({escaped}) /Author (internal) >>"),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len() as u32);
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

fn pdf_with_outline_title(title: &str) -> Vec<u8> {
    let escaped = title.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Outlines /Count 1 /First 7 0 R /Last 7 0 R >>".to_string(),
        format!("<< /Title ({escaped}) /Parent 6 0 R >>"),
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len() as u32);
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

fn xlsx_with_defined_name(formula: &str) -> Vec<u8> {
    let wb = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets>
<definedNames><definedName name="HiddenPii">"{}"</definedName></definedNames>
</workbook>"#,
        xml_escape(formula)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#;
    write_zip(&[
        ("xl/workbook.xml".into(), wb.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
    ])
    .expect("xlsx definedName zip")
}

fn xlsx_with_defined_name_attr(name: &str, cell: &str) -> Vec<u8> {
    let wb = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets>
<definedNames><definedName name="{}">Sheet1!A1</definedName></definedNames>
</workbook>"#,
        xml_escape(name)
    );
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    write_zip(&[
        ("xl/workbook.xml".into(), wb.into_bytes()),
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
    ])
    .expect("xlsx definedName attr zip")
}

fn xlsx_with_sheet_name(name: &str, cell: &str) -> Vec<u8> {
    let wb = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheets><sheet name="{}" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets>
</workbook>"#,
        xml_escape(name)
    );
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    write_zip(&[
        ("xl/workbook.xml".into(), wb.into_bytes()),
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
    ])
    .expect("xlsx sheet name zip")
}

fn xlsx_with_connection_name(name: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    let conn = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<connections xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<connection id="1" name="{}" type="5"/>
</connections>"#,
        xml_escape(name)
    );
    write_zip(&[
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/connections.xml".into(), conn.into_bytes()),
    ])
    .expect("xlsx connection zip")
}

fn pdf_with_open_action_js(script: &str) -> Vec<u8> {
    let escaped = script.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /OpenAction 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /S /JavaScript /JS ({escaped}) >>"),
    ];
    pdf_from_objects(&objects)
}

fn pdf_from_objects(objects: &[String]) -> Vec<u8> {
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len() as u32);
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
}

fn pdf_with_embedded_filespec(desc: &str) -> Vec<u8> {
    let escaped = desc.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Names 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /EmbeddedFiles 7 0 R >>".to_string(),
        "<< /Names [(attach) 8 0 R] >>".to_string(),
        format!("<< /Type /Filespec /F (attach.pdf) /UF (attach.pdf) /Desc ({escaped}) >>"),
    ];
    pdf_from_objects(&objects)
}

fn pdf_with_uri_action(uri: &str) -> Vec<u8> {
    let escaped = uri.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [6 0 R] /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [0 0 100 20] /A 7 0 R >>".to_string(),
        format!("<< /S /URI /URI ({escaped}) >>"),
    ];
    pdf_from_objects(&objects)
}

fn pdf_with_named_dest(label: &str) -> Vec<u8> {
    let escaped = label.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Names 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Dests 7 0 R >>".to_string(),
        format!("<< /Names [({escaped}) 8 0 R] >>"),
        "<< /D [3 0 R /Fit] >>".to_string(),
    ];
    pdf_from_objects(&objects)
}

fn pdf_with_struct_alt(alt: &str) -> Vec<u8> {
    let escaped = alt.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = "BT /F1 12 Tf 72 720 Td (visible) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /StructTreeRoot /K 7 0 R >>".to_string(),
        format!("<< /Type /StructElem /S /Figure /P 6 0 R /Alt ({escaped}) >>"),
    ];
    pdf_from_objects(&objects)
}

fn xlsx_with_hyperlink_display(display: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData>
<hyperlinks><hyperlink ref="A1" r:id="rId1" display="{}" tooltip="메모"/></hyperlinks>
</worksheet>"#,
        xml_escape(display)
    );
    write_zip(&[
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
    ])
    .expect("xlsx hyperlink zip")
}

fn xlsx_with_chart_title(title: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    let chart = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<c:chart><c:title><c:tx><c:rich>
<a:p><a:r><a:t>{}</a:t></a:r></a:p>
</c:rich></c:tx></c:title><c:plotArea/></c:chart>
</c:chartSpace>"#,
        xml_escape(title)
    );
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        ("xl/charts/chart1.xml".into(), chart.into_bytes()),
    ])
    .expect("xlsx chart zip")
}

fn xlsx_with_print_header(header: &str) -> Vec<u8> {
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData/>
<headerFooter><oddHeader>&amp;C{}</oddHeader></headerFooter></worksheet>"#,
        xml_escape(header)
    );
    write_zip(&[
        ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
    ])
    .expect("xlsx headerFooter zip")
}

fn xlsx_with_comment(comment: &str) -> Vec<u8> {
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#;
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<commentList><comment ref="A1"><text><t>{}</t></text></comment></commentList></comments>"#,
        xml_escape(comment)
    );
    write_zip(&[
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/comments1.xml".into(), comments.into_bytes()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
    ])
    .expect("xlsx comments zip")
}

fn pdf_with_annotation(body: &str, note: &str) -> Vec<u8> {
    let escaped_body = body.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let escaped_note = note.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let stream = format!("BT /F1 12 Tf 72 720 Td ({escaped_body}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [6 0 R] /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Type /Annot /Subtype /Text /Contents ({escaped_note}) /Rect [72 680 220 710] /Name /Comment >>"
        ),
    ];
    let mut body_pdf = String::from("%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body_pdf.len() as u32);
        body_pdf.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_at = body_pdf.len();
    body_pdf.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body_pdf.push_str("0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        body_pdf.push_str(&format!("{off:010} 00000 n \n"));
    }
    body_pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    body_pdf.into_bytes()
}

fn docx_with_text(text: &str) -> Vec<u8> {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:body></w:document>"#,
        xml_escape(text)
    );
    write_zip(&[("word/document.xml".into(), xml.into_bytes())]).expect("docx zip")
}

fn xlsx_with_text(text: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(text)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
    ])
    .expect("xlsx zip")
}

fn pptx_slide_xml(text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree><p:sp><p:txBody>
<a:p><a:r><a:t>{}</a:t></a:r></a:p>
</p:txBody></p:sp></p:spTree></p:cSld>
</p:sld>"#,
        xml_escape(text)
    )
}

fn pptx_notes_xml(text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree><p:sp><p:txBody>
<a:p><a:r><a:t>{}</a:t></a:r></a:p>
</p:txBody></p:sp></p:spTree></p:cSld>
</p:notes>"#,
        xml_escape(text)
    )
}

fn pptx_with_text(text: &str) -> Vec<u8> {
    write_zip(&[
        ("ppt/presentation.xml".into(), b"<p:presentation/>".to_vec()),
        ("ppt/slides/slide1.xml".into(), pptx_slide_xml(text).into_bytes()),
    ])
    .expect("pptx zip")
}

fn odf_manifest() -> Vec<u8> {
    b"<?xml version=\"1.0\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"/>".to_vec()
}

fn odt_with_text(text: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:p>{}</text:p>
</office:text></office:body>
</office:document-content>"#,
        xml_escape(text)
    );
    write_zip(&[
        ("META-INF/manifest.xml".into(), odf_manifest()),
        ("content.xml".into(), content.into_bytes()),
    ])
    .expect("odt zip")
}

fn epub_with_text(text: &str) -> Vec<u8> {
    let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/chapter.xhtml" media-type="application/xhtml+xml"/></rootfiles>
</container>"#;
    let xhtml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{}</p></body></html>"#,
        xml_escape(text)
    );
    write_zip(&[
        ("META-INF/container.xml".into(), container.as_bytes().to_vec()),
        ("OEBPS/chapter.xhtml".into(), xhtml.into_bytes()),
    ])
    .expect("epub zip")
}

fn ods_with_text(text: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:spreadsheet>
<table:table><table:table-row><table:table-cell>
<text:p>{}</text:p>
</table:table-cell></table:table-row></table:table>
</office:spreadsheet></office:body>
</office:document-content>"#,
        xml_escape(text)
    );
    write_zip(&[
        ("META-INF/manifest.xml".into(), odf_manifest()),
        ("content.xml".into(), content.into_bytes()),
    ])
    .expect("ods zip")
}

fn pptx_with_comment_only(comment: &str, slide: &str) -> Vec<u8> {
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cm authorId="0" dt="2024-01-01T00:00:00"><p:text>{}</p:text></p:cm>
</p:cmLst>"#,
        xml_escape(comment)
    );
    write_zip(&[
        ("ppt/presentation.xml".into(), b"<p:presentation/>".to_vec()),
        ("ppt/slides/slide1.xml".into(), pptx_slide_xml(slide).into_bytes()),
        ("ppt/comments/comment1.xml".into(), comments.into_bytes()),
    ])
    .expect("pptx comments zip")
}

fn pptx_with_master_only(master: &str, slide: &str) -> Vec<u8> {
    let master_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree><p:sp><p:txBody>
<a:p><a:r><a:t>{}</a:t></a:r></a:p>
</p:txBody></p:sp></p:spTree></p:cSld>
</p:sldMaster>"#,
        xml_escape(master)
    );
    write_zip(&[
        ("ppt/presentation.xml".into(), b"<p:presentation/>".to_vec()),
        ("ppt/slides/slide1.xml".into(), pptx_slide_xml(slide).into_bytes()),
        (
            "ppt/slideMasters/slideMaster1.xml".into(),
            master_xml.into_bytes(),
        ),
    ])
    .expect("pptx master zip")
}

fn xlsx_with_pivot_cache(value: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    let cache = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<pivotCacheRecords xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1">
<r><s v="{}"/><n v="1"/></r>
</pivotCacheRecords>"#,
        xml_escape(value)
    );
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        (
            "xl/pivotCache/pivotCacheRecords1.xml".into(),
            cache.into_bytes(),
        ),
    ])
    .expect("xlsx pivot zip")
}

fn xlsx_with_table_display_name(name: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    let table = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Table1" displayName="{}" ref="A1:A2">
<tableColumns count="1"><tableColumn id="1" name="col"/></tableColumns>
</table>"#,
        xml_escape(name)
    );
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        ("xl/tables/table1.xml".into(), table.into_bytes()),
    ])
    .expect("xlsx table zip")
}

fn pptx_with_notes_only(notes: &str, slide: &str) -> Vec<u8> {
    write_zip(&[
        ("ppt/presentation.xml".into(), b"<p:presentation/>".to_vec()),
        ("ppt/slides/slide1.xml".into(), pptx_slide_xml(slide).into_bytes()),
        (
            "ppt/notesSlides/notesSlide1.xml".into(),
            pptx_notes_xml(notes).into_bytes(),
        ),
    ])
    .expect("pptx notes zip")
}

fn xlsx_with_drawing(drawing: &str, cell: &str) -> Vec<u8> {
    let ss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>{}</t></si></sst>"#,
        xml_escape(cell)
    );
    let sheet = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
    let drawing_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<xdr:twoCellAnchor><xdr:sp><xdr:txBody>
<a:p><a:r><a:t>{}</a:t></a:r></a:p>
</xdr:txBody></xdr:sp></xdr:twoCellAnchor>
</xdr:wsDr>"#,
        xml_escape(drawing)
    );
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
        ("xl/drawings/drawing1.xml".into(), drawing_xml.into_bytes()),
    ])
    .expect("xlsx drawing zip")
}

fn hwpx_with_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new();
    let mut section = Section::default();
    section.body.push(Block::Paragraph(Paragraph::from_text(text)));
    doc.sections.push(section);
    docagent_hwpx::write(&doc).expect("hwpx write")
}

fn hwp5_with_table_cell(cell: &str) -> Vec<u8> {
    let mut doc = Document::new();
    let mut section = Section::default();
    section.body.push(Block::Table(Table::from_cells(vec![
        vec!["이름".into(), cell.into()],
    ])));
    doc.sections.push(section);
    docagent_hwp5::write(&doc).expect("hwp5 table write")
}

fn xlsx_numeric_cell(value: &str) -> Vec<u8> {
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1"><v>{}</v></c></row></sheetData></worksheet>"#,
        xml_escape(value)
    );
    write_zip(&[
        ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
    ])
    .expect("xlsx numeric zip")
}

fn euckr_bytes(text: &str) -> Vec<u8> {
    let (cow, _, _) = encoding_rs::EUC_KR.encode(text);
    cow.into_owned()
}

fn utf16le_bytes(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
    out
}

fn hwp_with_prvtext_preview(body: &str, preview: &str) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let bytes = hwp5_with_text(body);
    let mut cursor = Cursor::new(bytes);
    {
        let mut comp = cfb::CompoundFile::open(&mut cursor).expect("cfb open");
        let utf16: Vec<u8> = preview
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .chain([0u8, 0])
            .collect();
        let mut stream = if comp.exists("PrvText") {
            comp.open_stream("PrvText").expect("open PrvText")
        } else {
            comp.create_stream("PrvText").expect("create PrvText")
        };
        let _ = stream.set_len(utf16.len() as u64);
        stream.write_all(&utf16).expect("write PrvText");
    }
    cursor.into_inner()
}

fn docx_with_custom_xml(ssn: &str, body: &str) -> Vec<u8> {
    let body_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
        xml_escape(body)
    );
    let item = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<item><name>홍길동</name><ssn>{}</ssn></item>"#,
        xml_escape(ssn)
    );
    write_zip(&[
        ("word/document.xml".into(), body_xml.into_bytes()),
        ("customXml/item1.xml".into(), item.into_bytes()),
    ])
    .expect("docx customXml zip")
}

fn docx_with_instr_hyperlink(instr: &str, body: &str) -> Vec<u8> {
    let body_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>{}</w:t></w:r></w:p>
<w:p><w:r><w:instrText xml:space="preserve">HYPERLINK "{}"</w:instrText></w:r></w:p>
</w:body></w:document>"#,
        xml_escape(body),
        xml_escape(instr)
    );
    write_zip(&[("word/document.xml".into(), body_xml.into_bytes())]).expect("docx instr zip")
}

fn docx_with_rel_target(target: &str, body: &str) -> Vec<u8> {
    let body_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
        xml_escape(body)
    );
    let rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>
</Relationships>"#,
        xml_escape(target)
    );
    write_zip(&[
        ("word/document.xml".into(), body_xml.into_bytes()),
        ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
    ])
    .expect("docx rels zip")
}

fn docx_with_core_description(description: &str, body: &str) -> Vec<u8> {
    let body_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
        xml_escape(body)
    );
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:description>{}</dc:description>
</cp:coreProperties>"#,
        xml_escape(description)
    );
    write_zip(&[
        ("word/document.xml".into(), body_xml.into_bytes()),
        ("docProps/core.xml".into(), core.into_bytes()),
    ])
    .expect("docx core zip")
}

fn hwp5_with_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new();
    let mut section = Section::default();
    section.body.push(Block::Paragraph(Paragraph::from_text(text)));
    doc.sections.push(section);
    docagent_hwp5::write(&doc).expect("hwp5 write")
}

fn confirmed_raws(name: &str, bytes: &[u8]) -> Vec<(String, String, Confidence)> {
    let r = process_file(name, bytes, &cfg(MaskMode::Full, false)).expect("process");
    r.report
        .findings
        .into_iter()
        .map(|f| (f.rule_id, f.raw, f.confidence))
        .collect()
}

#[test]
fn builtin_rules_parse() {
    let set = RuleSet::builtin().expect("parse rules.toml");
    assert!(set.rules.iter().any(|r| r.id == "resident_id"));
    assert!(set.rules.iter().any(|r| r.id == "credit_card"));
}

#[test]
fn txt_valid_rrn_is_confirmed_via_process_file() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("계약자 {rrn} 입니다");
    let hits = confirmed_raws("a.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "resident_id" && raw == &rrn && *c == Confidence::Confirmed
        }),
        "process_file missed confirmed RRN {rrn}: {hits:?}"
    );
}

#[test]
fn txt_bad_rrn_checksum_is_suspicious() {
    let rrn = "900101-1234560"; // same shape, wrong check digit vs 8
    let text = format!("계약자 {rrn} 입니다");
    let hits = confirmed_raws("a.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "resident_id" && raw == rrn && *c == Confidence::Suspicious
        }),
        "invalid checksum should be 의심: {hits:?}"
    );
}

#[test]
fn already_masked_rrn_is_classified() {
    let text = "이미 가림 900101-1****** 처리";
    let hits = confirmed_raws("a.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "resident_id" && raw == "900101-1******" && *c == Confidence::AlreadyMasked
        }),
        "already-masked form not classified: {hits:?}"
    );
}

#[test]
fn luhn_card_and_non_luhn_via_process_file() {
    let ok = card_string();
    let bad = "4111-1111-1111-1112";
    let text = format!("카드 {ok} 그리고 {bad}");
    let hits = confirmed_raws("card.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "credit_card" && raw == &ok && *c == Confidence::Confirmed
        }),
        "Luhn card missed: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "credit_card" && raw == bad && *c == Confidence::Suspicious
        }),
        "non-Luhn should be 의심: {hits:?}"
    );
}

#[test]
fn biz_and_corp_checksums_via_process_file() {
    let biz = biz_string([2, 2, 0, 8, 1, 6, 2, 5, 1]);
    let corp = corp_string([1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 1]);
    let text = format!("사업자 {biz} 법인 {corp}");
    let hits = confirmed_raws("ids.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "business_id" && raw == &biz && *c == Confidence::Confirmed
        }),
        "biz missed: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "corp_id" && raw == &corp && *c == Confidence::Confirmed
        }),
        "corp missed: {hits:?}"
    );
}

#[test]
fn custom_rule_overlay_is_used_by_process_file() {
    let extra = r#"
[[rules]]
id = "employee_no"
label = "사번"
priority = 200
validator = "none"
replace_token = "[사번]"
partial = "full"
pattern = 'EMP-\d{6}'
"#;
    let rules = RuleSet::builtin()
        .unwrap()
        .with_extra(extra)
        .expect("extra rules");
    let cfg = ProcessConfig {
        rules,
        mask_mode: MaskMode::Replace,
        do_mask: true,
    };
    let bytes = "담당 EMP-123456 확인".as_bytes();
    let out = process_file("x.txt", bytes, &cfg).expect("process");
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.rule_id == "employee_no" && f.raw == "EMP-123456"),
        "custom rule not applied: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("masked")).unwrap();
    assert!(
        masked.contains("[사번]"),
        "replace token missing in {masked:?}"
    );
    assert!(!masked.contains("EMP-123456"));
}

#[test]
fn mask_modes_change_txt_output() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let src = format!("값 {rrn} 끝");
    let full = process_file("a.txt", src.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    let partial = process_file("a.txt", src.as_bytes(), &cfg(MaskMode::Partial, true)).unwrap();
    let repl = process_file("a.txt", src.as_bytes(), &cfg(MaskMode::Replace, true)).unwrap();
    let del = process_file("a.txt", src.as_bytes(), &cfg(MaskMode::Delete, true)).unwrap();
    let full_s = String::from_utf8(full.masked_bytes.unwrap()).unwrap();
    let part_s = String::from_utf8(partial.masked_bytes.unwrap()).unwrap();
    let repl_s = String::from_utf8(repl.masked_bytes.unwrap()).unwrap();
    let del_s = String::from_utf8(del.masked_bytes.unwrap()).unwrap();
    assert!(!full_s.contains(&rrn), "full still has raw: {full_s}");
    assert!(full_s.contains("******"), "full mask stars missing: {full_s}");
    assert!(
        part_s.contains("900101-1"),
        "partial should keep prefix: {part_s}"
    );
    assert!(repl_s.contains("[주민번호]"), "replace token: {repl_s}");
    assert!(!del_s.contains(&rrn), "delete still has raw: {del_s}");
    assert!(
        !full.report.diffs.is_empty() && full.report.diffs[0].before.contains(&rrn),
        "diff preview missing"
    );
}

#[test]
fn json_and_csv_roundtrip_offsets() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let json = format!(r#"{{"name":"홍길동","rrn":"{rrn}"}}"#);
    let out = process_file("p.json", json.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Json);
    let masked = String::from_utf8(out.masked_bytes.unwrap()).unwrap();
    assert!(masked.contains("\"rrn\""), "json key lost: {masked}");
    assert!(!masked.contains(&rrn));

    let csv = format!("name,rrn\n홍길동,{rrn}\n");
    let out = process_file("p.csv", csv.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Csv);
    let masked = String::from_utf8(out.masked_bytes.unwrap()).unwrap();
    assert!(masked.starts_with("name,rrn"), "csv header lost: {masked}");
}

#[test]
fn docx_and_xlsx_via_process_file() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("첨부 {rrn}");
    let docx = docx_with_text(&text);
    let out = process_file("a.docx", &docx, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "docx findings: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.unwrap();
    let extracted = crate::parse::extract("a.docx", &masked).unwrap();
    assert!(
        !extracted.full_text.contains(&rrn),
        "docx rewrite left raw PII: {}",
        extracted.full_text
    );

    let xlsx = xlsx_with_text(&text);
    let out = process_file("a.xlsx", &xlsx, &cfg(MaskMode::Partial, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(out
        .report
        .findings
        .iter()
        .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed));
}

#[test]
fn pdf_text_layer_and_empty_scan() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let pdf = pdf_with_text(&format!("ID {rrn}"));
    let out = process_file("a.pdf", &pdf, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "pdf text layer missed RRN: {:?} warnings={:?}",
        out.report.findings,
        out.report.warnings
    );
    assert!(
        out.fallback_note
            .as_deref()
            .unwrap_or("")
            .contains("TXT"),
        "pdf should fall back to TXT: {:?}",
        out.fallback_note
    );

    let scan = empty_pdf();
    let out = process_file("scan.pdf", &scan, &cfg(MaskMode::Full, false)).unwrap();
    assert!(
        out.report.warnings.iter().any(|w| w.contains("텍스트 없음")),
        "scan pdf should report no text: {:?}",
        out.report.warnings
    );
}

#[test]
fn hwpx_and_hwp5_via_docagent_write_then_process_file() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("한글 문서 {rrn} 끝");
    let hwpx = hwpx_with_text(&text);
    let out = process_file("doc.hwpx", &hwpx, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Hwpx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "hwpx findings={:?} warnings={:?}",
        out.report.findings,
        out.report.warnings
    );
    let masked = out.masked_bytes.expect("hwpx masked");
    let again = crate::parse::extract("doc.hwpx", &masked).unwrap();
    assert!(
        !again.full_text.contains(&rrn),
        "hwpx rewrite left raw: {}",
        again.full_text
    );

    let hwp = hwp5_with_text(&text);
    let out = process_file("doc.hwp", &hwp, &cfg(MaskMode::Partial, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Hwp);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "hwp5 findings={:?}",
        out.report.findings
    );
}

#[test]
fn phone_email_ip_and_reports_include_filename() {
    let text = "연락 010-1234-5678 메일 user@example.com 서버 192.168.0.1";
    let out = process_file("contact.txt", text.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    let ids: Vec<_> = out.report.findings.iter().map(|f| f.rule_id.as_str()).collect();
    assert!(ids.contains(&"mobile_phone"), "{ids:?}");
    assert!(ids.contains(&"email"), "{ids:?}");
    assert!(ids.contains(&"ip_address"), "{ids:?}");
    let csv = csv_report(&[out.report.clone()]);
    assert!(csv.contains("contact.txt"), "csv missing filename: {csv}");
    assert!(csv.contains("mobile_phone") || csv.contains("휴대전화"));
    let json = json_report(&[out.report.clone()]).unwrap();
    assert!(json.contains("contact.txt"));
    let html = html_report(&[out.report.clone()]);
    assert!(html.contains("contact.txt"));
    assert!(html.contains("확정"));
}

#[test]
fn table_preview_masks_value_itself() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("앞문맥 {rrn} 뒷문맥");
    let out = process_file("t.txt", text.as_bytes(), &cfg(MaskMode::Full, false)).unwrap();
    let preview = &out.report.summaries[0].preview;
    assert!(
        !preview.contains(&rrn),
        "preview leaked raw PII: {preview}"
    );
    assert!(preview.contains("앞문맥") && preview.contains("뒷문맥"));
}

#[test]
fn already_masked_remaining_types_via_process_file() {
    let card = "4111-****-****-1111";
    let license = "서울-12-******-**";
    let passport_m = "M*******";
    let passport_ab = "AB******";
    let account = "110-***-******";
    let email = "u***@example.com";
    let ip = "192.168.0.*";
    let text = format!(
        "카드 {card} 면허 {license} 여권 {passport_m} {passport_ab} 계좌 {account} 메일 {email} 서버 {ip}"
    );
    let hits = confirmed_raws("masked.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "credit_card" && raw == card && *c == Confidence::AlreadyMasked
        }),
        "masked card not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "driver_license" && raw == license && *c == Confidence::AlreadyMasked
        }),
        "masked driver license not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "passport" && raw == passport_m && *c == Confidence::AlreadyMasked
        }),
        "masked passport M******* not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "passport" && raw == passport_ab && *c == Confidence::AlreadyMasked
        }),
        "masked passport AB****** not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "bank_account" && raw == account && *c == Confidence::AlreadyMasked
        }),
        "masked bank account not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "email" && raw == email && *c == Confidence::AlreadyMasked
        }),
        "masked email not classified: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "ip_address" && raw == ip && *c == Confidence::AlreadyMasked
        }),
        "masked IP not classified: {hits:?}"
    );
}

fn hwp3_with_text(text: &str) -> Vec<u8> {
    docagent_hwp3::write_classic(text)
}

fn assert_detect_and_mask(name: &str, bytes: &[u8], expected: crate::FileFormat, rrn: &str) {
    let out = process_file(name, bytes, &cfg(MaskMode::Full, true))
        .unwrap_or_else(|e| panic!("{name}: process_file failed: {e}"));
    assert_eq!(
        out.report.format, expected,
        "{name}: sniffed {:?}, want {:?}",
        out.report.format, expected
    );
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "{name}: RRN not confirmed. findings={:?} warnings={:?} text-preview skipped",
        out.report.findings,
        out.report.warnings
    );
    let masked = out
        .masked_bytes
        .as_ref()
        .unwrap_or_else(|| panic!("{name}: no masked bytes"));
    let reparse_name = out.output_filename.as_deref().unwrap_or(name);
    if expected == crate::FileFormat::Pdf {
        assert!(
            out.fallback_note
                .as_deref()
                .unwrap_or("")
                .to_ascii_uppercase()
                .contains("TXT"),
            "{name}: PDF must declare TXT fallback, got {:?}",
            out.fallback_note
        );
        let text = String::from_utf8_lossy(masked);
        assert!(!text.contains(rrn), "{name}: PDF TXT fallback still has raw PII: {text}");
        return;
    }
    let again = crate::parse::extract(reparse_name, masked)
        .unwrap_or_else(|e| panic!("{name}: re-extract masked failed: {e}"));
    assert!(
        !again.full_text.contains(rrn),
        "{name}: rewrite left raw PII in {}: {}",
        reparse_name,
        again.full_text
    );
}

#[test]
fn every_supported_extension_detects_and_masks_via_process_file() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("문서 {rrn} 끝");
    let json = format!(r#"{{"body":"{text}"}}"#);
    let csv = format!("col\n{text}\n");

    assert_detect_and_mask("sample.txt", text.as_bytes(), crate::FileFormat::Txt, &rrn);
    assert_detect_and_mask("sample.TXT", text.as_bytes(), crate::FileFormat::Txt, &rrn);
    assert_detect_and_mask("sample.text", text.as_bytes(), crate::FileFormat::Txt, &rrn);
    assert_detect_and_mask("sample.md", text.as_bytes(), crate::FileFormat::Txt, &rrn);
    assert_detect_and_mask("sample.log", text.as_bytes(), crate::FileFormat::Txt, &rrn);
    assert_detect_and_mask("sample.csv", csv.as_bytes(), crate::FileFormat::Csv, &rrn);
    assert_detect_and_mask("sample.tsv", csv.as_bytes(), crate::FileFormat::Csv, &rrn);
    assert_detect_and_mask("sample.JSON", json.as_bytes(), crate::FileFormat::Json, &rrn);
    assert_detect_and_mask("sample.json", json.as_bytes(), crate::FileFormat::Json, &rrn);
    assert_detect_and_mask(
        "sample.pdf",
        &pdf_with_text(&text),
        crate::FileFormat::Pdf,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.docx",
        &docx_with_text(&text),
        crate::FileFormat::Docx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.DOCX",
        &docx_with_text(&text),
        crate::FileFormat::Docx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.xlsx",
        &xlsx_with_text(&text),
        crate::FileFormat::Xlsx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.pptx",
        &pptx_with_text(&text),
        crate::FileFormat::Pptx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.PPTX",
        &pptx_with_text(&text),
        crate::FileFormat::Pptx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.odt",
        &odt_with_text(&text),
        crate::FileFormat::Odt,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.ODS",
        &ods_with_text(&text),
        crate::FileFormat::Ods,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.epub",
        &epub_with_text(&text),
        crate::FileFormat::Epub,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.EPUB",
        &epub_with_text(&text),
        crate::FileFormat::Epub,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.hwpx",
        &hwpx_with_text(&text),
        crate::FileFormat::Hwpx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.HWPX",
        &hwpx_with_text(&text),
        crate::FileFormat::Hwpx,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.hwp",
        &hwp5_with_text(&text),
        crate::FileFormat::Hwp,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.HWP",
        &hwp5_with_text(&text),
        crate::FileFormat::Hwp,
        &rrn,
    );
    assert_detect_and_mask(
        "sample.hwp3",
        &hwp3_with_text(&text),
        crate::FileFormat::Hwp3,
        &rrn,
    );
}

#[test]
fn process_file_full_mask_reports_zero_residual_confirmed() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let text = format!("계약자 {rrn} 보관");
    let out = process_file("gate.txt", text.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert!(
        out.report.confirmed >= 1,
        "fixture must be detected first: {:?}",
        out.report.findings
    );
    assert_eq!(
        out.report.residual_confirmed, 0,
        "full mask must leave 0 confirmed residuals, warnings={:?} findings={:?}",
        out.report.warnings, out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.unwrap()).unwrap();
    assert!(!masked.contains(&rrn));
}

#[test]
fn process_file_json_number_mask_stays_parseable() {
    let card = card_string();
    let digits: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    let json = format!(r#"{{"card":{digits},"ok":true}}"#);
    let out = process_file("n.json", json.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.rule_id == "credit_card" && f.confidence == Confidence::Confirmed),
        "unquoted JSON number card missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("masked json")).unwrap();
    serde_json::from_str::<serde_json::Value>(&masked)
        .unwrap_or_else(|e| panic!("masked JSON must parse: {e} in {masked}"));
    assert!(
        !masked.contains(&digits),
        "raw card digits still in JSON: {masked}"
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn unknown_zip_is_not_parsed_as_txt() {
    let odd = write_zip(&[("foo/bar.xml".into(), b"<p>not office</p>".to_vec())]).unwrap();
    let out = process_file("pack.zip", &odd, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Unknown);
    assert!(
        out.report.warnings.iter().any(|w| w.contains("지원하지 않는")),
        "warnings={:?}",
        out.report.warnings
    );
    assert!(out.report.findings.is_empty());
}

#[test]
fn process_file_masks_hwp_table_cell() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = hwp5_with_table_cell(&format!("셀 {rrn}"));
    let out = process_file("tbl.hwp", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Hwp);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "table cell RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked hwp");
    let again = crate::parse::extract("tbl.hwp", &masked).unwrap();
    assert!(
        !again.full_text.contains(&rrn),
        "table rewrite left raw: {}",
        again.full_text
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_utf16le_txt_keeps_bom_and_masks() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let src = format!("UTF16 {rrn} 끝");
    let bytes = utf16le_bytes(&src);
    let out = process_file("u.txt", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "utf16 RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked utf16");
    assert_eq!(&masked[..2], &[0xFF, 0xFE], "UTF-16 LE BOM lost");
    let (text, _, enc) = crate::parse::text::decode(&masked);
    assert_eq!(enc, crate::types::TextEncoding::Utf16Le);
    assert!(!text.contains(&rrn), "utf16 masked still has raw: {text}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_quoted_csv_masks_field() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let csv = format!("name,rrn\n\"홍길동\",\"{rrn}\"\n");
    let out = process_file("q.csv", csv.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Csv);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "quoted csv missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.unwrap()).unwrap();
    assert!(masked.starts_with("name,rrn"), "csv header lost: {masked}");
    assert!(!masked.contains(&rrn), "quoted csv still has raw: {masked}");
    assert!(masked.contains('\"'), "quotes dropped: {masked}");
}

#[test]
fn process_file_xlsx_numeric_cell_card_is_masked() {
    let card = card_string();
    let digits: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    let bytes = xlsx_numeric_cell(&digits);
    let out = process_file("num.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.rule_id == "credit_card" && f.raw == digits && f.confidence == Confidence::Confirmed),
        "numeric cell card missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked xlsx");
    let again = crate::parse::extract("num.xlsx", &masked).unwrap();
    assert!(
        !again.full_text.contains(&digits),
        "numeric cell rewrite left raw: {}",
        again.full_text
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_json_replace_keeps_object_and_csv_residual_column() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let json = format!(r#"{{"rrn":"{rrn}","n":1}}"#);
    let out = process_file("r.json", json.as_bytes(), &cfg(MaskMode::Replace, true)).unwrap();
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "json string RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("masked json")).unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&masked).unwrap_or_else(|e| panic!("replace JSON parse: {e} {masked}"));
    assert_eq!(v["rrn"], "[주민번호]");
    assert_eq!(v["n"], 1);
    assert_eq!(out.report.residual_confirmed, 0);
    let csv = csv_report(&[out.report.clone()]);
    assert!(
        csv.contains("residual_confirmed"),
        "csv header missing residual: {csv}"
    );
    assert!(csv.contains("r.json"));
}

#[test]
fn process_file_euckr_txt_roundtrip_and_html_residual() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let src = format!("계약 {rrn}");
    let bytes = euckr_bytes(&src);
    assert_ne!(&bytes[..2], &[0xEF, 0xBB], "fixture must not be utf-8 bom");
    let out = process_file("kr.txt", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "euc-kr RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked euc-kr");
    let (text, _, enc) = crate::parse::text::decode(&masked);
    assert_eq!(enc, crate::types::TextEncoding::EucKr);
    assert!(!text.contains(&rrn), "euc-kr masked still has raw: {text}");
    assert_eq!(out.report.residual_confirmed, 0);
    let html = html_report(&[out.report.clone()]);
    assert!(
        html.contains("잔여 확정 0"),
        "html report must show residual from process_file: {html}"
    );
    assert!(html.contains("kr.txt"));
}

#[test]
fn process_file_pdf_annotation_contents_are_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_annotation("공개 본문", &format!("숨긴 메모 {rrn}"));
    let out = process_file("note.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF annotation RRN missed: {:?} warnings={:?}",
        out.report.findings,
        out.report.warnings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt fallback")).unwrap();
    assert!(!masked.contains(&rrn), "annotation PII left in fallback: {masked}");
    assert!(masked.contains("공개 본문"), "visible body lost: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_form_xobject_header_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_form_xobject("페이지 본문", &format!("머리글 {rrn}"));
    let out = process_file("xf.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF XObject header RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "xobject PII left: {masked}");
    assert!(masked.contains("페이지 본문"), "page body lost: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_comment_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_comment(&format!("메모 {rrn}"));
    let out = process_file("memo.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "xlsx comment RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked xlsx comment");
    let again = crate::parse::extract("memo.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "comment left raw: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_print_header_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_print_header(&format!("인쇄머리 {rrn}"));
    let out = process_file("hf.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "print header RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked xlsx hf");
    let again = crate::parse::extract("hf.xlsx", &masked).unwrap();
    assert!(
        !again.full_text.contains(&rrn),
        "print header left raw: {}",
        again.full_text
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_info_subject_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_info_subject(&format!("메타 {rrn}"));
    let out = process_file("meta.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF Info Subject RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "info PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_hwp_prvtext_preview_is_detected_and_cleared() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = hwp_with_prvtext_preview("본문만", &format!("미리보기 {rrn}"));
    let out = process_file("prv.hwp", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Hwp);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PrvText RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked hwp");
    let again = crate::parse::extract("prv.hwp", &masked).unwrap();
    assert!(
        !again.full_text.contains(&rrn),
        "PrvText/body still has raw: {}",
        again.full_text
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_docx_core_description_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = docx_with_core_description(&format!("속성 {rrn}"), "본문만");
    let out = process_file("core.docx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "core.xml RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked docx core");
    let again = crate::parse::extract("core.docx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "core prop left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "body lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_defined_name_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_defined_name(&rrn);
    let out = process_file("name.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "definedName RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked definedName");
    let again = crate::parse::extract("name.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "definedName left raw: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_outline_title_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_outline_title(&format!("북마크 {rrn}"));
    let out = process_file("bm.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF outline title RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "outline PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_csv_multiline_quoted_field_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let csv = format!("name,note\n\"홍길동\",\"첫째줄\n{rrn}\n셋째줄\"\n");
    let out = process_file("multi.csv", csv.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Csv);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "multiline csv RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("masked csv")).unwrap();
    assert!(!masked.contains(&rrn), "multiline csv left raw: {masked}");
    assert!(
        masked.starts_with("name,note"),
        "csv header lost: {masked}"
    );
    assert!(
        masked.matches('"').count() >= 4,
        "quotes dropped around multiline field: {masked}"
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_docx_custom_xml_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = docx_with_custom_xml(&rrn, "본문만");
    let out = process_file("cx.docx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "customXml RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked customXml");
    let again = crate::parse::extract("cx.docx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "customXml left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "body lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pptx_notes_are_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pptx_with_notes_only(&format!("발표자 메모 {rrn}"), "본문만");
    let out = process_file("deck.pptx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pptx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PPTX notes RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked pptx");
    let again = crate::parse::extract("deck.pptx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "notes left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "slide body lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_drawing_textbox_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_drawing(&format!("도형 {rrn}"), "본문만");
    let out = process_file("box.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX drawing RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked drawing");
    let again = crate::parse::extract("box.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "drawing left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_open_action_js_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_open_action_js(&format!("var id='{rrn}';"));
    let out = process_file("js.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF OpenAction JS RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "JS PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_chart_title_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_chart_title(&format!("실적 {rrn}"), "본문만");
    let out = process_file("chart.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX chart title RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked chart");
    let again = crate::parse::extract("chart.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "chart title left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_odt_paragraph_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = odt_with_text(&format!("공문 {rrn}"));
    let out = process_file("memo.odt", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Odt);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "ODT RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked odt");
    let again = crate::parse::extract("memo.odt", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "odt left raw: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_ods_cell_text_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = ods_with_text(&format!("셀 {rrn}"));
    let out = process_file("grid.ods", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Ods);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "ODS RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked ods");
    let again = crate::parse::extract("grid.ods", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "ods left raw: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pptx_comment_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pptx_with_comment_only(&format!("검토 {rrn}"), "본문만");
    let out = process_file("notes.pptx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pptx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PPTX comment RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked pptx comment");
    let again = crate::parse::extract("notes.pptx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "comment left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "slide lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_pivot_cache_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_pivot_cache(&rrn, "본문만");
    let out = process_file("pivot.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "pivot cache RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked pivot");
    let again = crate::parse::extract("pivot.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "pivot left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pptx_slide_master_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pptx_with_master_only(&format!("마스터 {rrn}"), "본문만");
    let out = process_file("master.pptx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pptx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PPTX slideMaster RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked master");
    let again = crate::parse::extract("master.pptx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "master left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "slide lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_embedded_filespec_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_embedded_filespec(&format!("첨부 {rrn}"));
    let out = process_file("attach.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF Filespec Desc RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "Filespec PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_table_display_name_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_table_display_name(&format!("명단{rrn}"), "본문만");
    let out = process_file("table.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX table displayName RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked table");
    let again = crate::parse::extract("table.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "table name left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_tsv_multiline_quoted_field_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let tsv = format!("name\tnote\n\"홍길동\"\t\"첫째줄\n{rrn}\n셋째줄\"\n");
    let out = process_file("multi.tsv", tsv.as_bytes(), &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Csv);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "multiline tsv RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("masked tsv")).unwrap();
    assert!(!masked.contains(&rrn), "multiline tsv left raw: {masked}");
    assert!(
        masked.starts_with("name\tnote"),
        "tsv header lost: {masked}"
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_epub_xhtml_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = epub_with_text(&format!("장 {rrn}"));
    let out = process_file("book.epub", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Epub);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "EPUB XHTML RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked epub");
    let again = crate::parse::extract("book.epub", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "epub left raw: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_uri_action_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_uri_action(&format!("https://hr.example/?ssn={rrn}"));
    let out = process_file("link.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF URI RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "URI PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_docx_instr_hyperlink_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = docx_with_instr_hyperlink(&format!("https://hr.example/?ssn={rrn}"), "본문만");
    let out = process_file("field.docx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "DOCX instrText RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked instr");
    let again = crate::parse::extract("field.docx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "instrText left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "body lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_docx_rels_target_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = docx_with_rel_target(&format!("https://hr.example/?ssn={rrn}"), "본문만");
    let out = process_file("rel.docx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "DOCX rels Target RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked rels");
    let again = crate::parse::extract("rel.docx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "rels Target left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "body lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_sheet_name_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_sheet_name(&format!("명단{rrn}"), "본문만");
    let out = process_file("tabs.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX sheet name RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked sheet name");
    let again = crate::parse::extract("tabs.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "sheet name left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_connection_name_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_connection_name(&format!("쿼리{rrn}"), "본문만");
    let out = process_file("conn.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX connection name RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked connection");
    let again = crate::parse::extract("conn.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "connection left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_defined_name_attr_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_defined_name_attr(&format!("범위{rrn}"), "본문만");
    let out = process_file("nattr.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "definedName name attr RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked name attr");
    let again = crate::parse::extract("nattr.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "name attr left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_named_dest_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_named_dest(&format!("점프 {rrn}"));
    let out = process_file("dest.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF named dest RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "named dest PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_hyperlink_display_is_masked() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = xlsx_with_hyperlink_display(&format!("링크 {rrn}"), "본문만");
    let out = process_file("link.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Xlsx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "XLSX hyperlink display RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked hyperlink");
    let again = crate::parse::extract("link.xlsx", &masked).unwrap();
    assert!(!again.full_text.contains(&rrn), "hyperlink display left raw: {}", again.full_text);
    assert!(again.full_text.contains("본문만"), "cell lost: {}", again.full_text);
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_pdf_struct_alt_is_detected() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = pdf_with_struct_alt(&format!("도표 {rrn}"));
    let out = process_file("alt.pdf", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Pdf);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "PDF struct /Alt RRN missed: {:?}",
        out.report.findings
    );
    let masked = String::from_utf8(out.masked_bytes.expect("pdf txt")).unwrap();
    assert!(!masked.contains(&rrn), "struct alt PII left: {masked}");
    assert_eq!(out.report.residual_confirmed, 0);
}

fn docx_with_header_only(header: &str, body: &str) -> Vec<u8> {
    let body_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
        xml_escape(body)
    );
    let header_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:p><w:r><w:t>{}</w:t></w:r></w:p></w:hdr>"#,
        xml_escape(header)
    );
    write_zip(&[
        ("word/document.xml".into(), body_xml.into_bytes()),
        ("word/header1.xml".into(), header_xml.into_bytes()),
    ])
    .expect("docx header zip")
}

fn xlsx_shared_index_looks_like_card(index: &str) -> Vec<u8> {
    let ss = r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1">
<si><t>hello</t></si></sst>"#;
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="s"><v>{index}</v></c></row></sheetData></worksheet>"#
    );
    write_zip(&[
        ("xl/sharedStrings.xml".into(), ss.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
        ("xl/workbook.xml".into(), b"<workbook/>".to_vec()),
    ])
    .expect("xlsx index zip")
}

#[test]
fn process_file_masks_pii_in_docx_header() {
    let rrn = rrn_string([9, 0, 0, 1, 0, 1], 1, [2, 3, 4, 5, 6]);
    let bytes = docx_with_header_only(&format!("머리말 {rrn}"), "본문만");
    let out = process_file("header.docx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Docx);
    assert!(
        out.report
            .findings
            .iter()
            .any(|f| f.raw == rrn && f.confidence == Confidence::Confirmed),
        "header RRN missed: {:?}",
        out.report.findings
    );
    let masked = out.masked_bytes.expect("masked docx");
    let again = crate::parse::extract("header.docx", &masked).unwrap();
    assert!(
        !again.full_text.contains(&rrn),
        "header rewrite left raw: {}",
        again.full_text
    );
    assert!(
        again.full_text.contains("본문만"),
        "body lost: {}",
        again.full_text
    );
    assert_eq!(out.report.residual_confirmed, 0);
}

#[test]
fn process_file_xlsx_shared_string_index_is_not_card() {
    let card = card_string();
    let digits: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    let bytes = xlsx_shared_index_looks_like_card(&digits);
    let out = process_file("idx.xlsx", &bytes, &cfg(MaskMode::Full, true)).unwrap();
    assert!(
        out.report
            .findings
            .iter()
            .all(|f| f.rule_id != "credit_card"),
        "shared-string index must not be a card: {:?}",
        out.report.findings
    );
}

#[test]
fn driver_passport_bank_health_confirmed_via_process_file() {
    let license = "서울-01-123456-90";
    let passport = "M12345678";
    let account = "110-123-456789";
    let health = "123456-12345";
    let text = format!("면허 {license} 여권 {passport} 계좌 {account} 보험 {health}");
    let hits = confirmed_raws("ids.txt", text.as_bytes());
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "driver_license" && raw == license && *c == Confidence::Confirmed
        }),
        "driver license missed: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "passport" && raw == passport && *c == Confidence::Confirmed
        }),
        "passport missed: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "bank_account" && raw == account && *c == Confidence::Confirmed
        }),
        "bank account missed: {hits:?}"
    );
    assert!(
        hits.iter().any(|(id, raw, c)| {
            id == "health_insurance" && raw == health && *c == Confidence::Confirmed
        }),
        "health insurance missed: {hits:?}"
    );
}
