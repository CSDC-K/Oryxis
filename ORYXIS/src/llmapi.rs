use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::io::{self, Write};
use std::fs::File;              // READING PROMPT.TXT
use std::io::Read;              // READING PROMPT.TXT

use crate::script::{fix_json_multiline_strings, ScriptResponse, ActionType};
use crate::executer::{handle_general_execute, handle_fast_execute};
use crate::errors;              // ERROR TYPES

pub async fn llmapi(api_key : String, prompt : String, model : String) -> Result<(), errors::OryxisError> {
    let api_url = "https://internal.llmapi.ai/v1/chat/completions";

    let mut messages = vec![
        json!({"role" : "system", "content" : prompt})
    ];

    loop {
        print!("User : ");
        io::stdout().flush().unwrap();
        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input).unwrap();
        let input = user_input.trim();

        if input == "exit" { return Ok(()); }

        messages.push(json!({"role" : "user", "content" : input}));

        let client = reqwest::Client::new();

        loop {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap());

            let body = json!({
                "model" : model,
                "messages" : messages,
                "temperature" : 0.7
            });

            let res = client.post(api_url).headers(headers).json(&body).send().await.unwrap();

            if res.status().is_success() {
                let res_json: serde_json::Value = res.json().await.unwrap();
                let ai_answer = res_json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("");

                if ai_answer.contains("<ENDCODE>"){
                    break;
                }

                println!("Oryxis : {}", ai_answer);
                messages.push(json!({"role" : "assistant", "content" : ai_answer}));

                if let Some(json_start) = ai_answer.find("```json") {
                    let after_marker = json_start + 7;
                    if let Some(json_end) = ai_answer[after_marker..].find("```") {
                        let json_block = ai_answer[after_marker..after_marker + json_end].trim();
                        let fixed_json = fix_json_multiline_strings(json_block);

                        let exec_output = match serde_json::from_str::<ScriptResponse>(&fixed_json) {
                            Ok(action) if action.action == ActionType::fast_execute => {
                                let event_code = action.code.trim();
                                println!("\n╔════════════════════════════════════════╗");
                                println!("║        ⚡ FAST EXECUTE REQUEST         ║");
                                println!("╠════════════════════════════════════════╣");
                                println!("║ Event: {}", event_code);
                                println!("╚════════════════════════════════════════╝");

                                let result = handle_fast_execute(event_code).await;
                                let is_error = result.contains("\"error\"");

                                if is_error {
                                    println!("\n╔════════════════════════════════════════╗");
                                    println!("║          EXECUTION ERROR               ║");
                                    println!("╠════════════════════════════════════════╣");
                                } else {
                                    println!("\n╔════════════════════════════════════════╗");
                                    println!("║        ⚡ FAST EXECUTE RESULT          ║");
                                    println!("╠════════════════════════════════════════╣");
                                }
                                for line in result.lines() {
                                    println!("║  {}", line);
                                }
                                println!("╚════════════════════════════════════════╝");
                                Some(result)
                            },
                            Ok(action) if action.action == ActionType::execute => {
                                let event_code = action.code.trim();
                                println!("\n╔════════════════════════════════════════╗");
                                println!("║          🚀 EXECUTE REQUEST           ║");
                                println!("╠════════════════════════════════════════╣");
                                println!("║ Code: {}", event_code);
                                println!("╚════════════════════════════════════════╝");

                                let result = match handle_general_execute(event_code.to_string()).await {
                                    Ok(r) => {
                                        let is_error = r.contains("Python Error:");
                                        if is_error {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                        } else {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          🚀 EXECUTE RESULT            ║");
                                            println!("╠════════════════════════════════════════╣");
                                        }
                                        for line in r.lines() {
                                            println!("║  {}", line);
                                        }
                                        println!("╚════════════════════════════════════════╝");
                                        r
                                    },
                                    Err(e) => {
                                        let msg = format!("Error: {}", e);
                                        println!("\n╔════════════════════════════════════════╗");
                                        println!("║          EXECUTION ERROR               ║");
                                        println!("╠════════════════════════════════════════╣");
                                        println!("║  {}", msg);
                                        println!("╚════════════════════════════════════════╝");
                                        msg
                                    }
                                };
                                let body = json!({
                                    "role": "System",
                                    "content": format!("Execution result:\n{}", result)
                                });

                                let mut headers = HeaderMap::new();
                                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                                headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap());
                                client.post(api_url).headers(headers).json(&body).send().await.unwrap();

                                Some(result)
                            },
                            Ok(_) => {
                                println!("Received JSON with unrecognized action type.");
                                None
                            },
                            Err(e) => {
                                println!("Failed to parse JSON: {}", e);
                                println!("JSON was: {}", fixed_json);
                                None
                            }
                        };

                        // Removed erroneous client.add_messages call
                        // If you want to feed execution result back, push to messages and continue loop
                        if let Some(output) = exec_output {
                            messages.push(json!({"role": "system", "content": format!("Execution result:\n{}", output)}));
                            continue;
                        }
                    }
                } else {
                    break;
                }
            } else {
                return Err(errors::OryxisError::LLMApiRunError(res.status().to_string()));
            }
        }
    }
}