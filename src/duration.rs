use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parse a duration string into total seconds.
///
/// Supported formats:
/// - Plain number → minutes (e.g., `"5"` → 300s)
/// - Number + suffix: `s`, `m`, `h` (and variants like `"min"`, `"hours"`)
/// - Decimals: `"12.5m"` → 750s, `"1.1h"` → 3960s
pub fn parse_duration(input: &str) -> Result<f64, ParseError> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return Err(ParseError("empty input".into()));
    }

    let num_end = input
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(input.len());

    let num_str = &input[..num_end];
    let suffix = input[num_end..].trim();

    let value: f64 = num_str
        .parse()
        .map_err(|_| ParseError(format!("invalid number: '{num_str}'")))?;

    if value < 0.0 {
        return Err(ParseError("duration must be non-negative".into()));
    }

    let seconds = match suffix {
        "s" | "sec" | "secs" | "second" | "seconds" => value,
        "" | "m" | "min" | "mins" | "minute" | "minutes" => value * 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => value * 3600.0,
        other => return Err(ParseError(format!("unknown suffix: '{other}'"))),
    };

    Ok(seconds)
}

/// Format total seconds as `HH:MM:SS`. Negative values shown as `-HH:MM:SS`.
pub fn format_seconds(total_secs: f64) -> String {
    let negative = total_secs < 0.0;
    let total = std::time::Duration::from_secs_f64(total_secs.abs().round()).as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if negative {
        format!("-{h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_number_is_minutes() {
        assert_eq!(parse_duration("5").unwrap(), 300.0);
    }

    #[test]
    fn plain_zero() {
        assert_eq!(parse_duration("0").unwrap(), 0.0);
    }

    #[test]
    fn decimal_minutes() {
        assert_eq!(parse_duration("12.5m").unwrap(), 750.0);
    }

    #[test]
    fn decimal_hours() {
        let secs = parse_duration("1.1h").unwrap();
        assert!((secs - 3960.0).abs() < 0.01);
    }

    #[test]
    fn seconds_suffix() {
        assert_eq!(parse_duration("30s").unwrap(), 30.0);
    }

    #[test]
    fn seconds_suffix_variants() {
        assert_eq!(parse_duration("10sec").unwrap(), 10.0);
        assert_eq!(parse_duration("10secs").unwrap(), 10.0);
        assert_eq!(parse_duration("10second").unwrap(), 10.0);
        assert_eq!(parse_duration("10seconds").unwrap(), 10.0);
    }

    #[test]
    fn minutes_suffix_variants() {
        assert_eq!(parse_duration("2min").unwrap(), 120.0);
        assert_eq!(parse_duration("2mins").unwrap(), 120.0);
        assert_eq!(parse_duration("2minute").unwrap(), 120.0);
        assert_eq!(parse_duration("2minutes").unwrap(), 120.0);
    }

    #[test]
    fn hours_variants() {
        assert!((parse_duration("4.4hours").unwrap() - 15840.0).abs() < 0.01);
        assert!((parse_duration("3hrs").unwrap() - 10800.0).abs() < 0.01);
        assert_eq!(parse_duration("1hr").unwrap(), 3600.0);
        assert_eq!(parse_duration("1hour").unwrap(), 3600.0);
    }

    #[test]
    fn whitespace_trimmed() {
        assert_eq!(parse_duration("  5m  ").unwrap(), 300.0);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(parse_duration("5M").unwrap(), 300.0);
        assert_eq!(parse_duration("1H").unwrap(), 3600.0);
        assert_eq!(parse_duration("30S").unwrap(), 30.0);
    }

    #[test]
    fn very_small_decimal() {
        let secs = parse_duration("0.1m").unwrap();
        assert!((secs - 6.0).abs() < 0.01);
    }

    #[test]
    fn invalid_input() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("5x").is_err());
    }

    #[test]
    fn invalid_suffix() {
        assert!(parse_duration("5foo").is_err());
        assert!(parse_duration("5d").is_err());
    }

    #[test]
    fn just_dot_is_invalid() {
        assert!(parse_duration(".").is_err());
    }

    #[test]
    fn spaces_only_is_invalid() {
        assert!(parse_duration("   ").is_err());
    }

    #[test]
    fn format_positive() {
        assert_eq!(format_seconds(90.0), "00:01:30");
        assert_eq!(format_seconds(3661.0), "01:01:01");
    }

    #[test]
    fn format_negative() {
        assert_eq!(format_seconds(-90.0), "-00:01:30");
    }

    #[test]
    fn format_zero() {
        assert_eq!(format_seconds(0.0), "00:00:00");
    }

    #[test]
    fn format_large_value() {
        assert_eq!(format_seconds(86400.0), "24:00:00");
    }

    #[test]
    fn format_fractional_rounds() {
        assert_eq!(format_seconds(0.6), "00:00:01");
        assert_eq!(format_seconds(0.4), "00:00:00");
    }
}
