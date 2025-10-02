use termimad::MadSkin;

pub fn get_markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default();

    // Customize the skin for better readability
    skin.set_headers_fg(crossterm::style::Color::Cyan);
    skin.bold.set_fg(crossterm::style::Color::Yellow);
    skin.italic.set_fg(crossterm::style::Color::Green);
    skin.inline_code.set_fg(crossterm::style::Color::Magenta);
    skin.code_block.set_fg(crossterm::style::Color::Blue);

    // Add left padding for better readability
    skin.paragraph.set_fgbg(crossterm::style::Color::Reset, crossterm::style::Color::Reset);
    skin.paragraph.left_margin = 2;
    skin.headers[0].left_margin = 2;
    skin.headers[1].left_margin = 2;
    skin.headers[2].left_margin = 2;
    skin.code_block.left_margin = 4;

    skin
}

pub fn render_markdown(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let skin = get_markdown_skin();

    // Print the text with proper formatting
    println!("{}", skin.term_text(text));

    Ok(())
}
