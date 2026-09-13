use lopdf::{Document, Object};

use crate::error::{Error, Result};
use crate::types::{Extracted, FileFormat, ParaLoc, Paragraph};

pub fn extract(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let doc = Document::load_mem(bytes).map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    let mut paragraphs = Vec::new();
    let mut warnings = Vec::new();
    let pages = doc.get_pages();
    if pages.is_empty() {
        warnings.push("PDF 페이지가 없습니다.".into());
    }
    let mut any_text = false;
    for (page_num, page_id) in pages {
        let text = page_text(&doc, page_id).unwrap_or_default();
        if !text.trim().is_empty() {
            any_text = true;
        }
        for line in text.split('\n') {
            paragraphs.push(Paragraph {
                index: paragraphs.len(),
                text: line.to_string(),
                full_byte_start: 0,
                loc: ParaLoc::Pdf { page: page_num },
            });
        }
        for xo in page_xobject_texts(&doc, page_id) {
            if xo.trim().is_empty() {
                continue;
            }
            any_text = true;
            for line in xo.split('\n') {
                paragraphs.push(Paragraph {
                    index: paragraphs.len(),
                    text: line.to_string(),
                    full_byte_start: 0,
                    loc: ParaLoc::Pdf { page: page_num },
                });
            }
        }
    }
    for note in document_info_text(&doc) {
        if note.trim().is_empty() {
            continue;
        }
        any_text = true;
        paragraphs.push(Paragraph {
            index: paragraphs.len(),
            text: note,
            full_byte_start: 0,
            loc: ParaLoc::Pdf { page: 0 },
        });
    }
    for note in outline_titles(&doc) {
        if note.trim().is_empty() {
            continue;
        }
        any_text = true;
        paragraphs.push(Paragraph {
            index: paragraphs.len(),
            text: note,
            full_byte_start: 0,
            loc: ParaLoc::Pdf { page: 0 },
        });
    }
    for note in annotation_and_field_text(&doc) {
        if note.trim().is_empty() {
            continue;
        }
        any_text = true;
        paragraphs.push(Paragraph {
            index: paragraphs.len(),
            text: note,
            full_byte_start: 0,
            loc: ParaLoc::Pdf { page: 0 },
        });
    }
    for note in javascript_and_open_action_text(&doc) {
        if note.trim().is_empty() {
            continue;
        }
        any_text = true;
        paragraphs.push(Paragraph {
            index: paragraphs.len(),
            text: note,
            full_byte_start: 0,
            loc: ParaLoc::Pdf { page: 0 },
        });
    }
    if !any_text {
        warnings.push("텍스트 없음 (스캔 PDF이거나 텍스트 레이어가 없습니다. OCR은 지원하지 않습니다).".into());
    }
    Ok(super::finish(
        filename,
        FileFormat::Pdf,
        bytes.to_vec(),
        paragraphs,
        warnings,
        None,
        None,
    ))
}

fn page_text(doc: &Document, page_id: lopdf::ObjectId) -> Result<String> {
    let data = doc
        .get_page_content(page_id)
        .map_err(|e| Error::Parse(e.to_string()))?;
    content_stream_text(&data)
}

/// Header/footer graphics are often Form XObjects, not the page stream.
fn page_xobject_texts(doc: &Document, page_id: lopdf::ObjectId) -> Vec<String> {
    let Ok(page) = doc.get_object(page_id) else {
        return Vec::new();
    };
    let Ok(dict) = page.as_dict() else {
        return Vec::new();
    };
    let Ok(res_obj) = dict.get(b"Resources") else {
        return Vec::new();
    };
    let Some(res) = resolve_obj(doc, res_obj) else {
        return Vec::new();
    };
    let Object::Dictionary(res_dict) = res else {
        return Vec::new();
    };
    let Ok(xo) = res_dict.get(b"XObject") else {
        return Vec::new();
    };
    let Some(xo) = resolve_obj(doc, xo) else {
        return Vec::new();
    };
    let Object::Dictionary(xo_dict) = xo else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_name, obj) in xo_dict.iter() {
        if let Some(text) = form_xobject_text(doc, obj) {
            if !text.trim().is_empty() {
                out.push(text);
            }
        }
    }
    out
}

fn form_xobject_text(doc: &Document, obj: &Object) -> Option<String> {
    let resolved = resolve_obj(doc, obj)?;
    let Object::Stream(stream) = resolved else {
        return None;
    };
    let subtype = stream.dict.get(b"Subtype").ok().and_then(object_name)?;
    if subtype != "Form" {
        return None;
    }
    let mut stream = stream.clone();
    let _ = stream.decompress();
    content_stream_text(&stream.content).ok()
}

fn resolve_obj<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Object> {
    match obj {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

fn content_stream_text(data: &[u8]) -> Result<String> {
    let content = lopdf::content::Content::decode(data).map_err(|e| Error::Parse(e.to_string()))?;
    let mut out = String::new();
    for op in content.operations {
        match op.operator.as_str() {
            "Tj" | "'" => {
                if let Some(obj) = op.operands.first() {
                    push_pdf_string(obj, &mut out);
                }
            }
            "\"" => {
                if let Some(obj) = op.operands.get(2) {
                    push_pdf_string(obj, &mut out);
                }
            }
            "TJ" => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    for item in arr {
                        push_pdf_string(item, &mut out);
                    }
                }
            }
            "Td" | "TD" | "T*" => {
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn document_info_text(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(info) = doc.trailer.get(b"Info") {
        if let Some(resolved) = resolve_obj(doc, info) {
            if let Object::Dictionary(dict) = resolved {
                for key in [
                    b"Title".as_slice(),
                    b"Author".as_slice(),
                    b"Subject".as_slice(),
                    b"Keywords".as_slice(),
                    b"Creator".as_slice(),
                    b"Producer".as_slice(),
                ] {
                    if let Ok(v) = dict.get(key) {
                        if let Some(s) = pdf_obj_string(doc, v) {
                            if !s.trim().is_empty() {
                                out.push(s);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Ok(root) = doc.trailer.get(b"Root") {
        if let Some(catalog) = resolve_obj(doc, root) {
            if let Object::Dictionary(dict) = catalog {
                if let Ok(meta) = dict.get(b"Metadata") {
                    if let Some(s) = metadata_stream_text(doc, meta) {
                        out.push(s);
                    }
                }
            }
        }
    }
    out
}

fn outline_titles(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(root) = doc.trailer.get(b"Root") else {
        return out;
    };
    let Some(catalog) = resolve_obj(doc, root) else {
        return out;
    };
    let Object::Dictionary(dict) = catalog else {
        return out;
    };
    let Ok(outlines) = dict.get(b"Outlines") else {
        return out;
    };
    let mut seen = std::collections::HashSet::new();
    walk_outline(doc, outlines, &mut seen, &mut out, 0);
    out
}

fn walk_outline(
    doc: &Document,
    obj: &Object,
    seen: &mut std::collections::HashSet<(u32, u16)>,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let Some(resolved) = resolve_obj(doc, obj) else {
        return;
    };
    if let Object::Reference(id) = obj {
        if !seen.insert(*id) {
            return;
        }
    }
    let Object::Dictionary(dict) = resolved else {
        return;
    };
    if let Ok(title) = dict.get(b"Title") {
        if let Some(s) = pdf_obj_string(doc, title) {
            if !s.trim().is_empty() {
                out.push(s);
            }
        }
    }
    if let Ok(first) = dict.get(b"First") {
        walk_outline(doc, first, seen, out, depth + 1);
    }
    if let Ok(next) = dict.get(b"Next") {
        walk_outline(doc, next, seen, out, depth + 1);
    }
}

fn metadata_stream_text(doc: &Document, obj: &Object) -> Option<String> {
    let resolved = resolve_obj(doc, obj)?;
    let Object::Stream(stream) = resolved else {
        return None;
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    String::from_utf8(stream.content.clone())
        .ok()
        .or_else(|| Some(String::from_utf8_lossy(&stream.content).into_owned()))
}

/// Catalog /OpenAction and any dictionary /JS string or stream.
fn javascript_and_open_action_text(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(root) = doc.trailer.get(b"Root") {
        if let Some(catalog) = resolve_obj(doc, root) {
            if let Object::Dictionary(dict) = catalog {
                if let Ok(oa) = dict.get(b"OpenAction") {
                    collect_js_from_obj(doc, oa, &mut out, 0);
                }
                if let Ok(aa) = dict.get(b"AA") {
                    collect_js_from_obj(doc, aa, &mut out, 0);
                }
            }
        }
    }
    for object in doc.objects.values() {
        match object {
            Object::Dictionary(dict) => {
                if let Ok(js) = dict.get(b"JS") {
                    if let Some(s) = pdf_js_payload(doc, js) {
                        if !s.trim().is_empty() {
                            out.push(s);
                        }
                    }
                }
            }
            Object::Stream(stream) => {
                if let Ok(js) = stream.dict.get(b"JS") {
                    if let Some(s) = pdf_js_payload(doc, js) {
                        if !s.trim().is_empty() {
                            out.push(s);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_js_from_obj(doc: &Document, obj: &Object, out: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Some(resolved) = resolve_obj(doc, obj) else {
        return;
    };
    match resolved {
        Object::Dictionary(dict) => {
            if let Ok(js) = dict.get(b"JS") {
                if let Some(s) = pdf_js_payload(doc, js) {
                    if !s.trim().is_empty() {
                        out.push(s);
                    }
                }
            }
            for key in [b"OpenAction".as_slice(), b"JavaScript".as_slice(), b"AA".as_slice()] {
                if let Ok(v) = dict.get(key) {
                    collect_js_from_obj(doc, v, out, depth + 1);
                }
            }
        }
        Object::Array(arr) => {
            for item in arr {
                collect_js_from_obj(doc, item, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn pdf_js_payload(doc: &Document, obj: &Object) -> Option<String> {
    let resolved = resolve_obj(doc, obj)?;
    match resolved {
        Object::String(bytes, _) => Some(decode_pdf_bytes(bytes)),
        Object::Stream(stream) => {
            let mut stream = stream.clone();
            let _ = stream.decompress();
            Some(
                String::from_utf8(stream.content.clone())
                    .unwrap_or_else(|_| String::from_utf8_lossy(&stream.content).into_owned()),
            )
        }
        _ => None,
    }
}

/// Sticky notes, markup /Contents, and AcroForm /V values — not the page
/// content stream. Forms and comments are a common PII hideout.
fn annotation_and_field_text(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for object in doc.objects.values() {
        let Object::Dictionary(dict) = object else {
            continue;
        };
        let type_name = dict.get(b"Type").ok().and_then(object_name).unwrap_or("");
        let subtype = dict.get(b"Subtype").ok().and_then(object_name).unwrap_or("");
        let is_annot = type_name == "Annot"
            || matches!(
                subtype,
                "Text"
                    | "Highlight"
                    | "FreeText"
                    | "Widget"
                    | "Popup"
                    | "StrikeOut"
                    | "Underline"
                    | "Caret"
                    | "Square"
                    | "Circle"
                    | "Line"
            );
        let is_field = dict.get(b"FT").is_ok();
        if !is_annot && !is_field {
            continue;
        }
        if let Ok(contents) = dict.get(b"Contents") {
            if let Some(s) = pdf_obj_string(doc, contents) {
                out.push(s);
            }
        }
        if let Ok(v) = dict.get(b"V") {
            if let Some(s) = pdf_obj_string(doc, v) {
                out.push(s);
            }
        }
    }
    out
}

fn object_name(obj: &Object) -> Option<&str> {
    match obj {
        Object::Name(n) => std::str::from_utf8(n).ok(),
        _ => None,
    }
}

fn pdf_obj_string(doc: &Document, obj: &Object) -> Option<String> {
    let resolved = match obj {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    match resolved {
        Object::String(bytes, _) => Some(decode_pdf_bytes(bytes)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

fn push_pdf_string(obj: &Object, out: &mut String) {
    if let Object::String(bytes, _) = obj {
        out.push_str(&decode_pdf_bytes(bytes));
    }
}

fn decode_pdf_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        if s.chars().any(|c| c as u32 > 127) {
            return s.to_string();
        }
    }
    bytes.iter().map(|&b| b as char).collect()
}
