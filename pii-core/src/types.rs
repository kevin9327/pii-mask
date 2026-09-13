use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Hwp,
    Hwpx,
    Hwp3,
    Txt,
    Csv,
    Json,
    Pdf,
    Docx,
    Xlsx,
    Unknown,
}

impl FileFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            FileFormat::Hwp => "hwp",
            FileFormat::Hwpx => "hwpx",
            FileFormat::Hwp3 => "hwp3",
            FileFormat::Txt => "txt",
            FileFormat::Csv => "csv",
            FileFormat::Json => "json",
            FileFormat::Pdf => "pdf",
            FileFormat::Docx => "docx",
            FileFormat::Xlsx => "xlsx",
            FileFormat::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskMode {
    Full,
    Partial,
    Replace,
    Delete,
}

impl MaskMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "partial" | "부분" => MaskMode::Partial,
            "replace" | "치환" => MaskMode::Replace,
            "delete" | "삭제" => MaskMode::Delete,
            _ => MaskMode::Full,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            MaskMode::Full => "full",
            MaskMode::Partial => "partial",
            MaskMode::Replace => "replace",
            MaskMode::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Confirmed,
    Suspicious,
    AlreadyMasked,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Confirmed => "confirmed",
            Confidence::Suspicious => "suspicious",
            Confidence::AlreadyMasked => "already_masked",
        }
    }

    pub fn ko(self) -> &'static str {
        match self {
            Confidence::Confirmed => "확정",
            Confidence::Suspicious => "의심",
            Confidence::AlreadyMasked => "이미 마스킹됨",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub label: String,
    pub raw: String,
    pub masked_preview: String,
    pub confidence: Confidence,
    pub paragraph_index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub context_before: String,
    pub context_after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSummary {
    pub rule_id: String,
    pub label: String,
    pub count: usize,
    pub confirmed: usize,
    pub suspicious: usize,
    pub already_masked: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub paragraph_index: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReport {
    pub filename: String,
    pub format: FileFormat,
    pub warnings: Vec<String>,
    pub paragraph_count: usize,
    pub findings: Vec<Finding>,
    pub summaries: Vec<TypeSummary>,
    pub diffs: Vec<DiffHunk>,
    pub confirmed: usize,
    pub suspicious: usize,
    pub already_masked: usize,
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub index: usize,
    pub text: String,
    /// Byte offset of this paragraph in the concatenated `full_text`.
    pub full_byte_start: usize,
    pub loc: ParaLoc,
}

#[derive(Debug, Clone)]
pub enum ParaLoc {
    Sequential {
        byte_start: usize,
        byte_end: usize,
    },
    Hwp {
        section: usize,
        region: HwpRegion,
        segs: Vec<PathSeg>,
    },
    ZipXml {
        inner_path: String,
        /// Byte range of the concatenated paragraph text reconstructed into XML.
        para_ord: usize,
    },
    Pdf {
        page: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwpRegion {
    Body,
    Header,
    EvenHeader,
    FirstHeader,
    Footer,
    EvenFooter,
    FirstFooter,
    Footnote(usize),
    Endnote(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSeg {
    Block(usize),
    Table { row: usize, cell: usize },
}

#[derive(Debug, Clone)]
pub struct Extracted {
    pub format: FileFormat,
    pub filename: String,
    pub paragraphs: Vec<Paragraph>,
    pub full_text: String,
    pub warnings: Vec<String>,
    pub original: Vec<u8>,
    pub hwp_doc: Option<docagent_model::Document>,
    pub zip_parts: Option<Vec<(String, Vec<u8>)>>,
}
