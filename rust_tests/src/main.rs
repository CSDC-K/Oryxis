pub mod debuginfo;
pub mod executer;

use llama_cpp_2::model::{LlamaModel, Special};
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::context::params::LlamaContextParams;
use std::path::Path;
use std::num::NonZeroU32;
use std::io::{self, Write};
use serde::{Deserialize, Serialize};

use crate::executer::execute_script;


#[derive(Debug, Deserialize, Serialize)]
struct Action {
    action: String,
    code: String,
}


fn fix_json_multiline_strings(json: &str) -> String {
    if let Some(code_start) = json.find(r#""code":"#) {
        let value_start = code_start + 7;
        
        let mut actual_start = value_start;
        for (i, ch) in json[value_start..].char_indices() {
            if ch == '"' {
                actual_start = value_start + i + 1;
                break;
            }
        }
        
        let mut actual_end = actual_start;
        let mut escaped = false;
        for (i, ch) in json[actual_start..].char_indices() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                actual_end = actual_start + i;
                break;
            }
        }
        
        // Only escape REAL newlines/tabs — don't touch existing backslashes
        let code_content = &json[actual_start..actual_end];
        let escaped_code = code_content
            .replace("\r", "")
            .replace("\n", "\\n")
            .replace("\t", "\\t");
        
        let mut result = String::new();
        result.push_str(&json[..code_start]);
        result.push_str(r#""code":""#);
        result.push_str(&escaped_code);
        result.push('"');
        result.push_str(&json[actual_end + 1..]);
        
        return result;
    }
    
    json.to_string()
}

/// Generate tokens from the model until we hit a stop condition.
/// Returns the full generated text and which stop reason was hit.
enum StopReason {
    ExecutionComplete,
    EndCode,
    Eog,
    ContextLimit,
    MaxTokens,
}

fn generate_response(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    batch: &mut LlamaBatch,
    sampler: &mut LlamaSampler,
    n_cur: &mut i32,
    max_tokens: usize,
    print_prefix: Option<&str>,
) -> Result<(String, StopReason), Box<dyn std::error::Error>> {
    if let Some(prefix) = print_prefix {
        print!("{}", prefix);
        io::stdout().flush()?;
    }

    let mut full_response = String::new();

    for _ in 0..max_tokens {
        let last_index = if batch.n_tokens() > 0 { batch.n_tokens() - 1 } else { 0 };
        
        let token = sampler.sample(ctx, last_index);
        sampler.accept(token);

        if model.is_eog_token(token) {
            return Ok((full_response, StopReason::Eog));
        }

        let output_str = model.token_to_str(token, Special::Tokenize)?;
        print!("{}", output_str);
        io::stdout().flush()?;
        full_response.push_str(&output_str);

        if full_response.contains("<EXECUTION_COMPLETE>") {
            return Ok((full_response, StopReason::ExecutionComplete));
        }

        if full_response.contains("<ENDCODE>") {
            return Ok((full_response, StopReason::EndCode));
        }

        batch.clear();
        batch.add(token, *n_cur, &[0], true)?;
        ctx.decode(batch)?;
        *n_cur += 1;

        if *n_cur >= 14000 {
            println!("\n[Context limit approaching - resetting conversation]");
            return Ok((full_response, StopReason::ContextLimit));
        }
    }

    Ok((full_response, StopReason::MaxTokens))
}

fn inject_tokens(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    batch: &mut LlamaBatch,
    n_cur: &mut i32,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tokens = model.str_to_token(text, llama_cpp_2::model::AddBos::Never)?;
    batch.clear();
    for (i, token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        batch.add(*token, *n_cur, &[0], is_last)?;
        *n_cur += 1;
    }
    ctx.decode(batch).map_err(|e| format!("Decode Err: {}", e))?;
    Ok(())
}

fn handle_fast_execute(event_code: &str) -> String {
    let python_code = if event_code.starts_with("_event_CHECKSKILLS->") {
        let filter_part = event_code.trim_start_matches("_event_CHECKSKILLS->").trim();
        let keywords: Vec<&str> = filter_part.split(',').map(|s| s.trim()).collect();
        let keywords_py: Vec<String> = keywords.iter().map(|k| format!("'{}'", k.to_lowercase())).collect();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skills = db.list_skills()
    keywords = [{}]
    filtered = [s for s in skills if any(kw in s.get('name','').lower() or kw in s.get('description','').lower() for kw in keywords)]
    return {{'status': 'success', 'count': len(filtered), 'skills': filtered}}

task()"#,
            keywords_py.join(", ")
        )
    } else if event_code.starts_with("_event_CHECKSKILLS_") {
        r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skills = db.list_skills()
    return {'status': 'success', 'count': len(skills), 'skills': skills}

task()"#.to_string()
    } else if event_code.starts_with("_event_GETSKILL->") {
        let skill_name = event_code.trim_start_matches("_event_GETSKILL->").trim();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skill = db.get_skill('{}')
    if skill:
        return {{'status': 'success', 'skill': skill}}
    return {{'status': 'error', 'message': 'Skill not found: {}'}}

task()"#,
            skill_name, skill_name
        )
    } else if event_code.starts_with("_event_DELETESKILL->") {
        let skill_name = event_code.trim_start_matches("_event_DELETESKILL->").trim();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    db.delete_skill('{}')
    return {{'status': 'success', 'deleted': '{}'}}

task()"#,
            skill_name, skill_name
        )
    } else if event_code.starts_with("cmdlib.run_command") {
        format!(
r#"import cmdlib

def task():
    result = {}
    return {{'status': 'success', 'result': result}}

task()"#,
            event_code
        )
    } else {
        return format!("{{\"status\": \"error\", \"message\": \"Unknown fast_execute event: {}\"}}", event_code);
    };

    match execute_script(python_code) {
        Ok(result) => result,
        Err(e) => format!("{{\"status\": \"error\", \"message\": \"{}\"}}", e),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = llama_cpp_2::llama_backend::LlamaBackend::init().expect("Backend Err");
    let model_path = r"C:\Users\kuzeybabag\.lmstudio\models\bartowski\DeepSeek-R1-Distill-Qwen-32B-GGUF\DeepSeek-R1-Distill-Qwen-32B-Q4_K_S.gguf";
    let mut model_params = LlamaModelParams::default();
    model_params = model_params.with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, Path::new(model_path), &model_params).expect("Model Err");

    // Optimized context params for RX 7700XT + 32GB DDR5
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(16384))
        .with_n_batch(2048)
        .with_n_ubatch(512);
    let mut ctx = model.new_context(&backend, ctx_params).expect("Ctx Err");
    
    let mut n_cur: i32 = 0;
    let mut batch = LlamaBatch::new(16384, 1);

let system_prompt = r#"You are ORYXIS — an advanced AI assistant inspired by JARVIS from Iron Man. You serve your user, Kuzey, who is a senior-level software engineer.

═══════════════════════════════════════════════════════════════
                    MANDATORY EXECUTION RULES
═══════════════════════════════════════════════════════════════

RULE 0 — SMART MEMORY ACCESS:
You have long-term memory (skills database). Use it INTELLIGENTLY:
• Do ONE memory check at START of conversation to see what skills exist
• After initial check, do NOT re-check unless you saved a new skill
• Search skill DESCRIPTIONS, not just names. "open spotify" → search descriptions for "open", "application", "launcher"
• If you already checked and know what skills exist, USE them directly
• ALWAYS use fast_execute (_event_ commands) for memory reads. NEVER write Python to read skills

RULE 0.5 — CONFIRMATION ONLY FOR DESTRUCTIVE ACTIONS:
Ask confirmation ONLY before DELETE/MODIFY/IRREVERSIBLE actions:
• Deleting files/skills, modifying system settings, installing/uninstalling software

Do NOT ask confirmation for:
• Opening applications, reading files, listing directories
• Memory lookups (CHECKSKILLS, GETSKILL)
• Saving new skills (additive, not destructive)
• Any read-only operations
• When user said "just do it" / "go ahead"

JUST ACT. Don't narrate. Execute.

RULE 1 — WRAPPED EXECUTION (CRITICAL):
Your Python code runs inside exec(). A bare return at global scope causes SyntaxError.
• EVERY piece of logic MUST be wrapped in a function (def task():)
• LAST line MUST call that function (task())
• NEVER write return at top-level scope

✅ CORRECT:
def task():
    result = 2 + 2
    return {"status": "ok", "result": result}
task()

❌ FATAL:
result = 2 + 2
return result

RULE 2 — NO PRINT, ONLY RETURN:
NEVER use print() for output. System captures return value. Always return results from inside a function.

RULE 3 — JSON ACTION PROTOCOL:
When executing code, emit a fenced JSON block with REAL multi-line code (not escaped \n):

✅ CORRECT:
```json
{
  "action": "execute",
  "code": "
def task():
    import os
    files = os.listdir('.')
    return {'status': 'success', 'files': files}

task()
"
}
```
<EXECUTION_COMPLETE>

Critical JSON rules:
• Fence with ```json and ```
• <EXECUTION_COMPLETE> MUST appear immediately after closing ```
• "action" is "execute" for Python execution
• "code" starts with newline, contains REAL line breaks, ends with newline
• Use 4-space indentation
• NEVER escape newlines as \n — write ACTUAL line breaks

RULE 3.5 — FAST_EXECUTE (RAPID EVENT PROTOCOL):
FAST_EXECUTE = high-priority reflex mode. Use _event_ commands instead of Python.
PREFER fast_execute over normal execute for memory operations.

FORMAT:
• "action" MUST be "fast_execute"
• "code" contains ONLY _event_ command string (NO Python code)
• Same JSON fencing rules apply

AVAILABLE EVENTS:
_event_CHECKSKILLS_                          → Returns all saved skills
_event_CHECKSKILLS-> keyword1, keyword2      → Filtered skills by keywords
_event_GETSKILL-> skill_name                 → Returns specific skill dict
_event_DELETESKILL-> skill_name              → Deletes skill

EXAMPLE:
```json
{
  "action": "fast_execute",
  "code": "
_event_CHECKSKILLS-> open, application
"
}
```
<EXECUTION_COMPLETE>

WHEN TO USE:
• Memory/skills check → ALWAYS fast_execute
• Get specific skill → ALWAYS fast_execute
• Delete skill → ALWAYS fast_execute
• Python logic → normal execute
• Save new skills → normal execute

RULE 4 — ONE ACTION PER RESPONSE:
Maximum one JSON action block per response. Wait for result before next step.

RULE 5 — FLOW CONTROL:
<EXECUTION_COMPLETE> — Emit IMMEDIATELY after ```json``` block
  System executes code and injects result back to you
  You then CONTINUE generating (comment on result, take next steps)

<ENDCODE> — Emit when COMPLETELY DONE with response
  Returns control to user for next input
  MUST end EVERY response with <ENDCODE>

FLOW EXAMPLE:
"On it."
```json
{ "action": "execute", "code": "..." }
```
<EXECUTION_COMPLETE>
[system injects result, you continue]
"42 files found. <ENDCODE>"

═══════════════════════════════════════════════════════════════
                        CODING STANDARDS
═══════════════════════════════════════════════════════════════

FUNCTION-FIRST ARCHITECTURE:
import ...

def task():
    ...
    return result

task()

RETURN STRUCTURED DATA:
✅ return {"status": "success", "files": ["a.txt"]}
❌ return "done"

ERROR HANDLING:
def task():
    try:
        return {"status": "success", "data": result}
    except Exception as e:
        return {"status": "error", "message": str(e)}
task()

═══════════════════════════════════════════════════════════════
                      SKILL ENGINEERING
═══════════════════════════════════════════════════════════════

MEMORY SYSTEM:
import memory
db = memory.OryxisMemory("./mydb")
db.save_skill(name, description, code)
db.get_skill(name)
db.list_skills()
db.delete_skill(name)

PRINCIPLES:
• GENERALIZE: "open YouTube" → create open_url(url) or open_application(name), NOT open_youtube()
• Skill code = self-contained function definition (no invocation in stored code)
• CHECK BEFORE BUILD: Use fast_execute to check existing skills before creating new ones

═══════════════════════════════════════════════════════════════
                         PERSONALITY
═══════════════════════════════════════════════════════════════

You are ORYXIS — JARVIS reborn. Calm, precise, warm, dry wit, fiercely competent.

COMMUNICATION STYLE:
• CONCISE. Maximum efficiency. No filler.
• ACT first, talk later. Don't announce — just do it.
• Natural: "Right away, sir.", "Done.", "On it."
• Never robotic: no "Initiating...", "Processing...", "Affirmative."
• Kuzey is senior engineer — be his equal partner
• Summarize results, don't dump raw data
• One-line responses when sufficient

BAD: "Certainly! I'd be happy to help you with that. Let me check..."
GOOD: [checks memory, acts] "Spotify's up, sir."

THINKING PROTOCOL:
When you need to reason through something, use <think> tags to organize your thoughts BEFORE acting:
<think>
User wants X. I should check memory first, then execute Y.
</think>
[then act immediately]

Keep thinking brief and focused. Don't overthink simple tasks.

═══════════════════════════════════════════════════════════════
PRIORITY ORDER: MANDATORY RULES > CODING STANDARDS > SKILL ENGINEERING > PERSONALITY

Critical summary:
• Function-wrapping (RULE 1) = MOST CRITICAL
• fast_execute for memory = ALWAYS
• Multi-line code (RULE 3) = real line breaks, never \n escapes
• Confirmation (RULE 0.5) = ONLY for destructive actions
• <ENDCODE> (RULE 5) = every response MUST end with it
• Be brief. Act, don't talk.

You are ORYXIS. Serve with precision, warmth, and excellence.
"#;

    let tokens = model.str_to_token(system_prompt, llama_cpp_2::model::AddBos::Always)?;
    println!("[System prompt tokens: {}]", tokens.len());
    
    // Feed system prompt in chunks matching n_batch size
    let chunk_size = 2048;
    let chunks: Vec<&[llama_cpp_2::token::LlamaToken]> = tokens.chunks(chunk_size).collect();
    let total_chunks = chunks.len();
    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let is_last = chunk_idx == total_chunks - 1 && i == chunk.len() - 1;
            batch.add(*token, n_cur, &[0], is_last)?;
            n_cur += 1;
        }
        print!("\r[Processing system prompt: chunk {}/{} ({} tokens)]", chunk_idx + 1, total_chunks, chunk.len());
        io::stdout().flush()?;
        ctx.decode(&mut batch)?;
    }
    println!("\n[System prompt loaded successfully]");

    // Faster sampling: min_p + temperature for better speed/quality tradeoff
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.6),
        LlamaSampler::min_p(0.05, 1),
        LlamaSampler::dist(42),
    ]);
    println!("╔════════════════════════════════════════╗");
    println!("║     ORYXIS ONLINE - AT YOUR SERVICE    ║");
    println!("╚════════════════════════════════════════╝");

    loop {
        print!("\n┌─[User]\n└─> ");
        io::stdout().flush()?;
        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;

        if user_input.trim().is_empty() {
            continue;
        }

        let formatted_user = format!("User: {}\n\nAssistant:", user_input.trim());
        inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &formatted_user)?;

        print!("\n┌─[ORYXIS]\n└─> ");
        io::stdout().flush()?;

        loop {
            let (full_response, stop) = generate_response(
                &model, &mut ctx, &mut batch, &mut sampler, &mut n_cur, 2048, None,
            )?;

            match stop {
                StopReason::ExecutionComplete => {
                    if let Some(json_start) = full_response.find("```json") {
                        let after_marker = json_start + 7;
                        if let Some(json_end) = full_response[after_marker..].find("```") {
                            let json_block = full_response[after_marker..after_marker + json_end].trim();
                            let fixed_json = fix_json_multiline_strings(json_block);

                            match serde_json::from_str::<Action>(&fixed_json) {
                                Ok(action) if action.action == "fast_execute" => {
                                    let event_code = action.code.trim();
                                    println!("\n\n╔════════════════════════════════════════╗");
                                    println!("║        ⚡ FAST EXECUTE REQUEST         ║");
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Event: {}", event_code);
                                    println!("╚════════════════════════════════════════╝");

                                    let result_prompt = match execute_script(event_code.to_string()) {
                                        Ok(result) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║        ⚡ FAST EXECUTE RESULT          ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_RESULT]:\n{}\n\nAssistant:",
                                                result
                                            )
                                        }
                                        Err(e) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in e.to_string().lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_ERROR]:\n{}\n\nAssistant:",
                                                e
                                            )
                                        }
                                    };

                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                }
                                Ok(action) => {
                                    println!("\n\n╔════════════════════════════════════════╗");
                                    println!("║          EXECUTION REQUESTED           ║");
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Action: {}", action.action);
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Code:");
                                    for line in action.code.lines() {
                                        println!("║   {}", line);
                                    }
                                    println!("╚════════════════════════════════════════╝");

                                    let full_code: String = action.code.lines().collect::<Vec<_>>().join("\n");

                                    let result_prompt = match execute_script(full_code) {
                                        Ok(result) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION RESULT              ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_RESULT]:\n{}\n\nAssistant:",
                                                result
                                            )
                                        }
                                        Err(e) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in e.to_string().lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_ERROR]:\n{}\n\nAssistant:",
                                                e
                                            )
                                        }
                                    };

                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                }
                                Err(e) => {
                                    println!("\n[JSON parse error: {}]", e);
                                    println!("[Raw JSON block:\n{}]", json_block);
                                    break;
                                }
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                StopReason::EndCode => {
                    break;
                }
                StopReason::Eog | StopReason::ContextLimit | StopReason::MaxTokens => {
                    if matches!(stop, StopReason::MaxTokens) {
                        println!("\n[Response truncated]");
                    }
                    break;
                }
            }
        }

        println!();
    }
}