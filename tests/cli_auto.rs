//! `command | etoon` auto mode: a whole-buffer JSON document encodes as TOON,
//! anything else goes through log mode.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_etoon");

fn auto(input: &str) -> String {
    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn etoon");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait for etoon");
    assert!(out.status.success(), "etoon failed on {input:?}");
    String::from_utf8(out.stdout).expect("utf-8 output")
}

#[test]
fn test_auto_root_string_json_only_escapes_encodes_as_toon() {
    // `\b` and `\/` are JSON escapes that TOON decoders reject, so raw JSON is wrong.
    assert_eq!(auto("\"a\\bb\""), "\"a\\u0008b\"\n");
    assert_eq!(auto("\"\\/x\""), "/x\n");
}

#[test]
fn test_auto_root_string_encodes_as_toon() {
    assert_eq!(auto("\"v1.10\"\n"), "v1.10\n");
    assert_eq!(auto("\"\u{FEFF}8\""), "\"\u{FEFF}8\"\n");
}

#[test]
fn test_auto_bare_numbers_and_literals_pass_through_unchanged() {
    // Valid JSON, but already valid TOON as written: a version or a price keeps its text.
    for text in ["1.10\n", "-0.0", "1e3\n", "  true  ", "null"] {
        assert_eq!(auto(text), text);
    }
}

#[test]
fn test_auto_scalar_like_log_lines_pass_through_unchanged() {
    for log in [
        "42 requests served\n",
        "\"quoted\" then text\n",
        "true story\n",
        "null\nmore text\n",
        "-v flag set\n",
    ] {
        assert_eq!(auto(log), log);
    }
}

#[test]
fn test_auto_empty_and_blank_input_pass_through_unchanged() {
    assert_eq!(auto(""), "");
    assert_eq!(auto(" \n\t\n"), " \n\t\n");
}

#[test]
fn test_auto_root_object_still_encodes() {
    assert_eq!(auto("{\"a\":1}"), "a: 1\n");
}
