use gemini_rust::{GeminiBuilder, prelude::*};        // GEMINI API MANAGEMENT
use dotenv::dotenv;                                  // READING .ENV FILE
use std::env;                                        // READING .ENV FILE


fn main() {
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
        "Gemini3Pro" => Some(Model::Gemini3Pro),
        "Gemini3Flash" => Some(Model::Gemini3Flash),
        "Gemini25Pro" => Some(Model::Gemini25Pro),
        "Gemini25Flash" => Some(Model::Gemini25Flash),
        _ => None,
    };

    // Model Creation
    let gemini = match model_type {
        Some(model) => GeminiBuilder::new(&api_key)
            .with_model(model).build(),
        None => {
            println!("Invalid MODEL type in .env file");
            return;
        }
    };

    

}
