use std::io::{self, Write};

use groq_api_rs::completion::{client::Groq, message::Message, request::builder};
use anyhow;

pub async fn run_model(api_key: String, system_prompt: String) -> anyhow::Result<()> {
    let messages = vec![Message::UserMessage {
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
        let request = builder::RequestBuilder::new("meta-llama/llama-4-scout-17b-16e-instruct".to_string());
        let res = client.create(request).await;
        match res {
            Ok(groq_api_rs::completion::client::CompletionOption::NonStream(response)) => {
                if let Some(choice) = response.choices.first() {
                    println!("{}", choice.message.content);
                }
            }
            Err(e) => println!("Error: {:?}", e),
            _ => {}
        }       
    }

    Ok(())

}