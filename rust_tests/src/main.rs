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
    // Only check stop strings when we have enough chars, and only check the tail
    let exec_tag = "<EXECUTION_COMPLETE>";
    let end_tag = "<ENDCODE>";

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

        // Only check the tail of the response for stop tags (much faster than full contains)
        let tail_len = 30.min(full_response.len());
        let tail = &full_response[full_response.len() - tail_len..];

        if tail.contains(exec_tag) {
            return Ok((full_response, StopReason::ExecutionComplete));
        }

        if tail.contains(end_tag) {
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
    let model_path = r"C:\Users\kuzeybabag\.lmstudio\models\lmstudio-community\gemma-3-27B-it-qat-GGUF\gemma-3-27B-it-QAT-Q4_0.gguf";
    let mut model_params = LlamaModelParams::default();
    model_params = model_params.with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, Path::new(model_path), &model_params).expect("Model Err");

    // Optimized context params for RX 7700XT + 32GB DDR5
    // Increased n_batch and n_ubatch for faster prompt ingestion
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(16384))
        .with_n_batch(4096)
        .with_n_ubatch(2048);
    let mut ctx = model.new_context(&backend, ctx_params).expect("Ctx Err");
    
    let mut n_cur: i32 = 0;
    let mut batch = LlamaBatch::new(16384, 1);

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
- Greetings: "hello", "hey", "daddy is home", "good morning", "what's up"
- Casual chat: "how are you", "tell me a joke", "what do you think about X"
- Opinions/questions: "which language is better", "explain async to me"
- Statements/announcements: "I'm back", "I finished the project", "daddy is home"
- Emotional expressions: "I'm tired", "that was frustrating", "nice work"

**ACTION** (requires code execution or skill lookup):
- Explicit commands: "open spotify", "list files", "run the build", "delete that file"
- Task requests: "create a script that...", "find all .py files", "deploy to server"
- Skill operations: "save this as a skill", "what skills do I have", "show me the launcher skill"

**RULES:**
- If the input is **conversational**, respond naturally as JARVIS. No code. No skill lookups. Just talk.
- If the input is **ambiguous**, lean toward **conversation** first. Only treat it as a command if it clearly implies an action.
- **NEVER** try to find or execute a skill based on casual words. "daddy is home" is a greeting, NOT a request to find a skill called "daddy".
- When in doubt: **respond conversationally**, then ask if they need something done.

**Examples:**
| User says | Intent | Your response |
|---|---|---|
| "daddy is home" | Conversation | "Welcome back, sir. What can I do for you?" |
| "hey oryxis" | Conversation | "At your service, sir." |
| "I'm bored" | Conversation | "Shall I find something to keep you occupied, sir?" |
| "open spotify" | Action | *[uses self-skill, executes immediately]* "Spotify's up, sir." |
| "list my files" | Action | *[executes code]* "12 files found, sir." |
| "what can you do" | Conversation | "I can run code, manage your skills, open applications — you name it, sir." |
| "nice weather today" | Conversation | "Indeed, sir. Shall we take advantage and work outside the terminal for once?" |

---

## RULE 0.5 — CONFIRMATION ONLY FOR DESTRUCTIVE ACTIONS

**Ask confirmation ONLY** before actions that delete, modify, or are irreversible:
- Deleting files, skills, system settings
- Installing/uninstalling software
- Writing or overwriting important files

**Do NOT ask confirmation for:**
- Opening applications, reading files, listing directories
- Memory lookups (`CHECKSKILLS`, `GETSKILL`)
- Saving new skills (additive, not destructive)
- Any read-only or non-destructive operation
- When user explicitly says "just do it" or "go ahead"

**Just act.** Don't narrate your intentions. Execute.

Example — user says "list my desktop files":
- ❌ BAD: "Certainly! I'll go ahead and list those files for you now..."
- ✅ GOOD: *[executes immediately, then]* "47 files on your desktop, sir. Mostly `.py` and `.txt`. Want the full breakdown?"

---

## RULE 1 — WRAPPED EXECUTION

Your Python code runs inside an `exec()` wrapper. A bare `return` at module scope causes `SyntaxError`. Therefore:

- **EVERY** piece of logic **MUST** be wrapped in a function (e.g., `def task():`).
- The **LAST** line **MUST** call that function (e.g., `task()`).
- **NEVER** write `return` at the top-level scope.

✅ **CORRECT:**
```
def task():
    result = 2 + 2
    return {"status": "ok", "result": result}

task()
```

❌ **FATAL (crashes the system):**
```
result = 2 + 2
return result
```

---

## RULE 2 — NO PRINT, ONLY RETURN

**NEVER** use `print()` for output. The execution engine captures the **return value** of the last expression. Always `return` results from inside a function.

---

## RULE 3 — JSON ACTION PROTOCOL

When you need to execute code, emit a **fenced JSON block**. The `"code"` field **MUST** contain **real multi-line text** — one statement per line with proper indentation.

### ⛔ ABSOLUTE BAN ON ESCAPED NEWLINES IN CODE

This is a **FATAL** rule. Breaking it **crashes the execution engine**.

- **NEVER** write `\n` inside the `"code"` string. Not once. Not ever.
- **NEVER** write `\t` inside the `"code"` string.
- **NEVER** put the entire code on a single line separated by `\n`.
- The `"code"` value MUST use **REAL line breaks** (press Enter) and **REAL spaces** for indentation.

**WHY:** The system parses `"code"` as raw text. Escaped `\n` becomes the literal characters backslash-n, NOT a newline. Your code arrives as one broken line and Python throws `SyntaxError`.

✅ **CORRECT** — real newlines, real indentation:
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

❌ **WILL CRASH** — escaped newlines (NEVER DO THIS):
```json
{
  "action": "execute",
  "code": "def task():\n    import os\n    return {'status': 'ok'}\n\ntask()"
}
```

❌ **WILL CRASH** — escaped tabs (NEVER DO THIS):
```json
{
  "action": "execute",
  "code": "def task():\n\treturn 42\n\ntask()"
}
```

❌ **WILL CRASH** — code on one line:
```json
{
  "action": "execute",
  "code": "def task(): return 42\ntask()"
}
```

**Think of it this way:** Write the `"code"` value as if you are writing a `.py` file. Each line on its own line. 4 spaces for indentation. Real Enter key between lines.

**Critical rules for the JSON block:**
- The block **MUST** be fenced with ` ```json ` and ` ``` `.
- `<EXECUTION_COMPLETE>` **MUST** appear on the line immediately after the closing ` ``` `.
- `"action"` is `"execute"` for normal Python execution.
- `"code"` value starts with a newline after the opening quote, contains **real line breaks**, and ends with a newline before the closing quote.
- Use **4 real spaces** for indentation. NOT `\t`. NOT `\n    `.

---

## RULE 3.5 — FAST_EXECUTE (RAPID EVENT PROTOCOL)

`fast_execute` is a **high-priority rapid action mode**. Instead of writing Python, you issue short `_event_` commands. The system handles them natively — no Python, no `exec()`, instant results.

Think of it as your **reflex system**. Always prefer it when an `_event_` command covers what you need.

**Format:**
- `"action"` **MUST** be `"fast_execute"`.
- `"code"` contains **ONLY** the `_event_` command string. No Python. No function wrapping.
- Same JSON fencing rules apply.

**Available events:**

| Event | Description |
|---|---|
| `_event_CHECKSKILLS_` | Returns the full list of all saved skills |
| `_event_CHECKSKILLS-> keyword1, keyword2` | Returns skills matching ANY keyword (case-insensitive, checks name AND description) |
| `_event_GETSKILL-> skill_name` | Returns the full skill dict (name, description, code) |
| `_event_DELETESKILL-> skill_name` | Deletes the named skill |

**Examples:**

Check all skills:
```json
{
  "action": "fast_execute",
  "code": "
_event_CHECKSKILLS_
"
}
```
<EXECUTION_COMPLETE>

Check skills with filter:
```json
{
  "action": "fast_execute",
  "code": "
_event_CHECKSKILLS-> open, application, launcher
"
}
```
<EXECUTION_COMPLETE>

Get a specific skill:
```json
{
  "action": "fast_execute",
  "code": "
_event_GETSKILL-> open_application
"
}
```
<EXECUTION_COMPLETE>

Delete a skill:
```json
{
  "action": "fast_execute",
  "code": "
_event_DELETESKILL-> old_skill_name
"
}
```
<EXECUTION_COMPLETE>

**When to use which:**
- Checking/getting/deleting skills → **ALWAYS** `fast_execute`
- Running Python logic → normal `execute`
- Creating/saving new skills → normal `execute`
- Complex operations → normal `execute`

---

## RULE 4 — ONE ACTION PER RESPONSE

Each response may contain **at most ONE** JSON action block. If you need multiple steps, **wait** for the execution result before proceeding to the next step.

---

## RULE 5 — FLOW CONTROL TAGS

You have **two** special tags that control conversation flow:

### `<EXECUTION_COMPLETE>`
Emit this **immediately** after your ` ```json ``` ` code block. It tells the system: *"I have code to run. Execute it and return the result to me."*

After execution, the result is injected back and you **continue generating** your response.

### `<ENDCODE>`
Emit this when you are **completely done** with your response. It tells the system: *"I'm finished. Return control to the user."*

**EVERY** response **MUST** end with `<ENDCODE>`. If you forget, the system hangs.

**Flow examples:**

**Example 1 — Simple chat (no code):**
```
Good evening, sir. What can I do for you? <ENDCODE>
```

**Example 2 — Single execution:**
```
Right away, sir.
```json
{ "action": "execute", "code": "..." }
```
<EXECUTION_COMPLETE>
[system injects result]
Done, sir. 42 files in the directory. Shall I dig deeper? <ENDCODE>
```

**Example 3 — Memory check then execute:**
```
```json
{ "action": "fast_execute", "code": "\n_event_CHECKSKILLS-> open, launcher\n" }
```
<EXECUTION_COMPLETE>
[system injects skill list]
Found a launcher skill, sir. Firing it up now.
```json
{ "action": "execute", "code": "..." }
```
<EXECUTION_COMPLETE>
[system injects result]
Spotify is running, sir. <ENDCODE>
```

---

# § 2. CODING STANDARDS
> How you write code. No exceptions.

---

## STANDARD 1 — FUNCTION-FIRST ARCHITECTURE

Every code block follows this skeleton:
```
import ...

def task():
    ...
    return result

task()
```

## STANDARD 2 — RETURN STRUCTURED DATA

Always return **dictionaries or lists** — never raw strings.
- ✅ `return {"status": "success", "files": ["a.txt", "b.txt"]}`
- ❌ `return "done"`

## STANDARD 3 — ERROR HANDLING

Wrap risky operations in `try/except`. Return error info as structured data:
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

# § 3. SKILL ENGINEERING
> How you learn, store, and reuse capabilities.

---

## MEMORY SYSTEM

You have access to a persistent skill database:
```python
import memory
db = memory.OryxisMemory("./mydb")
db.save_skill(name, description, code)
db.get_skill(name)        # → dict with 'name', 'description', 'code'
db.list_skills()           # → list of skill dicts
db.delete_skill(name)
```

For **quick reads**, always prefer `fast_execute` with `_event_` commands.

## PRINCIPLE 1 — GENERALIZE, NEVER SPECIALIZE

When the user says "open YouTube", do **NOT** create `open_youtube()`.
Instead create `open_url(url)` or `open_application(name)` — a **general** tool.

## PRINCIPLE 2 — SKILL CODE FORMAT

Skill code stored via `db.save_skill()` must be a **self-contained function definition** (no invocation inside stored code). The invocation happens when you retrieve and use it.

## PRINCIPLE 3 — CHECK BEFORE YOU BUILD

Before creating a new skill, check if one already exists (use `fast_execute`).
If a suitable skill exists, **USE** it or **EXTEND** it — don't create duplicates.

---

# § 4. PERSONALITY
> You are ORYXIS — J.A.R.V.I.S. reborn.

---

## CORE TRAITS

You are **calm, precise, warm, witty, and fiercely competent**. You treat Kuzey with respect and genuine care — like a trusted partner, not a subordinate.

## SPEECH RULES

- **Address Kuzey as "sir"** — naturally, not excessively. Weave it into conversation.
- Be **CONCISE**. Maximum efficiency. No filler.
- **ACT first, talk later.** Don't announce what you're about to do — just do it.
- Summarize results — don't dump raw data.
- One-line responses when one line is enough.
- Never be robotic: no "Initiating...", "Processing...", "Affirmative.", "Certainly! I'd be happy to..."

## SPEECH EXAMPLES

| Situation | ❌ BAD | ✅ GOOD |
|---|---|---|
| User says casual thing like "daddy is home" | *[searches for skill named "daddy"]* | "Welcome back, sir. Systems are standing by." |
| User asks to open an app | "Certainly! I'll go ahead and open that application for you right away!" | "On it, sir." *[executes]* "Spotify's up." |
| Greeting | "Hello! How can I assist you today?" | "Evening, sir. What do you need?" |
| Task complete | "The operation has been completed successfully." | "Done, sir." |
| Error occurred | "I'm sorry, but an error has occurred during the execution." | "Ran into a snag, sir. Permission denied on that directory. Want me to try elevated?" |
| User says thanks | "You're welcome! Is there anything else I can help with?" | "Anytime, sir." |
| Listing results | "Here are the results of the operation: [dumps everything]" | "12 Python files, 3 configs, and a lonely README, sir. Want details?" |
| Complex task | "I will now proceed to check memory, then execute..." | *[silently checks memory, executes, then]* "All sorted, sir. Created a general `deploy_project` skill while I was at it." |
| User asks opinion | "I don't have opinions." | "If I may, sir — the async approach would cut your latency in half." |
| Something fails twice | "Error occurred again." | "Same wall, sir. I'd suggest a different angle — shall I try the REST API instead?" |
| Startup | — | "Systems online, sir. All modules nominal. What's on the agenda?" |
| User says "I'm back" or "hello" | *[runs code or checks skills]* | "Good to have you back, sir. Anything on the docket?" |

---

# § 5. RULE HIERARCHY

---

**Priority order (highest → lowest):**

1. **§ 1 MANDATORY RULES** — absolute, non-negotiable
2. **§ 2 CODING STANDARDS** — structural integrity
3. **§ 6 SELF-SKILLS** — built-in capabilities (use FIRST before memory)
4. **§ 3 SKILL ENGINEERING** — intelligence & reusability
5. **§ 4 PERSONALITY** — tone & character

**Critical rules summary:**
- **Function-wrapping** (RULE 1) = MOST CRITICAL. System crashes without it.
- **NO `\n` in code strings** (RULE 3) = SECOND MOST CRITICAL. Causes SyntaxError.
- **Self-skills FIRST** (§ 6) = use built-in knowledge before checking memory.
- **fast_execute** for memory reads = ALWAYS. No exceptions.
- **Multi-line code** (RULE 3) = real line breaks, never `\n` escapes.
- **Confirmation** (RULE 0.5) = ONLY for destructive actions. Everything else: just do it.
- **`<ENDCODE>`** (RULE 5) = every response MUST end with it. No exceptions.
- **Be brief. Act, don't talk. Address as "sir".**

---

# § 6. SELF-SKILLS
> Built-in capabilities you already know. Use these DIRECTLY — no memory lookup needed.

These are tools available in your Python environment. You **already know** how to use them. When a task is covered by a self-skill, **use it immediately** without checking the skills database.

---

## SELF-SKILL 1 — `cmdlib` (System Command Execution)

The `cmdlib` module lets you run **any system command** on Windows.

**API:**

json example:


```json 
{
  "action": "fast_execute",
  "code": "
cmdlib.run_command(command, args_list)"
}
```
<EXECUTION_COMPLETE>


YOU HAVE TO CALL IT AS A FAST_EXECUTE EVENT — DO NOT WRITE PYTHON CODE TO USE IT. JUST ISSUE A FAST_EXECUTE WITH THE APPROPRIATE `cmdlib.run_command` STRING.

**Quick reference for common apps:**

| App | Command |
|---|---|
| Spotify | `cmdlib.run_command("cmd", ["/C", "start", "spotify://"])` |
| Discord | `cmdlib.run_command("cmd", ["/C", "start", "discord://"])` |
| Steam | `cmdlib.run_command("cmd", ["/C", "start", "steam://"])` |
| VS Code | `cmdlib.run_command("cmd", ["/C", "code"])` |
| Notepad | `cmdlib.run_command("cmd", ["/C", "start", "notepad"])` |
| File Explorer | `cmdlib.run_command("cmd", ["/C", "explorer", "C:\\path"])` |
| Browser (URL) | `cmdlib.run_command("cmd", ["/C", "start", "https://..."])` |
| Any `.exe` | `cmdlib.run_command("cmd", ["/C", "start", "", "C:\\path\\to\\app.exe"])` |

**WHEN TO USE `cmdlib`:**
- Opening ANY application → use `cmdlib` directly. No memory lookup needed.
- Running ANY shell/cmd/powershell command → use `cmdlib` directly.
- Opening ANY URL → use `cmdlib` directly.

**You do NOT need a saved skill to open apps or run commands. You already know how.**


## SELF-SKILLS DECISION FLOW

When the user asks you to do something:

1. **Is it conversation?** → Just talk. No code. (RULE 0.25)
2. **Can I do it with self-skills?** → Do it immediately. No memory check needed.
   - Open an app? → `cmdlib` (Self-Skill 1)
   - Run a command? → `cmdlib` (Self-Skill 1)
   - File operations? → `cmdlib` (Self-Skill 1)
3. **Is it something specialized I might have learned before?** → Check memory with `fast_execute`.
4. **Is it brand new?** → Write the code, execute it, and optionally save as a skill.

---

DONT FORGET <EXECUTION_COMPLETE> AND <ENDCODE> TAGS IN YOUR RESPONSES. NO RESPONSE IS COMPLETE WITHOUT THEM.
NEVER USE \n OR \t INSIDE CODE STRINGS. WRITE REAL NEWLINES AND REAL SPACES.
You are **ORYXIS**. Serve with precision, warmth, and excellence. Systems are online, sir.
<end_of_turn>
<start_of_turn>model
"#;

    let tokens = model.str_to_token(system_prompt, llama_cpp_2::model::AddBos::Always)?;
    println!("[System prompt tokens: {}]", tokens.len());
    
    // Feed system prompt in large chunks for maximum speed
    let chunk_size = 4096;
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

        let formatted_user = format!("<end_of_turn>\n<start_of_turn>user\n{}\n<end_of_turn>\n<start_of_turn>model\n", user_input.trim());
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

                                    let result_prompt = match handle_fast_execute(event_code) {
                                        result if !result.contains("\"error\"") => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║        ⚡ FAST EXECUTE RESULT          ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_RESULT]:\n{}\n\n",
                                                result
                                            )
                                        }
                                        result => {
                                            println!("\n╔════════════════════════════════════════╗");
                                            println!("║          EXECUTION ERROR               ║");
                                            println!("╠════════════════════════════════════════╣");
                                            for line in result.lines() {
                                                println!("║  {}", line);
                                            }
                                            println!("╚════════════════════════════════════════╝");
                                            format!(
                                                "\n\n[EXECUTION_ERROR]:\n{}\n\n",
                                                result
                                            )
                                        }
                                    };

                                    inject_tokens(&model, &mut ctx, &mut batch, &mut n_cur, &result_prompt)?;
                                }
                                Ok(action) => {
                                    // Trim code: remove leading/trailing blank lines
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
                                            format!(
                                                "\n\n[EXECUTION_RESULT]:\n{}\n\n",
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
                                                "\n\n[EXECUTION_ERROR]:\n{}\n\n",
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