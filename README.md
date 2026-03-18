# 🌌 Oryxis (Alpha v0.1)

**Oryxis** is a high-performance, Rust-based personal AI assistant (Jarvis-inspired) designed to bridge the gap between Large Language Models and system-level execution.

Unlike standard chatbots, Oryxis is built for speed, memory, and action. It doesn't just talk; it thinks and interacts with your system.

## 🧠 Core Architecture

Oryxis is built on three main pillars:

1.  **Thinking Cycle:** A decision-making loop that analyzes user intent, queries long-term memory, and selects the most appropriate "skill" (LLM-driven tool use).
2.  **Long-Term Memory Integration:**  ** Coming soon... **
3.  **Search & Retrieval:** A smart skill-fetching mechanism that keeps the LLM's context window clean by only loading relevant tools on demand.

## ⚡ Powered by Rust & Groq
Built with **Rust** for memory safety and blazing-fast performance. Currently optimized for **Llama 4 Maverick (17B-128E)** via Groq, achieving over 500+ TPS for near-instant responses.

## 🚀 Quick Start

### Integrated Apis
- Gemini
- Groq
- LLMAPI.ai


### 1. Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [EdgeTTS python package](https://github.com/rany2/edge-tts) (latest)



### 2. Configuration
Create a `.env` file in the 'Oryxis' directory as the executable:

```env
API_TYPE=LLMAPI // GROQ or GEMINI
API_KEY=Your Api key
LLM_MODEL=gemini-2.5-flash // that line depends on api type.
TTS=en-AU-WilliamMultilingualNeural // Edge-tts
```

### 3. Creating skill
- You can write it python module like style or which language dou yo want (it have to can crate a .dll file and stable with ctypes)
- If your skill is not a .py and its .dll, you have to write python bridge like 'ORYXIS/skills/lib/*.py files'
- Create a good .yaml file (Oryxis/skills/*.yaml) and add your skill into skill index file (ORYXIS/memory/skills_index.json)

### 4. Running
- When you are done with configs and skills you can run oryxis now! (you have to build .dll files also look 'libraries_opensource/' for it and compile skills, then move .dll files into 'ORYXIS/skills/lib')
- cd ORYXIS
- cargo run --release

## NOTE
prompt is made by me and for me, so you can see a name or personalized texts in prompt file you have to change it for your self
