use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("규칙 파일 오류: {0}")]
    Rules(String),
    #[error("파싱 오류: {0}")]
    Parse(String),
    #[error("정규식 오류: {0}")]
    Regex(#[from] regex::Error),
}

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
