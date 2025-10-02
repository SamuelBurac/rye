mod conversation;
mod providers;
mod render;

use clap::Parser;
use conversation::Conversation;
use futures::StreamExt;
use providers::{anthropic::AnthropicProvider, LLMProvider};
use rustyline::DefaultEditor;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "rye")]
#[command(about = "A CLI tool to chat with LLM's and store conversations in markdown")]
struct Args {
    /// Continue a conversation by ID
    #[arg(short, long)]
    continue_conversation: Option<String>,

    /// LLM provider to use (currently only "anthropic" is supported)
    #[arg(short, long, default_value = "anthropic")]
    provider: String,
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
            eprintln!("Error: Unknown provider '{}'. Currently only 'anthropic' is supported.", args.provider);
            std::process::exit(1);
        }
    };

    let mut conversation = if let Some(conversation_id) = args.continue_conversation {
        match Conversation::load(&conversation_id) {
            Ok(conv) => {
                println!("Continuing conversation: {}", conversation_id);
                conv
            }
            Err(_) => {
                println!(
                    "Could not find conversation {}. Starting new conversation.",
                    conversation_id
                );
                Conversation::new()?
            }
        }
    } else {
        let conv = Conversation::new()?;
        println!("Started new conversation: {}", conv.id);
        conv
    };

    let mut rl = DefaultEditor::new()?;

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

        if input == "exit" {
            break;
        }

        if input == "help" {
            println!("\nCommands:");
            println!("  exit - Quit the program");
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
        println!("{}\n", "═".repeat(60));

        match llm_provider.generate_response_stream(&api_messages).await {
            Ok(mut stream) => {
                let mut full_response = String::new();

                // Stream and display response in real-time
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(chunk) => {
                            if !chunk.is_empty() {
                                print!("{}", chunk);
                                io::stdout().flush()?;
                                full_response.push_str(&chunk);
                            }
                        }
                        Err(e) => {
                            eprintln!("\nStream error: {}", e);
                            break;
                        }
                    }
                }

                println!("\n");

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
                                    eprintln!("Warning: Could not set conversation title: {}", e);
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
