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
    let content = lopdf::content::Content::decode(&data).map_err(|e| Error::Parse(e.to_string()))?;
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
