//! Shared-server UI. All visible measurements and tasks come from the HTTP cache.

use crate::{figure, theme::*, Opts};
use chrono::{DateTime, Utc};
use eframe::egui::{self, pos2, vec2, Align, Align2, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Shape, StrokeKind, Ui};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use ui_http::HttpBackend;
use ui_model::{fmt_num, Backend, IndicatorInfo, Role, TimedSeries};

pub(crate) struct LiveApp {
    backend: HttpBackend,
    figure: figure::Figure,
    mode: figure::Mode,
    role: Role,
    system: Option<String>,
    metric: Option<String>,
    inputs: HashMap<String, String>,
    notices: VecDeque<String>,
    ticked: Instant,
    started: Instant,
    screenshot: Option<String>,
    screenshot_requested: bool,
}

impl LiveApp {
    pub(crate) fn new(backend: HttpBackend, options: Opts) -> Self {
        Self {
            backend,
            figure: figure::Figure::new(),
            mode: match options.mode.as_deref() {
                Some("skeleton") => figure::Mode::Skeleton,
                Some("xray") => figure::Mode::XRay,
                _ => figure::Mode::Organs,
            },
            role: if options.role.as_deref() == Some("ground") {
                Role::Ground
            } else {
                Role::Crew
            },
            system: options.system,
            metric: options.metric,
            inputs: HashMap::new(),
            notices: VecDeque::new(),
            ticked: Instant::now(),
            started: Instant::now(),
            screenshot: options.shot,
            screenshot_requested: false,
        }
    }

    fn indicators(&self) -> Vec<IndicatorInfo> {
        self.backend
            .systems()
            .iter()
            .flat_map(|system| self.backend.indicators(&system.id))
            .collect()
    }

    fn notice(&mut self, text: String) {
        self.notices.push_front(text);
        self.notices.truncate(8);
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("live-header")
            .frame(Frame::new().fill(SURFACE).inner_margin(Margin::symmetric(18, 12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ASTROCHECK").font(display(23.0)).strong());
                    ui.add_space(18.0);
                    ui.selectable_value(&mut self.role, Role::Crew, "Crew");
                    ui.selectable_value(&mut self.role, Role::Ground, "Ground");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(self.backend.selected_crew()).color(MUT));
                    });
                });
                ui.add_space(5.0);
                ui.label(RichText::new(self.backend.connection_label()).color(if self.backend.overview().link_up { TXT } else { WARN }));
            });
        egui::TopBottomPanel::bottom("live-status")
            .frame(Frame::new().fill(SURFACE).inner_margin(Margin::symmetric(18, 9)))
            .show(ctx, |ui| {
                ui.label(RichText::new("Shared measurements · Clinical assessment is not connected").color(MUT));
                if self.backend.pending_writes() > 0 {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Sending — waiting for server confirmation");
                    });
                }
                if let Some(notice) = self.notices.front() {
                    ui.label(RichText::new(notice).color(TXT));
                }
            });
    }

    fn anatomy(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("live-anatomy")
            .default_width(340.0)
            .min_width(300.0)
            .max_width(430.0)
            .resizable(true)
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(14)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Body systems");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.mode, figure::Mode::Organs, "Organs");
                        ui.selectable_value(&mut self.mode, figure::Mode::Skeleton, "Skeleton");
                        ui.selectable_value(&mut self.mode, figure::Mode::XRay, "X-ray");
                    });
                    let clicked = self.figure.show(
                        ui,
                        vec2(ui.available_width(), 440.0),
                        &|system| self.backend.system_status(system),
                        self.system.as_deref().unwrap_or(""),
                        self.mode,
                        0.0,
                        false,
                    );
                    if let Some(system) = clicked {
                        self.system = if self.system.as_ref() == Some(&system) {
                            None
                        } else {
                            Some(system)
                        };
                        self.metric = None;
                    }
                    ui.small("Select an organ to filter. Status labels describe data availability; no diagnosis is calculated.");
                    if ui.selectable_label(self.system.is_none(), "All measurements").clicked() {
                        self.system = None;
                        self.metric = None;
                    }
                    for system in self.backend.systems() {
                        if ui
                            .selectable_label(self.system.as_ref() == Some(&system.id), &system.name)
                            .clicked()
                        {
                            self.system = Some(system.id);
                            self.metric = None;
                        }
                    }
                    ui.separator();
                    ui.heading("Tasks");
                    let tasks = self.backend.tasks();
                    if tasks.is_empty() {
                        ui.label(RichText::new("No tasks received from the server yet.").color(MUT));
                    }
                    for task in tasks {
                        let mut checked = task.completed_at.is_some();
                        let enabled = self.backend.overview().link_up && self.backend.pending_writes() == 0;
                        if ui.add_enabled(enabled, egui::Checkbox::new(&mut checked, &task.title)).changed() {
                            match self.backend.set_task_completed(&task.id, checked) {
                                Ok(()) => self.notice("Task change sent; waiting for confirmation.".into()),
                                Err(error) => self.notice(error),
                            }
                        }
                        if let Some(at) = task.completed_at {
                            ui.small(format!("Completed: {at}"));
                        }
                    }
                });
            });
    }

    fn metric_card(&mut self, ui: &mut Ui, indicator: &IndicatorInfo, expanded: bool) {
        let latest = self.backend.latest(&indicator.id);
        let details = self.backend.measurement_detail(&indicator.id);
        Frame::new()
            .fill(SURFACE)
            .stroke(stroke(1.0, LINE))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.metric.as_ref() == Some(&indicator.id), RichText::new(&indicator.name).strong())
                        .clicked()
                    {
                        self.metric = Some(indicator.id.clone());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&indicator.unit).color(MUT));
                        ui.label(
                            RichText::new(latest.map(|reading| fmt_num(reading.value)).unwrap_or_else(|| "—".into())).font(mono(23.0)),
                        );
                    });
                });
                ui.label(RichText::new(details).size(12.0).color(MUT));
                if expanded {
                    ui.add_space(8.0);
                    telemetry_chart(
                        ui,
                        &self.backend.data_series(&indicator.id),
                        &indicator.unit,
                        if self.role == Role::Ground { 150.0 } else { 210.0 },
                    );
                    ui.add_space(8.0);
                    self.entry(ui, indicator);
                } else if ui.button("View trend / enter measurement").clicked() {
                    self.metric = Some(indicator.id.clone());
                }
            });
        ui.add_space(10.0);
    }

    fn entry(&mut self, ui: &mut Ui, indicator: &IndicatorInfo) {
        let enabled = self.backend.overview().link_up && self.backend.pending_writes() == 0;
        let mut submit = false;
        ui.horizontal(|ui| {
            ui.label("New measurement");
            let text = self.inputs.entry(indicator.id.clone()).or_default();
            let input = ui.add_enabled(enabled, egui::TextEdit::singleline(text).desired_width(130.0).hint_text("Value"));
            ui.label(&indicator.unit);
            submit = ui
                .add_enabled(enabled && !text.trim().is_empty(), egui::Button::new("Save to server"))
                .clicked()
                || (enabled && input.lost_focus() && ui.input(|state| state.key_pressed(egui::Key::Enter)));
        });
        if submit {
            let text = self.inputs.get(&indicator.id).map(String::as_str).unwrap_or("");
            match parse_value(text) {
                Ok(value) => match self.backend.submit_measurement(&indicator.id, value) {
                    Ok(()) => self.notice(format!("Sending {} — waiting for server confirmation.", indicator.name)),
                    Err(error) => self.notice(error),
                },
                Err(error) => self.notice(error.into()),
            }
        }
        ui.small("Recorded as a manual measurement with the current time. The server validates and stores it.");
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(20)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading(if self.role == Role::Ground {
                        "Shared telemetry"
                    } else {
                        "Your measurements"
                    });
                    ui.label(RichText::new("The browser, window and terminal use the same server records.").color(MUT));
                    ui.add_space(12.0);
                    let indicators = self.indicators();
                    let filtered: Vec<_> = indicators
                        .iter()
                        .filter(|indicator| self.system.as_ref().is_none_or(|system| &indicator.system == system))
                        .cloned()
                        .collect();
                    if filtered.is_empty() {
                        ui.label("No measurements are connected for this body system.");
                    }
                    if self.metric.is_some() && ui.button("Show all metrics in this view").clicked() {
                        self.metric = None;
                    }
                    for indicator in filtered {
                        if self.metric.as_ref().is_some_and(|id| id != &indicator.id) {
                            continue;
                        }
                        self.metric_card(
                            ui,
                            &indicator,
                            self.role == Role::Ground || self.metric.as_ref() == Some(&indicator.id),
                        );
                    }
                    if self.notices.len() > 1 {
                        egui::CollapsingHeader::new("Recent connection and save messages").show(ui, |ui| {
                            for notice in &self.notices {
                                ui.label(notice);
                            }
                        });
                    }
                });
            });
    }

    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.as_ref() else { return };
        let elapsed = self.started.elapsed();
        let series_ready = !(self.role == Role::Ground || self.metric.is_some())
            || self
                .indicators()
                .iter()
                .filter(|indicator| {
                    self.system.as_ref().is_none_or(|system| &indicator.system == system)
                        && self.metric.as_ref().is_none_or(|metric| &indicator.id == metric)
                })
                .all(|indicator| {
                    let series = self.backend.data_series(&indicator.id);
                    series.to_ms > series.from_ms
                });
        // Allow the first asynchronous HTTP snapshot to arrive before capturing the window.
        if !self.screenshot_requested
            && ((self.backend.overview().link_up && series_ready && elapsed >= Duration::from_secs(3))
                || elapsed >= Duration::from_secs(12))
        {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if let Some(image) = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let [width, height] = image.size;
            match image::save_buffer(path, image.as_raw(), width as u32, height as u32, image::ColorType::Rgba8) {
                Ok(()) => println!("screenshot saved: {path} ({width}x{height})"),
                Err(error) => eprintln!("screenshot failed: {error}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if elapsed > Duration::from_secs(20) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for LiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ticked.elapsed() >= Duration::from_millis(250) {
            self.backend.tick();
            for message in self.backend.take_messages() {
                self.notice(message);
            }
            self.ticked = Instant::now();
        }
        self.header(ctx);
        self.anatomy(ctx);
        self.central(ctx);
        self.capture(ctx);
        ctx.request_repaint_after(Duration::from_millis(
            if self.backend.pending_writes() > 0 || self.screenshot.is_some() {
                250
            } else {
                1000
            },
        ));
    }
}

fn parse_value(text: &str) -> Result<f64, &'static str> {
    let value: f64 = text.trim().parse().map_err(|_| "Enter a number using a decimal point.")?;
    if !value.is_finite() {
        return Err("Enter a finite number.");
    }
    Ok(value)
}

fn utc_label(milliseconds: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(milliseconds)
        .map(|time| time.format("%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "Unknown time".into())
}

/// Split at explicit missing buckets. A gap must never become a connecting line.
fn chart_segments(series: &TimedSeries) -> Vec<Vec<(i64, f64)>> {
    let mut segments = Vec::new();
    let mut segment = Vec::new();
    for point in &series.points {
        if let Some(value) = point.value.filter(|value| value.is_finite()) {
            if point.at_ms >= series.from_ms && point.at_ms < series.to_ms {
                segment.push((point.at_ms, value));
            }
        } else if !segment.is_empty() {
            segments.push(std::mem::take(&mut segment));
        }
    }
    if !segment.is_empty() {
        segments.push(segment);
    }
    segments
}

fn telemetry_chart(ui: &mut Ui, series: &TimedSeries, unit: &str, height: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, DEEP);
    let segments = chart_segments(series);
    let values: Vec<_> = segments.iter().flatten().map(|(_, value)| *value).collect();
    if values.is_empty() || series.to_ms <= series.from_ms {
        painter.text(rect.center(), Align2::CENTER_CENTER, "No samples in this interval", mono(12.0), MUT);
    } else {
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        // The API accepts every finite positive numeric measurement. Normalize before
        // subtracting so even very large accepted values cannot overflow the plot.
        let scale = low.abs().max(high.abs()).max(1.0);
        let (low, high) = (low / scale, high / scale);
        let pad = ((high - low) * 0.12).max(0.005);
        let (low, high) = (low - pad, high + pad);
        let plot = Rect::from_min_max(
            pos2(rect.left() + 60.0, rect.top() + 12.0),
            pos2(rect.right() - 12.0, rect.bottom() - 28.0),
        );
        let x = |time: i64| plot.left() + ((time - series.from_ms) as f64 / (series.to_ms - series.from_ms) as f64) as f32 * plot.width();
        let normalized_y = |value: f64| plot.bottom() - ((value - low) / (high - low)) as f32 * plot.height();
        let y = |value: f64| normalized_y(value / scale);
        for value in [low, (low + high) / 2.0, high] {
            painter.line_segment(
                [pos2(plot.left(), normalized_y(value)), pos2(plot.right(), normalized_y(value))],
                stroke(1.0, LINE),
            );
            let raw_value = value * scale;
            let label = if raw_value.is_finite() {
                if raw_value.abs() >= 1e6 {
                    format!("{raw_value:.1e}")
                } else {
                    format!("{raw_value:.1}")
                }
            } else {
                "—".into()
            };
            painter.text(
                pos2(plot.left() - 6.0, normalized_y(value)),
                Align2::RIGHT_CENTER,
                label,
                mono(10.0),
                MUT,
            );
        }
        painter.text(
            pos2(plot.left(), rect.bottom() - 12.0),
            Align2::LEFT_CENTER,
            utc_label(series.from_ms),
            mono(10.0),
            MUT,
        );
        painter.text(
            pos2(plot.right(), rect.bottom() - 12.0),
            Align2::RIGHT_CENTER,
            utc_label(series.to_ms),
            mono(10.0),
            MUT,
        );
        for segment in &segments {
            let points: Vec<_> = segment.iter().map(|(time, value)| pos2(x(*time), y(*value))).collect();
            if points.len() > 1 {
                painter.add(Shape::line(points.clone(), stroke(1.5, ACCENT)));
            }
            for point in points {
                painter.circle_filled(point, 2.2, ACCENT);
            }
        }
        if let Some(pointer) = response.hover_pos() {
            if let Some((time, value)) = segments
                .iter()
                .flatten()
                .min_by(|a, b| (x(a.0) - pointer.x).abs().total_cmp(&(x(b.0) - pointer.x).abs()))
            {
                painter.line_segment([pos2(x(*time), plot.top()), pos2(x(*time), plot.bottom())], stroke(1.0, MUT));
                response.on_hover_text(format!("{}\n{} {}", utc_label(*time), fmt_num(*value), unit));
            }
        }
    }
    painter.rect_stroke(rect, CornerRadius::ZERO, stroke(1.0, LINE), StrokeKind::Inside);
    ui.label(RichText::new(&series.note).size(11.0).color(MUT));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_model::TimePoint;

    #[test]
    fn utc_chart_keeps_gaps_and_subday_positions() {
        let series = TimedSeries {
            from_ms: 0,
            to_ms: 86_400_000,
            points: vec![
                TimePoint {
                    at_ms: 1_000,
                    value: Some(70.0),
                },
                TimePoint {
                    at_ms: 2_000,
                    value: Some(72.0),
                },
                TimePoint { at_ms: 3_000, value: None },
                TimePoint {
                    at_ms: 4_000,
                    value: Some(71.0),
                },
            ],
            note: String::new(),
        };
        assert_eq!(
            chart_segments(&series),
            vec![vec![(1_000, 70.0), (2_000, 72.0)], vec![(4_000, 71.0)]]
        );
        assert_eq!(utc_label(0), "01-01 00:00 UTC");
    }

    #[test]
    fn manual_input_preserves_fractional_values_and_rejects_nonfinite() {
        assert_eq!(parse_value(" 72.25 "), Ok(72.25));
        for invalid in ["", "NaN", "inf", "1,5", "pulse"] {
            assert!(parse_value(invalid).is_err());
        }
    }
}
