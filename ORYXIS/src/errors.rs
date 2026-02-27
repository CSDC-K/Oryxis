use thiserror::Error;

#[derive(Debug, Error)]
pub enum OryxisError {
    #[error("Failed to read prompt.txt file : {0}")]
    PromptFileError(String),

    #[error("Not Foundend Api Type : {0}")]
    ApiTypeError(String),

    #[error("Not Foundend LLM Model : {0}")]
    LlmModelError(String),

    #[error("Wrong API Key : {0}")]
    WrongApiKey(String),

    #[error("Failed to run Json Parse : {0}")]
    JsonParseError(String),

    #[error("Execution Error : {0}")]
    PyExecutionError(String),

    // Groq API Errors
    #[error("Failed to run Groq API : {0}")]
    GroqRunError(String),


    // Gemini API Errors
    #[error("Failed to run Gemini API : {0}")]
    GeminiRunError(String),


    // LLM API Errors
    #[error("Failed to run LLM API : {0}")]
    LLMApiRunError(String),

    #[error("Unknown error occurred : {0}")]
    Unknown(String),
}

