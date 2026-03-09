use anyhow::Result;

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

pub fn get_from_clipboard() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new()?;
    let text = clipboard.get_text()?;
    Ok(text)
}
