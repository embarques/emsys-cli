#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    JournalNextPage,
    JournalPreviousPage,
    Quit,
    Refresh,
    ScrollDown,
    ScrollEnd,
    ScrollPageDown,
    ScrollPageUp,
    ScrollStart,
    ScrollUp,
    SelectNextStatement,
    SelectPreviousStatement,
    ShowEntries,
    ShowTotals,
}
