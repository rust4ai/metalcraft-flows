//! `{{path}}` interpolation for string fields in a flow.
//!
//! A single, deliberately small templating syntax: `{{name}}` or
//! `{{name.path.to.field}}` is replaced by the corresponding value looked up in
//! the flow's `variables` object (dotted JSON path). Missing paths resolve to the
//! empty string. Whitespace inside the braces is trimmed. `{{` with no closing
//! `}}` is left verbatim.
//!
//! This is intentionally not a general template engine — no conditionals, loops,
//! or filters. It exists so node fields (`prompt`, tool `args`, `http.url`, …)
//! can reference upstream state.

use crate::state::lookup_path;
use serde_json::Value;

/// Resolve every `{{path}}` placeholder in `input` against `vars`.
///
/// A path resolving to a JSON string yields the raw string; other JSON values
/// yield their compact JSON form; a missing path yields `""`.
pub fn resolve(input: &str, vars: &Value) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{'
            && let Some(close) = find_close(input, i + 2)
        {
            let path = input[i + 2..close].trim();
            out.push_str(&render_value(lookup_path(vars, path)));
            i = close + 2;
            continue;
        }
        // Not a placeholder start — copy this char verbatim.
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Byte index of the `}}` closing a placeholder opened at `from`, if any.
fn find_close(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Length in bytes of a UTF-8 code point from its leading byte.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Render a looked-up value for substitution.
fn render_value(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_and_nested() {
        let vars = json!({ "repo": "acme/app", "triage": { "severity": "P0" }, "_last": 18 });
        assert_eq!(resolve("repo={{repo}}", &vars), "repo=acme/app");
        assert_eq!(resolve("sev={{triage.severity}}", &vars), "sev=P0");
        assert_eq!(resolve("{{_last}}°F", &vars), "18°F");
    }

    #[test]
    fn missing_is_empty_and_whitespace_trimmed() {
        let vars = json!({ "a": "x" });
        assert_eq!(resolve("[{{ a }}][{{ missing }}]", &vars), "[x][]");
    }

    #[test]
    fn unclosed_and_plain_text_preserved() {
        let vars = json!({});
        assert_eq!(resolve("no placeholders", &vars), "no placeholders");
        assert_eq!(resolve("dangling {{ oops", &vars), "dangling {{ oops");
    }

    #[test]
    fn unicode_outside_placeholders() {
        let vars = json!({ "n": "Zürich" });
        assert_eq!(resolve("café {{n}} 你好", &vars), "café Zürich 你好");
    }
}
