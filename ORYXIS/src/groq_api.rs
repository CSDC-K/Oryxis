use groq_api_rs::completion::{client::Groq, message::Message, request::builder};
use std::io::{self, Write};
use anyhow;

use crate::script::{fix_json_multiline_strings, ScriptResponse, ActionType};
use crate::executer::{handle_general_execute, handle_fast_execute};


pub async fn run_model(api_key: String, system_prompt: String) -> anyhow::Result<()> {
    let messages = vec![Message::SystemMessage {
        role: Some("system".to_string()),
        content: Some(system_prompt),
        name: None,
        tool_call_id: None,
    }];

    let mut client = Groq::new(api_key.as_str());
    client.add_messages(messages);



    loop {

        let mut userinput : String = String::new();
        print!("\nUSER : ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut userinput).expect("ERROR HANDLED IN LOOP_INPUT");

        let messages = vec![Message::UserMessage {
            role: Some("user".to_string()),
            content: Some(userinput.trim().to_string()),
            name: None,
            tool_call_id: None,
        }];
        client.add_messages(messages);
        let request = builder::RequestBuilder::new("llama-3.3-70b-versatile".to_string());
        let res = client.create(request).await;
        match res {
            Ok(groq_api_rs::completion::client::CompletionOption::NonStream(response)) => {
                if let Some(choice) = response.choices.first() {
                    println!("{}", choice.message.content);

                    // Remove leading/trailing backticks and whitespace

                    if let Some(json_start) = choice.message.content.find("```json") {
                        let after_marker = json_start + 7;
                        if let Some(json_end) = choice.message.content[after_marker..].find("```") {
                            let json_block = choice.message.content[after_marker..after_marker + json_end].trim();
                            let fixed_json = fix_json_multiline_strings(json_block);

                            match serde_json::from_str::<ScriptResponse>(&fixed_json) {
                                Ok(action) if action.action == ActionType::fast_execute => {
                                    let event_code = action.code.trim();
                                    println!("\n\n╔════════════════════════════════════════╗");
                                    println!("║        ⚡ FAST EXECUTE REQUEST         ║");
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Event: {}", event_code);
                                    println!("╚════════════════════════════════════════╝");
                                    let exec_result = handle_fast_execute(event_code);
                                    let is_error = exec_result.contains("\"error\"");

                                    if is_error {
                                        println!("\n╔════════════════════════════════════════╗");
                                        println!("║          EXECUTION ERROR               ║");
                                        println!("╠════════════════════════════════════════╣");
                                        for line in exec_result.lines() {
                                            println!("║  {}", line);
                                        }
                                        println!("╚════════════════════════════════════════╝");
                                    } else {
                                        println!("\n╔════════════════════════════════════════╗");
                                        println!("║        ⚡ FAST EXECUTE RESULT          ║");
                                        println!("╠════════════════════════════════════════╣");
                                        for line in exec_result.lines() {
                                            println!("║  {}", line);
                                        }
                                        println!("╚════════════════════════════════════════╝");
                                    }
                                    println!("END");

                                },
                                Ok(action) if action.action == ActionType::execute => {
                                    let event_code = action.code.trim();
                                    println!("\n\n╔════════════════════════════════════════╗");
                                    println!("║          🚀 EXECUTE REQUEST           ║");
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Code: {}", event_code);
                                    println!("╚════════════════════════════════════════╝");

                                    match handle_general_execute(event_code.to_string()) {
                                        Ok(exec_result) => {
                                            let is_error = exec_result.contains("Python Error:");
                                            if is_error {
                                                println!("\n╔════════════════════════════════════════╗");
                                                println!("║          EXECUTION ERROR               ║");
                                                println!("╠════════════════════════════════════════╣");
                                                for line in exec_result.lines() {
                                                    println!("║  {}", line);
                                                }
                                                println!("╚════════════════════════════════════════╝");
                                            } else {
                                                println!("\n╔════════════════════════════════════════╗");
                                                println!("║          🚀 EXECUTE RESULT            ║");
                                                println!("╠════════════════════════════════════════╣");
                                                for line in exec_result.lines() {
                                                    println!("║  {}", line);
                                                }
                                                println!("╚════════════════════════════════════════╝");
                                            }
                                        },
                                        Err(e) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            println!("║  Error: {}", e);
                                            println!("╚════════════════════════════════════════╝");
                                        }
                                    }

                                    println!("END");

                                },
                                Ok(_) => {
                                    println!("Received JSON with unrecognized action type.");
                                },
                                Err(e) => {
                                    println!("Failed to parse JSON: {}", e);
                                    println!("JSON was: {}", fixed_json);
                                }


                            }
                        }
                    }
                }
            }
            Err(e) => println!("Error: {:?}", e),
            _ => {}
        }

    // Remove Ok(()) here; the loop should not return a value.
}

}