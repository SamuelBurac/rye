use termimad::MadSkin;

pub fn render_markdown(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut skin = MadSkin::default();

    // Customize the skin for better readability
    skin.set_headers_fg(crossterm::style::Color::Cyan);
    skin.bold.set_fg(crossterm::style::Color::Yellow);
    skin.italic.set_fg(crossterm::style::Color::Green);
    skin.inline_code.set_fg(crossterm::style::Color::Magenta);
    skin.code_block.set_fg(crossterm::style::Color::Blue);

    // Get terminal width for proper wrapping (max 100 chars wide)
    let terminal_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);

    let _max_width = std::cmp::min(100, terminal_width.saturating_sub(4));

    // Print with proper wrapping
    skin.print_text(text);

    Ok(())
}
