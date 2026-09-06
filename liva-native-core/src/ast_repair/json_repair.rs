//! High-speed Token-Aware JSON AST Self-Healing Engine (Milestone 2).
//!
//! Provides sub-0.1ms (P99 < 0.2ms) deterministic JSON repair without LLM calls.
//! Repaired defects include:
//! - Trailing commas in objects and arrays
//! - Single quotes -> Double quotes (with apostrophe detection)
//! - Unquoted object keys (e.g. `{ action: "run", step_id: 1 }`)
//! - Unescaped double quotes inside strings (e.g. `{"title": "The "Great" Gatsby"}`)
//! - Pythonic/JS literals (`None`, `True`, `False`, `undefined`, `NaN`)
//! - Truncated JSON / unclosed brackets and braces at EOF (Stack auto-completion)
//! - Missing commas between key-value pairs or array items
//! - Equal signs (`=` / `=>`) instead of colons
//! - Markdown code blocks (```json ... ```) and prose stripping
//! - Comments (`//`, `/* */`) removal
//! - Control character normalization (\n, \t, \r in strings)

use serde_json::Value;
use std::time::Instant;
use thiserror::Error;

/// Errors produced during JSON AST repair.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AstRepairError {
    #[error("Empty or whitespace-only input")]
    EmptyInput,

    #[error("No JSON structure (object, array, or key-value) found in input: {0}")]
    NoJsonFound(String),

    #[error("Unrecoverable JSON syntax error: {0}")]
    UnrecoverableSyntax(String),

    #[error("Repaired JSON string failed deserialization: {0} (repaired: {1})")]
    DeserializationFailed(String, String),
}

/// Diagnostic statistics for a JSON AST repair operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstRepairStats {
    pub original_len: usize,
    pub repaired_len: usize,
    pub repair_time_micros: u64,
    pub repairs_applied: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexState {
    ExpectValueOrCloseArray,
    ExpectKeyOrCloseObject,
    ExpectColon,
    ExpectValue,
    ExpectCommaOrClose,
}

/// Attempts to repair a malformed JSON string into a valid `serde_json::Value`.
///
/// Executes in <0.1ms with zero LLM invocations.
pub fn repair_json_ast(raw: &str) -> Result<Value, AstRepairError> {
    let (val, _) = repair_json_ast_with_stats(raw)?;
    Ok(val)
}

/// Attempts to repair a malformed JSON string and returns both the parsed `Value` and repair metrics.
pub fn repair_json_ast_with_stats(raw: &str) -> Result<(Value, AstRepairStats), AstRepairError> {
    let start_time = Instant::now();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AstRepairError::EmptyInput);
    }

    // Fast path: if already valid JSON, parse directly
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        let elapsed = start_time.elapsed().as_micros() as u64;
        return Ok((
            val,
            AstRepairStats {
                original_len: raw.len(),
                repaired_len: trimmed.len(),
                repair_time_micros: elapsed,
                repairs_applied: Vec::new(),
            },
        ));
    }

    let (repaired_str, repairs) = repair_json_string_internal(trimmed)?;
    match serde_json::from_str::<Value>(&repaired_str) {
        Ok(val) => {
            let elapsed = start_time.elapsed().as_micros() as u64;
            Ok((
                val,
                AstRepairStats {
                    original_len: raw.len(),
                    repaired_len: repaired_str.len(),
                    repair_time_micros: elapsed,
                    repairs_applied: repairs,
                },
            ))
        }
        Err(e) => Err(AstRepairError::DeserializationFailed(
            e.to_string(),
            repaired_str,
        )),
    }
}

/// Repairs a malformed JSON string into a normalized RFC 8259 JSON string.
pub fn repair_json_string(raw: &str) -> Result<String, AstRepairError> {
    let (repaired, _) = repair_json_string_internal(raw.trim())?;
    Ok(repaired)
}

/// Strips markdown fences, extracts outermost JSON bounds, strips comments.
fn extract_json_candidate(raw: &str) -> (&str, Vec<String>) {
    let mut repairs = Vec::new();
    let mut s = raw.trim();

    let first_brace = s.find('{');
    let first_bracket = s.find('[');
    let first_container = match (first_brace, first_bracket) {
        (Some(b), Some(k)) => Some(b.min(k)),
        (Some(b), None) => Some(b),
        (None, Some(k)) => Some(k),
        (None, None) => None,
    };

    // Strip markdown code fences ```json ... ``` or ``` ... ``` only if fence is before JSON container
    if let Some(start) = s.find("```") {
        let is_fence_before_container = match first_container {
            Some(c_idx) => start < c_idx,
            None => true,
        };

        if is_fence_before_container {
            let after_fence = &s[start + 3..];
            let content_start = if let Some(newline_pos) = after_fence.find('\n') {
                start + 3 + newline_pos + 1
            } else {
                start + 3
            };
            let end = s[content_start..]
                .find("```")
                .map(|p| content_start + p)
                .unwrap_or(s.len());
            s = s[content_start..end].trim();
            repairs.push("stripped_markdown_code_fence".to_string());
        }
    }

    // If there is prose before JSON, find first '{' or '['
    let first_brace = s.find('{');
    let first_bracket = s.find('[');

    let start_idx = match (first_brace, first_bracket) {
        (Some(b), Some(k)) => Some(b.min(k)),
        (Some(b), None) => Some(b),
        (None, Some(k)) => Some(k),
        (None, None) => None,
    };

    if let Some(idx) = start_idx.filter(|&idx| idx > 0) {
        repairs.push("stripped_leading_prose".to_string());
        s = &s[idx..];
    }

    (s, repairs)
}

fn repair_json_string_internal(raw: &str) -> Result<(String, Vec<String>), AstRepairError> {
    if raw.is_empty() {
        return Err(AstRepairError::EmptyInput);
    }

    let (candidate, mut repairs) = extract_json_candidate(raw);

    // If candidate doesn't start with '{' or '[', check if it's a bare key-value list like `a: 1, b: 2`
    let (working_slice, wrapped_in_object) =
        if !candidate.starts_with('{') && !candidate.starts_with('[') {
            if candidate.contains(':') || candidate.contains("=>") || candidate.contains('=') {
                repairs.push("wrapped_bare_key_value_in_object".to_string());
                (candidate, true)
            } else {
                return Err(AstRepairError::NoJsonFound(raw.to_string()));
            }
        } else {
            (candidate, false)
        };

    let chars: Vec<char> = working_slice.chars().collect();
    let mut out = String::with_capacity(chars.len() + 64);
    let mut stack: Vec<Container> = Vec::with_capacity(16);
    let mut pos = 0;
    let len = chars.len();
    let mut has_opened_root = wrapped_in_object;

    if wrapped_in_object {
        out.push('{');
        stack.push(Container::Object);
    }

    let mut state = if wrapped_in_object {
        LexState::ExpectKeyOrCloseObject
    } else {
        LexState::ExpectValue
    };

    while pos < len {
        let loop_start_pos = pos;
        let loop_start_state = state;
        let loop_start_out_len = out.len();

        skip_whitespace_and_comments(&chars, &mut pos, &mut repairs);
        if pos >= len {
            break;
        }

        if !stack.is_empty() {
            has_opened_root = true;
        }

        let c = chars[pos];

        match state {
            LexState::ExpectValueOrCloseArray => {
                if c == ']' {
                    // Close array
                    out.push(']');
                    stack.pop();
                    pos += 1;
                    state = LexState::ExpectCommaOrClose;
                    if has_opened_root && stack.is_empty() {
                        break;
                    }
                } else if c == ',' {
                    // Redundant comma
                    pos += 1;
                } else {
                    parse_value(
                        &chars,
                        &mut pos,
                        &mut out,
                        &mut stack,
                        &mut state,
                        &mut repairs,
                    )?;
                }
            }

            LexState::ExpectKeyOrCloseObject => {
                if c == '}' {
                    // Close object
                    out.push('}');
                    stack.pop();
                    pos += 1;
                    state = LexState::ExpectCommaOrClose;
                    if has_opened_root && stack.is_empty() {
                        break;
                    }
                } else if c == ',' {
                    // Redundant leading / extra comma
                    pos += 1;
                } else {
                    parse_object_key(&chars, &mut pos, &mut out, &mut state, &mut repairs)?;
                }
            }

            LexState::ExpectColon => {
                if c == ':' {
                    out.push(':');
                    pos += 1;
                    state = LexState::ExpectValue;
                } else if c == '=' {
                    // Equal sign instead of colon
                    out.push(':');
                    pos += 1;
                    if pos < len && chars[pos] == '>' {
                        pos += 1; // skip '>' in '=>'
                    }
                    repairs.push("normalized_assignment_to_colon".to_string());
                    state = LexState::ExpectValue;
                } else {
                    // Missing colon before value!
                    out.push(':');
                    repairs.push("inserted_missing_colon".to_string());
                    state = LexState::ExpectValue;
                }
            }

            LexState::ExpectValue => {
                parse_value(
                    &chars,
                    &mut pos,
                    &mut out,
                    &mut stack,
                    &mut state,
                    &mut repairs,
                )?;
            }

            LexState::ExpectCommaOrClose => {
                if c == ',' {
                    // Lookahead: skip all consecutive commas, whitespace, and comments
                    let mut next_pos = pos;
                    let mut comma_count = 0;
                    while next_pos < len {
                        skip_whitespace_and_comments(&chars, &mut next_pos, &mut repairs);
                        if next_pos < len && chars[next_pos] == ',' {
                            next_pos += 1;
                            comma_count += 1;
                        } else {
                            break;
                        }
                    }

                    if next_pos >= len {
                        // Trailing comma(s) at EOF
                        pos = next_pos;
                        repairs.push("stripped_trailing_comma_at_eof".to_string());
                    } else if chars[next_pos] == '}' || chars[next_pos] == ']' {
                        // Trailing comma(s) before closing delimiter! Strip them completely
                        pos = next_pos;
                        repairs.push("stripped_trailing_comma".to_string());
                    } else {
                        // Intermediate commas between elements/fields: emit exactly one comma
                        out.push(',');
                        pos = next_pos;
                        if comma_count > 1 {
                            repairs.push("stripped_redundant_consecutive_commas".to_string());
                        }
                        match stack.last() {
                            Some(Container::Object) => state = LexState::ExpectKeyOrCloseObject,
                            Some(Container::Array) => state = LexState::ExpectValueOrCloseArray,
                            None => state = LexState::ExpectValue,
                        }
                    }
                } else if c == '}' {
                    if let Some(Container::Object) = stack.last() {
                        out.push('}');
                        stack.pop();
                        pos += 1;
                        state = LexState::ExpectCommaOrClose;
                        if has_opened_root && stack.is_empty() {
                            break;
                        }
                    } else {
                        // Mismatched brace
                        pos += 1;
                    }
                } else if c == ']' {
                    if let Some(Container::Array) = stack.last() {
                        out.push(']');
                        stack.pop();
                        pos += 1;
                        state = LexState::ExpectCommaOrClose;
                        if has_opened_root && stack.is_empty() {
                            break;
                        }
                    } else {
                        // Mismatched bracket
                        pos += 1;
                    }
                } else {
                    // Missing comma between elements/fields or trailing root content
                    if stack.is_empty() {
                        // At root level with an empty stack, root JSON value is complete
                        break;
                    }
                    out.push(',');
                    repairs.push("inserted_missing_comma".to_string());
                    match stack.last() {
                        Some(Container::Object) => state = LexState::ExpectKeyOrCloseObject,
                        Some(Container::Array) => state = LexState::ExpectValueOrCloseArray,
                        None => break,
                    }
                }
            }
        }

        // Failsafe progress invariant: guarantee loop termination
        if pos == loop_start_pos && state == loop_start_state && out.len() == loop_start_out_len {
            pos += 1;
        }
    }

    // Auto-close any unclosed stack containers (EOF stack auto-completion)
    if !stack.is_empty() {
        repairs.push(format!("auto_closed_{}_eof_containers", stack.len()));
        while let Some(container) = stack.pop() {
            match container {
                Container::Object => out.push('}'),
                Container::Array => out.push(']'),
            }
        }
    }

    Ok((out, repairs))
}

fn skip_whitespace_and_comments(chars: &[char], pos: &mut usize, repairs: &mut Vec<String>) {
    let len = chars.len();
    while *pos < len {
        let c = chars[*pos];
        if c.is_whitespace() {
            *pos += 1;
            continue;
        }

        // Single-line comment //
        if c == '/' && *pos + 1 < len && chars[*pos + 1] == '/' {
            *pos += 2;
            while *pos < len && chars[*pos] != '\n' {
                *pos += 1;
            }
            repairs.push("stripped_line_comment".to_string());
            continue;
        }

        // Multi-line comment /* ... */
        if c == '/' && *pos + 1 < len && chars[*pos + 1] == '*' {
            *pos += 2;
            while *pos + 1 < len && !(chars[*pos] == '*' && chars[*pos + 1] == '/') {
                *pos += 1;
            }
            if *pos + 1 < len {
                *pos += 2;
            } else {
                *pos = len;
            }
            repairs.push("stripped_block_comment".to_string());
            continue;
        }

        break;
    }
}

fn parse_object_key(
    chars: &[char],
    pos: &mut usize,
    out: &mut String,
    state: &mut LexState,
    repairs: &mut Vec<String>,
) -> Result<(), AstRepairError> {
    let len = chars.len();
    if *pos >= len {
        return Ok(());
    }

    let c = chars[*pos];

    if c == '"' || c == '\'' {
        // Quoted string key
        parse_string(chars, pos, out, c, repairs);
        *state = LexState::ExpectColon;
    } else {
        // Unquoted key (e.g. foo: 1, device_name: "light", step-1: 2)
        let key_start = *pos;
        while *pos < len {
            let ch = chars[*pos];
            if ch.is_whitespace() || ch == ':' || ch == '=' || ch == ',' || ch == '}' {
                break;
            }
            *pos += 1;
        }
        let key_raw: String = chars[key_start..*pos].iter().collect();
        let key_trimmed = key_raw.trim();
        if key_trimmed.is_empty() {
            // Fallback key if completely empty
            out.push_str("\"unknown_key\"");
        } else {
            out.push('"');
            out.push_str(key_trimmed);
            out.push('"');
            repairs.push(format!("quoted_unquoted_key_{key_trimmed}"));
        }
        *state = LexState::ExpectColon;
    }

    Ok(())
}

fn parse_value(
    chars: &[char],
    pos: &mut usize,
    out: &mut String,
    stack: &mut Vec<Container>,
    state: &mut LexState,
    repairs: &mut Vec<String>,
) -> Result<(), AstRepairError> {
    let len = chars.len();
    if *pos >= len {
        // EOF when expecting value -> insert null
        out.push_str("null");
        repairs.push("inserted_null_at_eof".to_string());
        *state = LexState::ExpectCommaOrClose;
        return Ok(());
    }

    let c = chars[*pos];

    match c {
        '{' => {
            out.push('{');
            stack.push(Container::Object);
            *pos += 1;
            *state = LexState::ExpectKeyOrCloseObject;
        }
        '[' => {
            out.push('[');
            stack.push(Container::Array);
            *pos += 1;
            *state = LexState::ExpectValueOrCloseArray;
        }
        '"' | '\'' => {
            parse_string(chars, pos, out, c, repairs);
            *state = LexState::ExpectCommaOrClose;
        }
        '-' | '+' => {
            let remaining: String = chars[*pos..].iter().take(10).collect();
            let rem_lower = remaining.to_lowercase();
            if rem_lower.starts_with("-infinity")
                || rem_lower.starts_with("+infinity")
                || rem_lower.starts_with("-nan")
                || rem_lower.starts_with("+nan")
            {
                parse_literal_or_identifier(chars, pos, out, state, repairs)?;
            } else {
                parse_number(chars, pos, out);
                *state = LexState::ExpectCommaOrClose;
            }
        }
        '0'..='9' | '.' => {
            parse_number(chars, pos, out);
            *state = LexState::ExpectCommaOrClose;
        }
        ',' | '}' | ']' => {
            // Missing value before delimiter -> insert null
            out.push_str("null");
            repairs.push("inserted_missing_null_value".to_string());
            *state = LexState::ExpectCommaOrClose;
        }
        ':' | '=' => {
            // Stray colon or equals in value position (e.g. ":" or "=" or "::")
            repairs.push("skipped_stray_colon_or_equals".to_string());
            *pos += 1;
            if *pos >= len {
                out.push_str("null");
                *state = LexState::ExpectCommaOrClose;
            }
        }
        _ => {
            // Identifier or unquoted literal
            parse_literal_or_identifier(chars, pos, out, state, repairs)?;
        }
    }

    Ok(())
}

fn parse_string(
    chars: &[char],
    pos: &mut usize,
    out: &mut String,
    quote_char: char,
    repairs: &mut Vec<String>,
) {
    let len = chars.len();
    *pos += 1; // skip opening quote
    out.push('"'); // JSON standard always uses double quotes

    if quote_char == '\'' {
        repairs.push("normalized_single_to_double_quotes".to_string());
    }

    let mut is_escaped = false;

    while *pos < len {
        let c = chars[*pos];

        if is_escaped {
            is_escaped = false;
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '/' => out.push('/'),
                'b' => out.push_str("\\b"),
                'f' => out.push_str("\\f"),
                'n' => out.push_str("\\n"),
                'r' => out.push_str("\\r"),
                't' => out.push_str("\\t"),
                '\'' => {
                    if quote_char == '\'' {
                        out.push('\''); // escaped single quote in single-quoted string
                    } else {
                        out.push('\'');
                    }
                }
                'u' => {
                    out.push_str("\\u");
                }
                other => {
                    // Non-standard escape (e.g. \a, \x, Windows path backslash C:\test)
                    out.push_str("\\\\");
                    out.push(other);
                    repairs.push("escaped_invalid_backslash".to_string());
                }
            }
            *pos += 1;
            continue;
        }

        if c == '\\' {
            is_escaped = true;
            *pos += 1;
            continue;
        }

        // Control characters normalization
        if c == '\n' {
            out.push_str("\\n");
            repairs.push("escaped_literal_newline".to_string());
            *pos += 1;
            continue;
        }
        if c == '\r' {
            out.push_str("\\r");
            repairs.push("escaped_literal_carriage_return".to_string());
            *pos += 1;
            continue;
        }
        if c == '\t' {
            out.push_str("\\t");
            repairs.push("escaped_literal_tab".to_string());
            *pos += 1;
            continue;
        }

        // Quote handling
        if c == quote_char {
            // Check for apostrophe in single quote mode: e.g. 'it's', 'don't', 'LIVA's'
            if quote_char == '\'' && *pos + 1 < len && chars[*pos + 1].is_alphabetic() {
                out.push('\'');
                repairs.push("preserved_interior_apostrophe".to_string());
                *pos += 1;
                continue;
            }

            // Check for unescaped interior double quote in double quote mode
            if quote_char == '"' {
                // Lookahead: does the token following this quote look like a delimiter?
                let mut lookahead = *pos + 1;
                while lookahead < len && (chars[lookahead].is_whitespace() || chars[lookahead] == '\n') {
                    lookahead += 1;
                }
                let is_delimiter = if lookahead < len {
                    let next_ch = chars[lookahead];
                    next_ch == ':' || next_ch == '=' || next_ch == ',' || next_ch == '}' || next_ch == ']'
                } else {
                    true // EOF counts as ending quote
                };

                if !is_delimiter {
                    // Unescaped internal double quote! (e.g. {"msg": "hello "world" here"})
                    out.push_str("\\\"");
                    repairs.push("escaped_unescaped_internal_quote".to_string());
                    *pos += 1;
                    continue;
                }
            }

            // Real closing quote!
            out.push('"');
            *pos += 1;
            return;
        }

        // If double quote inside single-quoted string, escape it
        if c == '"' && quote_char == '\'' {
            out.push_str("\\\"");
            *pos += 1;
            continue;
        }

        out.push(c);
        *pos += 1;
    }

    // Truncated string at EOF -> auto-close quote
    out.push('"');
    repairs.push("auto_closed_unclosed_string_at_eof".to_string());
}

fn parse_number(chars: &[char], pos: &mut usize, out: &mut String) {
    let len = chars.len();
    while *pos < len {
        let c = chars[*pos];
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
            out.push(c);
            *pos += 1;
        } else {
            break;
        }
    }
}

fn parse_literal_or_identifier(
    chars: &[char],
    pos: &mut usize,
    out: &mut String,
    state: &mut LexState,
    repairs: &mut Vec<String>,
) -> Result<(), AstRepairError> {
    let len = chars.len();
    let start = *pos;
    while *pos < len {
        let c = chars[*pos];
        if c.is_whitespace() || c == ',' || c == '}' || c == ']' || c == ':' || c == '=' {
            break;
        }
        *pos += 1;
    }

    let ident_raw: String = chars[start..*pos].iter().collect();
    if ident_raw.is_empty() {
        if *pos < len {
            let ch = chars[*pos];
            *pos += 1;
            out.push('"');
            out.push(ch);
            out.push('"');
            repairs.push(format!("escaped_unexpected_token_{ch}"));
        } else {
            out.push_str("null");
            repairs.push("inserted_null_at_eof".to_string());
        }
        *state = LexState::ExpectCommaOrClose;
        return Ok(());
    }

    let lower = ident_raw.to_lowercase();

    match lower.as_str() {
        "true" => {
            out.push_str("true");
            if ident_raw != "true" {
                repairs.push(format!("normalized_{ident_raw}_to_true"));
            }
        }
        "false" => {
            out.push_str("false");
            if ident_raw != "false" {
                repairs.push(format!("normalized_{ident_raw}_to_false"));
            }
        }
        "none" | "null" | "undefined" | "nan" | "nil" | "+nan" | "-nan" | "infinity" | "+infinity" | "-infinity" => {
            out.push_str("null");
            if ident_raw != "null" {
                repairs.push(format!("normalized_{ident_raw}_to_null"));
            }
        }
        _ => {
            // Bare word treated as string
            out.push('"');
            out.push_str(&ident_raw);
            out.push('"');
            repairs.push(format!("quoted_bare_value_{ident_raw}"));
        }
    }

    *state = LexState::ExpectCommaOrClose;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_json_passthrough() {
        let json_str = r#"{"name": "LIVA", "version": 2, "active": true, "items": [1, 2, 3]}"#;
        let res = repair_json_ast(json_str).expect("Should parse valid JSON");
        assert_eq!(res["name"], "LIVA");
        assert_eq!(res["version"], 2);
        assert_eq!(res["active"], true);
        assert_eq!(res["items"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_trailing_commas() {
        let raw = r#"{"a": 1, "b": [2, 3,], "c": {"d": 4,},}"#;
        let res = repair_json_ast(raw).expect("Should repair trailing commas");
        assert_eq!(res["a"], 1);
        assert_eq!(res["b"], serde_json::json!([2, 3]));
        assert_eq!(res["c"]["d"], 4);
    }

    #[test]
    fn test_single_quotes_and_apostrophes() {
        let raw = r#"{'cmd': 'write_file', 'path': 'doc.txt', 'text': 'it\'s working'}"#;
        let res = repair_json_ast(raw).expect("Should repair single quotes");
        assert_eq!(res["cmd"], "write_file");
        assert_eq!(res["path"], "doc.txt");
        assert_eq!(res["text"], "it's working");
    }

    #[test]
    fn test_unquoted_keys() {
        let raw = r#"{device: "light", state: true, brightness_level: 80, step-id: "s1"}"#;
        let res = repair_json_ast(raw).expect("Should quote unquoted keys");
        assert_eq!(res["device"], "light");
        assert_eq!(res["state"], true);
        assert_eq!(res["brightness_level"], 80);
        assert_eq!(res["step-id"], "s1");
    }

    #[test]
    fn test_unescaped_internal_quotes() {
        let raw = r#"{"title": "The "Great" Gatsby", "status": "ok"}"#;
        let res = repair_json_ast(raw).expect("Should escape internal quotes");
        assert_eq!(res["title"], "The \"Great\" Gatsby");
        assert_eq!(res["status"], "ok");
    }

    #[test]
    fn test_pythonic_literals() {
        let raw = r#"{"is_active": True, "timeout": None, "flag": False, "undefined_val": undefined, "nan_val": NaN}"#;
        let res = repair_json_ast(raw).expect("Should normalize pythonic literals");
        assert_eq!(res["is_active"], true);
        assert_eq!(res["timeout"], Value::Null);
        assert_eq!(res["flag"], false);
        assert_eq!(res["undefined_val"], Value::Null);
        assert_eq!(res["nan_val"], Value::Null);
    }

    #[test]
    fn test_unclosed_brackets_eof_autocompletion() {
        let raw = r#"{"query": "Rust concurrency", "filters": ["async", "tokio""#;
        let res = repair_json_ast(raw).expect("Should auto-complete EOF brackets");
        assert_eq!(res["query"], "Rust concurrency");
        assert_eq!(res["filters"], serde_json::json!(["async", "tokio"]));
    }

    #[test]
    fn test_deeply_nested_unclosed_eof() {
        let raw = r#"{"a": {"b": {"c": [1, 2"#;
        let res = repair_json_ast(raw).expect("Should auto-complete nested structures");
        assert_eq!(res["a"]["b"]["c"], serde_json::json!([1, 2]));
    }

    #[test]
    fn test_missing_commas_between_fields() {
        let raw = r#"{"a": 1 "b": 2 "c": [1 2 3]}"#;
        let res = repair_json_ast(raw).expect("Should insert missing commas");
        assert_eq!(res["a"], 1);
        assert_eq!(res["b"], 2);
        assert_eq!(res["c"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_markdown_and_prose_stripping() {
        let raw = "Chắc chắn rồi! TOOL: 1\nARGS:\n```json\n{\"command\": \"git status\", \"dry_run\": false}\n```";
        let res = repair_json_ast(raw).expect("Should strip markdown and prose");
        assert_eq!(res["command"], "git status");
        assert_eq!(res["dry_run"], false);
    }

    #[test]
    fn test_comments_removal() {
        let raw = r#"
        // Configuration
        {
            /* Base settings */
            "host": "localhost",
            "port": 8080 // default port
        }
        "#;
        let res = repair_json_ast(raw).expect("Should strip comments");
        assert_eq!(res["host"], "localhost");
        assert_eq!(res["port"], 8080);
    }

    #[test]
    fn test_speed_performance_budget() {
        let raw = r#"
        {
            action: 'execute_query',
            params: {
                sql: "SELECT * FROM users WHERE note = "vip" AND active = True",
                limit: 50,
                filters: ['admin', 'manager',],
                timeout: None,
            },
            retry_count: 3
        "#;
        // Warm-up
        for _ in 0..20 {
            let _ = repair_json_ast(raw);
        }

        let iterations = 100;
        let start = Instant::now();
        for _ in 0..iterations {
            let val = repair_json_ast(raw).expect("Should repair complex payload");
            assert_eq!(val["action"], "execute_query");
        }
        let total_micros = start.elapsed().as_micros() as f64;
        let avg_micros = total_micros / iterations as f64;
        println!("Average repair execution time: {:.2} µs", avg_micros);
        assert!(
            avg_micros < 100.0,
            "Average repair latency was {:.2} µs (>100 µs SLA)",
            avg_micros
        );
    }

    #[test]
    fn test_broken_json_corpus_success_rate() {
        let corpus = vec![
            r#"{"a": 1,}"#,
            r#"[1, 2, 3,]"#,
            r#"{'key': 'value'}"#,
            r#"{foo: "bar"}"#,
            r#"{"msg": "say "hi""}"#,
            r#"{"nested": {'a': None, 'b': True,}}"#,
            r#"{"truncated": [1, 2, {"x": 3"#,
            r#"{"missing_comma": 1 "next": 2}"#,
            r#"```json\n{"status": "ok"}\n```"#,
            r#"// comment\n{"a": 1}"#,
            r#"{"eq_sign" = "val"}"#,
            r#"{"multi": "line\nbreak"}"#,
            r#"{unquoted_key: 123, list: [1, 2, 3,]}"#,
            r#"{"arr": [1 2 3]}"#,
            r#"{"truncated_str": "hello wo"#,
        ];

        let mut successes = 0;
        let total = corpus.len();
        for sample in &corpus {
            match repair_json_ast(sample) {
                Ok(_) => successes += 1,
                Err(e) => eprintln!("Failed on: {} -> {}", sample, e),
            }
        }

        let rate = (successes as f64) / (total as f64) * 100.0;
        println!("Repair success rate: {:.1}% ({}/{})", rate, successes, total);
        assert!(rate >= 90.0, "Success rate was {:.1}%, expected >= 90%", rate);
    }

    #[test]
    fn test_redundant_consecutive_commas() {
        let raw = r#"{"a": 1,,,, "b": [1,,,, 2,,,,],,,, "c": {"d": "ok",,},,}"#;
        let res = repair_json_ast(raw).expect("Should repair consecutive redundant commas");
        assert_eq!(res["a"], 1);
        assert_eq!(res["b"], serde_json::json!([1, 2]));
        assert_eq!(res["c"]["d"], "ok");
    }

    #[test]
    fn test_infinity_literals_normalization() {
        let raw = r#"{"inf": Infinity, "pos_inf": +Infinity, "neg_inf": -Infinity, "nan": NaN, "pos_nan": +NaN, "neg_nan": -NaN}"#;
        let res = repair_json_ast(raw).expect("Should normalize infinity and nan to null");
        assert_eq!(res["inf"], Value::Null);
        assert_eq!(res["pos_inf"], Value::Null);
        assert_eq!(res["neg_inf"], Value::Null);
        assert_eq!(res["nan"], Value::Null);
        assert_eq!(res["pos_nan"], Value::Null);
        assert_eq!(res["neg_nan"], Value::Null);
    }

    #[test]
    fn test_trailing_prose_after_root_closure() {
        let raw = r#"{"status": "ok", "code": 200} trailing text and ```fence```"#;
        let res = repair_json_ast(raw).expect("Should discard trailing prose after root object");
        assert_eq!(res["status"], "ok");
        assert_eq!(res["code"], 200);
    }
}
