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
    application::income_statement::{IncomeStatementDetail, IncomeStatementService},
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        income_statement::{IncomeStatement, IncomeStatementSummary, SummaryTotalLine},
    },
    tui::{action::Action, event, terminal::TerminalSession},
};

#[derive(Debug)]
enum ScreenState {
    Loading,
    Loaded(Box<IncomeStatementDetail>),
    Error(String),
}

#[derive(Debug)]
struct LoadMessage {
    generation: u64,
    result: anyhow::Result<IncomeStatementDetail>,
}

pub struct App {
    context: AppContext,
    should_quit: bool,
    state: ScreenState,
    load_generation: u64,
    load_started_at: Option<Instant>,
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
            load_generation: 0,
            load_started_at: None,
            scroll_offset: 0,
            loader_tx,
            loader_rx,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = TerminalSession::enter()?;
        self.refresh();

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
            Action::Refresh => self.refresh(),
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

    fn refresh(&mut self) {
        self.load_generation += 1;
        self.load_started_at = Some(Instant::now());
        self.state = ScreenState::Loading;
        self.scroll_offset = 0;

        let generation = self.load_generation;
        let sender = self.loader_tx.clone();
        let config = self.context.config.clone();

        tokio::spawn(async move {
            let api = EmsysApiClient::new(&config);
            let service = IncomeStatementService::new(api);
            let result = service.latest().await;
            let _ = sender.send(LoadMessage { generation, result });
        });
    }

    fn receive_loads(&mut self) {
        while let Ok(message) = self.loader_rx.try_recv() {
            if message.generation != self.load_generation {
                continue;
            }

            self.state = match message.result {
                Ok(detail) => ScreenState::Loaded(Box::new(detail)),
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

        let summary_width = chunks[2].width.saturating_sub(2).max(20) as usize;
        let scroll_offset = match &self.state {
            ScreenState::Loaded(detail) => scroll_offset(
                self.scroll_offset,
                summary_lines(&detail.summary, summary_width).len(),
                chunks[2].height,
            ),
            _ => 0,
        };

        match &self.state {
            ScreenState::Loaded(detail) => {
                let metadata = Paragraph::new(metadata_lines(&detail.statement, &detail.summary))
                    .block(
                        Block::default()
                            .title(" Statement ")
                            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM),
                    );
                frame.render_widget(Clear, chunks[1]);
                frame.render_widget(metadata, chunks[1]);

                let summary_lines = visible_lines(
                    summary_lines(&detail.summary, summary_width),
                    scroll_offset,
                    chunks[2].height,
                );
                let summary = Paragraph::new(summary_lines).block(
                    Block::default()
                        .title(" Summary Totals ")
                        .borders(Borders::LEFT | Borders::RIGHT),
                );
                frame.render_widget(Clear, chunks[2]);
                frame.render_widget(summary, chunks[2]);
            }
            _ => {
                let body = Paragraph::new(status_lines(&self.state, self.load_started_at))
                    .block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
                let area = merged_area(chunks[1], chunks[2]);
                frame.render_widget(Clear, area);
                frame.render_widget(body, area);
            }
        }

        let footer = Paragraph::new(key_menu_lines(scroll_offset))
            .block(Block::default().title(" Keys ").borders(Borders::ALL));
        frame.render_widget(Clear, chunks[3]);
        frame.render_widget(footer, chunks[3]);
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
                Line::from("Loading latest income statement..."),
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

fn metadata_lines(
    statement: &IncomeStatement,
    summary: &IncomeStatementSummary,
) -> Vec<Line<'static>> {
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

    vec![
        Line::from(vec![
            Span::styled("Statement: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("#{}", statement.id)),
            Span::raw("   "),
            Span::styled("Date: ", Style::default().fg(Color::Gray)),
            Span::raw(statement.date.clone()),
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

fn key_menu_lines(scroll_offset: u16) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("Up/k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Scroll up   "),
            Span::styled("Down/j", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Scroll down   "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Page"),
        ]),
        Line::from(vec![
            Span::styled("Home/End", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Top/bottom   "),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Refresh   "),
            Span::styled("q/Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Quit   "),
            Span::styled(
                format!("Line {}", scroll_offset.saturating_add(1)),
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

    fn line_contains(line: &Line<'_>, expected: &str) -> bool {
        line.spans
            .iter()
            .any(|span| span.content.as_ref() == expected)
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
}
