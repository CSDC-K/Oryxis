use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::io::{self, Write};

use crate::action_executor::{process_ai_response, display_response, ExecuteResult};
use crate::errors;

pub async fn llmapi(api_key: String, prompt: String, model: String, tts_voice: String) -> Result<(), errors::OryxisError> {
    let api_url = "https://internal.llmapi.ai/v1/chat/completions";
    let client = reqwest::Client::new();

    let mut messages = vec![
        json!({"role": "system", "content": prompt})
    ];

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

        messages.push(json!({"role": "user", "content": user_input}));

        // Agentic inner loop
        loop {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| errors::OryxisError::LLMApiRunError(e.to_string()))?,
            );

            let body = json!({
                "model": model,
                "messages": messages,
                "temperature": 0.7
            });

            let res = client.post(api_url).headers(headers).json(&body).send().await
                .map_err(|e| errors::OryxisError::LLMApiRunError(e.to_string()))?;

            if !res.status().is_success() {
                return Err(errors::OryxisError::LLMApiRunError(res.status().to_string()));
            }

            let res_json: serde_json::Value = res.json().await
                .map_err(|e| errors::OryxisError::LLMApiRunError(e.to_string()))?;

            let ai_answer = res_json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            display_response(&ai_answer, &tts_voice).await;
            messages.push(json!({"role": "assistant", "content": ai_answer}));

            match process_ai_response(&ai_answer).await {
                ExecuteResult::EndCode => break,
                ExecuteResult::NoAction => break,
                ExecuteResult::Output(result) => {
                    messages.push(json!({
                        "role": "system",
                        "content": format!("Execution result:\n{}", result)
                    }));
                }
            }
        }
    }
}