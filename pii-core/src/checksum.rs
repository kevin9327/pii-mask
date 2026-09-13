//! Checksums used after regex matching. These are the shipped validators.

pub fn rrn_check_digit(first12: &[u8]) -> u8 {
    const W: [u32; 12] = [2, 3, 4, 5, 6, 7, 8, 9, 2, 3, 4, 5];
    let sum: u32 = first12
        .iter()
        .take(12)
        .zip(W)
        .map(|(d, w)| u32::from(*d) * w)
        .sum();
    ((11 - (sum % 11)) % 10) as u8
}

pub fn rrn_checksum_ok(digits: &[u8]) -> bool {
    if digits.len() != 13 {
        return false;
    }
    rrn_check_digit(&digits[..12]) == digits[12]
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Century from the 7th digit (1-based gender/century code).
pub fn rrn_century(code: u8) -> Option<u32> {
    match code {
        1 | 2 | 5 | 6 => Some(1900),
        3 | 4 | 7 | 8 => Some(2000),
        _ => None,
    }
}

pub fn rrn_date_ok(digits: &[u8]) -> bool {
    if digits.len() < 7 {
        return false;
    }
    let Some(century) = rrn_century(digits[6]) else {
        return false;
    };
    let year = century + u32::from(digits[0]) * 10 + u32::from(digits[1]);
    let month = u32::from(digits[2]) * 10 + u32::from(digits[3]);
    let day = u32::from(digits[4]) * 10 + u32::from(digits[5]);
    if !(1..=12).contains(&month) {
        return false;
    }
    day >= 1 && day <= days_in_month(year, month)
}

pub fn luhn_ok(digits: &[u8]) -> bool {
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = u32::from(*d);
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum.is_multiple_of(10)
}

pub fn luhn_check_digit(payload: &[u8]) -> u8 {
    for d in 0u8..10 {
        let mut v = payload.to_vec();
        v.push(d);
        if luhn_ok(&v) {
            return d;
        }
    }
    0
}

/// 사업자등록번호 10자리. `first9` is digits 0..8, return digit 9.
pub fn biz_check_digit(first9: &[u8]) -> u8 {
    const W: [u32; 9] = [1, 3, 7, 1, 3, 7, 1, 3, 5];
    let mut a = 0u32;
    for i in 0..9 {
        a += u32::from(first9[i]) * W[i];
    }
    let b = (u32::from(first9[8]) * 5) / 10;
    let c = (a + b) % 10;
    ((10 - c) % 10) as u8
}

pub fn biz_checksum_ok(digits: &[u8]) -> bool {
    if digits.len() != 10 {
        return false;
    }
    biz_check_digit(&digits[..9]) == digits[9]
}

/// 법인등록번호 13자리.
pub fn corp_check_digit(first12: &[u8]) -> u8 {
    const W: [u32; 12] = [1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2];
    let sum: u32 = first12
        .iter()
        .take(12)
        .zip(W)
        .map(|(d, w)| u32::from(*d) * w)
        .sum();
    ((10 - (sum % 10)) % 10) as u8
}

pub fn corp_checksum_ok(digits: &[u8]) -> bool {
    if digits.len() != 13 {
        return false;
    }
    corp_check_digit(&digits[..12]) == digits[12]
}

pub fn extract_digit_stars(s: &str) -> Vec<u8> {
    s.chars()
        .filter_map(|c| match c {
            '0'..='9' => Some(c as u8 - b'0'),
            '*' => Some(0xFF),
            _ => None,
        })
        .collect()
}

pub fn extract_digits(s: &str) -> Vec<u8> {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| c as u8 - b'0')
        .collect()
}

pub fn has_mask_star(s: &str) -> bool {
    s.chars().any(|c| c == '*')
}
