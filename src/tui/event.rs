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
            KeyCode::Char('r') => Ok(Some(Action::Refresh)),
            KeyCode::Char('j') | KeyCode::Down => Ok(Some(Action::ScrollDown)),
            KeyCode::Char('k') | KeyCode::Up => Ok(Some(Action::ScrollUp)),
            KeyCode::PageDown => Ok(Some(Action::ScrollPageDown)),
            KeyCode::PageUp => Ok(Some(Action::ScrollPageUp)),
            KeyCode::Home => Ok(Some(Action::ScrollStart)),
            KeyCode::End => Ok(Some(Action::ScrollEnd)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}
