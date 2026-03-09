use groq_api_rs::completion::{client::Groq, message::Message, request::builder};
use std::io::{self, Write};

use crate::action_executor::{process_ai_response, display_response, ExecuteResult};
use crate::errors;


pub async fn groq_api(api_key: String, prompt: String, model_type: String, tts_voice: String) -> Result<(), errors::OryxisError> {
    let mut client = Groq::new(api_key.as_str());
    client.add_messages(vec![Message::SystemMessage {
        role: Some("system".to_string()),
        content: Some(prompt),
        name: None,
        tool_call_id: None,
    }]);

    println!("Oryxis hazır. Çıkmak için 'exit' yazın.\n");

    loop {
        print!("USER: ");
        io::stdout().flush().unwrap();
        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input).expect("Input error");
        let user_input = user_input.trim().to_string();

        if user_input == "exit" {
            return Ok(());
        }

        client.add_messages(vec![Message::UserMessage {
            role: Some("user".to_string()),
            content: Some(user_input),
            name: None,
            tool_call_id: None,
        }]);

        // Agentic inner loop
        loop {
            let request = builder::RequestBuilder::new(model_type.clone());
            let res = client.create(request).await;

            match res {
                Ok(groq_api_rs::completion::client::CompletionOption::NonStream(response)) => {
                    let Some(choice) = response.choices.first() else { break };
                    let content = choice.message.content.clone();

                    display_response(&content, &tts_voice).await;

                    client.add_messages(vec![Message::AssistantMessage {
                        role: Some("assistant".to_string()),
                        content: Some(content.clone()),
                        name: None,
                        tool_call_id: None,
                        tool_calls: None,
                    }]);

                    match process_ai_response(&content).await {
                        ExecuteResult::EndCode => break,
                        ExecuteResult::NoAction => break,
                        ExecuteResult::Output(result) => {
                            client.add_messages(vec![Message::UserMessage {
                                role: Some("user".to_string()),
                                content: Some(format!("Execution result:\n{}", result)),
                                name: None,
                                tool_call_id: None,
                            }]);
                        }
                    }
                }
                Err(e) => { return Err(errors::OryxisError::GroqRunError(format!("{:?}", e))) },
                _ => break,
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}