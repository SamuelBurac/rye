use clap::Parser;
use reqwest::Client;
use rustyline::{DefaultEditor, Result as RustylineResult};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use termimad::*;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "rye")]
#[command(about = "A CLI tool to chat with LLM's and store conversations in markdown")]
struct Args {
    /// Continue a conversation by ID
    #[arg(short, long)]
    continue_conversation: Option<String>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

struct Conversation {
    id: String,
    file_path: PathBuf,
    messages: Vec<(String, String)>, // (role, content)
    title: Option<String>,
}

impl Conversation {
    fn new() -> io::Result<Self> {
        let id = Uuid::new_v4().to_string();
        let conversations_dir = get_conversations_dir()?;
        fs::create_dir_all(&conversations_dir)?;

        let file_path = conversations_dir.join(format!("{}.md", id));

        let mut conversation = Self {
            id: id.clone(),
            file_path,
            messages: Vec::new(),
            title: None,
        };

        conversation.write_header()?;
        Ok(conversation)
    }

    fn load(id: &str) -> io::Result<Self> {
        let conversations_dir = get_conversations_dir()?;

        // First try exact match
        let file_path = conversations_dir.join(format!("{}.md", id));

        let final_file_path = if file_path.exists() {
            file_path
        } else {
            // If not found, search for files containing the id as a substring
            find_conversation_file(&conversations_dir, id)?
        };

        let content = fs::read_to_string(&final_file_path)?;
        let (messages, title) = parse_markdown_conversation(&content);

        // Extract the actual ID from the filename
        let actual_id = final_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(id)
            .to_string();

        Ok(Self {
            id: actual_id,
            file_path: final_file_path,
            messages,
            title,
        })
    }

    fn write_header(&self) -> io::Result<()> {
        let header = if let Some(ref title) = self.title {
            format!("# {}\n\n", title)
        } else {
            format!("# Conversation {}\n\n", self.id)
        };
        fs::write(&self.file_path, header)?;
        Ok(())
    }

    fn set_title(&mut self, title: String) -> io::Result<()> {
        let sanitized_title = sanitize_filename(&title);
        let conversations_dir = get_conversations_dir()?;
        let new_file_path = conversations_dir.join(format!("{}.md", sanitized_title));

        // Rename the file
        fs::rename(&self.file_path, &new_file_path)?;

        self.title = Some(title.clone());
        self.file_path = new_file_path;

        // Rewrite the file with the new title
        self.rewrite_file_with_title()?;
        Ok(())
    }

    fn rewrite_file_with_title(&self) -> io::Result<()> {
        let mut content = String::new();

        // Write header with title
        if let Some(ref title) = self.title {
            content.push_str(&format!("# {}\n\n", title));
        } else {
            content.push_str(&format!("# Conversation {}\n\n", self.id));
        }

        // Write all messages
        for (role, message_content) in &self.messages {
            let role_header = if role == "user" {
                "## You"
            } else {
                "## Assistant"
            };
            content.push_str(&format!("\n{}\n\n{}\n\n", role_header, message_content));
        }

        fs::write(&self.file_path, content)?;
        Ok(())
    }

    fn add_message(&mut self, role: &str, content: &str) -> io::Result<()> {
        self.messages.push((role.to_string(), content.to_string()));

        let role_header = if role == "user" {
            "## You"
        } else {
            "## Assistant"
        };
        let message_content = format!("\n{}\n\n{}\n\n", role_header, content);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        file.write_all(message_content.as_bytes())?;
        Ok(())
    }

    fn get_context_for_llm(&self) -> String {
        if self.messages.is_empty() {
            return String::new();
        }

        format!(
            "Previous conversation context (refer to sections instead of repeating):\n\n{}",
            self.messages
                .iter()
                .map(|(role, content)| format!("{}: {}", role, content))
                .collect::<Vec<_>>()
                .join("\n\n")
        )
    }
}

fn find_conversation_file(conversations_dir: &PathBuf, id: &str) -> io::Result<PathBuf> {
    let entries = fs::read_dir(conversations_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.ends_with("md")
            && let Some(filename) = path.to_str()
        {
            // Check if the filename contains the id (for partial matches)
            if filename.contains(id) {
                return Ok(path);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("No conversation file found matching '{}'", id),
    ))
}

fn get_conversations_dir() -> io::Result<PathBuf> {
    if let Ok(custom_path) = env::var("RYE_CONVERSATIONS") {
        let path = PathBuf::from(custom_path);
        if path.exists() || path.parent().is_some_and(|p| p.exists()) {
            return Ok(path);
        }
    }

    if let Some(home_dir) = dirs::home_dir() {
        Ok(home_dir.join(".rye"))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find home directory",
        ))
    }
}

fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_markdown_conversation(content: &str) -> (Vec<(String, String)>, Option<String>) {
    let mut messages = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut title = None;

    // Extract title from first line if it starts with #
    if !lines.is_empty() && lines[0].starts_with("# ") {
        let title_text = lines[0].trim_start_matches("# ").trim();
        if !title_text.starts_with("Conversation ") {
            title = Some(title_text.to_string());
        }
        i = 1; // Skip the title line
    }

    while i < lines.len() {
        if lines[i].starts_with("## You") {
            i += 1;
            let mut user_content = Vec::new();
            while i < lines.len() && !lines[i].starts_with("## ") {
                if !lines[i].trim().is_empty() {
                    user_content.push(lines[i]);
                }
                i += 1;
            }
            if !user_content.is_empty() {
                messages.push(("user".to_string(), user_content.join("\n")));
            }
        } else if lines[i].starts_with("## Assistant") {
            i += 1;
            let mut assistant_content = Vec::new();
            while i < lines.len() && !lines[i].starts_with("## ") {
                if !lines[i].trim().is_empty() {
                    assistant_content.push(lines[i]);
                }
                i += 1;
            }
            if !assistant_content.is_empty() {
                messages.push(("assistant".to_string(), assistant_content.join("\n")));
            }
        } else {
            i += 1;
        }
    }

    (messages, title)
}

async fn generate_title(
    client: &Client,
    user_message: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

    let title_prompt = format!(
        "Generate a concise, descriptive title (max 50 characters) for a conversation that starts with this user message: \"{}\"\n\nRespond with ONLY the title, no additional text or formatting.",
        user_message
    );

    let request = AnthropicRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 100,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: title_prompt,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err("Failed to generate title".into());
    }

    let api_response: AnthropicResponse = response.json().await?;

    if let Some(content) = api_response.content.first() {
        Ok(content.text.trim().to_string())
    } else {
        Err("No title generated".into())
    }
}

async fn call_anthropic_api(
    client: &Client,
    messages: &[(String, String)],
) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

    let mut api_messages = Vec::new();

    // Add system message to encourage markdown output and reference previous sections
    let system_message = "You are a helpful assistant. Always respond in markdown format. When referring to information you've previously provided in this conversation, reference the relevant sections instead of repeating the information. Be concise and avoid unnecessary repetition.";

    for (role, content) in messages {
        api_messages.push(AnthropicMessage {
            role: role.clone(),
            content: if role == "user" && !messages.is_empty() {
                format!("{}\n\nSystem instruction: {}", content, system_message)
            } else {
                content.clone()
            },
        });
    }

    let request = AnthropicRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 4096,
        messages: api_messages,
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(format!("API Error: {}", error_text).into());
    }

    let api_response: AnthropicResponse = response.json().await?;

    if let Some(content) = api_response.content.first() {
        Ok(content.text.clone())
    } else {
        Err("No content in API response".into())
    }
}

fn render_markdown(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let skin = MadSkin::default();
    skin.print_text(text);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("🥃 Welcome to Rye - Your LLM conversation tool");
    println!("Conversations are stored in markdown files for easy searching");
    println!("Type 'exit' to quit, 'help' for commands\n");

    let client = Client::new();
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
        let input = match rl.readline("You: ") {
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

        print!("Assistant: ");
        io::stdout().flush()?;

        match call_anthropic_api(&client, &api_messages).await {
            Ok(response) => {
                println!();
                render_markdown(&response)?;
                conversation.add_message("assistant", &response)?;

                // Generate title after first exchange if conversation doesn't have one
                if conversation.title.is_none() && conversation.messages.len() == 2 {
                    if let Some((_, first_user_message)) = conversation.messages.first() {
                        match generate_title(&client, first_user_message).await {
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
