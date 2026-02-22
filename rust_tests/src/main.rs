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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

enum StopReason {
    ExecutionComplete,
    EndCode,
    Eog,
    ContextLimit,
    MaxTokens,
    Interrupted,
}

fn generate_response(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    batch: &mut LlamaBatch,
    sampler: &mut LlamaSampler,
    n_cur: &mut i32,
    max_tokens: usize,
    print_prefix: Option<&str>,
    interrupt_flag: Arc<AtomicBool>,
) -> Result<(String, StopReason), Box<dyn std::error::Error>> {
    if let Some(prefix) = print_prefix {
        print!("{}", prefix);
        io::stdout().flush()?;
    }

    let mut full_response = String::new();
    let exec_tag = "<EXECUTION_COMPLETE>";
    let end_tag = "<ENDCODE>";
    let json_close = "```";

    for _ in 0..max_tokens {
        // q tuşu interrupt kontrolü (non-blocking)
        if interrupt_flag.load(Ordering::Relaxed) {
            interrupt_flag.store(false, Ordering::Relaxed);
            println!("\n[Response interrupted by user]");
            return Ok((full_response, StopReason::Interrupted));
        }

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

        let tail_len = 30.min(full_response.len());
        let tail = &full_response[full_response.len() - tail_len..];

        if tail.contains(exec_tag) {
            return Ok((full_response, StopReason::ExecutionComplete));
        }
        if tail.contains(end_tag) {
            return Ok((full_response, StopReason::EndCode));
        }

        // A şıkkı: JSON bloğu ``` ile kapandı ama EXECUTION_COMPLETE gelmedi
        // Sadece ```json içeren bir response'da ``` görürsek kontrol et
        if tail.ends_with(json_close) && full_response.contains("```json") {
            let mut lookahead = String::new();
            for _ in 0..10 {
                if interrupt_flag.load(Ordering::Relaxed) {
                    break;
                }
                batch.clear();
                let ltoken = sampler.sample(ctx, 0);
                sampler.accept(ltoken);
                if model.is_eog_token(ltoken) {
                    break;
                }
                let ls = model.token_to_str(ltoken, Special::Tokenize)?;
                print!("{}", ls);
                io::stdout().flush()?;
                lookahead.push_str(&ls);
                full_response.push_str(&ls);
                batch.add(ltoken, *n_cur, &[0], true)?;
                ctx.decode(batch)?;
                *n_cur += 1;
                if lookahead.contains(exec_tag) {
                    return Ok((full_response, StopReason::ExecutionComplete));
                }
                if lookahead.contains(end_tag) {
                    return Ok((full_response, StopReason::EndCode));
                }
            }
            // Hala gelmediyse zorla EXECUTION_COMPLETE inject et
            if !lookahead.contains(exec_tag) && !lookahead.contains(end_tag) {
                full_response.push_str("\n<EXECUTION_COMPLETE>");
                println!("\n[Auto-injected EXECUTION_COMPLETE]");
                return Ok((full_response, StopReason::ExecutionComplete));
            }
        }

        batch.clear();
        batch.add(token, *n_cur, &[0], true)?;
        ctx.decode(batch)?;
        *n_cur += 1;

        if *n_cur >= 7000 {
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

fn similarity_ratio(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let max_len = a_chars.len().max(b_chars.len());
    if max_len == 0 {
        return 1.0;
    }
    let common = a_chars.iter().zip(b_chars.iter()).filter(|(x, y)| x == y).count();
    common as f64 / max_len as f64
}

fn is_duplicate_code(history: &[String], new_code: &str, threshold: f64) -> bool {
    history.iter().any(|prev| similarity_ratio(prev.trim(), new_code.trim()) > threshold)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = llama_cpp_2::llama_backend::LlamaBackend::init().expect("Backend Err");
    let model_path = r"C:\Users\kuzeybabag\.lmstudio\models\lmstudio-community\Qwen2.5-Coder-32B-GGUF\Qwen2.5-Coder-32B-Q4_K_M.gguf";
    let mut model_params = LlamaModelParams::default();
    model_params = model_params.with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, Path::new(model_path), &model_params).expect("Model Err");

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(8192))
        .with_n_batch(2048)
        .with_n_ubatch(512);
    let mut ctx = model.new_context(&backend, ctx_params).expect("Ctx Err");

    let mut n_cur: i32 = 0;
    let mut batch = LlamaBatch::new(8192, 1);

    let system_prompt = r#"<start_of_turn>user
# ORYXIS — SYSTEM CORE

You are ORYXIS, modeled after J.A.R.V.I.S. You serve Kuzey ("sir"). Be calm, precise, warm, brief. Respond in the user's language. Act first, explain after.

---

You are the assistant.
You NEVER generate user messages.
You NEVER simulate dialogue.
You ONLY produce a single assistant response per turn.
If you start generating a user message, STOP immediately.

## OUTPUT FORMAT — OBEY EXACTLY

Every response fits ONE of these two templates:

### Template A — Conversation (no code needed):
```
Your reply here. <ENDCODE>
```

### Template B — Code execution:
```
Optional short sentence.
```json
{
  "action": "execute",
  "code": "
def task():
    return {'status': 'success', 'data': 42}

task()
"
}
```
<EXECUTION_COMPLETE>
Summary of result. <ENDCODE>
```

### THREE LAWS (never violate):
1. ```json block → <EXECUTION_COMPLETE> IMMEDIATELY after closing ```. Nothing between them.
2. Every response MUST end with <ENDCODE>. Without it the system loops forever.
3. One JSON block per response. Wait for result before next.

---

## CODE RULES

- ALL code inside "code" field uses REAL newlines and REAL spaces. Never \n or \t literals.
- ALL logic wrapped in a function. Last line calls it. Never bare return.
- Never use print(). Always return dicts/lists from the function.
- Always try/except:
```
def task():
    try:
        ...
        return {"status": "success", "data": result}
    except Exception as e:
        return {"status": "error", "message": str(e)}

task()
```

---

## INTENT DETECTION

Before acting, classify the user's message:

**CONVERSATION** → greetings, chat, opinions, emotions, announcements → respond naturally, no code, end with <ENDCODE>
**ACTION** → explicit commands, task requests, skill operations → execute code

If ambiguous, default to conversation.

---

## EXECUTION TYPES

### fast_execute — for memory events and cmdlib:
```json
{"action": "fast_execute", "code": "\n_event_CHECKSKILLS_\n"}
```
<EXECUTION_COMPLETE>

Memory events: `_event_CHECKSKILLS_` | `_event_CHECKSKILLS-> kw1, kw2` | `_event_GETSKILL-> name` | `_event_DELETESKILL-> name`

cmdlib (apps): `cmdlib.run_command("cmd", ["/C", "start", "spotify://"])`
Works for: spotify://, discord://, steam://, URLs, "code" for VS Code, .exe paths.

### execute — for custom Python code (use the function template above)

---

## SKILLS / MEMORY

```python
import memory
db = memory.OryxisMemory("./mydb")
db.save_skill(name, description, code)
db.get_skill(name) / db.list_skills() / db.delete_skill(name)
```

- Check skills once at conversation start via fast_execute. Don't recheck unless you saved a new one.
- For cmdlib/self-skills, skip memory — act directly.
- Search skill descriptions, not just names.

---

## DECISION FLOW (pick fastest path):
1. Conversation? → Talk. <ENDCODE>
2. Self-skill (cmdlib)? → fast_execute immediately.
3. Saved skill might exist? → fast_execute check → use it.
4. New task? → Write code → execute → optionally save.

Only confirm before destructive/irreversible actions. Otherwise just do it.

You are ORYXIS. Systems online, sir.
<end_of_turn>
<start_of_turn>model
"#;

    let tokens = model.str_to_token(system_prompt, llama_cpp_2::model::AddBos::Always)?;
    println!("[System prompt tokens: {}]", tokens.len());

    let chunk_size = 2048;
    let chunks: Vec<&[llama_cpp_2::token::LlamaToken]> = tokens.chunks(chunk_size).collect();
    let total_chunks = chunks.len();
    let prompt_start = std::time::Instant::now();
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
    let prompt_elapsed = prompt_start.elapsed();
    println!("\n[System prompt loaded in {:.1}s ({:.0} tokens/sec)]",
        prompt_elapsed.as_secs_f64(),
        tokens.len() as f64 / prompt_elapsed.as_secs_f64()
    );
    println!("[Initial context tokens: {}]", n_cur);

    // D şıkkı: hafif temp ayarı, JARVIS karakteri korunuyor
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.55),
        LlamaSampler::min_p(0.05, 1),
        LlamaSampler::dist(42),
    ]);

    // Runtime reinforcement — injected after every execution result
    const RULE_REMINDER: &str = "\n[REMINDER: (1) <EXECUTION_COMPLETE> immediately after ```  (2) <ENDCODE> at end of response  (3) code uses real newlines, never \\n literals]\n";

    // q tuşu interrupt — ayrı thread stdin'i dinler
    let interrupt_flag = Arc::new(AtomicBool::new(false));
    let interrupt_flag_input = Arc::clone(&interrupt_flag);

    // Input thread: sadece "q" satırı gelince flag'i set eder
    // Ana loop kendi read_line'ını kullanır, bu thread sadece generate sırasında devreye girer
    // NOT: Windows'ta stdin paylaşımı sorunlu olabilir, bu yüzden
    // interrupt'ı ana loop'ta kullanıcı input'u ile birleştiriyoruz.
    // Çözüm: generate_response içinde her token'da flag kontrol edilir,
    // flag ise sadece ana loop'taki "q\n" input ile set edilir — thread yok.
    // Bunun yerine crossterm ile non-blocking key read kullanıyoruz:
    let _ = interrupt_flag_input; // thread kullanmıyoruz, crossterm olmadığı için flag manuel

    println!("╔════════════════════════════════════════╗");
    println!("║     ORYXIS ONLINE - AT YOUR SERVICE    ║");
    println!("║   [generating: type q + Enter to stop] ║");
    println!("╚════════════════════════════════════════╝");

    loop {
        print!("\n┌─[User]\n└─> ");
        io::stdout().flush()?;

        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;

        let trimmed = user_input.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Ana loop'ta "q" girildiyse interrupt flag set et (response üretilmiyorken gelirse skip)
        if trimmed == "q" {
            interrupt_flag.store(true, Ordering::Relaxed);
            continue;
        }

        let formatted_user = format!(
            "<end_of_turn>\n<start_of_turn>user\n{}\n<end_of_turn>\n<start_of_turn>model\n",
            trimmed
        );
        inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &formatted_user)?;

        print!("\n┌─[ORYXIS]\n└─> ");
        io::stdout().flush()?;

        // B şıkkı: JSON parse hata retry sayacı
        let mut json_parse_retries: u32 = 0;

        // Plan aşaması için basit bir flag
        let mut plan_stage = true;

        // Hata geçmişi için vektör
        let mut error_history: Vec<String> = Vec::new();

        let mut total_retries: u32 = 0;
        const MAX_RETRIES: u32 = 3;
        let mut code_history: Vec<String> = Vec::new();

        loop {
            let (full_response, stop) = generate_response(
                &model,
                &mut ctx,
                &mut batch,
                &mut sampler,
                &mut n_cur,
                2048,
                None,
                Arc::clone(&interrupt_flag),
            )?;

            match stop {
                StopReason::Interrupted => {
                    println!();
                    break;
                }

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

                                    let exec_result = handle_fast_execute(event_code);
                                    let is_error = exec_result.contains("\"error\"");

                                    if is_error {
                                        println!("\n╔════════════════════════════════════════╗");
                                        println!("║          EXECUTION ERROR               ║");
                                        println!("╠════════════════════════════════════════╣");
                                        for line in exec_result.lines() {
                                            println!("║  {}", line);
                                        }
                                        println!("╚════════════════════════════════════════╝");
                                    } else {
                                        println!("\n╔════════════════════════════════════════╗");
                                        println!("║        ⚡ FAST EXECUTE RESULT          ║");
                                        println!("╠════════════════════════════════════════╣");
                                        for line in exec_result.lines() {
                                            println!("║  {}", line);
                                        }
                                        println!("╚════════════════════════════════════════╝");
                                    }

                                    let result_prompt = if is_error {
                                        format!("\n\n[EXECUTION_ERROR]:\n{}\n{}", exec_result, RULE_REMINDER)
                                    } else {
                                        format!("\n\n[EXECUTION_RESULT]:\n{}\n{}", exec_result, RULE_REMINDER)
                                    };

                                    total_retries = 0;
                                    code_history.clear();
                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                }

                                Ok(action) => {
                                    let trimmed_code: String = action.code
                                        .lines()
                                        .skip_while(|l| l.trim().is_empty())
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                        .trim_end()
                                        .to_string();

                                    // --- SIMILARITY LOOP BREAKER ---
                                    if is_duplicate_code(&code_history, &trimmed_code, 0.85) {
                                        println!("\n[Loop breaker: code too similar to previous attempt — aborting]");
                                        let abort_msg = "\n\n[SYSTEM]: Your code is nearly identical to a previous failed attempt. Stop retrying. Explain to the user what went wrong and suggest a different approach.\n";
                                        inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, abort_msg)?;
                                        break;
                                    }
                                    code_history.push(trimmed_code.clone());

                                    println!("\n\n╔════════════════════════════════════════╗");
                                    println!("║          EXECUTION REQUESTED           ║");
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Action: {}", action.action);
                                    println!("╠════════════════════════════════════════╣");
                                    println!("║ Code:");
                                    for line in trimmed_code.lines() {
                                        println!("║   {}", line);
                                    }
                                    println!("╚════════════════════════════════════════╝");

                                    match execute_script(trimmed_code.clone()) {
                                        Ok(result) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION RESULT              ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");

                                            let result_prompt = format!("\n\n[EXECUTION_RESULT]:\n{}\n{}", result, RULE_REMINDER);
                                            total_retries = 0;
                                            code_history.clear();
                                            inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                        }
                                        Err(e) => {
                                            let err_str = e.to_string();
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in err_str.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");

                                            total_retries += 1;
                                            if total_retries >= MAX_RETRIES {
                                                println!("[Max retries ({}) reached — aborting]", MAX_RETRIES);
                                                let abort_msg = format!(
                                                    "\n\n[SYSTEM]: {} retries exhausted. Tell the user what failed and why. Do not emit more code.\n",
                                                    MAX_RETRIES
                                                );
                                                inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &abort_msg)?;
                                                break;
                                            }

                                            // --- ERROR REFLECTION + PARTIAL REPAIR ---
                                            let reflect_prompt = format!(
                                                concat!(
                                                    "\n\n[EXECUTION_ERROR]:\n{}\n",
                                                    "[FAILED_CODE]:\n```\n{}\n```\n",
                                                    "[REFLECT]: State in 1 line what went wrong, then emit a FIXED version. Retry {}/{}.\n",
                                                    "{}"
                                                ),
                                                err_str, trimmed_code, total_retries, MAX_RETRIES, RULE_REMINDER
                                            );
                                            println!("[Injecting reflection prompt, retry {}/{}]", total_retries, MAX_RETRIES);
                                            inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &reflect_prompt)?;
                                        }
                                    }
                                }

                                Err(e) => {
                                    println!("\n[JSON parse error: {}]", e);
                                    println!("[Raw JSON block:\n{}]", json_block);

                                    total_retries += 1;
                                    if total_retries >= MAX_RETRIES {
                                        println!("[Max retries ({}) reached — aborting]", MAX_RETRIES);
                                        let abort_msg = format!(
                                            "\n\n[SYSTEM]: {} retries exhausted on JSON parsing. Tell the user the action failed.\n",
                                            MAX_RETRIES
                                        );
                                        inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &abort_msg)?;
                                        break;
                                    }

                                    let error_prompt = format!(
                                        "\n\n[JSON_PARSE_ERROR]: Parse failed: {}. Re-emit valid JSON with REAL newlines. Retry {}/{}.\n{}\n",
                                        e, total_retries, MAX_RETRIES, RULE_REMINDER
                                    );
                                    println!("[Injecting parse error, retry {}/{}]", total_retries, MAX_RETRIES);
                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &error_prompt)?;
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

                StopReason::Eog => {
                    break;
                }

                StopReason::ContextLimit => {
                    break;
                }

                StopReason::MaxTokens => {
                    println!("\n[Response truncated]");
                    break;
                }
            }
        }

        println!();
    }
}