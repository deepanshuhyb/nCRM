//! Small input checks shared by the studio modules. Messages are sentences
//! for the person filling in the form.

use crate::{Error, Result};

/// Trimmed text that must be present and at most `max` characters.
/// `what` names the field in the message ("Room names").
pub fn required(value: &str, max: usize, what: &str) -> Result<String> {
    let value = value.trim();
    let len = value.chars().count();
    if len == 0 || len > max {
        return Err(Error::Invalid(format!(
            "{what} must be 1 to {max} characters."
        )));
    }
    Ok(value.to_string())
}

/// Trimmed text that may be absent; empty counts as absent.
pub fn optional(value: Option<String>, max: usize, what: &str) -> Result<Option<String>> {
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if value.as_ref().is_some_and(|v| v.chars().count() > max) {
        return Err(Error::Invalid(format!(
            "{what} can be at most {max} characters."
        )));
    }
    Ok(value)
}

/// A whole number within `min..=max`, when given.
pub fn range(value: Option<i32>, min: i32, max: i32, what: &str) -> Result<Option<i32>> {
    match value {
        Some(v) if v < min || v > max => Err(Error::Invalid(format!(
            "{what} must be between {min} and {max}."
        ))),
        other => Ok(other),
    }
}

/// Rupees as text for messages: 150000 paise → "₹1,500".
pub fn rupees(paise: i64) -> String {
    let rupees = paise / 100;
    let paise_part = paise % 100;
    // Indian grouping: last three digits, then pairs.
    let digits = rupees.to_string();
    let grouped = if digits.len() <= 3 {
        digits
    } else {
        let (head, tail) = digits.split_at(digits.len() - 3);
        let mut parts: Vec<&str> = Vec::new();
        let mut rest = head;
        while rest.len() > 2 {
            let (a, b) = rest.split_at(rest.len() - 2);
            parts.insert(0, b);
            rest = a;
        }
        if !rest.is_empty() {
            parts.insert(0, rest);
        }
        format!("{},{tail}", parts.join(","))
    };
    if paise_part == 0 {
        format!("₹{grouped}")
    } else {
        format!("₹{grouped}.{paise_part:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rupees_use_indian_grouping() {
        assert_eq!(rupees(0), "₹0");
        assert_eq!(rupees(150_000), "₹1,500");
        assert_eq!(rupees(12_450_000), "₹1,24,500");
        assert_eq!(rupees(100_000_000), "₹10,00,000");
        assert_eq!(rupees(99_950), "₹999.50");
    }

    #[test]
    fn text_is_trimmed_and_limited() {
        assert_eq!(required("  Studio A ", 60, "Names").unwrap(), "Studio A");
        assert!(required("   ", 60, "Names").is_err());
        assert!(required(&"x".repeat(61), 60, "Names").is_err());
        assert_eq!(optional(Some("  ".into()), 40, "Style").unwrap(), None);
        assert!(range(Some(0), 1, 500, "Capacity").is_err());
        assert_eq!(range(None, 1, 500, "Capacity").unwrap(), None);
    }
}
