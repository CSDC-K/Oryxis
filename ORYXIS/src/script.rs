use serde::Deserialize;
use regex;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum ActionType {
    fast_execute,
    execute
}

#[derive(Deserialize, Debug)]
struct ScriptResponse {
    #[serde(rename = "action")]
    action_type: ActionType,
    code: String
}

pub async fn extract_json(response : String) -> String {
    let start_tag = "```json";
    let end_tag = "```";

    let mut json_content = None;

    if let Some(start_pos) = response.find(start_tag) {
        let content_start = start_pos + start_tag.len();
        if let Some(end_pos) = response[content_start..].find(end_tag) {
            let content = &response[content_start..content_start + end_pos];
            json_content = Some(content.trim().to_string());
        }
    }

    // Fallback: extract between first '{' and last '}'
    if json_content.is_none() {
        if let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) {
            let content = &response[start..=end];
            json_content = Some(content.trim().to_string());
        }
    }

    if let Some(ref mut json_str) = json_content {
        // Escape literal newlines inside JSON string values
        *json_str = escape_newlines_in_json_strings(json_str);

        match serde_json::from_str::<ScriptResponse>(json_str) {
            Ok(script_response) => {
                println!("Action Type: {:?}", script_response.action_type);
                println!("Code: {}", script_response.code);
                return json_str.clone();
            },
            Err(e) => {
                println!("Failed to parse JSON: {}", e);
                println!("JSON was: {}", json_str);
            }
        }
    }

    String::from("NONE")
}

/// Escapes literal newlines inside JSON string values (between unescaped quotes)
fn escape_newlines_in_json_strings(input: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Push the escape and the next character as-is
                result.push(c);
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            '"' => {
                in_string = !in_string;
                result.push(c);
            }
            '\n' if in_string => result.push_str("\\n"),
            '\r' if in_string => result.push_str("\\r"),
            '\t' if in_string => result.push_str("\\t"),
            _ => result.push(c),
        }
    }

    result
}