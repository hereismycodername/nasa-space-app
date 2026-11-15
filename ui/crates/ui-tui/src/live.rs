//! Live HTTP view. Clinical demo tools are kept out of the shared-data workflow.

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{
        Axis, Block, Cell, Chart, Clear, Dataset, GraphType, Paragraph, Row, Table, TableState,
        Wrap,
    },
};
use std::time::{Duration, Instant};
use ui_model::{Backend, HealthReport, HealthStatus, IndicatorInfo, Role, TimedSeries};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Metrics,
    Tasks,
}

struct LiveApp<B: Backend> {
    backend: B,
    role: Role,
    focus: Focus,
    metric: usize,
    task: usize,
    edit: Option<String>,
    notice: String,
    help: bool,
    details: bool,
    nasa: bool,
    nasa_all: bool,
    health: bool,
    flight_only: bool,
    last_health_status: HealthStatus,
    details_scroll: u16,
    quit: bool,
}

impl<B: Backend> LiveApp<B> {
    fn new(backend: B, role: Role) -> Self {
        Self {
            backend,
            role,
            focus: Focus::Metrics,
            metric: 0,
            task: 0,
            edit: None,
            notice: String::new(),
            help: false,
            details: false,
            nasa: false,
            nasa_all: false,
            health: false,
            flight_only: false,
            last_health_status: HealthStatus::NotEvaluated,
            details_scroll: 0,
            quit: false,
        }
    }

    fn indicators(&self) -> Vec<IndicatorInfo> {
        self.backend
            .systems()
            .iter()
            .flat_map(|s| self.backend.indicators(&s.id))
            .collect()
    }

    fn tick(&mut self) {
        self.backend.tick();
        let status = self.backend.health_report().status;
        if status == HealthStatus::Critical && self.last_health_status != HealthStatus::Critical {
            self.health = true;
            self.flight_only = false;
            self.details_scroll = 0;
        }
        self.last_health_status = status;
        let notifications = self.backend.take_messages();
        if !notifications.is_empty() {
            self.notice = notifications.join(" | ");
        }
    }

    fn move_selection(&mut self, forward: bool) {
        let (current, count) = match self.focus {
            Focus::Metrics => (self.metric, self.indicators().len()),
            Focus::Tasks => (self.task, self.backend.tasks().len()),
        };
        if count == 0 {
            return;
        }
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        match self.focus {
            Focus::Metrics => self.metric = next,
            Focus::Tasks => self.task = next,
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.nasa && !self.nasa_all && !self.health {
            let indicators = self.indicators();
            let insight = indicators.get(self.metric).and_then(|i| self.backend.research_insight(&i.id));
            if let Some(insight) = insight {
                let requested = match key.code {
                    KeyCode::Char('1') => Some(ui_model::research::ActionKind::Record),
                    KeyCode::Char('2') => Some(ui_model::research::ActionKind::History),
                    KeyCode::Char('3') => Some(ui_model::research::ActionKind::Guidance),
                    _ => None,
                };
                if let Some(action) = requested.filter(|kind| insight.actions.iter().any(|a| &a.kind == kind)) {
                    self.nasa = false;
                    self.focus = Focus::Metrics;
                    match action {
                        ui_model::research::ActionKind::Record => self.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        ui_model::research::ActionKind::History => { self.notice = "История выбранного показателя — график в основной панели".into(); },
                        ui_model::research::ActionKind::Guidance => { self.health = true; self.flight_only = false; self.details_scroll = 0; },
                    }
                    return;
                }
            }
        }
        if self.details || self.health || self.nasa {
            match key.code {
                KeyCode::Down => {
                    self.details_scroll = self.details_scroll.saturating_add(1).min(2000)
                }
                KeyCode::PageDown => {
                    self.details_scroll = self.details_scroll.saturating_add(10).min(2000)
                }
                KeyCode::Up => self.details_scroll = self.details_scroll.saturating_sub(1),
                KeyCode::PageUp => self.details_scroll = self.details_scroll.saturating_sub(10),
                _ => {
                    self.nasa = false;
                    self.details = false;
                    self.health = false;
                }
            }
            return;
        }
        if self.edit.is_some() {
            self.edit_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.nasa = true;
                self.nasa_all = key.code == KeyCode::Char('N');
                self.details_scroll = 0;
            },
            KeyCode::Char('f') => {
                self.health = true;
                self.flight_only = true;
                self.details_scroll = 0;
            }
            KeyCode::Char('h') => {
                self.health = true;
                self.flight_only = false;
                self.details_scroll = 0;
            }
            KeyCode::Char('i') => {
                self.details = true;
                self.details_scroll = 0;
            }
            KeyCode::Char('r') => {
                self.role = if self.role == Role::Crew {
                    Role::Ground
                } else {
                    Role::Crew
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Metrics {
                    Focus::Tasks
                } else {
                    Focus::Metrics
                };
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(true),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(false),
            KeyCode::Enter if self.focus == Focus::Metrics => {
                if self.backend.pending_writes() > 0 {
                    self.notice = "A write is pending. Wait for the server response.".into();
                } else if let Some(indicator) = self.indicators().get(self.metric) {
                    self.edit = Some(String::new());
                    self.notice = format!(
                        "Enter {} in {}. Enter sends; Esc cancels.",
                        indicator.name, indicator.unit
                    );
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') if self.focus == Focus::Tasks => self.toggle_task(),
            _ => {}
        }
    }

    fn toggle_task(&mut self) {
        if self.backend.pending_writes() > 0 {
            self.notice = "A write is pending. Wait for the server response.".into();
            return;
        }
        if let Some(task) = self.backend.tasks().get(self.task) {
            self.notice = match self
                .backend
                .set_task_completed(&task.id, task.completed_at.is_none())
            {
                Ok(()) => "Sending task update; awaiting server confirmation.".into(),
                Err(error) => format!("Task was not sent: {error}"),
            };
        }
    }

    fn edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.edit = None;
                self.notice = "Input cancelled.".into();
            }
            KeyCode::Backspace => {
                if let Some(value) = self.edit.as_mut() {
                    value.pop();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(value) = self.edit.as_mut() {
                    // Accept decimal/scientific notation; validation remains the server's job.
                    if value.len() < 64 && (c.is_ascii_digit() || ".,+-eE".contains(c)) {
                        value.push(c);
                    }
                }
            }
            KeyCode::Enter => {
                let value = self
                    .edit
                    .as_deref()
                    .unwrap_or("")
                    .replace(',', ".")
                    .parse::<f64>();
                let Ok(value) = value else {
                    self.notice = "Enter a finite number. Nothing was sent.".into();
                    return;
                };
                if !value.is_finite() {
                    self.notice = "Enter a finite number. Nothing was sent.".into();
                    return;
                }
                if let Some(indicator) = self.indicators().get(self.metric) {
                    match self.backend.submit_measurement(&indicator.id, value) {
                        Ok(()) => {
                            self.edit = None;
                            self.notice =
                                "Sending measurement; awaiting server confirmation.".into();
                        }
                        Err(error) => self.notice = format!("Measurement was not sent: {error}"),
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 70 || area.height < 22 {
            frame.render_widget(Paragraph::new(format!("AstroCheck live\n{}\nResize to at least 70x22 (80x24 recommended).\nq / Ctrl+C exits.", self.backend.connection_label())).wrap(Wrap { trim: false }), area);
            return;
        }
        let layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(11),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);
        let profile = self.backend.selected_crew();
        let title = format!(
            " ASTROCHECK / {} / {} ",
            if self.role == Role::Crew {
                "Crew"
            } else {
                "Ground"
            },
            if profile.is_empty() {
                "Waiting for profile"
            } else {
                &profile
            }
        );
        frame.render_widget(
            Paragraph::new(self.backend.connection_label())
                .block(Block::bordered().title(title))
                .wrap(Wrap { trim: false }),
            layout[0],
        );
        self.draw_measurements(frame, layout[1]);
        self.draw_tasks(frame, layout[2]);
        let report = self.backend.health_report();
        let pending = self.backend.pending_writes();
        let notice = if self.notice.is_empty() {
            if report.message.is_empty() {
                self.backend.connection_label()
            } else {
                report.message.clone()
            }
        } else {
            self.notice.clone()
        };
        let notice = if pending > 0 {
            format!("Pending writes: {pending}. {notice}")
        } else {
            notice
        };
        let mut lines = Vec::new();
        if let Some(rule) = report.evaluation.as_ref().and_then(|health| {
            health
                .rules
                .iter()
                .find(|rule| rule.current_status() == HealthStatus::Critical)
        }) {
            // Keep the action separate from delivery notices, including while a write is pending.
            lines.push(Line::styled(
                format!("Next step: {}", rule.suggested_action),
                Style::new().fg(Color::Red),
            ));
        }
        lines.push(Line::raw(notice));
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(format!(
                        " {} / h: health & flight guidance ",
                        report.status.label()
                    ))
                    .border_style(Style::new().fg(health_color(report.status))),
            ),
            layout[3],
        );
        frame.render_widget(
            Paragraph::new(
                " Arrows select  Enter edit  Tab tasks  Space toggle  i detail  h health  f flight  n/N NASA  q quit",
            )
            .style(Style::new().fg(Color::Gray)),
            layout[4],
        );
        if self.help {
            let help = Rect::new(
                area.x + 3,
                area.y + 3,
                area.width.saturating_sub(6),
                area.height.saturating_sub(6),
            );
            frame.render_widget(Clear, help);
            frame.render_widget(Paragraph::new("Shared Rust API\n\nn: NASA sources for selected metric; N: full NASA catalogue. Arrows/PageDown scroll; Esc closes.\n\nUp/Down or j/k: choose a measurement or task.\nEnter on a measurement: enter its measured value; Enter again sends.\nTab: switch between measurements and tasks.\nSpace or Enter on a task: request completion/undo.\ni: full measurement details and history description; arrows scroll.\nr: switch Crew/Ground. Both see the same server profile.\n\nA pending message is not a saved measurement. Success appears only after the server replies.\nOffline data is a cached view; reconnect is automatic.\nChart gaps are missing samples, not zero values. Dates are UTC.\nHealth rules run on the server; h shows guidance, flight hazards and sources. A rule alert is not a diagnosis. Simulated demo tools are unavailable here.\n\nAny key closes help. Ctrl+C always exits.").block(Block::bordered().title(" Controls ")).wrap(Wrap { trim: false }), help);
        }
        if self.details {
            let popup = Rect::new(
                area.x + 3,
                area.y + 3,
                area.width.saturating_sub(6),
                area.height.saturating_sub(6),
            );
            let indicators = self.indicators();
            let detail = indicators
                .get(self.metric)
                .map(|indicator| {
                    let rule = self
                        .backend
                        .health_rule(&indicator.id)
                        .map(|rule| rule.detail_text())
                        .unwrap_or_else(|| {
                            "No health guidance received for this measurement".into()
                        });
                    format!(
                        "{}\n\n{}\n\nData quality: {}\n\n{}",
                        indicator.name,
                        rule,
                        self.backend.measurement_detail(&indicator.id),
                        self.backend.data_series(&indicator.id).note
                    )
                })
                .unwrap_or_else(|| "Waiting for server data".into());
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(detail)
                    .block(
                        Block::bordered()
                            .title(" Measurement details / Up-Down scroll / any other key closes "),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((self.details_scroll, 0)),
                popup,
            );
        }
        if self.nasa && !self.health {
            let popup = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), area.height.saturating_sub(2));
            let indicators = self.indicators();
            let metric = if self.nasa_all { None } else { indicators.get(self.metric).map(|i| i.id.as_str()) };
            let mut text = match ui_model::nasa::catalog() {
                Ok(catalog) => catalog.text(metric),
                Err(message) => message.to_owned(),
            };
            if let Some(insight) = metric.and_then(|id| self.backend.research_insight(id)) {
                let mut live = format!("Что это значит в полёте\n{}\n", insight.observation);
                for action in &insight.actions {
                    let key = match action.kind { ui_model::research::ActionKind::Record => 1, ui_model::research::ActionKind::History => 2, ui_model::research::ActionKind::Guidance => 3 };
                    live.push_str(&format!("[{key}] {}\n", action.label));
                }
                for context in &insight.contexts {
                    live.push_str(&format!("\n{}\n{}\nКак используем: {}\nОграничение: {}\n{}: {} — {}\n", context.title, context.explanation, context.project_use, context.limitation, context.source_agency, context.source_section, context.source_url));
                }
                text = format!("{live}\nСвязи и действия — решения команды. {}\n", insight.catalog_version);
            }
            frame.render_widget(Clear, popup);
            frame.render_widget(Paragraph::new(text).block(Block::bordered().title(" NASA / arrows scroll / Esc closes ")).wrap(Wrap { trim: false }).scroll((self.details_scroll, 0)), popup);
        }
        if self.health {
            let popup = Rect::new(
                area.x + 2,
                area.y + 1,
                area.width.saturating_sub(4),
                area.height.saturating_sub(2),
            );
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(if self.flight_only {
                    flight_text(&report)
                } else {
                    health_text(&report)
                })
                .block(
                    Block::bordered()
                        .title(" Health & flight guidance / arrows scroll / other key closes ")
                        .border_style(Style::new().fg(health_color(report.status))),
                )
                .wrap(Wrap { trim: false })
                .scroll((self.details_scroll, 0)),
                popup,
            );
        }
    }

    fn draw_measurements(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        let indicators = self.indicators();
        let rows = indicators.iter().map(|indicator| {
            let latest = self.backend.latest(&indicator.id);
            Row::new(vec![
                Cell::from(indicator.name.clone()),
                Cell::from(
                    latest
                        .map(|r| number(r.value))
                        .unwrap_or_else(|| "—".into()),
                ),
                Cell::from(indicator.unit.clone()),
            ])
            .style(
                Style::new().fg(health_color(
                    self.backend
                        .health_rule(&indicator.id)
                        .map(|rule| rule.current_status())
                        .unwrap_or_default(),
                )),
            )
        });
        let mut selected =
            TableState::default().with_selected((!indicators.is_empty()).then_some(self.metric));
        let table = Table::new(
            rows,
            [
                Constraint::Min(12),
                Constraint::Length(9),
                Constraint::Length(5),
            ],
        )
        .header(Row::new(["Measurement", "Latest", "Unit"]).style(Style::new().fg(Color::Gray)))
        .row_highlight_style(Style::new().add_modifier(Modifier::BOLD))
        .highlight_symbol(if self.focus == Focus::Metrics {
            "> "
        } else {
            "  "
        })
        .block(
            Block::bordered()
                .title(" Measurements ")
                .border_style(Style::new().fg(if self.focus == Focus::Metrics {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })),
        );
        frame.render_stateful_widget(table, cols[0], &mut selected);
        let Some(indicator) = indicators.get(self.metric) else {
            frame.render_widget(
                Paragraph::new("Waiting for server measurements").block(Block::bordered()),
                cols[1],
            );
            return;
        };
        let rows = Layout::vertical([
            Constraint::Length(if self.edit.is_some() { 3 } else { 0 }),
            Constraint::Length(6),
            Constraint::Min(4),
        ])
        .split(cols[1]);
        if let Some(value) = &self.edit {
            let prompt = format!("{} [{}]: {}_", indicator.name, indicator.unit, value);
            frame.render_widget(
                Paragraph::new(prompt).block(
                    Block::bordered()
                        .title(" New measurement / Enter sends ")
                        .border_style(Style::new().fg(Color::Cyan)),
                ),
                rows[0],
            );
        }
        frame.render_widget(
            Paragraph::new(format!(
                "Health guidance: {}\nData quality: {}",
                self.backend
                    .health_rule(&indicator.id)
                    .map(|rule| rule.current_status().label())
                    .unwrap_or("Not evaluated"),
                self.backend.measurement_detail(&indicator.id)
            ))
            .block(Block::bordered().title(" Time/source/quality / i: full details "))
            .wrap(Wrap { trim: false }),
            rows[1],
        );
        draw_chart(
            frame,
            rows[2],
            &self.backend.data_series(&indicator.id),
            &indicator.unit,
        );
    }

    fn draw_tasks(&self, frame: &mut Frame, area: Rect) {
        let tasks = self.backend.tasks();
        let rows = tasks.iter().map(|task| {
            Row::new(vec![
                Cell::from(if task.completed_at.is_some() {
                    "[x]"
                } else {
                    "[ ]"
                }),
                Cell::from(task.title.clone()),
                Cell::from(
                    task.completed_at
                        .clone()
                        .unwrap_or_else(|| "Not completed".into()),
                ),
            ])
        });
        let mut selected = TableState::default().with_selected(
            (!tasks.is_empty()).then_some(self.task.min(tasks.len().saturating_sub(1))),
        );
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Min(15),
                Constraint::Length(27),
            ],
        )
        .block(
            Block::bordered()
                .title(if tasks.is_empty() {
                    " Tasks / waiting for server "
                } else {
                    " Shared tasks / completion stored on server "
                })
                .border_style(Style::new().fg(if self.focus == Focus::Tasks {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })),
        )
        .row_highlight_style(Style::new().fg(if self.focus == Focus::Tasks {
            Color::Cyan
        } else {
            Color::White
        }))
        .highlight_symbol(if self.focus == Focus::Tasks {
            "> "
        } else {
            "  "
        });
        frame.render_stateful_widget(table, area, &mut selected);
    }
}

fn health_color(status: HealthStatus) -> Color {
    match status {
        HealthStatus::NoAlert => Color::Green,
        HealthStatus::Attention => Color::Yellow,
        HealthStatus::Critical => Color::Red,
        HealthStatus::InsufficientData | HealthStatus::NotEvaluated => Color::Gray,
    }
}

fn health_text(report: &HealthReport) -> String {
    let mut text = format!(
        "{}\n{}\nEvaluated at: {}\n",
        report.status.label(),
        report.message,
        report.evaluated_at.as_deref().unwrap_or("not received")
    );
    let Some(health) = &report.evaluation else {
        return text;
    };
    text.push_str(&format!("\nURGENT SYMPTOMS / WHEN TO GET HELP\n{}\nThe application does not send a message automatically.\n\n", health.emergency_guidance));
    text.push_str(&health.disclaimer);
    text.push_str(&format!(
        "\nRule set: {} (demonstration)\n\n",
        health.rule_set_version
    ));
    // Highest-priority current results are readable before routine education.
    let mut rules: Vec<_> = health.rules.iter().collect();
    rules.sort_by_key(|rule| match rule.current_status() {
        HealthStatus::Critical => 0,
        HealthStatus::Attention => 1,
        _ => 2,
    });
    for rule in rules {
        text.push_str(&rule.detail_text());
        text.push_str("\n\n");
    }
    text.push_str("Press f after closing this panel for all five spaceflight hazards.\n");
    text
}

fn flight_text(report: &HealthReport) -> String {
    let Some(health) = &report.evaluation else {
        return report.message.clone();
    };
    let mut text = format!(
        "SPACEFLIGHT HAZARDS AND PREVENTION\n\n{}\n\n",
        health.disclaimer
    );
    for topic in &health.education {
        text.push_str(&format!(
            "{}\n{}\nNext step: {}\nSources:\n{}\n\n",
            topic.title,
            topic.explanation,
            topic.suggested_action,
            topic.source_urls.join("\n")
        ));
    }
    text
}

fn number(value: f64) -> String {
    if value.abs() >= 1_000_000.0 || (value != 0.0 && value.abs() < 0.001) {
        format!("{value:.3e}")
    } else {
        format!("{value:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    }
}

fn timestamp(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|time| time.format("%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "Unknown time".into())
}

/// Each contiguous series becomes a separate dataset, so missing buckets are never joined.
fn chart_segments(series: &TimedSeries, scale: f64) -> Vec<Vec<(f64, f64)>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for point in &series.points {
        match point.value.filter(|value| value.is_finite()) {
            Some(value) => current.push((point.at_ms as f64, value / scale)),
            None if !current.is_empty() => segments.push(std::mem::take(&mut current)),
            None => {}
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn draw_chart(frame: &mut Frame, area: Rect, series: &TimedSeries, unit: &str) {
    if area.height < 4 {
        return;
    }
    let regions = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    let scale = series
        .points
        .iter()
        .filter_map(|p| p.value)
        .filter(|v| v.is_finite())
        .map(f64::abs)
        .fold(1.0, f64::max);
    let segments = chart_segments(series, scale);
    if segments.is_empty() || series.from_ms >= series.to_ms {
        frame.render_widget(
            Paragraph::new("No samples in this time range.")
                .block(Block::bordered().title(" History / UTC ")),
            regions[0],
        );
    } else {
        let datasets = segments
            .iter()
            .map(|points| {
                Dataset::default()
                    .marker(if super::braille::use_braille() {
                        symbols::Marker::Braille
                    } else {
                        symbols::Marker::Dot
                    })
                    .graph_type(GraphType::Line)
                    .style(Style::new().fg(Color::Cyan))
                    .data(points)
            })
            .collect();
        let chart = Chart::new(datasets)
            .block(Block::bordered().title(format!(" History / {unit} / UTC ")))
            .x_axis(
                Axis::default()
                    .bounds([series.from_ms as f64, series.to_ms as f64])
                    .labels([timestamp(series.from_ms), timestamp(series.to_ms)]),
            )
            .y_axis(
                Axis::default()
                    .bounds([0.0, 1.05])
                    .labels(["0".to_owned(), number(scale)]),
            );
        frame.render_widget(chart, regions[0]);
    }
    frame.render_widget(
        Paragraph::new(series.note.clone()).style(Style::new().fg(Color::Gray)),
        regions[1],
    );
}

/// RAII also restores the terminal if a rendering/event error unwinds out of the loop.
struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

pub(super) fn run<B: Backend>(backend: B, role: Role) -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let _restore = RestoreTerminal;
    let mut app = LiveApp::new(backend, role);
    let mut last = Instant::now();
    let mut redraw = true;
    while !app.quit {
        if redraw {
            terminal.draw(|frame| app.draw(frame))?;
            redraw = false;
        }
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Resize(..) => {}
                _ => continue,
            }
            redraw = true;
        }
        if last.elapsed() >= Duration::from_millis(500) {
            app.tick();
            last = Instant::now();
            redraw = true;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;
    use ui_model::*;

    struct FakeBackend {
        demo: DemoBackend,
        pending: usize,
        writes: Vec<(String, f64)>,
        tasks: Vec<TaskInfo>,
        messages: Vec<String>,
        fail: bool,
        latest: f64,
        report: HealthReport,
        insight: Option<ui_model::research::ResearchInsight>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                demo: DemoBackend::new(),
                pending: 0,
                writes: vec![],
                tasks: vec![TaskInfo {
                    id: "daily-check".into(),
                    title: "Daily measurements".into(),
                    completed_at: None,
                }],
                messages: vec![],
                fail: false,
                latest: 72.0,
                report: HealthReport::default(),
                insight: None,
            }
        }
    }

    impl Backend for FakeBackend {
        fn research_insight(&self, _: &str) -> Option<ui_model::research::ResearchInsight> { self.insight.clone() }

        fn is_demo(&self) -> bool {
            false
        }
        fn health_report(&self) -> HealthReport {
            self.report.clone()
        }
        fn health_rule(&self, _: &str) -> Option<HealthRule> {
            self.report.evaluation.as_ref()?.rules.first().cloned()
        }
        fn connection_label(&self) -> String {
            "Connected to http://shared:8080".into()
        }
        fn measurement_detail(&self, _: &str) -> String {
            "Source: manual. Measured: 2026-11-15T01:00:00Z. Received: 2026-11-15T01:00:01Z. Quality: stale. Latest received: unknown-time. Not a health assessment.".into()
        }
        fn data_series(&self, _: &str) -> TimedSeries {
            TimedSeries {
                from_ms: 1_794_700_800_000,
                to_ms: 1_795_305_600_000,
                points: vec![
                    TimePoint {
                        at_ms: 1_794_700_800_000,
                        value: Some(72.0),
                    },
                    TimePoint {
                        at_ms: 1_795_000_000_000,
                        value: None,
                    },
                    TimePoint {
                        at_ms: 1_795_300_000_000,
                        value: Some(73.0),
                    },
                ],
                note: "7 days / gaps retained / measuredAt".into(),
            }
        }
        fn submit_measurement(&mut self, id: &str, value: f64) -> Result<(), String> {
            if self.fail {
                return Err("Network unavailable".into());
            }
            self.writes.push((id.into(), value));
            self.pending = 1;
            Ok(())
        }
        fn pending_writes(&self) -> usize {
            self.pending
        }
        fn tasks(&self) -> Vec<TaskInfo> {
            self.tasks.clone()
        }
        fn set_task_completed(&mut self, _: &str, _: bool) -> Result<(), String> {
            if self.fail {
                return Err("Network unavailable".into());
            }
            self.pending = 1;
            Ok(())
        }
        fn overview(&self) -> Overview {
            self.demo.overview()
        }
        fn systems(&self) -> Vec<SystemInfo> {
            self.demo
                .systems()
                .into_iter()
                .filter(|s| s.id == "heart")
                .collect()
        }
        fn indicators(&self, system: &str) -> Vec<IndicatorInfo> {
            self.demo
                .indicators(system)
                .into_iter()
                .filter(|i| i.id == "hr")
                .collect()
        }
        fn system_status(&self, _: &str) -> Status {
            Status::Unassessed
        }
        fn indicator_status(&self, _: &str) -> Status {
            Status::Unassessed
        }
        fn latest(&self, _: &str) -> Option<Reading> {
            Some(Reading {
                day: 1,
                value: self.latest,
            })
        }
        fn history(&self, _: &str) -> Vec<Reading> {
            vec![]
        }
        fn thresholds(&self, _: &str) -> Thresholds {
            Thresholds {
                ok: (0., 0.),
                warn: (0., 0.),
            }
        }
        fn evaluate(&self, id: &str, value: f64) -> Evaluation {
            self.demo.evaluate(id, value)
        }
        fn is_due(&self, _: &str) -> bool {
            false
        }
        fn environments(&self) -> Vec<EnvInfo> {
            vec![]
        }
        fn environment(&self) -> EnvInfo {
            self.demo.environment()
        }
        fn hazard_label(&self, _: &str) -> String {
            String::new()
        }
        fn queue(&self, _: usize) -> (QueueSummary, Vec<QueueRow>) {
            (QueueSummary::default(), vec![])
        }
        fn health(&self) -> Health {
            self.demo.health()
        }
        fn blackout(&self) -> bool {
            false
        }
        fn log(&mut self, _: &str, value: f64) -> LogResult {
            LogResult {
                status: Status::Unassessed,
                value,
            }
        }
        fn set_environment(&mut self, _: &str) {}
        fn set_blackout(&mut self, _: bool) {}
        fn run_selftest(&mut self) {}
        fn export(&mut self) -> Result<String, String> {
            Err("Unavailable".into())
        }
        fn selected_crew(&self) -> String {
            "demo-astronaut".into()
        }
        fn tick(&mut self) -> bool {
            true
        }
        fn take_messages(&mut self) -> Vec<String> {
            std::mem::take(&mut self.messages)
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn render(app: &LiveApp<FakeBackend>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn nasa_server_actions_open_input_history_and_guidance_without_writing() {
        let mut backend = FakeBackend::new();
        backend.insight = Some(serde_json::from_str(include_str!("../../../../docs/examples/research-insight.json")).unwrap());
        let mut app = LiveApp::new(backend, Role::Crew);
        app.on_key(key(KeyCode::Char('n')));
        assert!(render(&app, 160, 55).contains("[1]"));
        app.on_key(key(KeyCode::Char('1')));
        assert!(app.edit.is_some());
        assert!(app.backend.writes.is_empty());
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('n')));
        app.on_key(key(KeyCode::Char('2')));
        assert!(!app.nasa);
        assert!(app.notice.contains("график"));
        app.on_key(key(KeyCode::Char('n')));
        app.on_key(key(KeyCode::Char('3')));
        assert!(app.health);
        assert!(!app.flight_only);
    }

    #[test]
    fn nasa_catalog_opens_filters_scrolls_and_closes_without_measurements() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Crew);
        app.on_key(key(KeyCode::Char('n')));
        assert!(app.nasa);
        assert!(!app.nasa_all);
        let screen = render(&app, 160, 55);
        assert!(screen.contains("NASA / CSA"));
        assert!(screen.contains("https://www.nasa.gov/hrp/risks/"));
        assert!(!screen.contains("ARCHeR"));
        app.on_key(key(KeyCode::PageDown));
        assert_eq!(app.details_scroll, 10);
        app.on_key(key(KeyCode::Esc));
        assert!(!app.nasa);
        assert!(!app.quit);
        app.on_key(key(KeyCode::Char('N')));
        assert!(app.nasa_all);
        assert_eq!(app.details_scroll, 0);
        render(&app, 40, 12);
    }

    #[test]
    fn manual_input_stays_pending_until_server_confirmation_and_refresh() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Crew);
        app.on_key(key(KeyCode::Enter));
        for c in "73,5".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.backend.writes, vec![("hr".into(), 73.5)]);
        assert_eq!(
            app.backend.latest, 72.0,
            "Unconfirmed values must not replace server readings"
        );
        assert!(app.notice.contains("awaiting server confirmation"));
        app.on_key(key(KeyCode::Enter));
        assert!(
            app.edit.is_none(),
            "Repeated input must not enqueue a second pending write"
        );
        app.backend.latest = 73.5;
        app.backend.pending = 0;
        app.backend
            .messages
            .push("Measurement saved by server".into());
        app.tick();
        assert_eq!(app.notice, "Measurement saved by server");
        assert!(render(&app, 100, 30).contains("73.5"));
    }

    #[test]
    fn input_validation_and_submission_errors_do_not_claim_success_or_close_input() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Crew);
        app.on_key(key(KeyCode::Enter));
        app.edit = Some("1e9999".into());
        app.on_key(key(KeyCode::Enter));
        assert!(app.backend.writes.is_empty());
        assert!(app.notice.contains("Nothing was sent"));
        app.edit = Some("72.5".into());
        app.backend.fail = true;
        app.on_key(key(KeyCode::Enter));
        assert!(app.backend.writes.is_empty());
        assert!(app.edit.is_some());
        assert!(app.notice.contains("Network unavailable"));
    }

    #[test]
    fn task_toggles_are_confirmed_by_server_not_optimistically_checked() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Ground);
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Char(' ')));
        assert!(app.backend.tasks[0].completed_at.is_none());
        assert!(app.notice.contains("awaiting server confirmation"));
        assert!(render(&app, 100, 30).contains("[ ]"));
        app.backend.tasks[0].completed_at = Some("2026-11-15T01:00:00Z".into());
        app.backend.pending = 0;
        app.backend.messages.push("Task updated by server".into());
        app.tick();
        assert!(render(&app, 100, 30).contains("[x]"));
    }

    #[test]
    fn live_views_show_provenance_without_demo_clinical_claims_and_resize_safely() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Crew);
        for role in [Role::Crew, Role::Ground] {
            app.role = role;
            let screen = render(&app, 120, 35);
            assert!(screen.contains("demo-astronaut"));
            assert!(screen.contains("Source: manual"));
            assert!(screen.contains("unknown-time"));
            assert!(screen.contains("UTC"));
            for simulated in ["NORMAL", "READINESS", "CRC", "XP", "vs baseline", "ACT NOW"] {
                assert!(
                    !screen.contains(simulated),
                    "Live view must not show {simulated}"
                );
            }
        }
        assert!(render(&app, 30, 8).contains("Resize to at least"));
        let _ = render(&app, 80, 24);
        app.on_key(key(KeyCode::Char('i')));
        assert!(render(&app, 80, 24).contains("unknown-time"));
    }

    #[test]
    fn chart_preserves_missing_time_buckets_as_real_gaps() {
        let series = FakeBackend::new().data_series("hr");
        let segments = chart_segments(&series, 73.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].len(), 1);
        assert_eq!(segments[1].len(), 1);
        assert!(segments[1][0].0 > segments[0][0].0);
        assert!(
            timestamp(series.from_ms).contains(':'),
            "Chart labels must contain actual UTC time"
        );
    }
    #[test]
    fn new_urgent_guidance_opens_with_actions_and_all_flight_topics_are_accessible() {
        let mut app = LiveApp::new(FakeBackend::new(), Role::Crew);
        app.backend.report = HealthReport {
            evaluated_at: Some("2026-11-15T01:00:00Z".into()),
            status: HealthStatus::Critical,
            message: "1/6 evaluated".into(),
            evaluation: Some(HealthEvaluation {
                rule_set_version: "be08-demo-1".into(),
                demonstration_only: true,
                status: HealthStatus::Critical,
                explanation: "Server rule triggered".into(),
                disclaimer: "Prototype, not a diagnosis".into(),
                emergency_guidance: "Involve crew now; do not wait for ground response".into(),
                evaluated_metric_count: 1,
                total_metric_count: 6,
                rules: vec![HealthRule {
                    rule_id: "test-rule".into(),
                    rule_version: "be08-demo-1".into(),
                    metric: "heartRate".into(),
                    title: "Test criterion".into(),
                    status: HealthStatus::Critical,
                    explanation: "Server explanation".into(),
                    measurement_ids: vec!["id".into()],
                    valid_until: Some("2026-11-15T01:00:15Z".into()),
                    suggested_action: "Contact crew medical officer".into(),
                    escalation: Escalation::Urgent,
                    flight_context: "Distance delays support".into(),
                    source_urls: vec!["https://www.nasa.gov/hrp/hazards/".into()],
                    demonstration_only: true,
                    current: true,
                    unavailable_reason: None,
                }],
                education: [
                    "Gravity",
                    "Isolation",
                    "Radiation",
                    "Distance",
                    "Environment",
                ]
                .into_iter()
                .map(|title| FlightEducation {
                    id: title.into(),
                    title: title.into(),
                    explanation: "Spaceflight context".into(),
                    suggested_action: "Follow the mission prevention plan".into(),
                    source_urls: vec!["https://www.nasa.gov/hrp/hazards/".into()],
                })
                .collect(),
            }),
        };
        app.tick();
        assert!(app.health);
        let screen = render(&app, 100, 40);
        assert!(screen.contains("Urgent action"));
        assert!(screen.contains("Involve crew now"));
        assert!(screen.contains("Contact crew medical officer"));
        app.on_key(key(KeyCode::Esc));
        app.tick();
        assert!(!app.health, "Dismissed alert must not reopen every refresh");
        let main = render(&app, 80, 24);
        assert!(main.contains("Next step: Contact crew medical officer"));
        assert!(!main.contains("Waiting for server measurements"));
        app.backend.pending = 1;
        app.backend
            .messages
            .push("Sending measurement; awaiting server confirmation.".into());
        app.tick();
        let sending = render(&app, 80, 24);
        assert!(sending.contains("Next step: Contact crew medical officer"));
        assert!(sending.contains("Pending writes: 1"));
        app.backend.pending = 0;
        app.backend.report.status = HealthStatus::InsufficientData;
        app.backend.report.evaluation.as_mut().unwrap().rules[0].current = false;
        assert!(!render(&app, 80, 24).contains("Next step: Contact crew medical officer"));
        app.on_key(key(KeyCode::Char('f')));
        let topics = flight_text(&app.backend.report);
        for title in [
            "Gravity",
            "Isolation",
            "Radiation",
            "Distance",
            "Environment",
        ] {
            assert!(topics.contains(title));
        }
        assert!(topics.contains("https://www.nasa.gov/hrp/hazards/"));
        assert_eq!(health_color(HealthStatus::NoAlert), Color::Green);
        assert_eq!(health_color(HealthStatus::Attention), Color::Yellow);
        assert_eq!(health_color(HealthStatus::Critical), Color::Red);
        assert_eq!(health_color(HealthStatus::NotEvaluated), Color::Gray);
        for _ in 0..15 {
            app.on_key(key(KeyCode::PageDown));
        }
        assert!(
            app.details_scroll > 100,
            "Long source/action text must remain reachable"
        );
    }
}
