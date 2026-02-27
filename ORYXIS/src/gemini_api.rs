use gemini_rust::{prelude::*};                       // GEMINI API LIBRARY
use dotenv::dotenv;                                  // READING .ENV FILE
use std::env;                                        // READING .ENV FILE
use std::fs::File;                                   // READING PROMPT.TXT
use std::io::{self, Read, Write};                    // READING PROMPT.TXT
use tokio;                                           // ASYNC PROCESS

use crate::errors;                                   // ERROR TYPES
use crate::script::{fix_json_multiline_strings,      // SCRIPT PARSING
ScriptResponse, ActionType};                         // PYTHON CODE EXECUTER
use crate::executer::{handle_general_execute, 
handle_fast_execute};


async fn generate_response_from_api(            // GENERATE RESPONSE
    temp_val : Option<f32>,                     // CONFIG
    top_p_val : Option<f32>,                    // CONFIG
    top_k_val : Option<i32>,                    // CONFIG
    seed_val : Option<i32>,                     // CONFIG
    message : &String,                          // RESPONSE
    ctx_builder : &mut ContentBuilder           // BUILDER
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
        .clone()
        .with_generation_config(config)
        .with_user_message(message)
        .execute()
        .await;

    response
}


pub async fn gemini_api(api_key: String, prompt: String, model_type: String) -> Result<(), errors::OryxisError> {

    let model_type = match model_type.as_str() {
        "Gemini3Pro" => Model::Gemini3Pro,
        "Gemini3Flash" => Model::Gemini3Flash,
        "Gemini25Pro" => Model::Gemini25Pro,
        "Gemini25Flash" => Model::Gemini25Flash,
        "Gemini25Flashlite" => Model::Gemini25FlashLite,
        _ => Model::Gemini25Flash,
    };

    println!("MODEL NAME : {}", model_type.as_str());

    // Model Creation
    let client = Gemini::with_model(&api_key, model_type).unwrap();
    let contextbuilder = Gemini::generate_content(&client);

    // System Prompt Integration
    let mut contextbuilder = contextbuilder.with_system_prompt(&prompt);
    println!("You can write 'exit' to quit");

    loop {
        let mut userinput : String = String::new();
        print!("\nUSER : ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut userinput).expect("ERROR HANDLED IN LOOP_INPUT");
        userinput = userinput.trim().to_string();

        let result = generate_response_from_api(
            Some(0.7),
            Some(0.9),
            None,
            None,
            &userinput,
            &mut contextbuilder
        ).await;

        match result {
            Ok(succes) => {
                
                
                println!("RESPONSE : {}", succes.text());

                loop {
                    
                    if let Some(json_start) = succes.text().find("```json") {
                        let after_marker = json_start + 7;
                        if let Some(json_end) = succes.text()[after_marker..].find("```") {
                            let succes_text = succes.text();
                            let json_block = succes_text[after_marker..after_marker + json_end].trim();
                            let fixed_json = fix_json_multiline_strings(json_block);

                            let exec_output: Result<String, errors::OryxisError> = match serde_json::from_str::<ScriptResponse>(&fixed_json) {
                                Ok(action) if action.action == ActionType::fast_execute => {
                                    let event_code = action.code.trim();
                                    Ok(handle_fast_execute(event_code).await)
                                },
                                Ok(action) if action.action == ActionType::execute => {
                                    let event_code = action.code.trim();
                                    Ok(handle_general_execute(event_code.to_string()).await.map_err(|e| errors::OryxisError::PyExecutionError(e.to_string()))?)
                                },
                                Ok(_) => Err(errors::OryxisError::JsonParseError("Unknown action type".to_string())),
                                Err(e) => Err(errors::OryxisError::JsonParseError(e.to_string())),
                            };

                            match exec_output {
                                Ok(output) => println!("EXECUTION OUTPUT : {}", output),
                                Err(e) => println!("EXECUTION ERROR : {}", e),
                            }
                        }
                    }

                }

        
        },

            Err(e_) => {
                return Err(errors::OryxisError::GeminiRunError(format!("{:?}", e_)));
            }
        }

    }

}
