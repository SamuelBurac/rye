mod conversation;
mod providers;
mod render;
mod streaming;

use clap::Parser;
use conversation::{Conversation, list_conversations};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal,
};
use providers::{LLMProvider, anthropic::AnthropicProvider};
use render::render_markdown;
use skim::prelude::*;
use std::io::{self, Write};
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

fn select_command() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let commands = vec!["/new-conversation - Start a new conversation"];

    let options = SkimOptionsBuilder::default()
        .height("50%".to_string())
        .prompt("Select a command: ".to_string())
        .layout("reverse".to_string()) // Display results below the prompt
        .build()
        .unwrap();

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();

    for cmd in commands {
        tx.send(Arc::new(cmd.to_string())).unwrap();
    }
    drop(tx);

    let output = Skim::run_with(&options, Some(rx));

    // Don't clear the screen, just move down
    println!();

    match output {
        Some(out) if !out.is_abort => {
            if let Some(selected) = out.selected_items.first() {
                let selected_text = selected.output().to_string();
                // Extract command (everything before " - ")
                let cmd = if let Some(pos) = selected_text.find(" - ") {
                    selected_text[..pos].to_string()
                } else {
                    selected_text
                };
                Ok(Some(cmd))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
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

fn cleanup_and_exit(conversation: &Conversation) {
    // Delete conversation file if no messages were added
    if conversation.messages.is_empty() {
        if let Err(e) = std::fs::remove_file(&conversation.file_path) {
            eprintln!("Warning: Could not delete empty conversation file: {}", e);
        }
    } else {
        println!(
            "Conversation saved to: {}",
            conversation.file_path.display()
        );
    }
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

    let mut running = true;
    while running {
        // Print a visually appealing separator before input
        println!("\n{}", "─".repeat(60));

        // Check first character to see if it's a command
        terminal::enable_raw_mode()?;

        print!("➤ ");
        io::stdout().flush()?;

        let Event::Key(key_event) = event::read()? else {
            terminal::disable_raw_mode()?;
            continue;
        };

        let input = match key_event.code {
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                terminal::disable_raw_mode()?;
                println!("\nExiting...");
                cleanup_and_exit(&conversation);
                running = false;
                String::new()
            }
            KeyCode::Char('/') => {
                // Switch to command mode immediately
                // Clear current line and redraw with cyan
                execute!(io::stdout(), cursor::MoveToColumn(0))?;
                execute!(
                    io::stdout(),
                    terminal::Clear(terminal::ClearType::CurrentLine)
                )?;
                execute!(io::stdout(), cursor::MoveUp(1))?;
                execute!(
                    io::stdout(),
                    terminal::Clear(terminal::ClearType::CurrentLine)
                )?;

                execute!(io::stdout(), SetForegroundColor(Color::Cyan))?;
                println!("{}", "─".repeat(60));
                print!("➤ /");
                execute!(io::stdout(), ResetColor)?;
                io::stdout().flush()?;

                terminal::disable_raw_mode()?;
                println!();

                // Show command selector
                match select_command()? {
                    Some(cmd) => cmd,
                    None => {
                        println!("No command selected.");
                        String::new()
                    }
                }
            }
            KeyCode::Char(c) => {
                // Not a command, use normal input
                print!("{}", c);
                io::stdout().flush()?;
                terminal::disable_raw_mode()?;

                // Read the rest of the line normally
                let mut rest = String::new();
                io::stdin().read_line(&mut rest)?;
                format!("{}{}", c, rest.trim())
            }
            KeyCode::Enter => {
                terminal::disable_raw_mode()?;
                println!();
                String::new()
            }
            _ => {
                terminal::disable_raw_mode()?;
                String::new()
            }
        };

        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        let input_lower = input.to_lowercase();

        if input_lower == "exit" || input_lower == "quit" {
            cleanup_and_exit(&conversation);
            running = false;
            continue;
        }

        if input_lower == "help" {
            println!("\nCommands:");
            println!("  exit/quit - Quit the program (case insensitive)");
            println!("  help - Show this help");
            println!("\nSlash Commands:");
            println!("  / - Open command selector (fuzzy search)");
            println!("  /new-conversation - Start a new conversation");
            println!("\nCurrent Conversation:");
            println!("  ID: {}", conversation.id);
            println!("  File: {}\n", conversation.file_path.display());
        }

        // Handle slash commands (for direct typing like /new-conversation)
        if input.starts_with('/') {
            match input_lower.as_str() {
                "/new-conversation" => {
                    // Check if current conversation is empty and delete if so
                    if conversation.messages.is_empty() {
                        if let Err(e) = std::fs::remove_file(&conversation.file_path) {
                            eprintln!("Warning: Could not delete empty conversation file: {}", e);
                        } else {
                            println!("Empty conversation deleted.");
                        }
                    } else {
                        println!(
                            "Current conversation saved to: {}",
                            conversation.file_path.display()
                        );
                    }
                    conversation = Conversation::new()?;
                    println!("Started new conversation: {}", conversation.id);
                    continue;
                }
                _ => {
                    println!(
                        "Unknown command: {}. Type 'help' for available commands.",
                        input_lower
                    );
                    continue;
                }
            }
        }

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
                        if conversation.title.is_none()
                            && conversation.messages.len() == 2
                            && let Some((_, first_user_message)) = conversation.messages.first()
                        {
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

    Ok(())
}
