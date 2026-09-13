use regex::Regex;
use serde::Deserialize;

use crate::error::{Error, Result};

pub const DEFAULT_RULES_TOML: &str = include_str!("../rules.toml");

#[derive(Debug, Clone)]
pub struct RuleSet {
    pub rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: String,
    pub label: String,
    pub priority: i32,
    pub validator: String,
    pub replace_token: String,
    pub partial: String,
    pub regexes: Vec<Regex>,
}

#[derive(Debug, Deserialize)]
struct RulesFile {
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    label: String,
    #[serde(default = "default_priority")]
    priority: i32,
    #[serde(default = "default_validator")]
    validator: String,
    #[serde(default)]
    replace_token: String,
    #[serde(default)]
    partial: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    patterns: Option<Vec<String>>,
}

fn default_priority() -> i32 {
    0
}
fn default_validator() -> String {
    "none".into()
}

impl RuleSet {
    pub fn builtin() -> Result<Self> {
        Self::from_toml(DEFAULT_RULES_TOML)
    }

    pub fn from_toml(toml_src: &str) -> Result<Self> {
        let parsed: RulesFile =
            toml::from_str(toml_src).map_err(|e| Error::Rules(e.to_string()))?;
        Self::compile(parsed.rules)
    }

    /// Extra `[[rules]]` overlay: same `id` replaces, new ids append.
    pub fn with_extra(&self, extra_toml: &str) -> Result<Self> {
        if extra_toml.trim().is_empty() {
            return Ok(self.clone());
        }
        let extra = Self::from_toml(extra_toml)?;
        let mut rules = self.rules.clone();
        for er in extra.rules {
            if let Some(existing) = rules.iter_mut().find(|r| r.id == er.id) {
                *existing = er;
            } else {
                rules.push(er);
            }
        }
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(Self { rules })
    }

    fn compile(raw: Vec<RawRule>) -> Result<Self> {
        let mut rules = Vec::new();
        for r in raw {
            let mut pats = Vec::new();
            if let Some(p) = r.pattern {
                pats.push(p);
            }
            if let Some(ps) = r.patterns {
                pats.extend(ps);
            }
            if pats.is_empty() {
                return Err(Error::Rules(format!("규칙 '{}' 에 pattern 이 없습니다", r.id)));
            }
            let mut regexes = Vec::new();
            for p in pats {
                regexes.push(Regex::new(&p)?);
            }
            let replace_token = if r.replace_token.is_empty() {
                format!("[{}]", r.label)
            } else {
                r.replace_token
            };
            rules.push(CompiledRule {
                id: r.id,
                label: r.label,
                priority: r.priority,
                validator: r.validator,
                replace_token,
                partial: r.partial,
                regexes,
            });
        }
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(Self { rules })
    }
}
