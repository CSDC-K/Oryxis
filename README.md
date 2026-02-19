# 🌌 ORYXIS: The Strategic AI Brain

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Language](https://img.shields.io/badge/language-Rust%20%7C%20Python-orange.svg)
![Hardware](https://img.shields.io/badge/accelerator-AMD%207700%20XT-red.svg)
![Architecture](https://img.shields.io/badge/architecture-Thinking--Cycle-blueviolet.svg)

**Oryxis** is an advanced AI core inspired by the JARVIS vision. It is built as a high-logic personal assistant that bridges the gap between Large Language Models (LLMs) and local system execution. Oryxis is not just a chatbot; it is an **Autonomous Strategic Executor**.

---

## 🧠 Core Architecture: The Thinking Cycle

Oryxis operates on a proprietary three-stage cycle designed to ensure accuracy, memory persistence, and lean context management.

### 1. Long-Term Memory (LTM) Integration
Oryxis maintains a persistent event log. Every interaction and decision is stored with:
* **Event Type:** Categorization for rapid filtering.
* **Description:** High-level summary used for semantic retrieval.
* **Timestamp:** Temporal context for historical reasoning.

### 2. The Thinking Cycle (Logic Flow)
The system does not jump to conclusions. It follows a rigorous path:
1. **Perception:** Captures user intent or environmental data.
2. **Memory Query:** Scans LTM for historical context.
3. **Skill Retrieval:** Dynamically pulls Python-based skills from the local bank based on descriptions (avoiding context window bloat).
4. **Decision:** The LLM brain (Qwen-32B / Mistral-Small) decides the optimal skill or event to trigger.
5. **Execution:** Runs the selected task via `FAST_EXECUTE` or `EXECUTE` protocols.

### 3. Search & Retrieval (Efficient Context)
Instead of overwhelming the LLM with all possible commands, Oryxis uses a targeted retrieval system. It searches for specific skills by name and semantic meaning, ensuring the "Brain" only sees what it needs for the current task.

---

## 🛠️ Tech Stack

- **Kernel:** **Rust** (High-performance system management, memory safety, and orchestration).
- **Reasoning Engine:** **Qwen2.5-Coder-32B** (Quantized for AMD ROCm).
- **Automation Layer:** **Python** (Modular skill definitions and system-level scripting).
- **Hardware:** **AMD Radeon RX 7700 XT (12GB VRAM)** for local inference.
- **Protocol:** Custom JSON Action Protocol with `FAST_EXECUTE` reflex commands.

---

## 🚀 Key Features

* **Silent Valet Persona:** Minimal chatter, maximum execution. No unnecessary "How can I help you?" filler.
* **FAST_EXECUTE Reflex:** Native rapid event triggers that bypass standard Python execution for common tasks like memory reads.
* **Modular Skill Bank:** Easily extendable system where new capabilities (skills) are learned, stored, and retrieved on the fly.
* **User Consent First:** A strict security layer requiring explicit confirmation for destructive or system-altering actions.

---

## 📂 System Structure

```bash
oryxis/
├── core/               # Rust kernel for performance & orchestration
├── brain/              # Prompt engineering & LLM inference management
├── memory/             # LTM (Long-Term Memory) & Skill Database
├── skills/             # Python-based modular capabilities
└── sensory/            # (In-Development) Camera and HUD (Glass) integration
```

## 🎯 Project Vision

Created and maintained by Kuzey. Oryxis is a testament to the power of local AI. It is designed to be a private, secure, and fiercely competent partner for high-level engineering and personal management.

    "A machine that thinks is a tool. A machine that remembers and acts is a partner."

## 🛡️ License

Distributed under the MIT License. See LICENSE for more information.
