use crate::checksum::extract_digits;
use crate::types::{Finding, MaskMode};

pub fn apply_mode(raw: &str, mode: MaskMode, partial_kind: &str, replace_token: &str) -> String {
    match mode {
        MaskMode::Full => mask_full(raw),
        MaskMode::Partial => mask_partial(raw, partial_kind),
        MaskMode::Replace => replace_token.to_string(),
        MaskMode::Delete => String::new(),
    }
}

/// Replace alphanumeric with `*`, keep separators.
pub fn mask_full(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                '*'
            } else {
                c
            }
        })
        .collect()
}

pub fn mask_partial(raw: &str, kind: &str) -> String {
    match kind {
        "rrn" | "corp" => mask_keep_prefix_digits(raw, 7),
        "card" => mask_card(raw),
        "phone" => mask_phone(raw),
        "email" => mask_email(raw),
        "biz" => mask_keep_prefix_digits(raw, 3),
        "account" => mask_keep_suffix_digits(raw, 4),
        "driver" => mask_driver(raw),
        "passport" => mask_passport(raw),
        "health" => mask_keep_prefix_digits(raw, 6),
        "ip" => mask_ip(raw),
        _ => mask_full(raw),
    }
}

fn mask_keep_prefix_digits(raw: &str, keep: usize) -> String {
    let mut seen = 0usize;
    raw.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                if seen < keep {
                    seen += 1;
                    c
                } else {
                    seen += 1;
                    '*'
                }
            } else if c.is_ascii_alphabetic() {
                '*'
            } else {
                c
            }
        })
        .collect()
}

fn mask_keep_suffix_digits(raw: &str, keep: usize) -> String {
    let total = extract_digits(raw).len();
    let skip = total.saturating_sub(keep);
    let mut seen = 0usize;
    raw.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                let out = if seen < skip { '*' } else { c };
                seen += 1;
                out
            } else if c.is_ascii_alphabetic() {
                '*'
            } else {
                c
            }
        })
        .collect()
}

fn mask_card(raw: &str) -> String {
    let total = extract_digits(raw).len();
    let mut seen = 0usize;
    raw.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                let keep = seen < 4 || seen >= total.saturating_sub(4);
                seen += 1;
                if keep {
                    c
                } else {
                    '*'
                }
            } else {
                c
            }
        })
        .collect()
}

fn mask_phone(raw: &str) -> String {
    let total = extract_digits(raw).len();
    let mut seen = 0usize;
    raw.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                let keep = seen < 3 || seen >= total.saturating_sub(4);
                seen += 1;
                if keep {
                    c
                } else {
                    '*'
                }
            } else {
                c
            }
        })
        .collect()
}

fn mask_email(raw: &str) -> String {
    let Some((local, domain)) = raw.split_once('@') else {
        return mask_full(raw);
    };
    let mut masked_local = String::new();
    for (i, c) in local.chars().enumerate() {
        if i == 0 {
            masked_local.push(c);
        } else if c == '.' || c == '_' || c == '-' {
            masked_local.push(c);
        } else {
            masked_local.push('*');
        }
    }
    format!("{masked_local}@{domain}")
}

fn mask_driver(raw: &str) -> String {
    let mut out = String::new();
    let mut seen_digit = false;
    for c in raw.chars() {
        if c.is_ascii_digit() {
            seen_digit = true;
            out.push('*');
        } else if c.is_alphabetic() && seen_digit {
            out.push('*');
        } else {
            out.push(c);
        }
    }
    out
}

fn mask_passport(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_digit() { '*' } else { c })
        .collect()
}

fn mask_ip(raw: &str) -> String {
    if let Some(idx) = raw.rfind('.') {
        format!("{}.*", &raw[..idx])
    } else if let Some(idx) = raw.rfind(':') {
        format!("{}:*", &raw[..idx])
    } else {
        mask_full(raw)
    }
}

/// Apply findings (must be for this `text`) from the end so offsets stay valid.
pub fn apply_findings(text: &str, findings: &[Finding], mode: MaskMode, rules: &crate::rules::RuleSet) -> String {
    let mut buf = text.to_string();
    let mut items: Vec<&Finding> = findings.iter().collect();
    items.sort_by_key(|f| std::cmp::Reverse(f.byte_start));
    for f in items {
        if f.confidence == crate::types::Confidence::AlreadyMasked {
            continue;
        }
        if f.byte_end > buf.len() || f.byte_start > f.byte_end {
            continue;
        }
        let partial = rules
            .rules
            .iter()
            .find(|r| r.id == f.rule_id)
            .map(|r| r.partial.as_str())
            .unwrap_or("");
        let token = rules
            .rules
            .iter()
            .find(|r| r.id == f.rule_id)
            .map(|r| r.replace_token.as_str())
            .unwrap_or("[PII]");
        let repl = apply_mode(&f.raw, mode, partial, token);
        buf.replace_range(f.byte_start..f.byte_end, &repl);
    }
    buf
}
