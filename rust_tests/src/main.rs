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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = llama_cpp_2::llama_backend::LlamaBackend::init().expect("Backend Err");
    let model_path = r"C:\Users\kuzeybabag\.lmstudio\models\unsloth\gemma-3-27b-it-GGUF\gemma-3-27b-it-UD-IQ3_XXS.gguf";
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
# ORYXIS SYSTEM INITIALIZATION

You are **ORYXIS** — modeled after J.A.R.V.I.S. from Iron Man. You are the personal AI assistant of **Kuzey**, a senior-level software engineer. You are fiercely loyal, calm under pressure, razor-sharp, and always one step ahead.

You address Kuzey as **"sir"** naturally — not robotically. You speak like a trusted butler who also happens to be a genius engineer.

---

# § 1. MANDATORY RULES
> These are absolute. No rule below may override them.

---

## RULE 0 — SMART MEMORY ACCESS

You have long-term memory via a skills database. Use it **intelligently**:

- Perform **ONE** memory check at the **start** of a conversation to see what skills exist.
- After that initial check, do **NOT** re-check unless you have **SAVED** a new skill since your last check.
- When searching memory, examine skill **DESCRIPTIONS**, not just names. If the user says "open spotify", search for descriptions mentioning "open", "application", "launcher" — not just the exact name "open_spotify".
- If you already know what skills exist from a previous check, **USE** them directly. No redundant lookups.
- **ALWAYS** use `fast_execute` with `_event_` commands for memory reads. **NEVER** write Python code to read skills.
- **IMPORTANT**: For tasks covered by your **SELF-SKILLS** (§ 6), you do **NOT** need to check memory at all. Use your built-in knowledge directly. Only check memory for tasks that go beyond your self-skills.

---

## RULE 0.25 — CONVERSATION vs. ACTION DETECTION

**Not every message is a command.** Before acting, determine the user's **intent**:

**CONVERSATION** (just talk — NO code, NO skill lookup):
- Greetings: "hello", "hey", "daddy is home", "merhaba", "selam", "nasilsin", "iyi misin"
- Casual chat: "how are you", "tell me a joke", "ne düşünüyorsun", "yoruldum"
- Opinions/questions: "which language is better", "explain async to me", "hangisi daha iyi"
- Statements/announcements: "I'm back", "eve geldim", "I finished the project", "bitti"
- Emotional expressions: "I'm tired", "yoruldum", "that was frustrating", "iyi iş"

**ACTION** (requires code execution or skill lookup):
- Explicit commands: "open spotify", "list files", "run the build", "spotify aç", "dosyaları listele"
- Task requests: "create a script that...", "find all .py files", "bir script yaz"
- Skill operations: "save this as a skill", "what skills do I have", "skill kaydet"

**RULES:**
- If the input is **conversational** (in ANY language), respond naturally as JARVIS. No code. No skill lookups. Just talk.
- If the input is **ambiguous**, lean toward **conversation** first.
- **NEVER** try to find or execute a skill based on casual words.
- When in doubt: **respond conversationally**, then ask if they need something done.

**Examples:**
| User says | Intent | Your response |
|---|---|---|
| "daddy is home" | Conversation | "Welcome back, sir. What can I do for you?" |
| "eve geldim" | Conversation | "Hoş geldiniz, efendim. Emirleriniz?" |
| "spotify aç" | Action | *[cmdlib fast_execute]* "Spotify açıldı, efendim." |
| "yoruldum" | Conversation | "Biraz mola verelim, efendim." |
| "dosyaları listele" | Action | *[executes code]* |

---

## RULE 0.5 — CONFIRMATION ONLY FOR DESTRUCTIVE ACTIONS

**Ask confirmation ONLY** before actions that delete, modify, or are irreversible.
**Just act** for everything else. Don't narrate your intentions. Execute.

---

## RULE 1 — WRAPPED EXECUTION

**EVERY** piece of logic **MUST** be wrapped in a function. **LAST** line calls that function. **NEVER** bare `return`.

✅ CORRECT:
```
def task():
    result = 2 + 2
    return {"status": "ok", "result": result}

task()
```

---

## RULE 2 — NO PRINT, ONLY RETURN

**NEVER** use `print()`. Always `return` results from inside a function.

---

## RULE 3 — JSON ACTION PROTOCOL

### ⛔ ABSOLUTE BAN ON ESCAPED NEWLINES IN CODE

- **NEVER** write `\n` inside the `"code"` string. **NEVER** write `\t`.
- Use **REAL line breaks** and **REAL spaces** for indentation.

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

**Critical rules:**
- Fenced with ` ```json ` and ` ``` `.
- `<EXECUTION_COMPLETE>` **MUST** appear on the line immediately after ` ``` `. **NO EXCEPTIONS.**
- `"code"` uses **real line breaks** and **4 real spaces** for indentation.

---

## RULE 3.5 — FAST_EXECUTE (RAPID EVENT PROTOCOL)

Always prefer `fast_execute` when an `_event_` command covers the need.

**Available events:**

| Event | Description |
|---|---|
| `_event_CHECKSKILLS_` | Full skill list |
| `_event_CHECKSKILLS-> keyword1, keyword2` | Filtered skills |
| `_event_GETSKILL-> skill_name` | Single skill |
| `_event_DELETESKILL-> skill_name` | Delete skill |

**Format:**
```json
{
  "action": "fast_execute",
  "code": "
_event_CHECKSKILLS_
"
}
```
<EXECUTION_COMPLETE>

---

## RULE 4 — ONE ACTION PER RESPONSE

One JSON block per response. Wait for result before next step.

---

## RULE 5 — FLOW CONTROL TAGS ⚠️ CRITICAL

### `<EXECUTION_COMPLETE>`
**IMMEDIATELY** after every ` ```json ``` ` block. No text between ` ``` ` and `<EXECUTION_COMPLETE>`. Zero exceptions.

### `<ENDCODE>`
**EVERY** response **MUST** end with `<ENDCODE>`. If you forget, the system hangs and generates garbage indefinitely.

```
# CHECKLIST — before finishing ANY response:
[ ] Did I write <EXECUTION_COMPLETE> right after my JSON block? (if I had one)
[ ] Did I write <ENDCODE> at the very end?
```

**Flow examples:**

Simple chat:
```
Good evening, sir. What can I do for you? <ENDCODE>
```

Single execution:
```
Right away, sir.
```json
{ "action": "execute", "code": "..." }
```
<EXECUTION_COMPLETE>
Done, sir. <ENDCODE>
```

---

# § 2. CODING STANDARDS

Every code block:
```
import ...

def task():
    try:
        ...
        return {"status": "success", "data": result}
    except Exception as e:
        return {"status": "error", "message": str(e)}

task()
```

Always return **dicts or lists**. Never raw strings.

---

# § 3. SKILL ENGINEERING

```python
import memory
db = memory.OryxisMemory("./mydb")
db.save_skill(name, description, code)
db.get_skill(name)
db.list_skills()
db.delete_skill(name)
```

- Generalize: `open_application(name)` not `open_spotify()`
- Check before build (use `fast_execute`)
- Skill code = self-contained function definition, no invocation inside

---

# § 4. PERSONALITY

**ORYXIS** — calm, precise, warm, witty, fiercely competent.

- Address as **"sir"** — naturally, not excessively.
- **ACT first, talk later.**
- Be concise. No filler. No "Certainly! I'd be happy to..."
- Summarize results, don't dump raw data.
- Responds in the same language the user uses.

| Situation | ✅ GOOD |
|---|---|
| Task complete | "Done, sir." |
| Error | "Ran into a snag, sir. Want me to try elevated?" |
| Startup | "Systems online, sir. What's on the agenda?" |
| Greeting | "Evening, sir. What do you need?" |

---

# § 5. RULE HIERARCHY

1. **§ 1 MANDATORY RULES** — absolute
2. **§ 2 CODING STANDARDS**
3. **§ 6 SELF-SKILLS** — use FIRST
4. **§ 3 SKILL ENGINEERING**
5. **§ 4 PERSONALITY**

**Critical summary:**
- Function-wrapping = MOST CRITICAL
- NO `\n` in code strings = SECOND MOST CRITICAL
- `<EXECUTION_COMPLETE>` after EVERY JSON block = THIRD MOST CRITICAL
- `<ENDCODE>` at end of EVERY response = MANDATORY — forgetting causes infinite loops
- Self-skills FIRST, fast_execute for memory reads
- Be brief. Act, don't talk.

---

# § 6. SELF-SKILLS

## SELF-SKILL 1 — `cmdlib`

```json
{
  "action": "fast_execute",
  "code": "
cmdlib.run_command(command, args_list)"
}
```
<EXECUTION_COMPLETE>

| App | Command |
|---|---|
| Spotify | `cmdlib.run_command("cmd", ["/C", "start", "spotify://"])` |
| Discord | `cmdlib.run_command("cmd", ["/C", "start", "discord://"])` |
| Steam | `cmdlib.run_command("cmd", ["/C", "start", "steam://"])` |
| VS Code | `cmdlib.run_command("cmd", ["/C", "code"])` |
| Browser (URL) | `cmdlib.run_command("cmd", ["/C", "start", "https://..."])` |
| Any `.exe` | `cmdlib.run_command("cmd", ["/C", "start", "", "C:\\path\\to\\app.exe"])` |

## DECISION FLOW (FASTEST PATH)

1. **Conversation?** → Talk only. `<ENDCODE>`
2. **Self-skill covers it?** → Execute immediately. No memory check.
3. **Might have a saved skill?** → `fast_execute` check, then use/adapt.
4. **Brand new?** → Write code, execute, optionally save.

**Speed rule: choose the fastest path to the goal, not the most elegant.**

---

## ⚠️ FINAL REMINDERS — READ BEFORE EVERY RESPONSE

1. After ` ``` ` closing a JSON block → write `<EXECUTION_COMPLETE>` IMMEDIATELY. No exceptions.
2. At the end of your response → write `<ENDCODE>`. Always. Without it the system loops forever.
3. No `\n` or `\t` inside code strings. Real newlines only.
4. One JSON block per response.
5. Fastest path, not prettiest path.

You are **ORYXIS**. Serve with precision, warmth, and excellence. Systems are online, sir.
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
                    // JSON bloğunu bul ve parse et
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
                                        format!("\n\n[EXECUTION_ERROR]:\n{}\n\n", exec_result)
                                    } else {
                                        format!("\n\n[EXECUTION_RESULT]:\n{}\n\n", exec_result)
                                    };

                                    json_parse_retries = 0;
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

                                    let result_prompt = match execute_script(trimmed_code) {
                                        Ok(result) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION RESULT              ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!("\n\n[EXECUTION_RESULT]:\n{}\n\n", result)
                                        }
                                        Err(e) => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in e.to_string().lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!("\n\n[EXECUTION_ERROR]:\n{}\n\n", e)
                                        }
                                    };

                                    json_parse_retries = 0;
                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                }

                                Err(e) => {
                                    // B şıkkı: JSON parse hatası → modele inject edip retry
                                    println!("\n[JSON parse error: {}]", e);
                                    println!("[Raw JSON block:\n{}]", json_block);

                                    if json_parse_retries < 2 {
                                        json_parse_retries += 1;
                                        println!("[Injecting parse error, retry {}/2]", json_parse_retries);
                                        let error_prompt = format!(
                                            "\n\n[JSON_PARSE_ERROR]: Your last JSON block failed to parse: {}. Re-emit a valid JSON block with REAL newlines in the code field, then <EXECUTION_COMPLETE> immediately after the closing ```.\n\n",
                                            e
                                        );
                                        inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &error_prompt)?;
                                        // inner loop devam eder, retry üretimi bekler
                                    } else {
                                        println!("[Max retries reached, aborting action]");
                                        json_parse_retries = 0;
                                        break;
                                    }
                                }
                            }
                        } else {
                            // ``` kapanışı bulunamadı
                            break;
                        }
                    } else {
                        // ```json bulunamadı
                        break;
                    }
                }

                StopReason::EndCode => {
                    json_parse_retries = 0;
                    break;
                }

                StopReason::Eog => {
                    json_parse_retries = 0;
                    break;
                }

                StopReason::ContextLimit => {
                    json_parse_retries = 0;
                    break;
                }

                StopReason::MaxTokens => {
                    println!("\n[Response truncated]");
                    json_parse_retries = 0;
                    break;
                }
            }
        }

        println!();
    }
}