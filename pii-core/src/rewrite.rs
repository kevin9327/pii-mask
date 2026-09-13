use crate::detect::detect_text;
use crate::error::Result;
use crate::mask::apply_findings;
use crate::parse::hwp::{paragraph_at_mut, set_paragraph_text, write_doc};
use crate::parse::ooxml::{rewrite_docx_paragraphs, rewrite_shared_strings, rewrite_sheet_values, write_zip};
use crate::parse::text::split_paragraphs;
use crate::rules::RuleSet;
use crate::types::{
    Confidence, DiffHunk, Extracted, FileFormat, Finding, MaskMode, ParaLoc,
};

pub struct RewriteOut {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub fallback_note: Option<String>,
    pub diffs: Vec<DiffHunk>,
    pub masked_paragraphs: Vec<String>,
}

pub fn rewrite(
    extracted: &Extracted,
    findings: &[Finding],
    mode: MaskMode,
    rules: &RuleSet,
) -> Result<RewriteOut> {
    let mut masked_paras: Vec<String> = Vec::new();
    let mut diffs = Vec::new();
    for p in &extracted.paragraphs {
        let local: Vec<Finding> = findings
            .iter()
            .filter(|f| f.paragraph_index == p.index)
            .cloned()
            .collect();
        let after = apply_findings(&p.text, &local, mode, rules);
        if after != p.text {
            diffs.push(DiffHunk {
                paragraph_index: p.index,
                before: p.text.clone(),
                after: after.clone(),
            });
        }
        masked_paras.push(after);
    }

    match extracted.format {
        FileFormat::Hwp | FileFormat::Hwpx | FileFormat::Hwp3 => {
            match rewrite_hwp(extracted, &masked_paras) {
                Ok(bytes) => Ok(RewriteOut {
                    bytes,
                    filename: extracted.filename.clone(),
                    fallback_note: None,
                    diffs,
                    masked_paragraphs: masked_paras,
                }),
                Err(e) => {
                    let txt = masked_paras.join("\n");
                    Ok(RewriteOut {
                        bytes: txt.into_bytes(),
                        filename: replace_ext(&extracted.filename, "txt"),
                        fallback_note: Some(format!(
                            "원 포맷({}) 재작성에 실패하여 TXT로 대체했습니다: {e}",
                            extracted.format.as_str()
                        )),
                        diffs,
                        masked_paragraphs: masked_paras,
                    })
                }
            }
        }
        FileFormat::Docx | FileFormat::Xlsx => match rewrite_zip(extracted, &masked_paras) {
            Ok(bytes) => Ok(RewriteOut {
                bytes,
                filename: extracted.filename.clone(),
                fallback_note: None,
                diffs,
                masked_paragraphs: masked_paras,
            }),
            Err(e) => {
                let txt = masked_paras.join("\n");
                Ok(RewriteOut {
                    bytes: txt.into_bytes(),
                    filename: replace_ext(&extracted.filename, "txt"),
                    fallback_note: Some(format!(
                        "원 포맷 재작성에 실패하여 TXT로 대체했습니다: {e}"
                    )),
                    diffs,
                    masked_paragraphs: masked_paras,
                })
            }
        },
        FileFormat::Pdf => {
            let txt = masked_paras.join("\n");
            Ok(RewriteOut {
                bytes: txt.into_bytes(),
                filename: replace_ext(&extracted.filename, "txt"),
                fallback_note: Some(
                    "PDF 텍스트 레이어 재작성은 지원하지 않아 TXT로 대체 출력합니다.".into(),
                ),
                diffs,
                masked_paragraphs: masked_paras,
            })
        }
        FileFormat::Unknown => Ok(RewriteOut {
            bytes: extracted.original.clone(),
            filename: extracted.filename.clone(),
            fallback_note: Some("지원하지 않는 형식이라 원본을 그대로 둡니다.".into()),
            diffs,
            masked_paragraphs: masked_paras,
        }),
        FileFormat::Json => {
            let bytes = rewrite_json(extracted, findings, mode, rules);
            Ok(RewriteOut {
                bytes: bytes.into_bytes(),
                filename: extracted.filename.clone(),
                fallback_note: None,
                diffs,
                masked_paragraphs: masked_paras,
            })
        }
        FileFormat::Txt | FileFormat::Csv => {
            let bytes = rewrite_plain(extracted, findings, mode, rules);
            Ok(RewriteOut {
                bytes: bytes.into_bytes(),
                filename: extracted.filename.clone(),
                fallback_note: None,
                diffs,
                masked_paragraphs: masked_paras,
            })
        }
    }
}

fn rewrite_plain(
    extracted: &Extracted,
    findings: &[Finding],
    mode: MaskMode,
    rules: &RuleSet,
) -> String {
    let (text, _) = crate::parse::text::decode(&extracted.original);
    // Map paragraph findings back onto original text using sequential byte ranges.
    let mut ops: Vec<(usize, usize, String)> = Vec::new();
    for f in findings {
        if f.confidence == Confidence::AlreadyMasked {
            continue;
        }
        let Some(p) = extracted.paragraphs.get(f.paragraph_index) else {
            continue;
        };
        if let ParaLoc::Sequential {
            byte_start,
            ..
        } = p.loc
        {
            ops.push((byte_start + f.byte_start, byte_start + f.byte_end, {
                let rule = rules.rules.iter().find(|r| r.id == f.rule_id);
                crate::mask::apply_mode(
                    &f.raw,
                    mode,
                    rule.map(|r| r.partial.as_str()).unwrap_or(""),
                    rule.map(|r| r.replace_token.as_str()).unwrap_or("[PII]"),
                )
            }));
        }
    }
    ops.sort_by_key(|(s, _, _)| std::cmp::Reverse(*s));
    let mut buf = text;
    for (s, e, repl) in ops {
        if e <= buf.len() && s <= e {
            buf.replace_range(s..e, &repl);
        }
    }
    buf
}

/// JSON rewrite keeps a parseable document: hits outside strings (numbers)
/// are emitted as JSON strings so `****************` cannot break the file.
fn rewrite_json(
    extracted: &Extracted,
    findings: &[Finding],
    mode: MaskMode,
    rules: &RuleSet,
) -> String {
    let (text, _) = crate::parse::text::decode(&extracted.original);
    let mut ops: Vec<(usize, usize, String, bool)> = Vec::new();
    for f in findings {
        if f.confidence == Confidence::AlreadyMasked {
            continue;
        }
        let Some(p) = extracted.paragraphs.get(f.paragraph_index) else {
            continue;
        };
        if let ParaLoc::Sequential { byte_start, .. } = p.loc {
            let start = byte_start + f.byte_start;
            let end = byte_start + f.byte_end;
            let rule = rules.rules.iter().find(|r| r.id == f.rule_id);
            let repl = crate::mask::apply_mode(
                &f.raw,
                mode,
                rule.map(|r| r.partial.as_str()).unwrap_or(""),
                rule.map(|r| r.replace_token.as_str()).unwrap_or("[PII]"),
            );
            let inside = json_span_in_string(&text, start);
            ops.push((start, end, repl, inside));
        }
    }
    ops.sort_by_key(|(s, _, _, _)| std::cmp::Reverse(*s));
    let mut buf = text;
    for (s, e, repl, inside) in ops {
        if e > buf.len() || s > e {
            continue;
        }
        let token = if inside {
            json_escape_inside_string(&repl)
        } else {
            format!("\"{}\"", json_escape_inside_string(&repl))
        };
        buf.replace_range(s..e, &token);
    }
    buf
}

fn json_span_in_string(src: &str, byte_index: usize) -> bool {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut in_string = false;
    let mut escape = false;
    while i < byte_index && i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        }
        i += 1;
    }
    in_string
}

fn json_escape_inside_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn rewrite_hwp(extracted: &Extracted, masked_paras: &[String]) -> Result<Vec<u8>> {
    let mut doc = extracted
        .hwp_doc
        .clone()
        .ok_or_else(|| crate::error::Error::msg("HWP IR 이 없습니다"))?;
    for (p, new_text) in extracted.paragraphs.iter().zip(masked_paras.iter()) {
        if let Some(para) = paragraph_at_mut(&mut doc, &p.loc) {
            set_paragraph_text(para, new_text.clone());
        }
    }
    write_doc(&doc, extracted.format)
}

fn rewrite_zip(extracted: &Extracted, masked_paras: &[String]) -> Result<Vec<u8>> {
    let mut parts = extracted
        .zip_parts
        .clone()
        .ok_or_else(|| crate::error::Error::msg("ZIP 파트가 없습니다"))?;

    // Group new texts by inner_path in paragraph order.
    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, t) in extracted.paragraphs.iter().zip(masked_paras.iter()) {
        if let ParaLoc::ZipXml { inner_path, .. } = &p.loc {
            grouped.entry(inner_path.clone()).or_default().push(t.clone());
        }
    }

    for (name, data) in parts.iter_mut() {
        let key = name.replace('\\', "/");
        let Some(texts) = grouped.get(&key) else {
            continue;
        };
        let xml = String::from_utf8_lossy(data).into_owned();
        let next = if key.starts_with("word/") && key.ends_with(".xml") {
            rewrite_docx_paragraphs(&xml, texts)
        } else if key == "xl/sharedStrings.xml" {
            rewrite_shared_strings(&xml, texts)
        } else if key.starts_with("xl/worksheets/") {
            rewrite_sheet_values(&xml, texts)
        } else {
            xml
        };
        *data = next.into_bytes();
    }
    write_zip(&parts)
}

pub fn replace_ext(name: &str, ext: &str) -> String {
    if let Some((stem, _)) = name.rsplit_once('.') {
        format!("{stem}.{ext}")
    } else {
        format!("{name}.{ext}")
    }
}

/// Re-detect after masking is not required; used by tests to rebuild paragraphs.
pub fn masked_as_paragraphs(texts: &[String]) -> Vec<crate::types::Paragraph> {
    split_paragraphs(&texts.join("\n"))
}

pub fn detect_extracted(extracted: &Extracted, rules: &RuleSet) -> Vec<Finding> {
    let mut all = Vec::new();
    for p in &extracted.paragraphs {
        all.extend(detect_text(&p.text, rules, p.index));
    }
    all
}
