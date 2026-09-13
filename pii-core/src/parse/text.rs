use encoding_rs::{EUC_KR, UTF_16BE, UTF_16LE};

use crate::error::Result;
use crate::types::{Extracted, FileFormat, ParaLoc, Paragraph, TextEncoding};

pub fn extract(filename: &str, bytes: &[u8], format: FileFormat) -> Result<Extracted> {
    let (text, warn, enc) = decode(bytes);
    let mut warnings = Vec::new();
    if let Some(w) = warn {
        warnings.push(w);
    }
    let paragraphs = split_paragraphs(&text);
    let mut extracted = super::finish(
        filename,
        if format == FileFormat::Unknown {
            FileFormat::Txt
        } else {
            format
        },
        bytes.to_vec(),
        paragraphs,
        warnings,
        None,
        None,
    );
    extracted.encoding = Some(enc);
    Ok(extracted)
}

pub fn decode(bytes: &[u8]) -> (String, Option<String>, TextEncoding) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (
            String::from_utf8_lossy(&bytes[3..]).into_owned(),
            None,
            TextEncoding::Utf8Bom,
        );
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (cow, _, _) = UTF_16LE.decode(&bytes[2..]);
        return (cow.into_owned(), None, TextEncoding::Utf16Le);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (cow, _, _) = UTF_16BE.decode(&bytes[2..]);
        return (cow.into_owned(), None, TextEncoding::Utf16Be);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), None, TextEncoding::Utf8);
    }
    let (cow, _, had_errors) = EUC_KR.decode(bytes);
    let warn = if had_errors {
        Some("UTF-8이 아니어서 EUC-KR로 해석했습니다.".to_string())
    } else {
        Some("EUC-KR/CP949 로 해석했습니다.".to_string())
    };
    (cow.into_owned(), warn, TextEncoding::EucKr)
}

pub fn encode(text: &str, enc: TextEncoding) -> Vec<u8> {
    match enc {
        TextEncoding::Utf8 => text.as_bytes().to_vec(),
        TextEncoding::Utf8Bom => {
            let mut out = vec![0xEF, 0xBB, 0xBF];
            out.extend_from_slice(text.as_bytes());
            out
        }
        TextEncoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            out.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
            out
        }
        TextEncoding::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            out.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
            out
        }
        TextEncoding::EucKr => {
            let (cow, _, _) = EUC_KR.encode(text);
            cow.into_owned()
        }
    }
}

pub fn split_paragraphs(text: &str) -> Vec<Paragraph> {
    if text.is_empty() {
        return vec![Paragraph {
            index: 0,
            text: String::new(),
            full_byte_start: 0,
            loc: ParaLoc::Sequential {
                byte_start: 0,
                byte_end: 0,
            },
        }];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            let end = i;
            let line = text[start..end].trim_end_matches('\r').to_string();
            out.push(Paragraph {
                index: out.len(),
                text: line,
                full_byte_start: 0,
                loc: ParaLoc::Sequential {
                    byte_start: start,
                    byte_end: end,
                },
            });
            start = i + 1;
        }
    }
    if start <= text.len() {
        let line = text[start..].trim_end_matches('\r').to_string();
        // Keep the last line even if empty only when there was no trailing split...
        out.push(Paragraph {
            index: out.len(),
            text: line,
            full_byte_start: 0,
            loc: ParaLoc::Sequential {
                byte_start: start,
                byte_end: text.len(),
            },
        });
    }
    out
}
