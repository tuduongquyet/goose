---
title: "Use goose completely free with local open models"
description: "A practical guide to running goose entirely free on your machine — Ollama, built-in inference, which models work, and how to get past the rough edges."
authors:
    - adewale
image: /img/blog/goose-local-models.png
---

![blog cover](/img/blog/goose-local-models.png)

Local and open models have continued to improve at a rapid pace, and goose's support for them has evolved right alongside. With the recent announcement of [Gemma4](https://deepmind.google/models/gemma/gemma-4/) - Google Deepmind's latest family of open models with native tool calling and vision support capable of working across multiple devices; mobile and desktop - the local model landscape has reached a new level of capability.

Combining goose with local models means you can have a powerful, agentic assistant running entirely on your machine, that comes at zero cost, maximum privacy, and offline access. 

<!--truncate-->

## Why would you want to run local open models?

Open models do not share their training code, data or architectural details publicly. However, you are able to inspect, fine-tune or adapt the model to your needs.
With these models you get:

- **Privacy**: Your code and data never leave your machine. No third-party servers, no data sharing.
- **Offline access**: No internet, no problem. Your agent is always available.
- **Cost control**: Costs $0. No API fees, no usage limits. Just the cost of electricity and hardware.
- **Ownership**: You have full control over the models, the stack, and the experience.

## Some of the best local models available today
- GPT-OSS-120B for strongest general reasoning and tool use.
- Gemma 4 for a strong multimodal open-weight family with permissive licensing.
- Kimi K2.5 for high-end open-weights performance, especially on reasoning-oriented workloads.
- GLM-5.1 / GLM-4.7 for leaderboard strength and coding-heavy use cases.

There are tradeoffs to consider when choosing local models compared to cloud options. The biggest one is tool calling — the ability for the model to decide when and how to call tools like shell commands, code execution, or MCP extensions. Local models often lack native support for this, which can limit their agentic capabilities. We'll cover how goose addresses this gap with emulation and the toolshim.

## Two paths to local

goose gives you two distinct ways to run local models. Pick the one that fits your setup.

### Ollama — bring your own server

If you already have [Ollama](https://ollama.com) installed, goose can talk to it directly. Ollama manages model downloads, GPU offloading, and serves an OpenAI-compatible API on `localhost:11434`.

```bash
# Install Ollama if you haven't
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull gemma4-e4b

# Point goose at it
export GOOSE_PROVIDER=ollama
export GOOSE_MODEL=gemma4-e4b
goose
```

Or, in the desktop app, **Settings → Providers → Add Provider → Ollama**. goose automatically detects your locally available models. Select from the options (e.g. `qwen3` or `gemma4-e4b`) and save. 


**Key config options:**

| Variable | Default | What it does |
|---|---|---|
| `OLLAMA_HOST` | `localhost` | Where Ollama is running |
| `OLLAMA_TIMEOUT` | `600` | Request timeout in seconds |
| `OLLAMA_STREAM_TIMEOUT` | `120` | Per-chunk timeout — raise this for slow hardware |
| `GOOSE_INPUT_LIMIT` | — | Override the context window size sent to Ollama |

### Built-in local inference — no server needed

goose also embeds [llama.cpp](https://github.com/ggml-org/llama.cpp) directly, so you can download and run GGUF models without any external dependencies. Head to **Settings → Local Inference** in the desktop app to browse and download models.

This path is ideal if you want a single-app experience — no Ollama, no Docker, no separate processes. goose handles model loading, GPU memory management, and even recommends the best model for your hardware based on available memory.

**Featured models in the built-in registry:**

| Model | Size | Native Tools | Vision | Good for |
|---|---|---|---|---|
| Llama 3.2 1B | ~1 GB | ❌ | ❌ | Quick tests, low-end hardware |
| Llama 3.2 3B | ~2 GB | ❌ | ❌ | Light tasks, fast responses |
| Hermes 2 Pro Mistral 7B | ~4 GB | ❌ | ❌ | General purpose |
| Mistral Small 24B | ~14 GB | ❌ | ❌ | Strong reasoning |
| **Gemma 4 E4B** | ~8 GB | ✅ | ✅ | **Best small local option** |
| **Gemma 4 26B-A4B** | ~16 GB | ✅ | ✅ | **Most capable local option** |


## Which model should you pick?

It depends on your hardware and what you're trying to do.

**If you have 8 GB of RAM or less:**
Start with **Llama 3.2 3B** via built-in inference. You'll get basic coding assistance with emulated tool calling. Don't expect multi-step agentic workflows.

**If you have 16 GB:**
**Gemma 4 E4B** is the sweet spot — native tool calling, vision support, and it fits comfortably. This is the model I'd recommend for most people trying local models with goose for the first time.

**If you have 32 GB+:**
**Gemma 4 26B-A4B** gives you the most capable local experience. Or run `qwen3-coder:30b` through Ollama for strong coding performance.

**If you want to use a model without native tool calling:**
Enable the toolshim on the Ollama path. Pair something like `deepseek-r1:14b` or `llama3.3:70b` with `mistral-nemo` as the interpreter.

## Practical tips and rough edges

A few things I've learned from running goose locally:

**First request is slow.** Models take 30–120 seconds to load into GPU memory on the first request. goose has retry logic for this (10 retries with backoff), so don't panic if it seems stuck. Subsequent requests are fast.

**Use Code Mode for longer sessions.** Local models typically have smaller context windows (4K–8K tokens vs. 128K+ for cloud models). [Code Mode](/blog/2026/02/06/8-things-you-didnt-know-about-code-mode) reduces context rot and token usage, which makes a real difference when you're working within tighter limits.

**Disable thinking for quick tasks.** Qwen3 models reason by default, which adds latency. For simple command execution, [add `/no_think` to skip reasoning](https://goose-docs.ai/blog/2025/05/12/local-goose-qwen3) — it's faster and often works better for short tool-calling tasks.

**Raise timeouts on slow hardware.** If you're running on CPU or older hardware, set `OLLAMA_STREAM_TIMEOUT` higher than the default 120 seconds. goose will otherwise assume the model has stalled and error out.

**MCP extensions have limits locally.** Emulated tool calling only supports shell and code execution tools. If you need goose to interact with Slack, GitHub, or other MCP servers, you need a model with native tool calling (Gemma 4) or the toolshim.

**The `command_not_found_handler` trick.** This one's from [Michael Neale's Qwen3 post](/blog/2025/05/12/local-goose-qwen3) and it's great — add this to your `~/.zshrc` to turn typos into goose commands, fully offline:

```zsh
command_not_found_handler() {
  local cmd="$*"
  echo "🪿:"
  GOOSE_PROVIDER=ollama GOOSE_MODEL=qwen3 goose run -t "can you try to run this command please: $cmd"
}
```

## What's next for local models in goose

Local models with goose are genuinely usable today. The tool calling gap is the frontier — and it's shrinking.

## Quick reference

| Setup | Provider | Model | Tool Calling | Install |
|---|---|---|---|---|
| Ollama (simple) | `ollama` | `qwen3` | Via toolshim | `brew install ollama` |
| Ollama (coding) | `ollama` | `qwen3-coder:30b` | Via toolshim | `brew install ollama` |
| Built-in (small) | `local` | Gemma 4 E4B | ✅ Native | Nothing — download in Settings |
| Built-in (large) | `local` | Gemma 4 26B-A4B | ✅ Native + Vision | Nothing — download in Settings |
| Toolshim (any model) | `ollama` | Any + `mistral-nemo` | ✅ Via shim | `GOOSE_TOOLSHIM=true` |

Pick the one that matches your hardware and privacy needs, and you're good to go.

<head>
  <meta property="og:title" content="Use goose with Your Local Models" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/04/16/use-goose-with-your-local-models" />
  <meta property="og:description" content="A practical guide to running goose entirely on your machine — Ollama, built-in inference, which models work, and how to get past the rough edges." />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Use goose with Your Local Models" />
  <meta name="twitter:description" content="A practical guide to running goose entirely on your machine — Ollama, built-in inference, which models work, and how to get past the rough edges." />
</head>
