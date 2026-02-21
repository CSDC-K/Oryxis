use gemini_rust::{prelude::*};                       // GEMINI API LIBRARY
use dotenv::dotenv;                                  // READING .ENV FILE
use std::env;                                        // READING .ENV FILE
use std::fs::File;                                   // READING PROMPT.TXT
use std::io::{self, Read};                           // READING PROMPT.TXT
use tokio;                                           // ASYNC PROCESS


async fn generate_response_from_api(            // GENERATE RESPONSE
    temp_val : Option<f32>,                     // CONFIG
    top_p_val : Option<f32>,                    // CONFIG
    top_k_val : Option<i32>,                    // CONFIG
    seed_val : Option<i32>,                     // CONFIG
    message : String,                           // RESPONSE
    ctx_builder : ContentBuilder                // BUILDER
) -> Result<GenerationResponse, ClientError> {

    let config = GenerationConfig {
        temperature: temp_val,
        top_p: top_p_val,
        top_k: top_k_val,
        seed: seed_val,
        // Add other fields if required by GenerationConfig
        ..Default::default()
    };

    let response = ctx_builder
        .with_generation_config(config)
        .with_user_message(message)
        .execute()
        .await;

    response
}

#[tokio::main]
async fn main() {
    // .env Reading
    dotenv().ok(); // Reads the .env file

    let api_key = env::var("API_KEY");
    let model_type = env::var("MODEL").unwrap();

    let api_key = match api_key {
        Ok(val) => val,
        Err(e) => {
            println!("Error API_KEY: {}", e);
            return;
        }
    };

    let model_type = match model_type.as_str() {
        "Gemini3Pro" => Model::Gemini3Pro,
        "Gemini3Flash" => Model::Gemini3Flash,
        "Gemini25Pro" => Model::Gemini25Pro,
        "Gemini25Flash" => Model::Gemini25Flash,
        _ => Model::Gemini25Flash,
    };

    println!("MODEL NAME : {}", model_type.as_str());

    // Model Creation
    let client = Gemini::with_model(&api_key, model_type).unwrap();
    let contextbuilder = Gemini::generate_content(&client);

    // System Prompt Integration
    let mut file = File::open("prompt.txt").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let contextbuilder = contextbuilder.with_system_prompt(&contents);

    
    let result = generate_response_from_api(
        Some(0.7),
        Some(0.9),
        None,
        None,
        "open spotify".to_string(),
        contextbuilder
    ).await;

    match result {
        Ok(succes) => println!("RESPONSE : {}", succes.text()),
        Err(e_) => println!("ERROR : {}", e_)
    }

}
