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
    application::income_statement::{
        IncomeStatementScreen, IncomeStatementService, JournalPage, JournalTransactionType,
        JournalTransactionValues, TransactionLookups,
    },
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        chart_account::ChartAccount,
        income_statement::{IncomeStatement, IncomeStatementSummary, SummaryTotalLine},
        invoice::Invoice,
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

#[derive(Debug)]
struct SubmitMessage {
    generation: u64,
    result: anyhow::Result<Journal>,
}

#[derive(Debug)]
struct StatusMessage {
    generation: u64,
    result: anyhow::Result<IncomeStatement>,
}

#[derive(Debug, Clone)]
struct StatusAction {
    statement_id: u32,
    open: bool,
    error: Option<String>,
    submitting: bool,
}

#[derive(Debug, Clone)]
struct AddTransactionForm {
    values: JournalTransactionValues,
    focus: usize,
    error: Option<String>,
    submitting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormField {
    TransactionType,
    Employee,
    Invoice,
    InvoiceNumber,
    InvoiceCost,
    InvoiceDiscount,
    PaymentMethod,
    PaymentAccount,
    ZelleDate,
    ZelleName,
    CheckNumber,
    Account,
    SourceAccount,
    Amount,
    RefNumber,
    Description,
}

pub struct App {
    context: AppContext,
    should_quit: bool,
    state: ScreenState,
    view_mode: ViewMode,
    load_generation: u64,
    submit_generation: u64,
    status_generation: u64,
    load_started_at: Option<Instant>,
    active_target: LoadTarget,
    scroll_offset: u16,
    notice: Option<String>,
    add_form: Option<AddTransactionForm>,
    status_action: Option<StatusAction>,
    loader_tx: mpsc::Sender<LoadMessage>,
    loader_rx: mpsc::Receiver<LoadMessage>,
    submit_tx: mpsc::Sender<SubmitMessage>,
    submit_rx: mpsc::Receiver<SubmitMessage>,
    status_tx: mpsc::Sender<StatusMessage>,
    status_rx: mpsc::Receiver<StatusMessage>,
}

impl App {
    pub fn new(context: AppContext) -> Self {
        let (loader_tx, loader_rx) = mpsc::channel();
        let (submit_tx, submit_rx) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel();

        Self {
            context,
            should_quit: false,
            state: ScreenState::Loading,
            view_mode: ViewMode::Entries,
            load_generation: 0,
            submit_generation: 0,
            status_generation: 0,
            load_started_at: None,
            active_target: LoadTarget {
                statement_page: 1,
                selected_index: 0,
                journal_page: 1,
            },
            scroll_offset: 0,
            notice: None,
            add_form: None,
            status_action: None,
            loader_tx,
            loader_rx,
            submit_tx,
            submit_rx,
            status_tx,
            status_rx,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = TerminalSession::enter()?;
        self.load(self.active_target);

        while !self.should_quit {
            self.receive_loads();
            self.receive_submits();
            self.receive_status_changes();
            terminal.draw(|frame| self.render(frame))?;

            if let Some(action) = event::next_action(Duration::from_millis(100))? {
                self.handle_action(action);
            }
        }

        Ok(())
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit if self.add_form.is_some() => {
                self.add_form = None;
                self.notice = Some("Add transaction canceled.".into());
            }
            Action::Quit if self.status_action.is_some() => {
                self.status_action = None;
                self.notice = Some("Status change canceled.".into());
            }
            Action::Quit => self.should_quit = true,
            Action::FormNextField => self.form_next_field_or_scroll_down(),
            Action::FormPreviousField => self.form_previous_field_or_scroll_up(),
            Action::FormNextChoice => self.form_next_choice_or_statement(),
            Action::FormPreviousChoice => self.form_previous_choice_or_statement(),
            Action::FormInput(value) => self.handle_character(value),
            Action::FormBackspace => self.form_backspace(),
            Action::FormSubmit => self.submit_active_prompt(),
            Action::ScrollEnd => self.scroll_offset = u16::MAX,
            Action::ScrollPageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(PAGE_SCROLL)
            }
            Action::ScrollPageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(PAGE_SCROLL)
            }
            Action::ScrollStart => self.scroll_offset = 0,
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

    fn open_add_transaction(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state {
            self.status_action = None;
            let mut values = JournalTransactionValues::default();
            values.employee_id = screen.lookups.employees.first().map(|employee| employee.id);
            values.account_id = first_account_id(&screen.lookups, values.transaction_type);
            values.source_account_id = first_source_account_id(&screen.lookups);
            values.payment_account_id = first_payment_account_id(&screen.lookups);
            values.invoice_id = screen.lookups.invoices.first().map(Invoice::id_string);
            values.zelle_transaction_date = short_date(&screen.detail.statement.date);

            self.notice = None;
            self.add_form = Some(AddTransactionForm {
                values,
                focus: 0,
                error: None,
                submitting: false,
            });
        }
    }

    fn open_status_action(&mut self) {
        if let ScreenState::Loaded(screen) = &self.state {
            let statement = &screen.detail.statement;
            let open = !statement.status.eq_ignore_ascii_case("open");
            self.add_form = None;
            self.notice = None;
            self.status_action = Some(StatusAction {
                statement_id: statement.id,
                open,
                error: None,
                submitting: false,
            });
        }
    }

    fn form_next_field_or_scroll_down(&mut self) {
        if self.add_form.is_some() {
            self.move_form_focus(1);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
    }

    fn form_previous_field_or_scroll_up(&mut self) {
        if self.add_form.is_some() {
            self.move_form_focus(-1);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        }
    }

    fn form_next_choice_or_statement(&mut self) {
        if self.add_form.is_some() {
            self.change_form_choice(1);
        } else {
            self.select_next_statement();
        }
    }

    fn form_previous_choice_or_statement(&mut self) {
        if self.add_form.is_some() {
            self.change_form_choice(-1);
        } else {
            self.select_previous_statement();
        }
    }

    fn handle_character(&mut self, value: char) {
        if self.add_form.is_some() {
            if value == 'q' {
                self.add_form = None;
                self.notice = Some("Add transaction canceled.".into());
            } else {
                self.form_input(value);
            }
            return;
        }

        if self.status_action.is_some() {
            if value == 'q' {
                self.status_action = None;
                self.notice = Some("Status change canceled.".into());
            }
            return;
        }

        match value {
            'a' => self.open_add_transaction(),
            'c' => self.open_status_action(),
            'e' => self.set_view_mode(ViewMode::Entries),
            'j' => self.scroll_offset = self.scroll_offset.saturating_add(1),
            'k' => self.scroll_offset = self.scroll_offset.saturating_sub(1),
            'n' => self.next_journal_page(),
            'p' => self.previous_journal_page(),
            'q' => self.should_quit = true,
            'r' => self.load(self.current_target()),
            't' => self.set_view_mode(ViewMode::Totals),
            _ => {}
        }
    }

    fn move_form_focus(&mut self, step: isize) {
        let Some(form) = self.add_form.as_mut() else {
            return;
        };
        let ScreenState::Loaded(screen) = &self.state else {
            return;
        };
        let fields = form_fields(&form.values, &screen.lookups);
        form.focus = shifted_index(form.focus, fields.len(), step);
    }

    fn change_form_choice(&mut self, step: isize) {
        let Some(form) = self.add_form.as_mut() else {
            return;
        };
        let ScreenState::Loaded(screen) = &self.state else {
            return;
        };
        let fields = form_fields(&form.values, &screen.lookups);
        let Some(field) = fields.get(form.focus).copied() else {
            return;
        };

        match field {
            FormField::TransactionType => {
                let all = JournalTransactionType::all();
                let current = all
                    .iter()
                    .position(|value| *value == form.values.transaction_type)
                    .unwrap_or_default();
                form.values.transaction_type = all[shifted_index(current, all.len(), step)];
                form.values.account_id =
                    first_account_id(&screen.lookups, form.values.transaction_type);
                form.values.source_account_id = first_source_account_id(&screen.lookups);
                form.values.payment_account_id = first_payment_account_id(&screen.lookups);
                form.focus = form
                    .focus
                    .min(form_fields(&form.values, &screen.lookups).len() - 1);
            }
            FormField::Employee => {
                form.values.employee_id =
                    cycle_employee(&screen.lookups, form.values.employee_id, step);
            }
            FormField::Invoice => {
                form.values.invoice_id =
                    cycle_invoice(&screen.lookups, form.values.invoice_id.as_deref(), step);
            }
            FormField::PaymentMethod => {
                form.values.payment_method_id =
                    cycle_payment_method(&screen.lookups, form.values.payment_method_id, step);
            }
            FormField::PaymentAccount => {
                form.values.payment_account_id = cycle_account(
                    bank_accounts(&screen.lookups),
                    form.values.payment_account_id,
                    step,
                );
            }
            FormField::Account => {
                form.values.account_id = cycle_account(
                    account_options(&screen.lookups, form.values.transaction_type),
                    form.values.account_id,
                    step,
                );
            }
            FormField::SourceAccount => {
                form.values.source_account_id = cycle_account(
                    asset_accounts(&screen.lookups),
                    form.values.source_account_id,
                    step,
                );
            }
            _ => {}
        }
        form.error = None;
    }

    fn form_input(&mut self, value: char) {
        let Some(form) = self.add_form.as_mut() else {
            return;
        };
        if form.submitting || value.is_control() {
            return;
        }
        let ScreenState::Loaded(screen) = &self.state else {
            return;
        };
        let fields = form_fields(&form.values, &screen.lookups);
        let Some(field) = fields.get(form.focus).copied() else {
            return;
        };

        match field {
            FormField::Amount => push_number(&mut form.values.amount, value),
            FormField::InvoiceCost => push_number(&mut form.values.invoice_cost, value),
            FormField::InvoiceDiscount => push_number(&mut form.values.invoice_discount, value),
            FormField::InvoiceNumber => form.values.invoice_number.push(value),
            FormField::RefNumber => form.values.ref_number.push(value),
            FormField::Description => form.values.description.push(value),
            FormField::ZelleDate => form.values.zelle_transaction_date.push(value),
            FormField::ZelleName => form.values.zelle_transaction_name.push(value),
            FormField::CheckNumber => form.values.check_number.push(value),
            _ => {}
        }
        form.error = None;
    }

    fn form_backspace(&mut self) {
        let Some(form) = self.add_form.as_mut() else {
            return;
        };
        let ScreenState::Loaded(screen) = &self.state else {
            return;
        };
        let fields = form_fields(&form.values, &screen.lookups);
        let Some(field) = fields.get(form.focus).copied() else {
            return;
        };

        match field {
            FormField::Amount => pop_number(&mut form.values.amount),
            FormField::InvoiceCost => pop_number(&mut form.values.invoice_cost),
            FormField::InvoiceDiscount => pop_number(&mut form.values.invoice_discount),
            FormField::InvoiceNumber => {
                form.values.invoice_number.pop();
            }
            FormField::RefNumber => {
                form.values.ref_number.pop();
            }
            FormField::Description => {
                form.values.description.pop();
            }
            FormField::ZelleDate => {
                form.values.zelle_transaction_date.pop();
            }
            FormField::ZelleName => {
                form.values.zelle_transaction_name.pop();
            }
            FormField::CheckNumber => {
                form.values.check_number.pop();
            }
            _ => {}
        }
        form.error = None;
    }

    fn form_submit(&mut self) {
        let Some(form) = self.add_form.as_mut() else {
            return;
        };
        if form.submitting {
            return;
        }
        let ScreenState::Loaded(screen) = &self.state else {
            return;
        };

        self.submit_generation += 1;
        form.submitting = true;
        form.error = None;

        let generation = self.submit_generation;
        let sender = self.submit_tx.clone();
        let config = self.context.config.clone();
        let statement = screen.detail.statement.clone();
        let lookups = screen.lookups.clone();
        let values = form.values.clone();

        tokio::spawn(async move {
            let api = EmsysApiClient::new(&config);
            let service = IncomeStatementService::new(api);
            let result = service
                .post_transaction(&statement, &lookups, &values)
                .await;
            let _ = sender.send(SubmitMessage { generation, result });
        });
    }

    fn submit_active_prompt(&mut self) {
        if self.add_form.is_some() {
            self.form_submit();
        } else if self.status_action.is_some() {
            self.status_submit();
        }
    }

    fn status_submit(&mut self) {
        let Some(status_action) = self.status_action.as_mut() else {
            return;
        };
        if status_action.submitting {
            return;
        }

        self.status_generation += 1;
        status_action.submitting = true;
        status_action.error = None;

        let generation = self.status_generation;
        let sender = self.status_tx.clone();
        let config = self.context.config.clone();
        let statement_id = status_action.statement_id;
        let open = status_action.open;

        tokio::spawn(async move {
            let api = EmsysApiClient::new(&config);
            let service = IncomeStatementService::new(api);
            let result = service.set_statement_open(statement_id, open).await;
            let _ = sender.send(StatusMessage { generation, result });
        });
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

    fn receive_submits(&mut self) {
        while let Ok(message) = self.submit_rx.try_recv() {
            if message.generation != self.submit_generation {
                continue;
            }

            match message.result {
                Ok(journal) => {
                    self.add_form = None;
                    self.notice = Some(format!(
                        "Created {} journal transaction for {}.",
                        type_label(&journal.transaction_type),
                        format_money(journal.transaction_amount)
                    ));
                    self.load(LoadTarget {
                        journal_page: 1,
                        ..self.current_target()
                    });
                }
                Err(error) => {
                    if let Some(form) = self.add_form.as_mut() {
                        form.submitting = false;
                        form.error = Some(safe_error_message(error));
                    }
                }
            }
        }
    }

    fn receive_status_changes(&mut self) {
        while let Ok(message) = self.status_rx.try_recv() {
            if message.generation != self.status_generation {
                continue;
            }

            match message.result {
                Ok(statement) => {
                    let status = status_label(&statement.status);
                    self.status_action = None;
                    self.notice = Some(format!(
                        "Income statement #{} is now {status}.",
                        statement.id
                    ));
                    self.load(self.current_target());
                }
                Err(error) => {
                    if let Some(status_action) = self.status_action.as_mut() {
                        status_action.submitting = false;
                        status_action.error = Some(safe_error_message(error));
                    }
                }
            }
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
            self.add_form.as_ref(),
            self.status_action.as_ref(),
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
        let metadata = Paragraph::new(metadata_lines(
            screen,
            self.view_mode,
            self.notice.as_deref(),
        ))
        .block(
            Block::default()
                .title(" Statement ")
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM),
        );
        frame.render_widget(Clear, chunks[1]);
        frame.render_widget(metadata, chunks[1]);

        let body_width = chunks[2].width.saturating_sub(2).max(20) as usize;
        let (body_lines, title) = if let Some(form) = &self.add_form {
            (
                add_form_lines(screen, form, body_width),
                " Add Journal Transaction ",
            )
        } else if let Some(status_action) = &self.status_action {
            (
                status_action_lines(screen, status_action, body_width),
                " Change Statement Status ",
            )
        } else {
            match self.view_mode {
                ViewMode::Entries => (journal_lines(screen, body_width), " Journal Entries "),
                ViewMode::Totals => (
                    summary_lines(&screen.detail.summary, body_width),
                    " Income Totals ",
                ),
            }
        };
        let scroll_offset = scroll_offset(self.scroll_offset, body_lines.len(), chunks[2].height);

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

fn metadata_lines(
    screen: &IncomeStatementScreen,
    view_mode: ViewMode,
    notice: Option<&str>,
) -> Vec<Line<'static>> {
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

    let mut lines = vec![
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
    ];

    if let Some(notice) = notice {
        lines.push(Line::from(Span::styled(
            truncate(notice, 90),
            Style::default().fg(Color::Green),
        )));
    }

    lines
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

fn add_form_lines(
    screen: &IncomeStatementScreen,
    form: &AddTransactionForm,
    width: usize,
) -> Vec<Line<'static>> {
    let fields = form_fields(&form.values, &screen.lookups);
    let mut lines = vec![
        Line::from(format!(
            "Statement #{} on {} ({})",
            screen.detail.statement.id,
            short_date(&screen.detail.statement.date),
            status_label(&screen.detail.statement.status)
        )),
        Line::from(""),
    ];

    if form.submitting {
        lines.push(Line::from(Span::styled(
            "Saving transaction...",
            Style::default().fg(Color::Yellow),
        )));
    }

    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(
            truncate(error, width),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    for (index, field) in fields.iter().enumerate() {
        let selected = index == form.focus;
        let marker = if selected { "> " } else { "  " };
        let label = field_label(*field);
        let value = field_value(*field, &form.values, &screen.lookups);
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(format!("{label:<18}"), style),
            Span::raw(truncate(&value, width.saturating_sub(22).max(8))),
        ]));
    }

    lines
}

fn status_action_lines(
    screen: &IncomeStatementScreen,
    status_action: &StatusAction,
    width: usize,
) -> Vec<Line<'static>> {
    let statement = &screen.detail.statement;
    let action = if status_action.open { "open" } else { "close" };
    let mut lines = vec![
        Line::from(format!(
            "Income statement #{} is currently {}.",
            statement.id,
            status_label(&statement.status)
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" to {action} this income statement.")),
        ]),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("q/Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to cancel."),
        ]),
    ];

    if status_action.submitting {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Saving {action} action..."),
            Style::default().fg(Color::Yellow),
        )));
    }

    if let Some(error) = &status_action.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            truncate(error, width),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
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

fn type_label(transaction_type: &str) -> String {
    transaction_type
        .split('-')
        .map(status_label)
        .collect::<Vec<_>>()
        .join(" ")
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
    add_form: Option<&AddTransactionForm>,
    status_action: Option<&StatusAction>,
) -> Vec<Line<'static>> {
    if status_action.is_some() {
        return vec![
            Line::from(vec![
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Confirm   "),
                Span::styled("q/Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Cancel"),
            ]),
            Line::from(""),
        ];
    }

    if add_form.is_some() {
        return vec![
            Line::from(vec![
                Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Add   "),
                Span::styled("Tab/Up/Down", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Field   "),
                Span::styled("Left/Right", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Choice   "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Save"),
            ]),
            Line::from(vec![
                Span::styled("Type", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Text   "),
                Span::styled("Backspace", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Delete   "),
                Span::styled("q/Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Cancel"),
            ]),
        ];
    }

    let page_label = match state {
        ScreenState::Loaded(screen) => format!(
            "S {}/{} J {}",
            statement_position(screen),
            screen.statement_total.max(screen.statements.len() as u64),
            screen.journals.page
        ),
        _ => "Loading".to_string(),
    };
    let status_action_label = match state {
        ScreenState::Loaded(screen)
            if screen.detail.statement.status.eq_ignore_ascii_case("open") =>
        {
            "Close"
        }
        ScreenState::Loaded(_) => "Open",
        _ => "Close/Open",
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
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Add   "),
            Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {status_action_label}   ")),
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

fn form_fields(values: &JournalTransactionValues, lookups: &TransactionLookups) -> Vec<FormField> {
    let transaction_type = values.transaction_type;
    let mut fields = vec![FormField::TransactionType, FormField::Employee];

    if transaction_type.needs_existing_invoice() {
        fields.push(FormField::Invoice);
    }

    if matches!(transaction_type, JournalTransactionType::InitialPayment) {
        fields.extend([
            FormField::InvoiceNumber,
            FormField::InvoiceCost,
            FormField::InvoiceDiscount,
        ]);
    }

    if transaction_type.needs_payment_method() {
        fields.push(FormField::PaymentMethod);
        if selected_payment_method(lookups, values.payment_method_id)
            .is_some_and(|method| matches!(method.name.as_str(), "DEPOSIT" | "ZELLE"))
        {
            fields.push(FormField::PaymentAccount);
        }
        if selected_payment_method(lookups, values.payment_method_id)
            .is_some_and(|method| method.name == "ZELLE")
        {
            fields.extend([FormField::ZelleDate, FormField::ZelleName]);
        }
        if selected_payment_method(lookups, values.payment_method_id)
            .is_some_and(|method| method.name == "CHECK")
        {
            fields.push(FormField::CheckNumber);
        }
    }

    if transaction_type.needs_account() {
        fields.push(FormField::Account);
    }
    if transaction_type.needs_source_account() {
        fields.push(FormField::SourceAccount);
    }

    fields.extend([
        FormField::Amount,
        FormField::RefNumber,
        FormField::Description,
    ]);
    fields
}

fn field_label(field: FormField) -> &'static str {
    match field {
        FormField::TransactionType => "Type",
        FormField::Employee => "Employee",
        FormField::Invoice => "Invoice",
        FormField::InvoiceNumber => "Invoice number",
        FormField::InvoiceCost => "Invoice cost",
        FormField::InvoiceDiscount => "Invoice discount",
        FormField::PaymentMethod => "Payment method",
        FormField::PaymentAccount => "Bank account",
        FormField::ZelleDate => "Zelle date",
        FormField::ZelleName => "Zelle name",
        FormField::CheckNumber => "Check number",
        FormField::Account => "Account",
        FormField::SourceAccount => "Source account",
        FormField::Amount => "Amount",
        FormField::RefNumber => "Reference",
        FormField::Description => "Description",
    }
}

fn field_value(
    field: FormField,
    values: &JournalTransactionValues,
    lookups: &TransactionLookups,
) -> String {
    match field {
        FormField::TransactionType => values.transaction_type.label().into(),
        FormField::Employee => lookups
            .employees
            .iter()
            .find(|employee| Some(employee.id) == values.employee_id)
            .map(|employee| employee.name.clone())
            .unwrap_or_else(|| "(none)".into()),
        FormField::Invoice => lookups
            .invoices
            .iter()
            .find(|invoice| Some(invoice.id_string()) == values.invoice_id)
            .map(invoice_label)
            .unwrap_or_else(|| "(none)".into()),
        FormField::InvoiceNumber => values.invoice_number.clone(),
        FormField::InvoiceCost => decimal_value(values.invoice_cost),
        FormField::InvoiceDiscount => decimal_value(values.invoice_discount),
        FormField::PaymentMethod => selected_payment_method(lookups, values.payment_method_id)
            .map(|method| method.name.clone())
            .unwrap_or_else(|| "(none)".into()),
        FormField::PaymentAccount => account_label(lookups, values.payment_account_id),
        FormField::ZelleDate => values.zelle_transaction_date.clone(),
        FormField::ZelleName => values.zelle_transaction_name.clone(),
        FormField::CheckNumber => values.check_number.clone(),
        FormField::Account => account_label(lookups, values.account_id),
        FormField::SourceAccount => account_label(lookups, values.source_account_id),
        FormField::Amount => decimal_value(values.amount),
        FormField::RefNumber => values.ref_number.clone(),
        FormField::Description => values.description.clone(),
    }
}

fn account_label(lookups: &TransactionLookups, id: Option<u32>) -> String {
    lookups
        .accounts
        .iter()
        .find(|account| Some(account.id) == id)
        .map(|account| account.label())
        .unwrap_or_else(|| "(none)".into())
}

fn invoice_label(invoice: &Invoice) -> String {
    format!(
        "{}  balance {}",
        invoice.number,
        format_money(invoice.balance)
    )
}

fn selected_payment_method(
    lookups: &TransactionLookups,
    id: Option<u16>,
) -> Option<&crate::infrastructure::journal::PaymentMethod> {
    lookups
        .payment_methods
        .iter()
        .find(|method| Some(method.id) == id)
}

fn first_account_id(
    lookups: &TransactionLookups,
    transaction_type: JournalTransactionType,
) -> Option<u32> {
    account_options(lookups, transaction_type)
        .first()
        .map(|account| account.id)
}

fn first_source_account_id(lookups: &TransactionLookups) -> Option<u32> {
    asset_accounts(lookups).first().map(|account| account.id)
}

fn first_payment_account_id(lookups: &TransactionLookups) -> Option<u32> {
    bank_accounts(lookups).first().map(|account| account.id)
}

fn account_options(
    lookups: &TransactionLookups,
    transaction_type: JournalTransactionType,
) -> Vec<&ChartAccount> {
    lookups
        .accounts
        .iter()
        .filter(|account| match transaction_type {
            JournalTransactionType::Expense => account.account_type == "EXPENSE",
            JournalTransactionType::Sales => {
                account.account_type == "REVENUE" && !account.system_account
            }
            JournalTransactionType::Transfer => account.account_type == "ASSET",
            _ => false,
        })
        .collect()
}

fn asset_accounts(lookups: &TransactionLookups) -> Vec<&ChartAccount> {
    lookups
        .accounts
        .iter()
        .filter(|account| account.account_type == "ASSET")
        .collect()
}

fn bank_accounts(lookups: &TransactionLookups) -> Vec<&ChartAccount> {
    lookups
        .accounts
        .iter()
        .filter(|account| account.account_type == "BANK")
        .collect()
}

fn cycle_employee(
    lookups: &TransactionLookups,
    current_id: Option<u16>,
    step: isize,
) -> Option<u16> {
    let current = lookups
        .employees
        .iter()
        .position(|employee| Some(employee.id) == current_id)
        .unwrap_or_default();
    lookups
        .employees
        .get(shifted_index(current, lookups.employees.len(), step))
        .map(|employee| employee.id)
}

fn cycle_invoice(
    lookups: &TransactionLookups,
    current_id: Option<&str>,
    step: isize,
) -> Option<String> {
    let current = lookups
        .invoices
        .iter()
        .position(|invoice| Some(invoice.id_string().as_str()) == current_id)
        .unwrap_or_default();
    lookups
        .invoices
        .get(shifted_index(current, lookups.invoices.len(), step))
        .map(Invoice::id_string)
}

fn cycle_payment_method(
    lookups: &TransactionLookups,
    current_id: Option<u16>,
    step: isize,
) -> Option<u16> {
    let current = lookups
        .payment_methods
        .iter()
        .position(|method| Some(method.id) == current_id)
        .unwrap_or_default();
    lookups
        .payment_methods
        .get(shifted_index(current, lookups.payment_methods.len(), step))
        .map(|method| method.id)
}

fn cycle_account(
    accounts: Vec<&ChartAccount>,
    current_id: Option<u32>,
    step: isize,
) -> Option<u32> {
    let current = accounts
        .iter()
        .position(|account| Some(account.id) == current_id)
        .unwrap_or_default();
    accounts
        .get(shifted_index(current, accounts.len(), step))
        .map(|account| account.id)
}

fn shifted_index(current: usize, len: usize, step: isize) -> usize {
    if len == 0 {
        return 0;
    }

    let len = len as isize;
    (current as isize + step).rem_euclid(len) as usize
}

fn push_number(value: &mut f64, character: char) {
    if !character.is_ascii_digit() && character != '.' {
        return;
    }

    let mut text = decimal_value(*value);
    if character == '.' && text.contains('.') {
        return;
    }
    if text == "0" && character.is_ascii_digit() {
        text.clear();
    }
    text.push(character);
    if let Ok(parsed) = text.parse::<f64>() {
        *value = parsed;
    }
}

fn pop_number(value: &mut f64) {
    let mut text = decimal_value(*value);
    text.pop();
    *value = text.parse::<f64>().unwrap_or_default();
}

fn decimal_value(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        let value = format!("{value:.2}");
        value
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
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

    #[test]
    fn key_menu_labels_status_toggle_action() {
        let closed_screen = ScreenState::Loaded(Box::new(screen_with_status("closed")));
        let closed_lines = key_menu_lines(&closed_screen, ViewMode::Entries, 0, None, None);
        assert!(line_contains(&closed_lines[0], "Open"));

        let open_screen = ScreenState::Loaded(Box::new(screen_with_status("open")));
        let open_lines = key_menu_lines(&open_screen, ViewMode::Entries, 0, None, None);
        assert!(line_contains(&open_lines[0], "Close"));
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
            lookups: TransactionLookups::default(),
        }
    }

    fn screen_with_status(status: &str) -> IncomeStatementScreen {
        let mut screen = screen_with_statement_page(1, 0, 20, 1);
        screen.statements[0].status = status.into();
        screen.detail.statement.status = status.into();
        screen
    }
}
