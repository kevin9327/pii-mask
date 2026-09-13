pub mod hwp;
pub mod ooxml;
pub mod pdf;
pub mod text;

use crate::error::Result;
use crate::types::{Extracted, FileFormat};

pub fn extract(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let format = sniff(filename, bytes);
    match format {
        FileFormat::Hwp | FileFormat::Hwpx | FileFormat::Hwp3 => hwp::extract(filename, bytes, format),
        FileFormat::Pdf => pdf::extract(filename, bytes),
        FileFormat::Docx => ooxml::extract_docx(filename, bytes),
        FileFormat::Xlsx => ooxml::extract_xlsx(filename, bytes),
        FileFormat::Json | FileFormat::Csv | FileFormat::Txt | FileFormat::Unknown => {
            text::extract(filename, bytes, format)
        }
    }
}

pub fn sniff(filename: &str, bytes: &[u8]) -> FileFormat {
    if docagent_hwp5::sniff(bytes) {
        return FileFormat::Hwp;
    }
    if docagent_hwp3::sniff(bytes) {
        return FileFormat::Hwp3;
    }
    if docagent_hwpx::sniff(bytes) {
        return FileFormat::Hwpx;
    }
    if bytes.starts_with(b"%PDF") {
        return FileFormat::Pdf;
    }
    let ext = extension(filename);
    if looks_like_zip(bytes) {
        if zip_has(bytes, "word/document.xml") {
            return FileFormat::Docx;
        }
        if zip_has(bytes, "xl/workbook.xml") || zip_has(bytes, "xl/sharedStrings.xml") {
            return FileFormat::Xlsx;
        }
        if ext == "hwpx" {
            return FileFormat::Hwpx;
        }
        if ext == "docx" {
            return FileFormat::Docx;
        }
        if ext == "xlsx" {
            return FileFormat::Xlsx;
        }
    }
    match ext.as_str() {
        "json" => FileFormat::Json,
        "csv" => FileFormat::Csv,
        "txt" | "text" | "log" | "md" => FileFormat::Txt,
        "hwp" => FileFormat::Hwp,
        "hwpx" => FileFormat::Hwpx,
        "pdf" => FileFormat::Pdf,
        "docx" => FileFormat::Docx,
        "xlsx" => FileFormat::Xlsx,
        _ => FileFormat::Txt,
    }
}

fn extension(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4B
}

fn zip_has(bytes: &[u8], name: &str) -> bool {
    let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return false;
    };
    let found = z.by_name(name).is_ok();
    found
}

pub(crate) fn finish(
    filename: &str,
    format: FileFormat,
    original: Vec<u8>,
    mut paragraphs: Vec<crate::types::Paragraph>,
    warnings: Vec<String>,
    hwp_doc: Option<docagent_model::Document>,
    zip_parts: Option<Vec<(String, Vec<u8>)>>,
) -> Extracted {
    let mut full = String::new();
    for (i, p) in paragraphs.iter_mut().enumerate() {
        p.index = i;
        p.full_byte_start = full.len();
        if !full.is_empty() {
            full.push('\n');
            p.full_byte_start = full.len();
        }
        full.push_str(&p.text);
    }
    Extracted {
        format,
        filename: filename.to_string(),
        paragraphs,
        full_text: full,
        warnings,
        original,
        hwp_doc,
        zip_parts,
    }
}
