use crossterm::event::{self, Event, KeyCode};
use ratatui::{Frame, widgets::{Block, Borders, Paragraph}};

use crate::tui::terminal::TerminalSession;

pub async fn run() -> anyhow::Result<()> {
    let mut terminal = TerminalSession::enter()?;

    loop {
        terminal.draw(render)?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn render(frame: &mut Frame<'_>) {
    let area = frame.area();
    let block = Block::default().title(" EMSYS CLI ").borders(Borders::ALL);
    let paragraph = Paragraph::new("Project foundation ready\n\nPress q or Esc to quit.").block(block);
    frame.render_widget(paragraph, area);
}
