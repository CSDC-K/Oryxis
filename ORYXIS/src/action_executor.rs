// YENİ DOSYA - tüm API modüllerindeki execute mantığını tek yere toplar

use crate::script::{fix_json_multiline_strings, ScriptResponse, ActionType};
use crate::executer::handle_general_execute;
use crate::tts;

pub enum ExecuteResult {
    Output(String),
    NoAction,
    EndCode,
}

/// Tüm API modülleri println yerine bunu çağırır.
/// Hem terminale yazar hem TTS'e gönderir.
pub async fn display_response(content: &str, tts_voice: &str) {
    println!("\nOryxis: {}", content);

    eprintln!("[DEBUG] display_response called, tts_voice='{}', content_len={}", tts_voice, content.len());

    if !tts_voice.is_empty() {
        eprintln!("[DEBUG] calling tts::speak...");
        tts::speak(tts_voice, content).await;
        eprintln!("[DEBUG] tts::speak returned");
    } else {
        eprintln!("[DEBUG] tts_voice is empty, skipping TTS");
    }
}

pub async fn process_ai_response(content: &str) -> ExecuteResult {
    if content.contains("<ENDCODE>") {
        return ExecuteResult::EndCode;
    }

    let Some(json_start) = content.find("```json") else {
        return ExecuteResult::NoAction;
    };

    let after_marker = json_start + 7;
    let Some(json_end) = content[after_marker..].find("```") else {
        eprintln!("[PARSE] No closing ``` found");
        return ExecuteResult::NoAction;
    };

    let json_block = content[after_marker..after_marker + json_end].trim();
    if json_block.is_empty() {
        eprintln!("[PARSE] Empty JSON block");
        return ExecuteResult::NoAction;
    }

    let fixed_json = fix_json_multiline_strings(json_block);

    let action: ScriptResponse = match serde_json::from_str(&fixed_json) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[PARSE] JSON error: {} | raw: {}", e, &fixed_json[..fixed_json.len().min(200)]);
            return ExecuteResult::NoAction;
        }
    };

    if action.action != ActionType::Execute {
        return ExecuteResult::NoAction;
    }

    println!("\n╔════════════════════════════════════════╗");
    println!("║          🚀 EXECUTE                    ║");
    println!("╠════════════════════════════════════════╣");
    for line in action.code.lines().take(5) {
        println!("║  {}", line);
    }
    println!("╚════════════════════════════════════════╝");

    let result = match handle_general_execute(action.code.trim().to_string()).await {
        Ok(r) => r,
        Err(e) => format!("Python Error: {}", e),
    };

    let is_error = result.contains("Python Error:");
    println!("\n╔════════════════════════════════════════╗");
    println!("║  {}  ║", if is_error { "❌ ERROR         " } else { "✅ RESULT        " });
    println!("╠════════════════════════════════════════╣");
    for line in result.lines() { println!("║  {}", line); }
    println!("╚════════════════════════════════════════╝");

    ExecuteResult::Output(result)
}
