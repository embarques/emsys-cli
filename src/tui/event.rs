use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::tui::action::Action;

pub fn next_action(timeout: Duration) -> std::io::Result<Option<Action>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Esc => Ok(Some(Action::Quit)),
            KeyCode::Tab => Ok(Some(Action::FormNextField)),
            KeyCode::BackTab => Ok(Some(Action::FormPreviousField)),
            KeyCode::Enter => Ok(Some(Action::FormSubmit)),
            KeyCode::Backspace => Ok(Some(Action::FormBackspace)),
            KeyCode::Right => Ok(Some(Action::FormNextChoice)),
            KeyCode::Left => Ok(Some(Action::FormPreviousChoice)),
            KeyCode::Down => Ok(Some(Action::FormNextField)),
            KeyCode::Up => Ok(Some(Action::FormPreviousField)),
            KeyCode::PageDown => Ok(Some(Action::ScrollPageDown)),
            KeyCode::PageUp => Ok(Some(Action::ScrollPageUp)),
            KeyCode::Home => Ok(Some(Action::ScrollStart)),
            KeyCode::End => Ok(Some(Action::ScrollEnd)),
            KeyCode::Char(value)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                Ok(Some(Action::FormInput(value)))
            }
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}
