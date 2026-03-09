pub mod gemini_api;             // GEMINI API
pub mod groq_api;               // GROQ API
pub mod llmapi;                 // LLMAPI
pub mod executer;               // PYTHON CODE EXECUTER
pub mod script;                 // RESPONSE CATCHER 
pub mod errors;                 // ERROR TYPES
pub mod action_executor;        // ACTION EXECUTOR
pub mod tts;                    // TTS MODULE


use dotenv::dotenv;             // READING .ENV FILE
use std::env;                   // READING .ENV FILE
use tokio;                      // ASYNC PROCESS
use std::fs::File;              // READING PROMPT.TXT
use std::io::{self, Read};      // READING PROMPT.TXT

#[tokio::main]
async fn main() -> Result<(), errors::OryxisError> {
    dotenv().ok();

    // Env Settings
    let api_key = env::var("API_KEY");
    let api_type = env::var("API_TYPE");
    let llm_model = env::var("LLM_MODEL");
    let tts_voice = env::var("TTS").unwrap_or_default();

    let api_key = match api_key {
        Ok(val) => val,
        Err(e) => {
            println!("Error API_KEY: {}", e);
            return Err(errors::OryxisError::WrongApiKey(e.to_string()));
        }
    };

    let api_type = match api_type {
        Ok(val) => val,
        Err(e) => {
            println!("Error API_TYPE: {}", e);
            return Err(errors::OryxisError::ApiTypeError(e.to_string()));
        }
    };

    let llm_model = match llm_model {
        Ok(val) => val,
        Err(e) => {
            println!("Error LLM_MODEL: {}", e);
            return Err(errors::OryxisError::LlmModelError(e.to_string()));
        }
    };


    let mut file = File::open("prompt.md").map_err(|e| errors::OryxisError::PromptFileError(e.to_string()))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| errors::OryxisError::PromptFileError(e.to_string()))?;

    match api_type.as_str() {
        "LLMAPI" => llmapi::llmapi(api_key, contents, llm_model, tts_voice)
            .await
            .map_err(|e| errors::OryxisError::LLMApiRunError(e.to_string()))?,
        "GEMINI" => gemini_api::gemini_api(api_key, contents, llm_model, tts_voice)
            .await
            .map_err(|e| errors::OryxisError::GeminiRunError(e.to_string()))?,
        "GROQ" => groq_api::groq_api(api_key, contents, llm_model, tts_voice)
            .await
            .map_err(|e| errors::OryxisError::GroqRunError(e.to_string()))?,
        _ => return Err(errors::OryxisError::ApiTypeError(format!("API_TYPE_ERROR! -> Not Founded API Type")))
    }

    Ok(())
}
