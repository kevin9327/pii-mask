//! End-to-end tests that drive `process_file` (the shipped entry point).

use crate::checksum::{biz_check_digit, corp_check_digit, luhn_check_digit, rrn_check_digit};
use crate::parse::ooxml::{xml_escape, write_zip};
use crate::report::{csv_report, html_report, json_report};
use crate::types::Confidence;
use crate::{process_file, MaskMode, ProcessConfig, RuleSet};
use docagent_model::{Block, Document, Paragraph, Section};

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

fn hwpx_with_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new();
    let mut section = Section::default();
    section.body.push(Block::Paragraph(Paragraph::from_text(text)));
    doc.sections.push(section);
    docagent_hwpx::write(&doc).expect("hwpx write")
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
fn unknown_zip_is_not_parsed_as_txt() {
    let pptx_like = write_zip(&[("ppt/slides/slide1.xml".into(), b"<p/>".to_vec())]).unwrap();
    let out = process_file("deck.pptx", &pptx_like, &cfg(MaskMode::Full, true)).unwrap();
    assert_eq!(out.report.format, crate::FileFormat::Unknown);
    assert!(
        out.report.warnings.iter().any(|w| w.contains("지원하지 않는")),
        "warnings={:?}",
        out.report.warnings
    );
    assert!(out.report.findings.is_empty());
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
