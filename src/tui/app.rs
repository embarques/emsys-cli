use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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
        }
    }

    fn refresh(&mut self) {
        self.load_generation += 1;
        self.load_started_at = Some(Instant::now());
        self.state = ScreenState::Loading;

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
        }
    }

    fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(3),
            ])
            .split(area);

        let header = Paragraph::new(header_lines(&self.context, &self.state)).block(
            Block::default()
                .title(" EMSYS - Income Statement ")
                .borders(Borders::ALL),
        );
        frame.render_widget(header, chunks[0]);

        let body = Paragraph::new(body_lines(&self.state, self.load_started_at))
            .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(body, chunks[1]);

        let footer = Paragraph::new(vec![Line::from("r Refresh"), Line::from("q/Esc Quit")])
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }
}

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

fn body_lines(state: &ScreenState, load_started_at: Option<Instant>) -> Vec<Line<'static>> {
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
        ScreenState::Loaded(detail) => loaded_lines(&detail.statement, &detail.summary),
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

fn loaded_lines(
    statement: &IncomeStatement,
    summary: &IncomeStatementSummary,
) -> Vec<Line<'static>> {
    let branch = statement
        .branch
        .as_ref()
        .map(|branch| branch.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("-");
    let currency = if summary.currency.is_empty() {
        statement.currency.as_str()
    } else {
        summary.currency.as_str()
    };

    let mut lines = vec![
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
        Line::from(""),
        Line::from(Span::styled(
            "Summary Totals",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];

    if summary.totals.is_empty() {
        lines.push(Line::from("No summary totals returned by the API."));
    } else {
        for line in &summary.totals {
            push_summary_line(&mut lines, line, 0);
        }
    }

    lines
}

fn push_summary_line(lines: &mut Vec<Line<'static>>, line: &SummaryTotalLine, depth: usize) {
    let indent = "  ".repeat(depth);
    let style = if depth == 0 {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    lines.push(Line::from(vec![
        Span::raw(indent),
        Span::styled(line.header.clone(), style),
        Span::raw("  "),
        Span::styled(format_money(line.value), style),
    ]));

    for detail in &line.details {
        push_summary_line(lines, detail, depth + 1);
    }
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
}
