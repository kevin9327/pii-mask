use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{Error, Result};
use crate::types::{Extracted, FileFormat, ParaLoc, Paragraph};

pub fn extract_docx(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let parts = read_zip(bytes)?;
    let mut paragraphs = Vec::new();
    let mut names: Vec<String> = parts
        .iter()
        .map(|(n, _)| n.replace('\\', "/"))
        .filter(|n| is_docx_text_part(n))
        .collect();
    names.sort();
    // Body first so table order stays predictable, then headers/footers/notes.
    names.sort_by_key(|n| if n == "word/document.xml" { 0 } else { 1 });
    for name in names {
        let Some((_, data)) = parts.iter().find(|(n, _)| n.replace('\\', "/") == name) else {
            continue;
        };
        let xml = String::from_utf8_lossy(data);
        let texts = if name.starts_with("customXml/") {
            xml_text_nodes(&xml)
        } else {
            docx_paragraphs(&xml)
        };
        for (i, t) in texts.into_iter().enumerate() {
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
    push_core_prop_paragraphs(&parts, &mut paragraphs);
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

const CORE_PROP_TAGS: &[&str] = &[
    "dc:title",
    "dc:subject",
    "dc:description",
    "dc:creator",
    "cp:lastModifiedBy",
    "cp:keywords",
    "dc:identifier",
    "Application",
    "Company",
];

fn is_core_props_part(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n == "docProps/core.xml" || n == "docProps/app.xml" || n == "docProps/custom.xml"
}

fn push_core_prop_paragraphs(parts: &[(String, Vec<u8>)], paragraphs: &mut Vec<Paragraph>) {
    for (name, data) in parts {
        if !is_core_props_part(name) {
            continue;
        }
        let xml = String::from_utf8_lossy(data);
        for (i, t) in core_prop_texts(&xml).into_iter().enumerate() {
            paragraphs.push(Paragraph {
                index: paragraphs.len(),
                text: t,
                full_byte_start: 0,
                loc: ParaLoc::ZipXml {
                    inner_path: name.replace('\\', "/"),
                    para_ord: i,
                },
            });
        }
    }
}

pub fn xml_text_nodes(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(gt) = rest.find('>') {
        let after = &rest[gt + 1..];
        if after.starts_with('<') {
            rest = after;
            continue;
        }
        if let Some(lt) = after.find('<') {
            let inner = &after[..lt];
            if !inner.trim().is_empty() {
                out.push(xml_unescape(inner));
            }
            rest = &after[lt..];
        } else {
            break;
        }
    }
    out
}

pub fn rewrite_xml_text_nodes(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(gt) = rest.find('>') {
        out.push_str(&rest[..=gt]);
        let after = &rest[gt + 1..];
        if after.starts_with('<') {
            rest = after;
            continue;
        }
        if let Some(lt) = after.find('<') {
            let inner = &after[..lt];
            if inner.trim().is_empty() {
                out.push_str(inner);
            } else if idx < new_texts.len() {
                out.push_str(&xml_escape(&new_texts[idx]));
                idx += 1;
            } else {
                out.push_str(inner);
            }
            rest = &after[lt..];
        } else {
            out.push_str(after);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

pub fn core_prop_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in CORE_PROP_TAGS {
        if let Some(inner) = xml_tag_inner(xml, tag) {
            if !inner.trim().is_empty() {
                out.push(xml_unescape(&inner));
            }
        }
    }
    out
}

pub fn rewrite_core_props(xml: &str, new_texts: &[String]) -> String {
    let mut out = xml.to_string();
    let mut idx = 0usize;
    for tag in CORE_PROP_TAGS {
        if xml_tag_inner(&out, tag).map(|s| s.trim().is_empty()).unwrap_or(true) {
            continue;
        }
        if idx >= new_texts.len() {
            break;
        }
        out = replace_tag_inner(&out, tag, &new_texts[idx]);
        idx += 1;
    }
    out
}

fn is_docx_text_part(name: &str) -> bool {
    let n = name.replace('\\', "/");
    if n.contains("/_rels/") || n.contains("itemProps") {
        return false;
    }
    (n.starts_with("word/") && n.ends_with(".xml"))
        || (n.starts_with("customXml/") && n.ends_with(".xml"))
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
        let key = name.replace('\\', "/");
        if key.starts_with("xl/worksheets/") && key.ends_with(".xml") {
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
            for (i, t) in xlsx_header_footer_texts(&xml).into_iter().enumerate() {
                paragraphs.push(Paragraph {
                    index: paragraphs.len(),
                    text: t,
                    full_byte_start: 0,
                    loc: ParaLoc::ZipXml {
                        inner_path: name.clone(),
                        para_ord: 10_000 + i,
                    },
                });
            }
        }
        if key == "xl/workbook.xml" || key.ends_with("/workbook.xml") && key.contains("xl/") {
            let xml = String::from_utf8_lossy(data).into_owned();
            for (i, t) in xlsx_defined_name_texts(&xml).into_iter().enumerate() {
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
        if key.starts_with("xl/comments") && key.ends_with(".xml") {
            let xml = String::from_utf8_lossy(data).into_owned();
            for (i, t) in xlsx_comment_texts(&xml).into_iter().enumerate() {
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
        if key.starts_with("xl/pivotCache/") && key.ends_with(".xml") && !key.contains("_rels") {
            let xml = String::from_utf8_lossy(data).into_owned();
            for (i, t) in pivot_cache_strings(&xml).into_iter().enumerate() {
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
        if is_drawingml_text_part(&key) {
            let xml = String::from_utf8_lossy(data).into_owned();
            for (i, t) in drawingml_paragraphs(&xml).into_iter().enumerate() {
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
    push_core_prop_paragraphs(&parts, &mut paragraphs);
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

pub fn extract_pptx(filename: &str, bytes: &[u8]) -> Result<Extracted> {
    let parts = read_zip(bytes)?;
    let mut paragraphs = Vec::new();
    let mut names: Vec<String> = parts
        .iter()
        .map(|(n, _)| n.replace('\\', "/"))
        .filter(|n| is_pptx_text_part(n))
        .collect();
    names.sort();
    names.sort_by_key(|n| {
        if n.starts_with("ppt/slides/") {
            0
        } else if n.starts_with("ppt/notesSlides/") {
            1
        } else {
            2
        }
    });
    for name in names {
        let Some((_, data)) = parts.iter().find(|(n, _)| n.replace('\\', "/") == name) else {
            continue;
        };
        let xml = String::from_utf8_lossy(data);
        let texts = if name.starts_with("ppt/comments") {
            xml_text_nodes(&xml)
        } else {
            drawingml_paragraphs(&xml)
        };
        for (i, t) in texts.into_iter().enumerate() {
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
    push_core_prop_paragraphs(&parts, &mut paragraphs);
    Ok(super::finish(
        filename,
        FileFormat::Pptx,
        bytes.to_vec(),
        paragraphs,
        Vec::new(),
        None,
        Some(parts),
    ))
}

pub fn is_drawingml_text_part(name: &str) -> bool {
    let n = name.replace('\\', "/");
    if n.contains("/_rels/") {
        return false;
    }
    n.ends_with(".xml")
        && (n.starts_with("ppt/slides/")
            || n.starts_with("ppt/notesSlides/")
            || n.starts_with("xl/drawings/")
            || n.starts_with("xl/charts/"))
}

pub fn extract_odf(filename: &str, bytes: &[u8], format: FileFormat) -> Result<Extracted> {
    let parts = read_zip(bytes)?;
    let mut paragraphs = Vec::new();
    let mut names: Vec<String> = parts
        .iter()
        .map(|(n, _)| n.replace('\\', "/"))
        .filter(|n| is_odf_text_part(n))
        .collect();
    names.sort();
    names.sort_by_key(|n| if n == "content.xml" { 0 } else { 1 });
    for name in names {
        let Some((_, data)) = parts.iter().find(|(n, _)| n.replace('\\', "/") == name) else {
            continue;
        };
        let xml = String::from_utf8_lossy(data);
        for (i, t) in xml_text_nodes(&xml).into_iter().enumerate() {
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
    Ok(super::finish(
        filename,
        format,
        bytes.to_vec(),
        paragraphs,
        Vec::new(),
        None,
        Some(parts),
    ))
}

fn is_odf_text_part(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n == "content.xml" || n == "meta.xml"
}

fn is_pptx_text_part(name: &str) -> bool {
    let n = name.replace('\\', "/");
    if n.contains("/_rels/") {
        return false;
    }
    n.ends_with(".xml")
        && (n.starts_with("ppt/slides/")
            || n.starts_with("ppt/notesSlides/")
            || n.starts_with("ppt/comments"))
}

/// DrawingML `a:p` runs (`a:t`), used by PPTX slides/notes and XLSX text boxes.
pub fn drawingml_paragraphs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(pstart) = find_open(rest, "a:p") {
        let after = &rest[pstart..];
        let Some(pend) = after.find("</a:p>") else {
            break;
        };
        out.push(concat_local_t(&after[..pend], "a:t"));
        rest = &after[pend + 6..];
    }
    out
}

pub fn pivot_cache_strings(xml: &str) -> Vec<String> {
    xml_tagged_attr_values(xml, "s", "v")
}

pub fn rewrite_pivot_cache_strings(xml: &str, new_texts: &[String]) -> String {
    rewrite_tagged_attr_values(xml, "s", "v", new_texts)
}

fn xml_tagged_attr_values(xml: &str, tag: &str, attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = find_open(rest, tag) {
        let after = &rest[s..];
        let Some(gt) = after.find('>') else {
            break;
        };
        if let Some(v) = attr_value(&after[..=gt], attr) {
            if !v.trim().is_empty() {
                out.push(xml_unescape(&v));
            }
        }
        rest = &after[gt + 1..];
    }
    out
}

fn rewrite_tagged_attr_values(xml: &str, tag: &str, attr: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(s) = find_open(rest, tag) {
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let open = &after[..=gt];
        if idx < new_texts.len() && attr_value(open, attr).map(|v| !v.trim().is_empty()).unwrap_or(false)
        {
            out.push_str(&replace_attr(open, attr, &new_texts[idx]));
            idx += 1;
        } else {
            out.push_str(open);
        }
        rest = &after[gt + 1..];
    }
    out.push_str(rest);
    out
}

fn attr_value(open: &str, attr: &str) -> Option<String> {
    for q in ['"', '\''] {
        let needle = format!("{attr}={q}");
        if let Some(i) = open.find(&needle) {
            let rest = &open[i + needle.len()..];
            if let Some(e) = rest.find(q) {
                return Some(rest[..e].to_string());
            }
        }
    }
    None
}

fn replace_attr(open: &str, attr: &str, new_text: &str) -> String {
    for q in ['"', '\''] {
        let needle = format!("{attr}={q}");
        if let Some(i) = open.find(&needle) {
            let rest = &open[i + needle.len()..];
            if let Some(e) = rest.find(q) {
                let mut out = String::new();
                out.push_str(&open[..i]);
                out.push_str(&needle);
                out.push_str(&xml_escape(new_text));
                out.push(q);
                out.push_str(&rest[e + 1..]);
                return out;
            }
        }
    }
    open.to_string()
}

pub fn rewrite_drawingml_paragraphs(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(pstart) = find_open(rest, "a:p") {
        out.push_str(&rest[..pstart]);
        let after = &rest[pstart..];
        let Some(pend) = after.find("</a:p>") else {
            out.push_str(after);
            return out;
        };
        let para = &after[..pend + 6];
        if idx < new_texts.len() {
            out.push_str(&replace_first_t(para, "a:t", &new_texts[idx]));
        } else {
            out.push_str(para);
        }
        idx += 1;
        rest = &after[pend + 6..];
    }
    out.push_str(rest);
    out
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

const XLSX_HF_TAGS: &[&str] = &[
    "oddHeader",
    "oddFooter",
    "evenHeader",
    "evenFooter",
    "firstHeader",
    "firstFooter",
];

pub fn xlsx_header_footer_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in XLSX_HF_TAGS {
        if let Some(inner) = xml_tag_inner(xml, tag) {
            if !inner.is_empty() {
                out.push(xml_unescape(&inner));
            }
        }
    }
    out
}

pub fn rewrite_xlsx_header_footers(xml: &str, new_texts: &[String]) -> String {
    let mut out = xml.to_string();
    let mut idx = 0usize;
    for tag in XLSX_HF_TAGS {
        if xml_tag_inner(&out, tag).is_none() {
            continue;
        }
        if idx >= new_texts.len() {
            break;
        }
        out = replace_tag_inner(&out, tag, &new_texts[idx]);
        idx += 1;
    }
    out
}

fn xml_tag_inner(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let s = find_open(xml, tag)?;
    let after = &xml[s..];
    let gt = after.find('>')?;
    if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
        return None;
    }
    let inner_start = s + gt + 1;
    let e = xml[inner_start..].find(&close)?;
    let _ = open;
    Some(xml[inner_start..inner_start + e].to_string())
}

fn replace_tag_inner(xml: &str, tag: &str, new_text: &str) -> String {
    let close = format!("</{tag}>");
    let Some(s) = find_open(xml, tag) else {
        return xml.to_string();
    };
    let after = &xml[s..];
    let Some(gt) = after.find('>') else {
        return xml.to_string();
    };
    if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
        return xml.to_string();
    }
    let inner_start = s + gt + 1;
    let Some(e) = xml[inner_start..].find(&close) else {
        return xml.to_string();
    };
    let mut out = String::new();
    out.push_str(&xml[..inner_start]);
    out.push_str(&xml_escape(new_text));
    out.push_str(&xml[inner_start + e..]);
    out
}

pub fn xlsx_defined_name_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = find_open(rest, "definedName") {
        let after = &rest[s..];
        let Some(gt) = after.find('>') else {
            break;
        };
        if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            rest = &after[gt + 1..];
            continue;
        }
        let inner_start = gt + 1;
        let Some(e) = after[inner_start..].find("</definedName>") else {
            break;
        };
        let inner = xml_unescape(&after[inner_start..inner_start + e]);
        if !inner.trim().is_empty() {
            out.push(inner);
        }
        rest = &after[inner_start + e + 14..];
    }
    out
}

pub fn rewrite_xlsx_defined_names(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(s) = find_open(rest, "definedName") {
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            out.push_str(&after[..=gt]);
            rest = &after[gt + 1..];
            continue;
        }
        let inner_start = gt + 1;
        let Some(e) = after[inner_start..].find("</definedName>") else {
            out.push_str(after);
            return out;
        };
        out.push_str(&after[..inner_start]);
        if idx < new_texts.len() {
            out.push_str(&xml_escape(&new_texts[idx]));
            idx += 1;
        } else {
            out.push_str(&after[inner_start..inner_start + e]);
        }
        out.push_str("</definedName>");
        rest = &after[inner_start + e + 14..];
    }
    out.push_str(rest);
    out
}

pub fn xlsx_comment_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find("<comment") {
        let after = &rest[s..];
        let Some(e) = after.find("</comment>") else {
            break;
        };
        out.push(concat_local_t(&after[..e], "t"));
        rest = &after[e + 10..];
    }
    out
}

pub fn rewrite_xlsx_comments(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    while let Some(s) = rest.find("<comment") {
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(e) = after.find("</comment>") else {
            out.push_str(after);
            return out;
        };
        let block = &after[..e + 10];
        if idx < new_texts.len() {
            out.push_str(&replace_first_t(block, "t", &new_texts[idx]));
        } else {
            out.push_str(block);
        }
        idx += 1;
        rest = &after[e + 10..];
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
    let mut rest = xml;
    loop {
        let a = rest.find("<c ");
        let b = rest.find("<c>");
        let start = match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            _ => None,
        };
        let Some(s) = start else {
            break;
        };
        let after = &rest[s..];
        let Some(gt) = after.find('>') else {
            break;
        };
        let open = &after[..=gt];
        let shared = open.contains("t=\"s\"") || open.contains("t='s'");
        let Some(end_rel) = after.find("</c>") else {
            break;
        };
        let cell = &after[..end_rel];
        if !shared {
            if let Some(is) = cell.find("<is>") {
                if let Some(ie) = cell[is..].find("</is>") {
                    out.push(concat_local_t(&cell[is..is + ie], "t"));
                }
            } else if let Some(vs) = cell.find("<v>") {
                let inner = &cell[vs + 3..];
                if let Some(ve) = inner.find("</v>") {
                    out.push(xml_unescape(&inner[..ve]));
                }
            }
        }
        rest = &after[end_rel + 4..];
    }
    out
}

pub fn rewrite_sheet_values(xml: &str, new_texts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let mut idx = 0usize;
    loop {
        let a = rest.find("<c ");
        let b = rest.find("<c>");
        let start = match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            _ => None,
        };
        let Some(s) = start else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..s]);
        let after = &rest[s..];
        let Some(end_rel) = after.find("</c>") else {
            out.push_str(after);
            break;
        };
        let cell = &after[..end_rel + 4];
        let gt = after.find('>').unwrap_or(0);
        let open = &after[..=gt];
        let shared = open.contains("t=\"s\"") || open.contains("t='s'");
        if !shared && idx < new_texts.len() {
            if cell.contains("<is>") {
                out.push_str(&replace_first_t(cell, "t", &new_texts[idx]));
                idx += 1;
            } else if cell.contains("<v>") {
                out.push_str(&replace_v(cell, &new_texts[idx]));
                idx += 1;
            } else {
                out.push_str(cell);
            }
        } else {
            out.push_str(cell);
        }
        rest = &after[end_rel + 4..];
    }
    out
}

fn replace_v(cell: &str, new_text: &str) -> String {
    let Some(vs) = cell.find("<v>") else {
        return cell.to_string();
    };
    let after = &cell[vs + 3..];
    let Some(ve) = after.find("</v>") else {
        return cell.to_string();
    };
    let mut out = String::new();
    out.push_str(&cell[..vs + 3]);
    out.push_str(&xml_escape(new_text));
    out.push_str(&cell[vs + 3 + ve..]);
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
    let needle = format!("<{local}");
    let mut start = 0usize;
    while let Some(rel) = xml[start..].find(&needle) {
        let i = start + rel;
        let after = i + needle.len();
        let boundary = xml.as_bytes().get(after).copied().unwrap_or(b'>');
        if matches!(boundary, b' ' | b'>' | b'/' | b'\n' | b'\r' | b'\t') {
            return Some(i);
        }
        start = i + 1;
    }
    None
}
