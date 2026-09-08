use std::time::Duration;

use ratatui::{
    Frame,
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    context::AppContext,
    tui::{action::Action, event, terminal::TerminalSession},
};

pub struct App {
    context: AppContext,
    should_quit: bool,
}

impl App {
    pub fn new(context: AppContext) -> Self {
        Self {
            context,
            should_quit: false,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = TerminalSession::enter()?;

        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(action) = event::next_action(Duration::from_millis(100))? {
                self.handle_action(action);
            }
        }

        Ok(())
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
        }
    }

    fn render(&self, frame: &mut Frame<'_>) {
        let _ = &self.context;
        let area = frame.area();
        let block = Block::default().title(" EMSYS CLI ").borders(Borders::ALL);
        let paragraph =
            Paragraph::new("Project foundation ready\n\nPress q or Esc to quit.").block(block);
        frame.render_widget(paragraph, area);
    }
}

pub fn run(context: AppContext) -> anyhow::Result<()> {
    App::new(context).run()
}
