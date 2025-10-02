use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone)]
pub struct ConversationInfo {
    pub id: String,
    pub title: Option<String>,
    pub file_path: PathBuf,
}

pub struct Conversation {
    pub id: String,
    pub file_path: PathBuf,
    pub messages: Vec<(String, String)>, // (role, content)
    pub title: Option<String>,
}

impl Conversation {
    pub fn new() -> io::Result<Self> {
        let id = Uuid::new_v4().to_string();
        let conversations_dir = get_conversations_dir()?;
        fs::create_dir_all(&conversations_dir)?;

        let file_path = conversations_dir.join(format!("{}.md", id));

        let conversation = Self {
            id: id.clone(),
            file_path,
            messages: Vec::new(),
            title: None,
        };

        conversation.write_header()?;
        Ok(conversation)
    }

    pub fn load(id: &str) -> io::Result<Self> {
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

    pub fn set_title(&mut self, title: String) -> io::Result<()> {
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

    pub fn add_message(&mut self, role: &str, content: &str) -> io::Result<()> {
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
            // Collect all lines until next header
            while i < lines.len() && !lines[i].starts_with("## ") {
                user_content.push(lines[i]);
                i += 1;
            }
            // Trim leading and trailing empty lines
            while user_content.first().map_or(false, |l| l.trim().is_empty()) {
                user_content.remove(0);
            }
            while user_content.last().map_or(false, |l| l.trim().is_empty()) {
                user_content.pop();
            }
            if !user_content.is_empty() {
                messages.push(("user".to_string(), user_content.join("\n")));
            }
        } else if lines[i].starts_with("## Assistant") {
            i += 1;
            let mut assistant_content = Vec::new();
            // Collect all lines until next header
            while i < lines.len() && !lines[i].starts_with("## ") {
                assistant_content.push(lines[i]);
                i += 1;
            }
            // Trim leading and trailing empty lines
            while assistant_content
                .first()
                .map_or(false, |l| l.trim().is_empty())
            {
                assistant_content.remove(0);
            }
            while assistant_content
                .last()
                .map_or(false, |l| l.trim().is_empty())
            {
                assistant_content.pop();
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

pub fn list_conversations() -> io::Result<Vec<ConversationInfo>> {
    let conversations_dir = get_conversations_dir()?;

    if !conversations_dir.exists() {
        return Ok(Vec::new());
    }

    let mut conversations = Vec::new();

    for entry in fs::read_dir(&conversations_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            let content = fs::read_to_string(&path)?;
            let (_, title) = parse_markdown_conversation(&content);

            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            conversations.push(ConversationInfo {
                id,
                title,
                file_path: path,
            });
        }
    }

    // Sort by modification time (newest first)
    conversations.sort_by(|a, b| {
        let a_time = fs::metadata(&a.file_path).and_then(|m| m.modified()).ok();
        let b_time = fs::metadata(&b.file_path).and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    Ok(conversations)
}
