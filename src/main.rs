mod conversation;
mod providers;
mod render;
mod streaming;

use clap::Parser;
use conversation::{list_conversations, Conversation};
use crossterm::{cursor, execute, terminal};
use providers::{anthropic::AnthropicProvider, LLMProvider};
use render::render_markdown;
use rustyline::config::Configurer;
use rustyline::{DefaultEditor, EditMode};
use skim::prelude::*;
use std::io;
use std::sync::Arc;
use streaming::stream_and_render_response;

#[derive(Parser)]
#[command(name = "rye")]
#[command(about = "A CLI tool to chat with LLM's and store conversations in markdown")]
struct Args {
    /// Continue a conversation (opens interactive selector if no ID provided)
    #[arg(short, long)]
    r#continue: Option<Option<String>>,

    /// LLM provider to use (currently only "anthropic" is supported)
    #[arg(short, long, default_value = "anthropic")]
    provider: String,
}

fn select_conversation() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let conversations = list_conversations()?;

    if conversations.is_empty() {
        println!("No previous conversations found.");
        return Ok(None);
    }

    // Prepare items for skim
    let items: Vec<String> = conversations
        .iter()
        .map(|conv| {
            if let Some(ref title) = conv.title {
                format!("{} - {}", title, conv.id)
            } else {
                conv.id.clone()
            }
        })
        .collect();

    let options = SkimOptionsBuilder::default()
        .height("50%".to_string())
        .prompt("Select a conversation: ".to_string())
        .build()
        .unwrap();

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();

    for item in items {
        tx.send(Arc::new(item)).unwrap();
    }
    drop(tx);

    let output = Skim::run_with(&options, Some(rx));

    // Clear the terminal after skim exits to remove the skim UI
    execute!(io::stdout(), terminal::Clear(terminal::ClearType::All))?;
    execute!(io::stdout(), cursor::MoveTo(0, 0))?;

    // Re-print the welcome message after clearing
    println!("🥃 Welcome to Rye - Your LLM conversation tool");
    println!("Conversations are stored in markdown files for easy searching");
    println!("Type 'exit' to quit, 'help' for commands\n");

    match output {
        Some(out) if !out.is_abort => {
            if let Some(selected) = out.selected_items.first() {
                let selected_text = selected.output().to_string();
                // Extract ID from the end (after the last " - ")
                let id = if let Some(pos) = selected_text.rfind(" - ") {
                    selected_text[pos + 3..].to_string()
                } else {
                    selected_text
                };
                Ok(Some(id))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn render_conversation_history(
    conversation: &Conversation,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read and render the entire markdown file
    let content = std::fs::read_to_string(&conversation.file_path)?;

    println!("\n{}", "═".repeat(60));
    println!("📜 Conversation History");
    println!("{}\n", "═".repeat(60));

    render_markdown(&content)?;

    println!("\n{}", "═".repeat(60));

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("🥃 Welcome to Rye - Your LLM conversation tool");
    println!("Conversations are stored in markdown files for easy searching");
    println!("Type 'exit' to quit, 'help' for commands\n");

    // Initialize LLM provider based on configuration
    let llm_provider: Box<dyn LLMProvider> = match args.provider.to_lowercase().as_str() {
        "anthropic" => Box::new(AnthropicProvider::new()?),
        _ => {
            eprintln!(
                "Error: Unknown provider '{}'. Currently only 'anthropic' is supported.",
                args.provider
            );
            std::process::exit(1);
        }
    };

    let mut conversation = if let Some(continue_arg) = args.r#continue {
        // --continue flag was provided
        match continue_arg {
            Some(id) => {
                // ID was explicitly provided
                match Conversation::load(&id) {
                    Ok(conv) => {
                        println!("Continuing conversation: {}", id);
                        render_conversation_history(&conv)?;
                        conv
                    }
                    Err(_) => {
                        println!(
                            "Could not find conversation {}. Starting new conversation.",
                            id
                        );
                        Conversation::new()?
                    }
                }
            }
            None => {
                // No ID provided, show interactive selector
                match select_conversation()? {
                    Some(id) => match Conversation::load(&id) {
                        Ok(conv) => {
                            println!("Continuing conversation: {}", id);
                            render_conversation_history(&conv)?;
                            conv
                        }
                        Err(_) => {
                            println!(
                                "Could not find conversation {}. Starting new conversation.",
                                id
                            );
                            Conversation::new()?
                        }
                    },
                    None => {
                        println!("No conversation selected. Starting new conversation.");
                        let conv = Conversation::new()?;
                        println!("Started new conversation: {}", conv.id);
                        conv
                    }
                }
            }
        }
    } else {
        let conv = Conversation::new()?;
        println!("Started new conversation: {}", conv.id);
        conv
    };

    let mut rl = DefaultEditor::new()?;

    // Set vi mode if EDITOR or VISUAL contains vi/vim/nvim
    if let Ok(editor) = std::env::var("EDITOR").or_else(|_| std::env::var("VISUAL")) {
        let editor_lower = editor.to_lowercase();
        if editor_lower.contains("vi")
            || editor_lower.contains("vim")
            || editor_lower.contains("nvim")
        {
            println!("Setting vi mode");
            rl.set_edit_mode(EditMode::Vi);
        }
    }

    loop {
        // Print a visually appealing separator before input
        println!("\n{}", "─".repeat(60));
        println!("💬 Your Message:");
        println!("{}", "─".repeat(60));

        let input = match rl.readline("➤ ") {
            Ok(line) => line.trim().to_string(),
            Err(_) => break,
        };

        if input.is_empty() {
            continue;
        }

        let input_lower = input.to_lowercase();

        if input_lower == "exit" || input_lower == "quit" {
            break;
        }

        if input_lower == "help" {
            println!("\nCommands:");
            println!("  exit/quit - Quit the program (case insensitive)");
            println!("  help - Show this help");
            println!("  Conversation ID: {}", conversation.id);
            println!("  File: {}\n", conversation.file_path.display());
            continue;
        }

        rl.add_history_entry(&input)?;

        // Add user message to conversation
        conversation.add_message("user", &input)?;

        // Prepare messages for API call
        let mut api_messages = Vec::new();
        for (role, content) in &conversation.messages {
            api_messages.push((role.clone(), content.clone()));
        }

        // Print a visually appealing separator before assistant response
        println!("\n{}", "═".repeat(60));
        println!("🤖 Assistant Response:");
        println!("{}", "═".repeat(60));
        println!();

        match llm_provider.generate_response_stream(&api_messages).await {
            Ok(stream) => {
                match stream_and_render_response(stream).await {
                    Ok(full_response) => {
                        println!();

                        // Save the complete response to conversation
                        if !full_response.is_empty() {
                            conversation.add_message("assistant", &full_response)?;
                        }

                        // Generate title after first exchange if conversation doesn't have one
                        if conversation.title.is_none() && conversation.messages.len() == 2 {
                            if let Some((_, first_user_message)) = conversation.messages.first() {
                                match llm_provider.generate_title(first_user_message).await {
                                    Ok(title) => {
                                        if let Err(e) = conversation.set_title(title) {
                                            eprintln!(
                                                "Warning: Could not set conversation title: {}",
                                                e
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Warning: Could not generate title: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Streaming error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }

        println!();
    }

    println!(
        "Conversation saved to: {}",
        conversation.file_path.display()
    );
    Ok(())
}
