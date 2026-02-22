pub mod gemini_api;             // GEMINI API
pub mod groq_api;               // GROQ API
pub mod executer;               // PYTHON CODE EXECUTER
pub mod script;                 // RESPONSE CATCHER 

use dotenv::dotenv;             // READING .ENV FILE
use std::env;                   // READING .ENV FILE
use tokio;                      // ASYNC PROCESS
use std::fs::File;              // READING PROMPT.TXT
use std::io::{self, Read};      // READING PROMPT.TXT

#[tokio::main]
async fn main() {
    // Env Settings
    dotenv().ok();

    let api_key = env::var("API_KEY");
    let api_type = env::var("API_TYPE");

    let api_key = match api_key {
        Ok(val) => val,
        Err(e) => {
            println!("Error API_KEY: {}", e);
            return;
        }
    };

    let api_type = match api_type {
        Ok(val) => val,
        Err(e) => {
            println!("Error API_KEY: {}", e);
            return;
        }
    };

    let mut file = File::open("prompt.txt").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    groq_api::run_model(api_key, contents).await;


}
