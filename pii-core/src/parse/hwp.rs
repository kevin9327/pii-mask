use docagent_model::{Block, Document, HeaderFooter, Paragraph as HwpPara, RunContent, Section};

use crate::error::{Error, Result};
use crate::types::{
    Extracted, FileFormat, HwpRegion, ParaLoc, Paragraph, PathSeg,
};

pub fn extract(filename: &str, bytes: &[u8], format: FileFormat) -> Result<Extracted> {
    let doc = read_doc(bytes, format)?;
    let mut paragraphs = Vec::new();
    for (si, section) in doc.sections.iter().enumerate() {
        collect_blocks(
            &section.body,
            si,
            HwpRegion::Body,
            &mut Vec::new(),
            &mut paragraphs,
        );
        collect_hf(&section.header, si, HwpRegion::Header, &mut paragraphs);
        collect_hf(&section.even_header, si, HwpRegion::EvenHeader, &mut paragraphs);
        collect_hf(&section.first_header, si, HwpRegion::FirstHeader, &mut paragraphs);
        collect_hf(&section.footer, si, HwpRegion::Footer, &mut paragraphs);
        collect_hf(&section.even_footer, si, HwpRegion::EvenFooter, &mut paragraphs);
        collect_hf(&section.first_footer, si, HwpRegion::FirstFooter, &mut paragraphs);
        for (i, note) in section.footnotes.iter().enumerate() {
            collect_blocks(
                &note.blocks,
                si,
                HwpRegion::Footnote(i),
                &mut Vec::new(),
                &mut paragraphs,
            );
        }
        for (i, note) in section.endnotes.iter().enumerate() {
            collect_blocks(
                &note.blocks,
                si,
                HwpRegion::Endnote(i),
                &mut Vec::new(),
                &mut paragraphs,
            );
        }
    }
    if format == FileFormat::Hwp {
        if let Some(preview) = read_prvtext(bytes) {
            if !preview.trim().is_empty() {
                paragraphs.push(Paragraph {
                    index: paragraphs.len(),
                    text: preview,
                    full_byte_start: 0,
                    loc: ParaLoc::Hwp {
                        section: 0,
                        region: HwpRegion::Body,
                        segs: Vec::new(),
                    },
                });
            }
        }
    }
    let warnings = doc.diagnostics.iter().map(|d| d.message.clone()).collect();
    Ok(super::finish(
        filename,
        format,
        bytes.to_vec(),
        paragraphs,
        warnings,
        Some(doc),
        None,
    ))
}

pub fn read_doc(bytes: &[u8], format: FileFormat) -> Result<Document> {
    match format {
        FileFormat::Hwp => {
            docagent_hwp5::read(bytes).map_err(|e| Error::Parse(e.to_string()))
        }
        FileFormat::Hwpx => {
            docagent_hwpx::read(bytes).map_err(|e| Error::Parse(e.to_string()))
        }
        FileFormat::Hwp3 => {
            docagent_hwp3::read(bytes).map_err(|e| Error::Parse(e.to_string()))
        }
        _ => Err(Error::Parse("HWP 형식이 아닙니다".into())),
    }
}

fn read_prvtext(bytes: &[u8]) -> Option<String> {
    use std::io::{Cursor, Read};
    let mut comp = cfb::CompoundFile::open(Cursor::new(bytes.to_vec())).ok()?;
    for name in ["PrvText", "/PrvText", "\\PrvText"] {
        if !comp.exists(name) {
            continue;
        }
        let mut stream = comp.open_stream(name).ok()?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).ok()?;
        if buf.len() < 2 {
            continue;
        }
        let u16s: Vec<u16> = buf
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        let s = String::from_utf16_lossy(&u16s);
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    None
}

pub fn write_doc(doc: &Document, format: FileFormat) -> Result<Vec<u8>> {
    match format {
        FileFormat::Hwp => docagent_hwp5::write(doc).map_err(|e| Error::Parse(e.to_string())),
        FileFormat::Hwpx => docagent_hwpx::write(doc).map_err(|e| Error::Parse(e.to_string())),
        FileFormat::Hwp3 => docagent_hwp3::write(doc).map_err(|e| Error::Parse(e.to_string())),
        _ => Err(Error::Parse("HWP 쓰기를 지원하지 않습니다".into())),
    }
}

fn collect_hf(
    hf: &Option<HeaderFooter>,
    section: usize,
    region: HwpRegion,
    out: &mut Vec<Paragraph>,
) {
    if let Some(h) = hf {
        collect_blocks(&h.blocks, section, region, &mut Vec::new(), out);
    }
}

fn collect_blocks(
    blocks: &[Block],
    section: usize,
    region: HwpRegion,
    prefix: &mut Vec<PathSeg>,
    out: &mut Vec<Paragraph>,
) {
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(p) => {
                prefix.push(PathSeg::Block(i));
                out.push(Paragraph {
                    index: out.len(),
                    text: p.plain_text(),
                    full_byte_start: 0,
                    loc: ParaLoc::Hwp {
                        section,
                        region,
                        segs: prefix.clone(),
                    },
                });
                prefix.pop();
            }
            Block::Table(t) => {
                prefix.push(PathSeg::Block(i));
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, cell) in row.cells.iter().enumerate() {
                        prefix.push(PathSeg::Table {
                            row: ri,
                            cell: ci,
                        });
                        collect_blocks(&cell.blocks, section, region, prefix, out);
                        prefix.pop();
                    }
                }
                prefix.pop();
            }
            Block::Float(_) | Block::Break(_) => {}
        }
    }
}

pub fn paragraph_at_mut<'a>(doc: &'a mut Document, loc: &ParaLoc) -> Option<&'a mut HwpPara> {
    let ParaLoc::Hwp {
        section,
        region,
        segs,
    } = loc
    else {
        return None;
    };
    let section = doc.sections.get_mut(*section)?;
    let blocks = region_blocks_mut(section, *region)?;
    resolve_para(blocks, segs)
}

fn region_blocks_mut(section: &mut Section, region: HwpRegion) -> Option<&mut Vec<Block>> {
    match region {
        HwpRegion::Body => Some(&mut section.body),
        HwpRegion::Header => section.header.as_mut().map(|h| &mut h.blocks),
        HwpRegion::EvenHeader => section.even_header.as_mut().map(|h| &mut h.blocks),
        HwpRegion::FirstHeader => section.first_header.as_mut().map(|h| &mut h.blocks),
        HwpRegion::Footer => section.footer.as_mut().map(|h| &mut h.blocks),
        HwpRegion::EvenFooter => section.even_footer.as_mut().map(|h| &mut h.blocks),
        HwpRegion::FirstFooter => section.first_footer.as_mut().map(|h| &mut h.blocks),
        HwpRegion::Footnote(i) => section.footnotes.get_mut(i).map(|n| &mut n.blocks),
        HwpRegion::Endnote(i) => section.endnotes.get_mut(i).map(|n| &mut n.blocks),
    }
}

fn resolve_para<'a>(blocks: &'a mut [Block], segs: &[PathSeg]) -> Option<&'a mut HwpPara> {
    if segs.is_empty() {
        return None;
    }
    match segs[0] {
        PathSeg::Block(i) => {
            let block = blocks.get_mut(i)?;
            match block {
                Block::Paragraph(p) if segs.len() == 1 => Some(p),
                Block::Table(t) => {
                    if segs.len() < 2 {
                        return None;
                    }
                    let PathSeg::Table { row, cell } = segs[1] else {
                        return None;
                    };
                    let cell = t.rows.get_mut(row)?.cells.get_mut(cell)?;
                    resolve_para(&mut cell.blocks, &segs[2..])
                }
                _ => None,
            }
        }
        PathSeg::Table { .. } => None,
    }
}

pub fn set_paragraph_text(para: &mut HwpPara, text: String) {
    let style = para
        .runs
        .first()
        .map(|r| r.style.clone())
        .unwrap_or_default();
    para.runs = vec![docagent_model::Run {
        style,
        content: RunContent::Text(text),
    }];
}
