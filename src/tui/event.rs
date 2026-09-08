use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::tui::action::Action;

pub fn next_action(timeout: Duration) -> std::io::Result<Option<Action>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Ok(Some(Action::Quit)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}
