//! Browser-oriented PII detection and masking. All logic is pure and I/O-free
//! aside from in-memory bytes.

pub mod checksum;
pub mod detect;
pub mod error;
pub mod mask;
pub mod parse;
pub mod process;
pub mod report;
pub mod rewrite;
pub mod rules;
pub mod types;

pub use error::{Error, Result};
pub use process::{process_file, process_many, ProcessConfig, ProcessResult};
pub use report::{csv_report, html_report, json_report};
pub use rules::{RuleSet, DEFAULT_RULES_TOML};
pub use types::{Confidence, FileFormat, FileReport, Finding, MaskMode};

#[cfg(test)]
mod pipeline_tests;

use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

/// Pack masked outputs into a zip for "download all".
pub fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in files {
            zip.start_file(name, opts)
                .map_err(|e| Error::msg(e.to_string()))?;
            zip.write_all(data).map_err(|e| Error::msg(e.to_string()))?;
        }
        zip.finish().map_err(|e| Error::msg(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}
