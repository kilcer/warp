use super::logcat::{parse_logcat_line, LogLevel, LogcatEntry};

// ========== LogLevel::from_char ==========

#[test]
fn loglevel_from_char_verbose() {
    assert_eq!(LogLevel::from_char('V'), Some(LogLevel::Verbose));
}

#[test]
fn loglevel_from_char_debug() {
    assert_eq!(LogLevel::from_char('D'), Some(LogLevel::Debug));
}

#[test]
fn loglevel_from_char_info() {
    assert_eq!(LogLevel::from_char('I'), Some(LogLevel::Info));
}

#[test]
fn loglevel_from_char_warn() {
    assert_eq!(LogLevel::from_char('W'), Some(LogLevel::Warn));
}

#[test]
fn loglevel_from_char_error() {
    assert_eq!(LogLevel::from_char('E'), Some(LogLevel::Error));
}

#[test]
fn loglevel_from_char_fatal() {
    assert_eq!(LogLevel::from_char('F'), Some(LogLevel::Fatal));
}

#[test]
fn loglevel_from_char_invalid() {
    assert_eq!(LogLevel::from_char('X'), None);
    assert_eq!(LogLevel::from_char(' '), None);
    assert_eq!(LogLevel::from_char('v'), None); // lowercase
}

// ========== parse_logcat_line ==========

#[test]
fn parse_valid_logcat_line_info() {
    let line = "05-16 10:30:45.123  1234  5678 I MyTag: This is a message";
    let entry = parse_logcat_line(line).expect("should parse valid line");
    assert_eq!(entry.timestamp, "05-16 10:30:45.123");
    assert_eq!(entry.pid, 1234);
    assert_eq!(entry.tid, 5678);
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.tag, "MyTag");
    assert_eq!(entry.message, "This is a message");
}

#[test]
fn parse_valid_logcat_line_error() {
    let line = "01-02 08:15:00.000  9999   111 E ErrorTag: Something went wrong";
    let entry = parse_logcat_line(line).expect("should parse valid line");
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "ErrorTag");
    assert_eq!(entry.message, "Something went wrong");
}

#[test]
fn parse_valid_logcat_line_verbose_trailing_spaces() {
    let line = "11-22 23:59:59.999   123  4567 V Tag:   spaced message  ";
    let entry = parse_logcat_line(line).expect("should parse valid line");
    assert_eq!(entry.level, LogLevel::Verbose);
    assert_eq!(entry.message, "  spaced message  ");
}

#[test]
fn parse_valid_logcat_line_warn_with_colon_in_message() {
    let line = "01-01 00:00:00.000     1    22 W Tag: url: https://example.com";
    let entry = parse_logcat_line(line).expect("should parse valid line");
    assert_eq!(entry.level, LogLevel::Warn);
    // ":" in message should be part of the message
    assert_eq!(entry.message, "url: https://example.com");
}

#[test]
fn parse_invalid_line_too_few_fields() {
    let line = "05-16 10:30"; // only 2 fields
    assert!(parse_logcat_line(line).is_none());
}

#[test]
fn parse_invalid_line_no_colon_separator() {
    let line = "05-16 10:30:45.123  1234  5678 I MyTag  This is a message"; // no ":"
    assert!(parse_logcat_line(line).is_none());
}

#[test]
fn parse_invalid_line_non_numeric_pid() {
    let line = "05-16 10:30:45.123  ABCD  5678 I MyTag: message";
    assert!(parse_logcat_line(line).is_none());
}

#[test]
fn parse_invalid_line_bad_log_level() {
    let line = "05-16 10:30:45.123  1234  5678 X MyTag: message"; // X is not a valid level
    assert!(parse_logcat_line(line).is_none());
}

#[test]
fn parse_empty_line() {
    assert!(parse_logcat_line("").is_none());
}
