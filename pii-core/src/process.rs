use crate::error::Result;
use crate::parse::extract;
use crate::report::summaries;
use crate::rewrite::rewrite;
use crate::rules::RuleSet;
use crate::types::{Confidence, FileReport, MaskMode};

pub struct ProcessConfig {
    pub rules: RuleSet,
    pub mask_mode: MaskMode,
    pub do_mask: bool,
}

pub struct ProcessResult {
    pub report: FileReport,
    pub masked_bytes: Option<Vec<u8>>,
    pub output_filename: Option<String>,
    pub fallback_note: Option<String>,
}

pub fn process_file(name: &str, bytes: &[u8], cfg: &ProcessConfig) -> Result<ProcessResult> {
    let extracted = extract(name, bytes)?;
    let findings = crate::rewrite::detect_extracted(&extracted, &cfg.rules);
    let confirmed = findings
        .iter()
        .filter(|f| f.confidence == Confidence::Confirmed)
        .count();
    let suspicious = findings
        .iter()
        .filter(|f| f.confidence == Confidence::Suspicious)
        .count();
    let already_masked = findings
        .iter()
        .filter(|f| f.confidence == Confidence::AlreadyMasked)
        .count();

    let mut report = FileReport {
        filename: name.to_string(),
        format: extracted.format,
        warnings: extracted.warnings.clone(),
        paragraph_count: extracted.paragraphs.len(),
        findings: findings.clone(),
        summaries: summaries(&findings),
        diffs: Vec::new(),
        confirmed,
        suspicious,
        already_masked,
    };

    if !cfg.do_mask {
        return Ok(ProcessResult {
            report,
            masked_bytes: None,
            output_filename: None,
            fallback_note: None,
        });
    }

    let out = rewrite(&extracted, &findings, cfg.mask_mode, &cfg.rules)?;
    report.diffs = out.diffs;
    if let Some(note) = &out.fallback_note {
        report.warnings.push(note.clone());
    }
    Ok(ProcessResult {
        report,
        masked_bytes: Some(out.bytes),
        output_filename: Some(out.filename),
        fallback_note: out.fallback_note,
    })
}

pub fn process_many(
    files: &[(String, Vec<u8>)],
    cfg: &ProcessConfig,
) -> Result<Vec<ProcessResult>> {
    let mut out = Vec::new();
    for (name, bytes) in files {
        out.push(process_file(name, bytes, cfg)?);
    }
    Ok(out)
}
