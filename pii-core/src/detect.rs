use crate::checksum::{
    biz_checksum_ok, corp_checksum_ok, extract_digit_stars, extract_digits, has_mask_star, luhn_ok,
    rrn_checksum_ok, rrn_date_ok,
};
use crate::mask::{mask_full, mask_partial};
use crate::rules::{CompiledRule, RuleSet};
use crate::types::{Confidence, Finding};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Skip,
    Confirmed,
    Suspicious,
    AlreadyMasked,
}

pub fn detect_text(text: &str, rules: &RuleSet, paragraph_index: usize) -> Vec<Finding> {
    let mut taken = vec![false; text.len()];
    let mut out = Vec::new();

    for rule in &rules.rules {
        let mut spans: Vec<(usize, usize, String)> = Vec::new();
        for re in &rule.regexes {
            for m in re.find_iter(text) {
                spans.push((m.start(), m.end(), m.as_str().to_string()));
            }
        }
        spans.sort_by_key(|(s, _, _)| *s);
        for (start, end, raw) in spans {
            if start >= end || end > text.len() {
                continue;
            }
            if !left_boundary(text, start) || !right_boundary(text, end) {
                continue;
            }
            if taken[start..end].iter().any(|t| *t) {
                continue;
            }
            match validate(rule, &raw) {
                Verdict::Skip => continue,
                v => {
                    for b in start..end {
                        taken[b] = true;
                    }
                    let confidence = match v {
                        Verdict::Confirmed => Confidence::Confirmed,
                        Verdict::Suspicious => Confidence::Suspicious,
                        Verdict::AlreadyMasked => Confidence::AlreadyMasked,
                        Verdict::Skip => unreachable!(),
                    };
                    let (before, after) = context_chars(text, start, end, 20);
                    let masked_preview = if confidence == Confidence::AlreadyMasked {
                        raw.clone()
                    } else {
                        mask_partial(&raw, &rule.partial)
                    };
                    out.push(Finding {
                        rule_id: rule.id.clone(),
                        label: rule.label.clone(),
                        raw,
                        masked_preview,
                        confidence,
                        paragraph_index,
                        byte_start: start,
                        byte_end: end,
                        char_start: text[..start].chars().count(),
                        char_end: text[..end].chars().count(),
                        context_before: before,
                        context_after: after,
                    });
                }
            }
        }
    }
    out.sort_by_key(|f| (f.byte_start, f.byte_end));
    out
}

fn left_boundary(text: &str, start: usize) -> bool {
    if start == 0 {
        return true;
    }
    let Some(prev) = text[..start].chars().next_back() else {
        return true;
    };
    !(prev.is_ascii_digit() || prev == '*' || prev.is_ascii_alphabetic())
}

fn right_boundary(text: &str, end: usize) -> bool {
    if end >= text.len() {
        return true;
    }
    let Some(next) = text[end..].chars().next() else {
        return true;
    };
    !(next.is_ascii_digit() || next == '*' || next.is_ascii_alphabetic())
}

fn context_chars(text: &str, start: usize, end: usize, n: usize) -> (String, String) {
    let before: String = text[..start].chars().rev().take(n).collect::<String>().chars().rev().collect();
    let after: String = text[end..].chars().take(n).collect();
    (before, after)
}

fn validate(rule: &CompiledRule, raw: &str) -> Verdict {
    let stars = has_mask_star(raw);
    match rule.validator.as_str() {
        "rrn" => validate_rrn(raw, stars, 1, 4),
        "frn" => validate_rrn(raw, stars, 5, 8),
        "corp" => validate_fixed(raw, stars, 13, corp_checksum_ok),
        "biz" => validate_biz(raw, stars),
        "luhn" => validate_card(raw, stars),
        "phone" => validate_phone(raw, stars),
        "email" => validate_email(raw, stars),
        "driver_license" => {
            if stars {
                Verdict::AlreadyMasked
            } else {
                Verdict::Confirmed
            }
        }
        "passport" => validate_passport(raw, stars),
        "bank" => validate_bank(raw, stars),
        "health" => validate_health(raw, stars),
        "ip" => validate_ip(raw, stars),
        _ => {
            if stars {
                Verdict::AlreadyMasked
            } else {
                Verdict::Confirmed
            }
        }
    }
}

fn validate_rrn(raw: &str, stars: bool, gender_lo: u8, gender_hi: u8) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.len() != 13 {
        return Verdict::Skip;
    }
    if stars {
        let gender = toks[6];
        if gender != 0xFF && !(gender_lo..=gender_hi).contains(&gender) {
            return Verdict::Skip;
        }
        return Verdict::AlreadyMasked;
    }
    let gender = toks[6];
    if !(gender_lo..=gender_hi).contains(&gender) {
        return Verdict::Skip;
    }
    if !rrn_date_ok(&toks) {
        return Verdict::Skip;
    }
    if rrn_checksum_ok(&toks) {
        Verdict::Confirmed
    } else {
        Verdict::Suspicious
    }
}

fn validate_fixed(raw: &str, stars: bool, len: usize, checksum: fn(&[u8]) -> bool) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.len() != len {
        return Verdict::Skip;
    }
    if stars {
        return Verdict::AlreadyMasked;
    }
    if checksum(&toks) {
        Verdict::Confirmed
    } else {
        Verdict::Suspicious
    }
}

fn validate_card(raw: &str, stars: bool) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.len() < 13 || toks.len() > 19 {
        return Verdict::Skip;
    }
    if stars {
        return Verdict::AlreadyMasked;
    }
    let first = toks[0];
    if !(3..=6).contains(&first) {
        return Verdict::Skip;
    }
    if luhn_ok(&toks) {
        Verdict::Confirmed
    } else {
        Verdict::Suspicious
    }
}

fn validate_phone(raw: &str, stars: bool) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.is_empty() || toks[0] != 0 {
        return Verdict::Skip;
    }
    let digits = extract_digits(raw);
    if !(10..=11).contains(&digits.len()) && !stars {
        return Verdict::Skip;
    }
    if !raw.starts_with("01") {
        return Verdict::Skip;
    }
    if stars {
        Verdict::AlreadyMasked
    } else {
        Verdict::Confirmed
    }
}

fn validate_email(raw: &str, stars: bool) -> Verdict {
    if !raw.contains('@') || !raw.contains('.') {
        return Verdict::Skip;
    }
    if stars {
        Verdict::AlreadyMasked
    } else {
        Verdict::Confirmed
    }
}

fn validate_passport(raw: &str, stars: bool) -> Verdict {
    let letters = raw.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    if !(1..=2).contains(&letters) {
        return Verdict::Skip;
    }
    let rest: String = raw.chars().skip(letters).collect();
    if !rest.chars().all(|c| c.is_ascii_digit() || c == '*') {
        return Verdict::Skip;
    }
    if stars {
        // M******* (7) / AB****** (6) and full 7–8 digit tails.
        if (6..=8).contains(&rest.len()) {
            Verdict::AlreadyMasked
        } else {
            Verdict::Skip
        }
    } else if rest.chars().all(|c| c.is_ascii_digit()) && (7..=8).contains(&rest.len()) {
        Verdict::Confirmed
    } else {
        Verdict::Skip
    }
}

fn validate_biz(raw: &str, stars: bool) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.len() != 10 {
        return Verdict::Skip;
    }
    if stars {
        return Verdict::AlreadyMasked;
    }
    if biz_checksum_ok(&toks) {
        Verdict::Confirmed
    } else if raw.chars().any(|c| c == '-' || c == ' ' || c == '–') {
        Verdict::Suspicious
    } else {
        Verdict::Skip
    }
}

fn validate_health(raw: &str, stars: bool) -> Verdict {
    let toks = extract_digit_stars(raw);
    if toks.len() != 11 {
        return Verdict::Skip;
    }
    if stars {
        return Verdict::AlreadyMasked;
    }
    if raw.chars().any(|c| c == '-' || c == ' ' || c == '–') {
        Verdict::Confirmed
    } else {
        Verdict::Skip
    }
}

fn validate_bank(raw: &str, stars: bool) -> Verdict {
    let n = extract_digit_stars(raw).len();
    if !(10..=14).contains(&n) {
        return Verdict::Skip;
    }
    if stars {
        Verdict::AlreadyMasked
    } else {
        Verdict::Confirmed
    }
}

fn validate_ip(raw: &str, stars: bool) -> Verdict {
    if raw.contains('.') {
        let parts: Vec<&str> = raw.split('.').collect();
        if parts.len() != 4 {
            return Verdict::Skip;
        }
        let last_star = parts[3] == "*";
        let n = if last_star { 3 } else { 4 };
        for p in parts.iter().take(n) {
            match p.parse::<u32>() {
                Ok(v) if v <= 255 => {}
                _ => return Verdict::Skip,
            }
        }
        if last_star || stars {
            Verdict::AlreadyMasked
        } else {
            Verdict::Confirmed
        }
    } else if raw.contains(':') {
        if stars {
            Verdict::AlreadyMasked
        } else {
            Verdict::Confirmed
        }
    } else {
        Verdict::Skip
    }
}

/// Masked display used in the results table: value itself is never shown raw.
pub fn preview_line(f: &Finding) -> String {
    format!(
        "{}{}{}",
        f.context_before,
        if f.confidence == Confidence::AlreadyMasked {
            f.raw.clone()
        } else {
            mask_full(&f.raw)
        },
        f.context_after
    )
}
