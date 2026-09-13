use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{Error, Result};
use crate::types::{Extracted, FileFormat, ParaLoc, Paragraph};

pub fn extract_docx(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let parts = read_zip(bytes)?;
    let xml = part_text(&parts, "word/document.xml").unwrap_or_default();
    let paras = docx_paragraphs(&xml);
    let mut paragraphs = Vec::new();
    for (i, t) in paras.into_iter().enumerate() {
        paragraphs.push(Paragraph {
            index: i,
            text: t,
            full_byte_start: 0,
            loc: ParaLoc::ZipXml {
                inner_path: "word/document.xml".into(),
                para_ord: i,
            },
        });
    }
    Ok(super::finish(
        filename,
        FileFormat::Docx,
        bytes.to_vec(),
        paragraphs,
        Vec::new(),
        None,
        Some(parts),
    ))
}

pub fn extract_xlsx(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let parts = read_zip(bytes)?;
    let mut paragraphs = Vec::new();
    if let Some(ss) = part_text(&parts, "xl/sharedStrings.xml") {
        for (i, t) in xlsx_shared_strings(&ss).into_iter().enumerate() {
            paragraphs.push(Paragraph {
                index: paragraphs.len(),
                text: t,
                full_byte_start: 0,
                loc: ParaLoc::ZipXml {
                    inner_path: "xl/sharedStrings.xml".into(),
                    para_ord: i,
                },
            });
        }
    }
    for (name, data) in &parts {
        if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
            let xml = String::from_utf8_lossy(data).into_owned();
            for (i, t) in xlsx_inline_and_values(&xml).into_iter().enumerate() {
                paragraphs.push(Paragraph {
                    index: paragraphs.len(),
                    text: t,
                    full_byte_start: 0,
                    loc: ParaLoc::ZipXml {
                        inner_path: name.clone(),
                        para_ord: i,
                    },
                });
            }
        }
    }
    Ok(super::finish(
        filename,
        FileFormat::Xlsx,
        bytes.to_vec(),
        paragraphs,
        Vec::new(),
        None,
        Some(parts),
    ))
}

pub fn read_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|e| Error::Parse(e.to_string()))?;
    let mut parts = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| Error::Parse(e.to_string()))?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| Error::Parse(e.to_string()))?;
        parts.push((name, data));
    }
    Ok(parts)
}

pub fn write_zip(parts: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in parts {
            zip.start_file(name, opts)
                .map_err(|e| Error::Parse(e.to_string()))?;
            zip.write_all(data).map_err(|e| Error::Parse(e.to_string()))?;
        }
        zip.finish().map_err(|e| Error::Parse(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn part_text(parts: &[(String, Vec<u8>)], name: &str) -> Option<String> {
    parts
        .iter()
        .find(|(n, _)| n == name || n.replace('\\', "/") == name)
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
}

pub fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Extract inner text of each `w:p` by concatenating `w:t` nodes.
pub fn docx_paragraphs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(pstart) = find_open(rest, "w:p") {
        let after = &rest[pstart..];
        let Some(pend) = after.find("</w:p>") else {
            break;
        };
        let inner = &after[..pend];
        out.push(concat_local_t(inner, "w:t"));
        rest = &after[pend + 6..];
    }
    out
}

pub fn rewrite_docx_paragraphs(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(pstart) = find_open(rest, "w:p") {
        out.push_str(&rest[..pstart]);
        let after = &rest[pstart..];
        let Some(pend) = after.find("</w:p>") else {
            out.push_str(after);
            return out;
        };
        let para = &after[..pend + 6];
        if idx < new_texts.len() {
            out.push_str(&replace_first_t(para, "w:t", &new_texts[idx]));
        } else {
            out.push_str(para);
        }
        idx += 1;
        rest = &after[pend + 6..];
    }
    out.push_str(rest);
    out
}

pub fn xlsx_shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = find_open(rest, "si") {
        let after = &rest[s..];
        let Some(e) = after.find("</si>") else {
            break;
        };
        out.push(concat_local_t(&after[..e], "t"));
        rest = &after[e + 5..];
    }
    out
}

pub fn rewrite_shared_strings(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(s) = find_open(rest, "si") {
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(e) = after.find("</si>") else {
            out.push_str(after);
            return out;
        };
        let si = &after[..e + 5];
        if idx < new_texts.len() {
            out.push_str(&replace_first_t(si, "t", &new_texts[idx]));
        } else {
            out.push_str(si);
        }
        idx += 1;
        rest = &after[e + 5..];
    }
    out.push_str(rest);
    out
}

pub fn xlsx_inline_and_values(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    // inlineStr
    let mut rest = xml;
    while let Some(s) = rest.find("<is>") {
        let after = &rest[s..];
        let Some(e) = after.find("</is>") else {
            break;
        };
        out.push(concat_local_t(&after[..e], "t"));
        rest = &after[e + 5..];
    }
    // numeric cell values (may be account/card numbers)
    let mut rest = xml;
    while let Some(s) = rest.find("<v>") {
        let after = &rest[s + 3..];
        if let Some(e) = after.find("</v>") {
            out.push(xml_unescape(&after[..e]));
            rest = &after[e + 4..];
        } else {
            break;
        }
    }
    out
}

pub fn rewrite_sheet_values(xml: &str, new_texts: &[String]) -> String {
    let mut rebuilt = xml.to_string();
    // Replace <v> contents in order for the numeric values we extracted after inlineStr.
    // Safer: replace concatenated t in <is> first, then <v>.
    let inline_count = xml.matches("<is>").count();
    let mut idx = 0usize;
    rebuilt = replace_seq_inner(&rebuilt, "<is>", "</is>", "t", new_texts, &mut idx);
    // skip already used inline texts
    let rest = if new_texts.len() > inline_count {
        &new_texts[inline_count..]
    } else {
        &[]
    };
    let mut v_idx = 0usize;
    let mut out = String::new();
    let mut remain = rebuilt.as_str();
    while let Some(s) = remain.find("<v>") {
        out.push_str(&remain[..s + 3]);
        let after = &remain[s + 3..];
        if let Some(e) = after.find("</v>") {
            if v_idx < rest.len() {
                out.push_str(&xml_escape(&rest[v_idx]));
            } else {
                out.push_str(&after[..e]);
            }
            v_idx += 1;
            out.push_str("</v>");
            remain = &after[e + 4..];
        } else {
            out.push_str(after);
            remain = "";
            break;
        }
    }
    out.push_str(remain);
    out
}

fn replace_seq_inner(
    xml: &str,
    open: &str,
    close: &str,
    t_local: &str,
    new_texts: &[String],
    idx: &mut usize,
) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(s) = rest.find(open) {
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(e) = after.find(close) else {
            out.push_str(after);
            return out;
        };
        let block = &after[..e + close.len()];
        if *idx < new_texts.len() {
            out.push_str(&replace_first_t(block, t_local, &new_texts[*idx]));
        } else {
            out.push_str(block);
        }
        *idx += 1;
        rest = &after[e + close.len()..];
    }
    out.push_str(rest);
    out
}

fn concat_local_t(xml: &str, local: &str) -> String {
    let mut s = String::new();
    let mut rest = xml;
    let close = format!("</{local}>");
    while let Some(start) = find_open(rest, local) {
        let after_tag = &rest[start..];
        let Some(gt) = after_tag.find('>') else {
            break;
        };
        if after_tag.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            rest = &after_tag[gt + 1..];
            continue;
        }
        let inner_start = start + gt + 1;
        if let Some(rel) = rest[inner_start..].find(&close) {
            s.push_str(&xml_unescape(&rest[inner_start..inner_start + rel]));
            rest = &rest[inner_start + rel + close.len()..];
        } else {
            break;
        }
    }
    s
}

fn replace_first_t(block: &str, local: &str, new_text: &str) -> String {
    let close = format!("</{local}>");
    let Some(start) = find_open(block, local) else {
        return block.to_string();
    };
    let after_tag = &block[start..];
    let Some(gt) = after_tag.find('>') else {
        return block.to_string();
    };
    if after_tag.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
        return block.to_string();
    }
    let inner_start = start + gt + 1;
    let Some(rel) = block[inner_start..].find(&close) else {
        return block.to_string();
    };
    let inner_end = inner_start + rel;
    let mut out = String::new();
    out.push_str(&block[..inner_start]);
    out.push_str(&xml_escape(new_text));
    out.push_str(&block[inner_end..]);
    // empty remaining t nodes
    let (head, tail) = out.split_at(inner_end + close.len() + (xml_escape(new_text).len() - (inner_end - inner_start)));
    // The split math is fragile; rebuild by emptying later t tags with a second pass.
    let _ = (head, tail);
    empty_later_t(&out, local, inner_start)
}

fn empty_later_t(block: &str, local: &str, keep_inner_start: usize) -> String {
    let close = format!("</{local}>");
    let mut out = String::new();
    let mut rest = block;
    let mut pos = 0usize;
    let mut first = true;
    while let Some(rel) = find_open(rest, local) {
        let abs = pos + rel;
        out.push_str(&block[pos..abs]);
        let after_tag = &block[abs..];
        let Some(gt) = after_tag.find('>') else {
            out.push_str(after_tag);
            return out;
        };
        let inner_start = abs + gt + 1;
        if after_tag.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            out.push_str(&after_tag[..=gt]);
            pos = abs + gt + 1;
            rest = &block[pos..];
            continue;
        }
        let Some(c) = block[inner_start..].find(&close) else {
            out.push_str(&block[abs..]);
            return out;
        };
        let inner_end = inner_start + c;
        out.push_str(&block[abs..inner_start]);
        if first && inner_start == keep_inner_start {
            out.push_str(&block[inner_start..inner_end]);
            first = false;
        } else if first {
            out.push_str(&block[inner_start..inner_end]);
            first = false;
        } else {
            // emptied
        }
        out.push_str(&close);
        pos = inner_end + close.len();
        rest = &block[pos..];
    }
    out.push_str(&block[pos..]);
    out
}

fn find_open(xml: &str, local: &str) -> Option<usize> {
    let a = format!("<{local}");
    let b = format!("<{local} ");
    let c = format!("<{local}>");
    let mut best = None;
    for pat in [&a, &b, &c] {
        if let Some(i) = xml.find(pat.as_str()) {
            let ok = xml[i + pat.len().saturating_sub(0)..]
                .chars()
                .next()
                .map(|ch| ch == ' ' || ch == '>' || ch == '/')
                .unwrap_or(true);
            // `<t` should not match `<tr` or `<tbl`
            let after = i + 1 + local.len();
            let boundary = xml.as_bytes().get(after).copied().unwrap_or(b'>');
            let is_boundary = matches!(boundary, b' ' | b'>' | b'/' | b'\n' | b'\r' | b'\t');
            if is_boundary && ok {
                best = Some(best.map_or(i, |b: usize| b.min(i)));
            }
        }
    }
    // Also prefixed: ignore, we search local with optional prefix already in `local` ("w:t").
    best
}
