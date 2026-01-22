//! Shared UI utilities and constants.
//!
//! This module contains common functions and constants used across
//! multiple UI components to avoid code duplication.

use std::borrow::Cow;

/// WiFi signal threshold for strong signal (dBm)
pub const WIFI_STRONG_THRESHOLD: i32 = -50;

/// WiFi signal threshold for medium signal (dBm)
pub const WIFI_MEDIUM_THRESHOLD: i32 = -70;

/// Default dBm value when signal cannot be parsed
pub const WIFI_DEFAULT_DBM: i32 = -100;

/// Prefix used in full printer model names from Bambu
pub const MODEL_PREFIX: &str = "Bambu Lab ";

/// Number of serial number digits to show in compact title
pub const SERIAL_SUFFIX_LENGTH: usize = 4;

/// Formats a compact printer title from model name and optional serial suffix.
///
/// Extracts the short model name (e.g., "P1S" from "Bambu Lab P1S") and appends
/// the last digits of the serial number for identification.
///
/// # Examples
///
/// - With serial: "Bambu Lab P1S" + "6789" -> "P1S ...6789"
/// - Without serial: "Bambu Lab P1S" + "" -> "P1S"
/// - Unknown model: "Bambu Printer" + "0428" -> "Bambu Printer ...0428"
///
/// Returns `Cow::Borrowed` when possible to avoid allocations.
pub fn format_compact_title<'a>(printer_model: &'a str, serial_suffix: &str) -> Cow<'a, str> {
    // Extract short model name by removing "Bambu Lab " prefix
    let short_model = printer_model
        .strip_prefix(MODEL_PREFIX)
        .unwrap_or(printer_model);

    if serial_suffix.is_empty() {
        // No serial suffix available, return just the model name
        if short_model.len() == printer_model.len() {
            // No prefix was stripped, return borrowed reference
            Cow::Borrowed(printer_model)
        } else {
            // Prefix was stripped, need to return the slice
            Cow::Borrowed(short_model)
        }
    } else {
        // Format with serial suffix
        Cow::Owned(format!("{} ...{}", short_model, serial_suffix))
    }
}

/// Extracts the last N characters from a serial number for display.
///
/// Returns an empty string if the serial is too short or empty.
pub fn extract_serial_suffix(serial: &str) -> &str {
    let len = serial.len();
    if len >= SERIAL_SUFFIX_LENGTH {
        &serial[len - SERIAL_SUFFIX_LENGTH..]
    } else {
        ""
    }
}

/// Parses dBm value from a string like "-45dBm" or "-45" without allocation.
///
/// Uses a streaming approach that extracts all contiguous digits while tracking
/// whether a leading minus sign was present.
pub fn parse_dbm(s: &str) -> Option<i32> {
    let mut result: i32 = 0;
    let mut negative = false;
    let mut found_digit = false;

    for c in s.chars() {
        if c == '-' && !found_digit {
            negative = true;
        } else if c.is_ascii_digit() {
            found_digit = true;
            result = result
                .saturating_mul(10)
                .saturating_add((c as i32) - ('0' as i32));
        }
    }

    if found_digit {
        Some(if negative { -result } else { result })
    } else {
        None
    }
}

/// Returns the status text for a given gcode state.
///
/// Maps printer gcode states to user-friendly display text.
/// This is the canonical implementation used by both the App and UI components.
pub fn gcode_state_to_status(gcode_state: &str) -> &'static str {
    match gcode_state {
        "IDLE" => "Idle",
        "PREPARE" => "Preparing",
        "RUNNING" => "Printing",
        "PAUSE" => "Paused",
        "FINISH" => "Finished",
        "FAILED" => "Failed",
        "" => "Connecting...",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_dbm_tests {
        use super::*;

        #[test]
        fn parses_negative_with_suffix() {
            assert_eq!(parse_dbm("-45dBm"), Some(-45));
            assert_eq!(parse_dbm("-70dBm"), Some(-70));
        }

        #[test]
        fn parses_negative_without_suffix() {
            assert_eq!(parse_dbm("-45"), Some(-45));
            assert_eq!(parse_dbm("-100"), Some(-100));
        }

        #[test]
        fn parses_positive_values() {
            assert_eq!(parse_dbm("45"), Some(45));
            assert_eq!(parse_dbm("0"), Some(0));
        }

        #[test]
        fn returns_none_for_empty() {
            assert_eq!(parse_dbm(""), None);
        }

        #[test]
        fn returns_none_for_no_digits() {
            assert_eq!(parse_dbm("dBm"), None);
            assert_eq!(parse_dbm("-"), None);
            assert_eq!(parse_dbm("abc"), None);
        }

        #[test]
        fn handles_whitespace_in_value() {
            assert_eq!(parse_dbm("Signal: -45 dBm"), Some(-45));
        }

        #[test]
        fn saturates_on_overflow() {
            let result = parse_dbm("99999999999999999999");
            assert!(result.is_some());
            assert_eq!(result, Some(i32::MAX));
        }

        #[test]
        fn handles_multiple_minus_signs() {
            assert_eq!(parse_dbm("--45"), Some(-45));
        }

        #[test]
        fn concatenates_all_digit_sequences() {
            assert_eq!(parse_dbm("-45abc67"), Some(-4567));
        }

        #[test]
        fn minus_after_digits_is_ignored() {
            assert_eq!(parse_dbm("45-67"), Some(4567));
        }
    }

    mod format_compact_title_tests {
        use super::*;

        #[test]
        fn formats_p1s_with_serial_suffix() {
            let result = format_compact_title("Bambu Lab P1S", "6789");
            assert_eq!(result, "P1S ...6789");
        }

        #[test]
        fn formats_x1c_with_serial_suffix() {
            let result = format_compact_title("Bambu Lab X1C", "0428");
            assert_eq!(result, "X1C ...0428");
        }

        #[test]
        fn formats_a1_mini_with_serial_suffix() {
            let result = format_compact_title("Bambu Lab A1 Mini", "1234");
            assert_eq!(result, "A1 Mini ...1234");
        }

        #[test]
        fn returns_model_only_without_serial() {
            let result = format_compact_title("Bambu Lab P1S", "");
            assert_eq!(result, "P1S");
            assert!(matches!(result, Cow::Borrowed(_)));
        }

        #[test]
        fn handles_unknown_model_with_serial() {
            let result = format_compact_title("Bambu Printer", "5678");
            assert_eq!(result, "Bambu Printer ...5678");
        }

        #[test]
        fn handles_unknown_model_without_serial() {
            let result = format_compact_title("Bambu Printer", "");
            assert_eq!(result, "Bambu Printer");
            assert!(matches!(result, Cow::Borrowed(_)));
        }

        #[test]
        fn handles_empty_model_with_serial() {
            let result = format_compact_title("", "9999");
            assert_eq!(result, " ...9999");
        }

        #[test]
        fn handles_empty_model_without_serial() {
            let result = format_compact_title("", "");
            assert_eq!(result, "");
            assert!(matches!(result, Cow::Borrowed(_)));
        }
    }

    mod extract_serial_suffix_tests {
        use super::*;

        #[test]
        fn extracts_last_4_chars() {
            assert_eq!(extract_serial_suffix("01P00A123456789"), "6789");
        }

        #[test]
        fn extracts_from_exact_4_char_serial() {
            assert_eq!(extract_serial_suffix("1234"), "1234");
        }

        #[test]
        fn returns_empty_for_short_serial() {
            assert_eq!(extract_serial_suffix("123"), "");
            assert_eq!(extract_serial_suffix("12"), "");
            assert_eq!(extract_serial_suffix("1"), "");
        }

        #[test]
        fn returns_empty_for_empty_serial() {
            assert_eq!(extract_serial_suffix(""), "");
        }

        #[test]
        fn handles_serial_with_letters() {
            assert_eq!(extract_serial_suffix("01P00AABCD"), "ABCD");
        }
    }

    mod gcode_state_to_status_tests {
        use super::*;

        #[test]
        fn maps_known_states() {
            assert_eq!(gcode_state_to_status("IDLE"), "Idle");
            assert_eq!(gcode_state_to_status("PREPARE"), "Preparing");
            assert_eq!(gcode_state_to_status("RUNNING"), "Printing");
            assert_eq!(gcode_state_to_status("PAUSE"), "Paused");
            assert_eq!(gcode_state_to_status("FINISH"), "Finished");
            assert_eq!(gcode_state_to_status("FAILED"), "Failed");
        }

        #[test]
        fn maps_empty_to_connecting() {
            assert_eq!(gcode_state_to_status(""), "Connecting...");
        }

        #[test]
        fn maps_unknown_to_unknown() {
            assert_eq!(gcode_state_to_status("FOOBAR"), "Unknown");
        }
    }
}
