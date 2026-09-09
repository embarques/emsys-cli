#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Refresh,
    ScrollDown,
    ScrollEnd,
    ScrollPageDown,
    ScrollPageUp,
    ScrollStart,
    ScrollUp,
}
