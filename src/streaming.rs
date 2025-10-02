use crate::render::render_markdown;
use futures::StreamExt;
use std::io::{self, Write};
use std::pin::Pin;

pub async fn stream_and_render_response(
    mut stream: Pin<Box<dyn futures::Stream<Item = Result<String, Box<dyn std::error::Error + Send>>> + Send>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut full_response = String::new();
    let mut current_line = String::new();
    let mut buffer = String::new();
    let mut in_code_block = false;

    // Stream and render with proper buffering for markdown elements
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                if !chunk.is_empty() {
                    full_response.push_str(&chunk);

                    for ch in chunk.chars() {
                        current_line.push(ch);

                        if ch == '\n' {
                            let trimmed = current_line.trim();

                            // Check if we're entering or exiting a code block
                            if trimmed.starts_with("```") {
                                if in_code_block {
                                    // End of code block - render it
                                    buffer.push_str(&current_line);
                                    render_markdown(&buffer)?;
                                    buffer.clear();
                                    in_code_block = false;
                                } else {
                                    // Flush any pending buffer before code block
                                    if !buffer.is_empty() {
                                        render_markdown(&buffer)?;
                                        buffer.clear();
                                    }
                                    // Start of code block
                                    buffer.push_str(&current_line);
                                    in_code_block = true;
                                }
                            } else if in_code_block {
                                // Inside code block - accumulate
                                buffer.push_str(&current_line);
                            } else if trimmed.is_empty() {
                                // Empty line - flush buffer and render
                                if !buffer.is_empty() {
                                    render_markdown(&buffer)?;
                                    buffer.clear();
                                }
                                println!();
                            } else if trimmed.starts_with('#') {
                                // Header - flush buffer, then render header alone
                                if !buffer.is_empty() {
                                    render_markdown(&buffer)?;
                                    buffer.clear();
                                }
                                render_markdown(&current_line)?;
                            } else if is_list_item(trimmed) {
                                // List item - accumulate
                                buffer.push_str(&current_line);
                            } else {
                                // Regular text - accumulate
                                buffer.push_str(&current_line);
                            }

                            current_line.clear();
                            io::stdout().flush()?;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("\nStream error: {}", e);
                break;
            }
        }
    }

    // Render any remaining content
    if !current_line.is_empty() {
        buffer.push_str(&current_line);
    }
    if !buffer.is_empty() {
        render_markdown(&buffer)?;
    }

    Ok(full_response)
}

fn is_list_item(trimmed: &str) -> bool {
    trimmed.starts_with('-')
        || trimmed.starts_with('*')
        || trimmed.starts_with('+')
        || (trimmed.len() > 2
            && trimmed.chars().next().unwrap().is_numeric()
            && trimmed.chars().nth(1) == Some('.'))
}
