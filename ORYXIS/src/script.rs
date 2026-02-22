use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    fast_execute,
    execute
}

#[derive(Deserialize, Debug)]
pub struct ScriptResponse {
    pub action: ActionType,
    pub code: String
}

/// Escapes literal newlines and unescaped inner quotes inside JSON string values
pub fn fix_json_multiline_strings(json: &str) -> String {
    if let Some(code_start) = json.find(r#""code":"#) {
        let value_start = code_start + 7;

        let mut actual_start = value_start;
        for (i, ch) in json[value_start..].char_indices() {
            if ch == '"' {
                actual_start = value_start + i + 1;
                break;
            }
        }

        let mut actual_end = actual_start;
        let mut escaped = false;
        for (i, ch) in json[actual_start..].char_indices() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                actual_end = actual_start + i;
                break;
            }
        }

        let code_content = &json[actual_start..actual_end];
        let escaped_code = code_content
            .replace("\r", "")
            .replace("\n", "\\n")
            .replace("\t", "\\t");

        let mut result = String::new();
        result.push_str(&json[..code_start]);
        result.push_str(r#""code":""#);
        result.push_str(&escaped_code);
        result.push('"');
        result.push_str(&json[actual_end + 1..]);

        return result;
    }

    json.to_string()
}