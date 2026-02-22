use groq_api_rs::completion::{client::Groq, message::Message, request::builder};
use std::io::{self, Write};
use anyhow;

use crate::script;



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

                    // Extract JSON substring from the content
                    let content = choice.message.content.clone();
                    let json_start = content.find('{');
                    let json_end = content.rfind('}');
                    let mut sanitized_content = if let (Some(start), Some(end)) = (json_start, json_end) {
                        content[start..=end].to_string()
                    } else {
                        content.trim().to_string()
                    };

                    // Remove leading/trailing backticks and whitespace
                    sanitized_content = sanitized_content.trim_matches('`').trim().to_string();

                    // Do NOT replace newlines or control characters here

                    let lib_response = script::extract_json(sanitized_content).await;
                    println!("LIB RESPONSE : {:?}", lib_response);

                }
            }
            Err(e) => println!("Error: {:?}", e),
            _ => {}
        }       
    }

    Ok(())

}