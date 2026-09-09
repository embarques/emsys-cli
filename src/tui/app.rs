use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    application::income_statement::{IncomeStatementScreen, IncomeStatementService, JournalPage},
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        income_statement::{IncomeStatementSummary, SummaryTotalLine},
        journal::Journal,
    },
    tui::{action::Action, event, terminal::TerminalSession},
};

#[derive(Debug)]
enum ScreenState {
    Loading,
    Loaded(Box<IncomeStatementScreen>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Entries,
    Totals,
}

#[derive(Debug, Clone, Copy)]
struct LoadTarget {
    statement_page: u64,
    selected_index: usize,
    journal_page: u64,
}

#[derive(Debug)]
struct LoadMessage {
    generation: u64,
    target: LoadTarget,
    result: anyhow::Result<IncomeStatementScreen>,
}

pub struct App {
    context: AppContext,
    should_quit: bool,
    state: ScreenState,
    view_mode: ViewMode,
    load_generation: u64,
    load_started_at: Option<Instant>,
    active_target: LoadTarget,
    scroll_offset: u16,
    loader_tx: mpsc::Sender<LoadMessage>,
    loader_rx: mpsc::Receiver<LoadMessage>,
}

impl App {
    pub fn new(context: AppContext) -> Self {
        let (loader_tx, loader_rx) = mpsc::channel();

        Self {
            context,
            should_quit: false,
            state: ScreenState::Loading,
            view_mode: ViewMode::Entries,
            load_generation: 0,
            load_started_at: None,
            active_target: LoadTarget {
                statement_page: 1,
                selected_index: 0,
                journal_page: 1,
            },
            scroll_offset: 0,
            loader_tx,
            loader_rx,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = TerminalSession::enter()?;
        self.load(self.active_target);

        while !self.should_quit {
            self.receive_loads();
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
            Action::Refresh => self.load(self.current_target()),
            Action::JournalNextPage => self.next_journal_page(),
            Action::JournalPreviousPage => self.previous_journal_page(),
            Action::SelectNextStatement => self.select_next_statement(),
            Action::SelectPreviousStatement => self.select_previous_statement(),
            Action::ShowEntries => self.set_view_mode(ViewMode::Entries),
            Action::ShowTotals => self.set_view_mode(ViewMode::Totals),
            Action::ScrollDown => self.scroll_offset = self.scroll_offset.saturating_add(1),
            Action::ScrollEnd => self.scroll_offset = u16::MAX,
            Action::ScrollPageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(PAGE_SCROLL)
            }
            Action::ScrollPageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(PAGE_SCROLL)
            }
            Action::ScrollStart => self.scroll_offset = 0,
            Action::ScrollUp => self.scroll_offset = self.scroll_offset.saturating_sub(1),
        }
    }

    fn current_target(&self) -> LoadTarget {
        match &self.state {
            ScreenState::Loaded(screen) => LoadTarget {
                statement_page: screen.statement_page,
                selected_index: screen.selected_index,
                journal_page: screen.journals.page,
            },
            _ => self.active_target,
        }
    }

    fn next_journal_page(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state
            && has_next_journal_page(&screen.journals)
        {
            self.load(LoadTarget {
                statement_page: screen.statement_page,
                selected_index: screen.selected_index,
                journal_page: screen.journals.page.saturating_add(1),
            });
        }
    }

    fn previous_journal_page(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state
            && screen.journals.page > 1
        {
            self.load(LoadTarget {
                statement_page: screen.statement_page,
                selected_index: screen.selected_index,
                journal_page: screen.journals.page - 1,
            });
        }
    }

    fn select_next_statement(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state {
            let next_index = screen.selected_index.saturating_add(1);
            if next_index < screen.statements.len() {
                self.load(LoadTarget {
                    statement_page: screen.statement_page,
                    selected_index: next_index,
                    journal_page: 1,
                });
            } else if has_next_statement_page(screen) {
                self.load(LoadTarget {
                    statement_page: screen.statement_page.saturating_add(1),
                    selected_index: 0,
                    journal_page: 1,
                });
            }
        }
    }

    fn select_previous_statement(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state
            && screen.selected_index > 0
        {
            self.load(LoadTarget {
                statement_page: screen.statement_page,
                selected_index: screen.selected_index - 1,
                journal_page: 1,
            });
        } else if let ScreenState::Loaded(screen) = &self.state
            && screen.statement_page > 1
        {
            self.load(LoadTarget {
                statement_page: screen.statement_page - 1,
                selected_index: usize::MAX,
                journal_page: 1,
            });
        }
    }

    fn set_view_mode(&mut self, view_mode: ViewMode) {
        if self.view_mode != view_mode {
            self.view_mode = view_mode;
            self.scroll_offset = 0;
        }
    }

    fn load(&mut self, target: LoadTarget) {
        self.load_generation += 1;
        self.load_started_at = Some(Instant::now());
        self.active_target = target;
        self.state = ScreenState::Loading;
        self.scroll_offset = 0;

        let generation = self.load_generation;
        let sender = self.loader_tx.clone();
        let config = self.context.config.clone();

        tokio::spawn(async move {
            let api = EmsysApiClient::new(&config);
            let service = IncomeStatementService::new(api);
            let result = service
                .screen(
                    target.statement_page,
                    target.selected_index,
                    target.journal_page,
                )
                .await;
            let _ = sender.send(LoadMessage {
                generation,
                target,
                result,
            });
        });
    }

    fn receive_loads(&mut self) {
        while let Ok(message) = self.loader_rx.try_recv() {
            if message.generation != self.load_generation {
                continue;
            }

            self.active_target = message.target;
            self.state = match message.result {
                Ok(screen) => ScreenState::Loaded(Box::new(screen)),
                Err(error) => ScreenState::Error(safe_error_message(error)),
            };
            self.load_started_at = None;
            self.scroll_offset = 0;
        }
    }

    fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(4),
                Constraint::Length(4),
            ])
            .split(area);

        let header = Paragraph::new(header_lines(&self.context, &self.state)).block(
            Block::default()
                .title(" EMSYS - Income Statement ")
                .borders(Borders::ALL),
        );
        frame.render_widget(Clear, chunks[0]);
        frame.render_widget(header, chunks[0]);

        match &self.state {
            ScreenState::Loaded(screen) => self.render_loaded(frame, chunks.as_ref(), screen),
            _ => {
                let body = Paragraph::new(status_lines(&self.state, self.load_started_at))
                    .block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
                let area = merged_area(chunks[1], chunks[2]);
                frame.render_widget(Clear, area);
                frame.render_widget(body, area);
            }
        }

        let footer = Paragraph::new(key_menu_lines(
            &self.state,
            self.view_mode,
            self.scroll_offset,
        ))
        .block(Block::default().title(" Keys ").borders(Borders::ALL));
        frame.render_widget(Clear, chunks[3]);
        frame.render_widget(footer, chunks[3]);
    }

    fn render_loaded(
        &self,
        frame: &mut Frame<'_>,
        chunks: &[Rect],
        screen: &IncomeStatementScreen,
    ) {
        let metadata = Paragraph::new(metadata_lines(screen, self.view_mode)).block(
            Block::default()
                .title(" Statement ")
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM),
        );
        frame.render_widget(Clear, chunks[1]);
        frame.render_widget(metadata, chunks[1]);

        let body_width = chunks[2].width.saturating_sub(2).max(20) as usize;
        let body_lines = match self.view_mode {
            ViewMode::Entries => journal_lines(screen, body_width),
            ViewMode::Totals => summary_lines(&screen.detail.summary, body_width),
        };
        let scroll_offset = scroll_offset(self.scroll_offset, body_lines.len(), chunks[2].height);
        let title = match self.view_mode {
            ViewMode::Entries => " Journal Entries ",
            ViewMode::Totals => " Income Totals ",
        };

        let body = Paragraph::new(visible_lines(body_lines, scroll_offset, chunks[2].height))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::LEFT | Borders::RIGHT),
            );
        frame.render_widget(Clear, chunks[2]);
        frame.render_widget(body, chunks[2]);
    }
}

const PAGE_SCROLL: u16 = 8;

pub fn run(context: AppContext) -> anyhow::Result<()> {
    App::new(context).run()
}

fn header_lines(context: &AppContext, state: &ScreenState) -> Vec<Line<'static>> {
    let connection = match state {
        ScreenState::Loaded(_) => "Connected",
        ScreenState::Loading => "Loading",
        ScreenState::Error(_) => "Needs attention",
    };

    vec![Line::from(vec![
        Span::styled("Environment: ", Style::default().fg(Color::Gray)),
        Span::raw(environment_label(&context.config.api_url)),
        Span::raw("   "),
        Span::styled("Status: ", Style::default().fg(Color::Gray)),
        Span::raw(connection),
    ])]
}

fn status_lines(state: &ScreenState, load_started_at: Option<Instant>) -> Vec<Line<'static>> {
    match state {
        ScreenState::Loading => {
            let elapsed = load_started_at
                .map(|started| started.elapsed().as_secs())
                .unwrap_or_default();
            vec![
                Line::from(""),
                Line::from("Loading income statement journal entries..."),
                Line::from(format!("Elapsed: {elapsed}s")),
            ]
        }
        ScreenState::Loaded(_) => Vec::new(),
        ScreenState::Error(message) => vec![
            Line::from(""),
            Line::from(Span::styled(
                "Unable to load income statement",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(message.clone()),
            Line::from(""),
            Line::from("Press r to retry or q to quit."),
        ],
    }
}

fn metadata_lines(screen: &IncomeStatementScreen, view_mode: ViewMode) -> Vec<Line<'static>> {
    let statement = &screen.detail.statement;
    let summary = &screen.detail.summary;
    let branch = statement
        .branch
        .as_ref()
        .map(|branch| branch.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("-");
    let currency = if !summary.currency.is_empty() {
        summary.currency.as_str()
    } else if !statement.currency.is_empty() {
        statement.currency.as_str()
    } else {
        "-"
    };
    let mode = match view_mode {
        ViewMode::Entries => "Entries",
        ViewMode::Totals => "Totals",
    };

    vec![
        Line::from(vec![
            Span::styled("Statement: ", Style::default().fg(Color::Gray)),
            Span::raw(format!(
                "#{} ({}/{})",
                statement.id,
                statement_position(screen),
                screen.statement_total.max(screen.statements.len() as u64)
            )),
            Span::raw("   "),
            Span::styled("Date: ", Style::default().fg(Color::Gray)),
            Span::raw(statement.date.clone()),
            Span::raw("   "),
            Span::styled("View: ", Style::default().fg(Color::Gray)),
            Span::raw(mode),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Gray)),
            Span::raw(status_label(&statement.status)),
            Span::raw("   "),
            Span::styled("Branch: ", Style::default().fg(Color::Gray)),
            Span::raw(branch.to_string()),
            Span::raw("   "),
            Span::styled("Currency: ", Style::default().fg(Color::Gray)),
            Span::raw(currency.to_string()),
        ]),
    ]
}

fn journal_lines(screen: &IncomeStatementScreen, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let journals = &screen.journals;

    if journals.entries.is_empty() {
        lines.push(Line::from(
            "No journal entries returned for this income statement.",
        ));
        return lines;
    }

    for (index, journal) in journals.entries.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        push_journal_lines(&mut lines, journal, width);
    }

    lines
}

fn push_journal_lines(lines: &mut Vec<Line<'static>>, journal: &Journal, width: usize) {
    let date = journal
        .date
        .as_deref()
        .map(short_date)
        .unwrap_or_else(|| "-".to_string());
    let reference = if journal.ref_number.is_empty() {
        "-".to_string()
    } else {
        journal.ref_number.clone()
    };
    let transaction_type = if journal.transaction_type.is_empty() {
        "-".to_string()
    } else {
        journal.transaction_type.clone()
    };
    let amount = format_money(journal.transaction_amount);
    let label = format!(
        "{}  {}  {}  {}",
        date,
        reference,
        transaction_type,
        truncate(&journal.description, width.saturating_sub(30).max(12))
    );
    let label = truncate(&label, width.saturating_sub(amount.len() + 2).max(12));
    let spaces = width.saturating_sub(label.len() + amount.len());

    lines.push(Line::from(vec![
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ".repeat(spaces)),
        Span::styled(amount, Style::default().add_modifier(Modifier::BOLD)),
    ]));

    for account in &journal.accounts {
        let account_name = if account.name.is_empty() {
            format!("Account #{}", account.id)
        } else {
            account.name.clone()
        };
        let debit = if account.debit.abs() > f64::EPSILON {
            format_money(account.debit)
        } else {
            "-".to_string()
        };
        let credit = if account.credit.abs() > f64::EPSILON {
            format_money(account.credit)
        } else {
            "-".to_string()
        };
        let prefix = format!(
            "  {}",
            truncate(&account_name, width.saturating_sub(24).max(12))
        );
        let spacer = width.saturating_sub(prefix.len() + debit.len() + credit.len() + 8);
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::raw(" ".repeat(spacer)),
            Span::styled("Dr ", Style::default().fg(Color::Gray)),
            Span::raw(debit),
            Span::raw("  "),
            Span::styled("Cr ", Style::default().fg(Color::Gray)),
            Span::raw(credit),
        ]));
    }
}

fn summary_lines(summary: &IncomeStatementSummary, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if summary.totals.is_empty() {
        lines.push(Line::from("No summary totals returned by the API."));
    } else {
        for line in &summary.totals {
            push_summary_line(&mut lines, line, 0, width);
        }
    }

    lines
}

fn merged_area(top: Rect, bottom: Rect) -> Rect {
    Rect {
        x: top.x,
        y: top.y,
        width: top.width,
        height: top.height.saturating_add(bottom.height),
    }
}

fn push_summary_line(
    lines: &mut Vec<Line<'static>>,
    line: &SummaryTotalLine,
    depth: usize,
    width: usize,
) {
    let style = if depth == 0 {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    lines.push(summary_line(&line.header, line.value, depth, width, style));

    for detail in &line.details {
        push_summary_line(lines, detail, depth + 1, width);
    }
}

fn summary_line(
    label: &str,
    value: f64,
    depth: usize,
    width: usize,
    style: Style,
) -> Line<'static> {
    let money = format_money(value);
    let indent_width = depth * 2;
    let label_width = width.saturating_sub(indent_width + money.len() + 2).max(12);
    let display_label = truncate(label, label_width);
    let spaces = width.saturating_sub(indent_width + display_label.len() + money.len());

    Line::from(vec![
        Span::raw(" ".repeat(indent_width)),
        Span::styled(display_label, style),
        Span::raw(" ".repeat(spaces)),
        Span::styled(money, style),
    ])
}

fn environment_label(api_url: &str) -> String {
    if api_url.contains("localhost") || api_url.contains("127.0.0.1") {
        "Local".into()
    } else {
        "Configured API".into()
    }
}

fn status_label(status: &str) -> String {
    let mut chars = status.chars();
    match chars.next() {
        Some(first) => {
            let mut label = String::new();
            label.extend(first.to_uppercase());
            label.push_str(chars.as_str());
            label
        }
        None => "-".into(),
    }
}

fn format_money(value: f64) -> String {
    format!("${value:.2}")
}

fn short_date(value: &str) -> String {
    value.split('T').next().unwrap_or(value).to_string()
}

fn has_next_journal_page(journals: &JournalPage) -> bool {
    let per_page = journals
        .results_per_page
        .max(journals.entries.len() as u64)
        .max(1);
    journals.page.saturating_mul(per_page) < journals.total
}

fn has_next_statement_page(screen: &IncomeStatementScreen) -> bool {
    let per_page = screen
        .statement_results_per_page
        .max(screen.statements.len() as u64)
        .max(1);
    screen.statement_page.saturating_mul(per_page) < screen.statement_total
}

fn statement_position(screen: &IncomeStatementScreen) -> u64 {
    let per_page = screen
        .statement_results_per_page
        .max(screen.statements.len() as u64)
        .max(1);
    (screen.statement_page.saturating_sub(1) * per_page) + screen.selected_index as u64 + 1
}

fn scroll_offset(offset: u16, line_count: usize, area_height: u16) -> u16 {
    let visible_lines = area_height.saturating_sub(2) as usize;
    let max_offset = line_count.saturating_sub(visible_lines) as u16;
    offset.min(max_offset)
}

fn visible_lines(lines: Vec<Line<'static>>, offset: u16, area_height: u16) -> Vec<Line<'static>> {
    let visible_count = area_height.saturating_sub(2) as usize;
    let mut visible: Vec<_> = lines
        .into_iter()
        .skip(offset as usize)
        .take(visible_count)
        .collect();

    while visible.len() < visible_count {
        visible.push(Line::from(""));
    }

    visible
}

fn key_menu_lines(
    state: &ScreenState,
    view_mode: ViewMode,
    scroll_offset: u16,
) -> Vec<Line<'static>> {
    let page_label = match state {
        ScreenState::Loaded(screen) => format!(
            "S {}/{} J {}",
            statement_position(screen),
            screen.statement_total.max(screen.statements.len() as u64),
            screen.journals.page
        ),
        _ => "Loading".to_string(),
    };
    let toggle = match view_mode {
        ViewMode::Entries => ("t", "Totals"),
        ViewMode::Totals => ("e", "Entries"),
    };

    vec![
        Line::from(vec![
            Span::styled("Left/Right", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Statement   "),
            Span::styled("p/n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Journal page   "),
            Span::styled(toggle.0, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {}   ", toggle.1)),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Refresh"),
        ]),
        Line::from(vec![
            Span::styled("Up/Down", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Scroll   "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Move   "),
            Span::styled("q/Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Quit   "),
            Span::styled(
                format!("{page_label}  Line {}", scroll_offset.saturating_add(1)),
                Style::default().fg(Color::Gray),
            ),
        ]),
    ]
}

fn truncate(value: &str, max_width: usize) -> String {
    if value.len() <= max_width {
        return value.to_string();
    }

    if max_width <= 1 {
        return value.chars().take(max_width).collect();
    }

    let mut truncated: String = value.chars().take(max_width - 1).collect();
    truncated.push('~');
    truncated
}

fn safe_error_message(error: anyhow::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_money_with_two_decimals() {
        assert_eq!(format_money(11335.0), "$11335.00");
        assert_eq!(format_money(-12.5), "$-12.50");
    }

    #[test]
    fn renders_summary_lines_recursively() {
        let mut lines = Vec::new();
        push_summary_line(
            &mut lines,
            &SummaryTotalLine {
                header: "Total Ingresos".into(),
                value: 12800.0,
                order: 1,
                details: vec![SummaryTotalLine {
                    header: "Efectivo".into(),
                    value: 4200.0,
                    order: 1,
                    details: Vec::new(),
                }],
            },
            0,
            40,
        );

        assert_eq!(lines.len(), 2);
        assert!(line_contains(&lines[0], "Total Ingresos"));
        assert!(line_contains(&lines[1], "Efectivo"));
    }

    #[test]
    fn summary_line_right_aligns_money() {
        let line = summary_line("Total Ingresos", 12800.0, 0, 32, Style::default());
        let rendered: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(rendered.len(), 32);
        assert!(rendered.ends_with("$12800.00"));
    }

    #[test]
    fn journal_lines_include_entry_and_account_amounts() {
        let mut lines = Vec::new();
        push_journal_lines(
            &mut lines,
            &Journal {
                id: serde_json::json!("66f000000000000000000001"),
                description: "Invoice payment".into(),
                date: Some("2026-09-08T00:00:00Z".into()),
                ref_number: "A-100".into(),
                payment_method: None,
                currency: "USD".into(),
                rate: 1.0,
                transaction_type: "PAYMENT".into(),
                invoice: None,
                income_statement: None,
                customer: None,
                employee: None,
                accounts: vec![crate::infrastructure::journal::JournalAccount {
                    id: serde_json::json!(1),
                    name: "Cash on Hand".into(),
                    account_type: "ASSET".into(),
                    debit: 120.0,
                    credit: 0.0,
                }],
                transaction_amount: 120.0,
                transaction_balance: 0.0,
            },
            80,
        );

        assert!(line_contains(&lines[0], "$120.00"));
        assert!(line_contains(&lines[1], "Cash on Hand"));
    }

    #[test]
    fn detects_next_journal_page() {
        assert!(has_next_journal_page(&JournalPage {
            entries: Vec::new(),
            page: 1,
            results_per_page: 10,
            total: 25,
            subtotal: 10,
        }));
        assert!(!has_next_journal_page(&JournalPage {
            entries: Vec::new(),
            page: 3,
            results_per_page: 10,
            total: 25,
            subtotal: 5,
        }));
    }

    #[test]
    fn detects_next_statement_page() {
        let screen = screen_with_statement_page(1, 19, 20, 41);

        assert!(has_next_statement_page(&screen));
        assert_eq!(statement_position(&screen), 20);
    }

    #[test]
    fn detects_last_statement_page() {
        let screen = screen_with_statement_page(3, 0, 20, 41);

        assert!(!has_next_statement_page(&screen));
        assert_eq!(statement_position(&screen), 41);
    }

    #[test]
    fn clamps_scroll_offset_to_visible_content() {
        assert_eq!(scroll_offset(50, 20, 10), 12);
        assert_eq!(scroll_offset(5, 4, 10), 0);
    }

    #[test]
    fn selects_visible_summary_window() {
        let lines = vec![
            Line::from("one"),
            Line::from("two"),
            Line::from("three"),
            Line::from("four"),
        ];
        let visible = visible_lines(lines, 1, 4);

        assert_eq!(visible.len(), 2);
        assert!(line_contains(&visible[0], "two"));
        assert!(line_contains(&visible[1], "three"));
    }

    fn line_contains(line: &Line<'_>, expected: &str) -> bool {
        line.spans
            .iter()
            .any(|span| span.content.as_ref().contains(expected))
    }

    fn screen_with_statement_page(
        statement_page: u64,
        selected_index: usize,
        results_per_page: u64,
        total: u64,
    ) -> IncomeStatementScreen {
        IncomeStatementScreen {
            statements: vec![crate::infrastructure::income_statement::IncomeStatement {
                id: 1,
                date: "2026-09-08T00:00:00Z".into(),
                branch: None,
                container: None,
                delivery: None,
                rate: 1.0,
                currency: "USD".into(),
                status: "closed".into(),
                summary_total: None,
                created_at: None,
                updated_at: None,
            }],
            statement_page,
            statement_results_per_page: results_per_page,
            statement_total: total,
            selected_index,
            detail: crate::application::income_statement::IncomeStatementDetail {
                statement: crate::infrastructure::income_statement::IncomeStatement {
                    id: 1,
                    date: "2026-09-08T00:00:00Z".into(),
                    branch: None,
                    container: None,
                    delivery: None,
                    rate: 1.0,
                    currency: "USD".into(),
                    status: "closed".into(),
                    summary_total: None,
                    created_at: None,
                    updated_at: None,
                },
                summary: IncomeStatementSummary {
                    currency: "USD".into(),
                    rate: 1.0,
                    totals: Vec::new(),
                },
            },
            journals: JournalPage {
                entries: Vec::new(),
                page: 1,
                results_per_page: 10,
                total: 0,
                subtotal: 0,
            },
        }
    }
}
