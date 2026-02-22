# 🌌 Oryxis (Alpha v0.1)

**Oryxis** is a high-performance, Rust-based personal AI assistant (Jarvis-inspired) designed to bridge the gap between Large Language Models and system-level execution.

Unlike standard chatbots, Oryxis is built for speed, memory, and action. It doesn't just talk; it thinks and interacts with your system.

## 🧠 Core Architecture

Oryxis is built on three main pillars:

1.  **Thinking Cycle:** A decision-making loop that analyzes user intent, queries long-term memory, and selects the most appropriate "skill" (LLM-driven tool use).
2.  **Long-Term Memory Integration:** An event-logging system that stores interactions with types, descriptions, and timestamps, allowing the assistant to "remember" over time.
3.  **Search & Retrieval:** A smart skill-fetching mechanism that keeps the LLM's context window clean by only loading relevant tools on demand.

## ⚡ Powered by Rust & Groq
Built with **Rust** for memory safety and blazing-fast performance. Currently optimized for **Llama 4 Maverick (17B-128E)** via Groq, achieving over 500+ TPS for near-instant responses.

## 🚀 Quick Start

### 1. Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- A Groq API Key or Gemini API Key. (in release version you have to use Groq key and ther is just meta-llama/llama-4-maverick-17b-128e-instruct model.)

### 2. Configuration
Create a `.env` file in the same directory as the executable:

```env
# Define the provider (Planned: GROQ, GEMINI, OPENAI)
API_TYPE=GROQ // For now i didn`t add api chose system so you have to use groq apis.

# Your API Key
API_KEY=your_api_key_here
