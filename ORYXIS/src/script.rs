use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Execute,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptResponse {
    pub action: ActionType,
    pub code: String,
}

pub fn fix_json_multiline_strings(raw: &str) -> String {
    let trimmed = raw.trim();

    // 1) Try direct parse first — if valid, return as-is
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return trimmed.to_string();
    }

    // 2) Find "code" field and fix multiline string
    let mut result = String::new();
    let mut chars = trimmed.chars().peekable();
    let mut inside_code_value = false;
    let mut quote_depth = 0;

    // Strategy: find `"code"` key, then capture everything between the quotes as-is
    // replacing raw newlines with \n
    if let Some(code_start) = find_code_value_start(trimmed) {
        let before = &trimmed[..code_start];
        let after_start = &trimmed[code_start..];

        // Find the opening quote of the code value
        if let Some(quote_pos) = after_start.find('"') {
            let value_start = code_start + quote_pos + 1;
            let rest = &trimmed[value_start..];

            // Find closing quote — must be `"` followed by `}` or `,` (with optional whitespace)
            if let Some(end_pos) = find_code_value_end(rest) {
                let code_content = &rest[..end_pos];
                let after_code = &rest[end_pos..]; // includes closing quote

                // Escape the code content properly
                let escaped = code_content
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "")
                    .replace('\t', "\\t")
                    .replace('"', "\\\"");

                result = format!("{}\"{}{}",
                    before,
                    escaped,
                    after_code
                );

                if serde_json::from_str::<serde_json::Value>(&result).is_ok() {
                    return result;
                }
            }
        }
    }

    // 3) Fallback: line-by-line escape within "code" value
    let mut output = String::new();
    let mut in_code = false;
    let mut prev_was_backslash = false;

    for line in trimmed.lines() {
        let l = line.trim_end();

        if !in_code {
            if l.contains("\"code\"") && l.contains(':') {
                output.push_str(l);
                // Check if code value starts on this line
                if let Some(colon) = l.rfind(':') {
                    let after_colon = l[colon + 1..].trim();
                    if after_colon.starts_with('"') && !after_colon.ends_with('"') {
                        in_code = true;
                    } else if after_colon.starts_with('"') && after_colon.len() > 1
                        && after_colon.ends_with('"') && !after_colon.ends_with("\\\"")
                    {
                        // Single-line code value, already fine
                    }
                }
                output.push('\n');
            } else {
                output.push_str(l);
                output.push('\n');
            }
        } else {
            // Inside multiline code value
            let trimmed_line = l.trim();
            // Check for closing: line ends with `"` followed by optional `}` or `,`
            let is_end = trimmed_line.ends_with("\"}") 
                || trimmed_line.ends_with("\",")
                || trimmed_line == "\""
                || trimmed_line == "\"}"
                ;

            if is_end {
                in_code = false;
                output.push_str(l);
                output.push('\n');
            } else {
                // Escape this line and append as continuation
                let escaped = l
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\t', "\\t");
                output.push_str(&escaped);
                output.push_str("\\n");
            }
        }
    }

    let output = output.trim().to_string();
    if serde_json::from_str::<serde_json::Value>(&output).is_ok() {
        return output;
    }

    // 4) Last resort: return original trimmed
    trimmed.to_string()
}

fn find_code_value_start(s: &str) -> Option<usize> {
    // Find `"code"` then `:` then start of value
    let code_key = s.find("\"code\"")?;
    let after_key = &s[code_key + 6..];
    let colon = after_key.find(':')?;
    Some(code_key + 6 + colon + 1)
}

fn find_code_value_end(s: &str) -> Option<usize> {
    // Find unescaped `"` that's followed by `}` or `,` or whitespace+`}`
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped char
            continue;
        }
        if bytes[i] == b'"' {
            // Check what follows
            let rest = &s[i + 1..].trim_start();
            if rest.starts_with('}') || rest.starts_with(',') || rest.is_empty() {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}