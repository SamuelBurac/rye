# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rye is a CLI tool for chatting with LLMs (currently Anthropic Claude) that stores conversations as markdown files for easy searching and reference. The tool emphasizes reducing repetitive LLM responses by prompting the model to reference previous sections instead of repeating information.

## Key Commands

### Building and Running
- **Build**: `cargo build`
- **Run**: `cargo run`
- **Run with flags**: `cargo run -- [flags]`
  - Start new conversation: `cargo run`
  - Continue conversation: `cargo run -- --continue` (opens interactive selector)
  - Continue specific conversation: `cargo run -- --continue <conversation-id>`
  - Specify provider: `cargo run -- --provider anthropic`

### Development
- **Check code**: `cargo check`
- **Format code**: `cargo fmt`
- **Linting**: `cargo clippy`

## Environment Variables

- **Required**: `ANTHROPIC_API_KEY` - API key for Anthropic Claude
- **Optional**: `ANTHROPIC_MODEL` - Model to use (defaults to `claude-sonnet-4-5-20250929`)
- **Optional**: `RYE_CONVERSATIONS` - Custom path for conversation storage (defaults to `~/.rye`)
- **Optional**: `EDITOR` or `VISUAL` - If set to vi/vim/nvim, enables vi mode in the CLI

## Architecture

### Core Modules

**Conversation Management** (`src/conversation.rs`)
- Handles all conversation persistence using markdown files
- Each conversation is stored as `<id>.md` or `<sanitized-title>.md` in the conversations directory
- Conversations are auto-titled after the first exchange using a separate LLM call
- Supports loading conversations by full ID or partial ID match
- Markdown structure: `# Title` followed by alternating `## You` and `## Assistant` sections

**Provider System** (`src/providers/`)
- Trait-based architecture (`LLMProvider` trait in `mod.rs`) for multiple LLM providers
- Currently only Anthropic is implemented (`anthropic.rs`)
- Each provider must implement:
  - `generate_response_stream()` - Returns streaming response for chat
  - `generate_title()` - Generates conversation title from first user message
- System message embedded in requests prompts LLM to respond in markdown and reference previous sections

**Streaming & Rendering** (`src/streaming.rs`, `src/render.rs`)
- Streaming handles SSE (Server-Sent Events) from Anthropic API
- Smart buffering system in `stream_and_render_response()`:
  - Accumulates text until logical markdown boundaries (empty lines, headers, code blocks)
  - Renders code blocks only when complete (after closing ```)
  - Handles list items by accumulation
  - Flushes buffer on headers to render them immediately
- Rendering uses `termimad` for terminal markdown with custom color scheme

**Main Loop** (`src/main.rs`)
- Uses `rustyline` for readline functionality with vi mode support
- Interactive conversation selector using `skim` fuzzy finder
- After selection, renders full conversation history before allowing new input
- Built-in commands: `exit`/`quit`, `help`

### Key Design Patterns

1. **Message Format**: Messages are stored as `(String, String)` tuples of `(role, content)` where role is "user" or "assistant"

2. **File Naming**: Conversations start with UUID, then rename to sanitized title after first exchange (special characters replaced with `_`)

3. **Streaming Architecture**: Uses Rust futures/streams with async/await, returning `Pin<Box<dyn Stream<...>>>` for streaming responses

4. **Error Handling**: Uses `Box<dyn std::error::Error>` throughout for flexible error propagation

## Adding New LLM Providers

1. Create new module in `src/providers/`
2. Implement `LLMProvider` trait with both streaming and title generation
3. Add provider initialization logic in `main.rs` match statement
4. Return stream compatible with existing streaming infrastructure
