use gemini_rust::prelude::*;
use std::io::{self, Write};

use crate::errors;
use crate::action_executor::{process_ai_response, display_response, ExecuteResult};

async fn generate_response_from_api(
    temp_val: Option<f32>,
    top_p_val: Option<f32>,
    top_k_val: Option<i32>,
    seed_val: Option<i32>,
    message: &String,
    ctx_builder: &mut ContentBuilder,
) -> Result<GenerationResponse, ClientError> {
    let config = GenerationConfig {
        temperature: temp_val,
        top_p: top_p_val,
        top_k: top_k_val,
        seed: seed_val,
        ..Default::default()
    };
    ctx_builder
        .clone()
        .with_generation_config(config)
        .with_user_message(message)
        .execute()
        .await
}

pub async fn gemini_api(api_key: String, prompt: String, model_type: String, tts_voice: String) -> Result<(), errors::OryxisError> {
    let model = match model_type.as_str() {
        "Gemini3Pro"        => Model::Gemini3Pro,
        "Gemini3Flash"      => Model::Gemini3Flash,
        "Gemini25Pro"       => Model::Gemini25Pro,
        "Gemini25Flash"     => Model::Gemini25Flash,
        "Gemini25Flashlite" => Model::Gemini25FlashLite,
        _                   => Model::Gemini25Flash,
    };

    println!("MODEL: {}", model.as_str());

    let client = Gemini::with_model(&api_key, model)
        .map_err(|e| errors::OryxisError::GeminiRunError(e.to_string()))?;
    let mut ctx_builder = Gemini::generate_content(&client).with_system_prompt(&prompt);

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

        let response = generate_response_from_api(
            Some(0.7), Some(0.9), None, None,
            &user_input, &mut ctx_builder,
        )
        .await
        .map_err(|e| errors::OryxisError::GeminiRunError(format!("{:?}", e)))?;

        let mut current_text = response.text().to_string();

        loop {
            display_response(&current_text, &tts_voice).await;

            match process_ai_response(&current_text).await {
                ExecuteResult::EndCode => break,
                ExecuteResult::NoAction => break,
                ExecuteResult::Output(result) => {
                    let feedback = format!("Execution result:\n{}", result);
                    let next = generate_response_from_api(
                        Some(0.7), Some(0.9), None, None,
                        &feedback, &mut ctx_builder,
                    )
                    .await
                    .map_err(|e| errors::OryxisError::GeminiRunError(format!("{:?}", e)))?;
                    current_text = next.text().to_string();
                }
            }
        }
    }
}
