//! Phone number normalization.
//!
//! WhatsApp identifies people by phone number, so every number is stored in
//! one canonical form (E.164, for example `+919876543210`). Two spellings of
//! the same number must never become two people.

use crate::{Error, Result};

/// Country code assumed when a number is written without one.
pub const DEFAULT_COUNTRY_CODE: &str = "91";

/// Turn a phone number as a person might type it into E.164.
///
/// Accepts `+91 98765 43210`, `098765-43210`, `9876543210`, `919876543210`
/// and `0091 9876543210`. Numbers with a `+` or `00` prefix keep their own
/// country code; bare 10-digit Indian mobile numbers get `+91`.
pub fn normalize(input: &str) -> Result<String> {
    let mut has_plus = false;
    let mut digits = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '0'..='9' => digits.push(ch),
            // A plus sign only counts before the first digit, and only once.
            '+' if digits.is_empty() && !has_plus => has_plus = true,
            ' ' | '-' | '(' | ')' | '.' => {}
            _ => return Err(invalid(input)),
        }
    }

    let national_or_full = if has_plus {
        digits
    } else if let Some(rest) = digits.strip_prefix("00") {
        rest.to_string()
    } else {
        let local = digits.strip_prefix('0').unwrap_or(&digits);
        if local.len() == 10 && is_indian_mobile(local) {
            format!("{DEFAULT_COUNTRY_CODE}{local}")
        } else if local.len() == 12
            && local.starts_with(DEFAULT_COUNTRY_CODE)
            && is_indian_mobile(&local[2..])
        {
            local.to_string()
        } else {
            return Err(invalid(input));
        }
    };

    let len = national_or_full.len();
    if !(8..=15).contains(&len) || national_or_full.starts_with('0') {
        return Err(invalid(input));
    }
    // An Indian number must be a valid 10-digit mobile number.
    if let Some(rest) = national_or_full.strip_prefix(DEFAULT_COUNTRY_CODE) {
        if rest.len() != 10 || !is_indian_mobile(rest) {
            return Err(invalid(input));
        }
    }
    Ok(format!("+{national_or_full}"))
}

fn is_indian_mobile(ten_digits: &str) -> bool {
    matches!(ten_digits.as_bytes().first(), Some(b'6'..=b'9'))
}

fn invalid(input: &str) -> Error {
    Error::Invalid(format!("not a valid phone number: {input:?}"))
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn the_same_number_written_many_ways_is_one_number() {
        for written in [
            "+919876543210",
            "+91 98765 43210",
            "+91-98765-43210",
            "9876543210",
            "98765 43210",
            "09876543210",
            "919876543210",
            "0091 9876543210",
            "  (+91) 98765.43210 ",
        ] {
            assert_eq!(normalize(written).unwrap(), "+919876543210", "{written}");
        }
    }

    #[test]
    fn foreign_numbers_keep_their_country_code() {
        assert_eq!(normalize("+1 415 555 2671").unwrap(), "+14155552671");
        assert_eq!(normalize("+971 50 123 4567").unwrap(), "+971501234567");
        assert_eq!(normalize("0044 7911 123456").unwrap(), "+447911123456");
    }

    #[test]
    fn junk_is_rejected() {
        for written in [
            "",
            "abc",
            "12345",
            "1234567890",      // 10 digits but not an Indian mobile number
            "+91 12345 67890", // Indian code, landline-style start
            "+9198765432",     // Indian code, too short
            "98765432101234567",
            "+0123456789",
            "98765x43210",
            "9876+543210",
        ] {
            assert!(
                normalize(written).is_err(),
                "{written:?} should be rejected"
            );
        }
    }
}
