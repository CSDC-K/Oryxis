use groq_api_rs::completion::{client::Groq, message::Message, request::builder};
use std::io::{self, Write};
use anyhow;

use crate::script::{fix_json_multiline_strings, ScriptResponse, ActionType};
use crate::executer::{handle_general_execute, handle_fast_execute};
use crate::errors;


pub async fn groq_api(api_key: String, prompt: String, model_type: String) -> anyhow::Result<(), errors::OryxisError> {
    let mut client = Groq::new(api_key.as_str());
    client.add_messages(vec![Message::SystemMessage {
        role: Some("system".to_string()),
        content: Some(prompt),
        name: None,
        tool_call_id: None,
    }]);

    loop {
        let mut userinput = String::new();
        print!("\nUSER : ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut userinput).expect("ERROR HANDLED IN LOOP_INPUT");

        client.add_messages(vec![Message::UserMessage {
            role: Some("user".to_string()),
            content: Some(userinput.trim().to_string()),
            name: None,
            tool_call_id: None,
        }]);

        // Agentic inner loop: keep calling model until <ENDCODE>
        loop {
            let request = builder::RequestBuilder::new("llama-3.3-70b-versatile".to_string());
            let res = client.create(request).await;

            match res {
                Ok(groq_api_rs::completion::client::CompletionOption::NonStream(response)) => {
                    if let Some(choice) = response.choices.first() {
                        let content = choice.message.content.clone();
                        println!("{}", content);

                        // Add assistant message to history
                        client.add_messages(vec![Message::AssistantMessage {
                            role: Some("assistant".to_string()),
                            content: Some(content.clone()),
                            name: None,
                            tool_call_id: None,
                            tool_calls: None,
                        }]);

                        // If model signals end, break inner loop
                        if content.contains("<ENDCODE>") {
                            break;
                        }

                        // Try to find and execute a JSON action block
                        if let Some(json_start) = content.find("```json") {
                            let after_marker = json_start + 7;
                            if let Some(json_end) = content[after_marker..].find("```") {
                                let json_block = content[after_marker..after_marker + json_end].trim();
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

                                // Feed execution result back to model
                                if let Some(output) = exec_output {
                                    client.add_messages(vec![Message::UserMessage {
                                        role: Some("user".to_string()),
                                        content: Some(format!("Execution result:\n{}", output)),
                                        name: None,
                                        tool_call_id: None,
                                    }]);
                                }
                            }
                        } else {
                            // No JSON block and no <ENDCODE>, treat as plain response — end inner loop
                            break;
                        }
                    }
                }
                Err(e) => {
                    return Err(errors::OryxisError::GroqRunError(format!("{:?}", e)));
                }
                _ => { break; }
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}