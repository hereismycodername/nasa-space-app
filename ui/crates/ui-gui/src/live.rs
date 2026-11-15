//! Shared-server window. Every measurement, task and health result comes from the HTTP cache of
//! `ui-http`. The look follows the instrument-panel prototype (`--demo`): a top bar with the link lamp,
//! chamfered cards, tracked mono captions, segmented controls, metric tiles and a status bar.
//! The window speaks Russian or English (`lang.rs`); server-written texts are shown as received.

use crate::lang::Lang;
use crate::{figure, link_label, theme, theme::*, widgets::*, Opts};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Frame, Galley, Layout, Margin,
    Painter, Pos2, Rect, RichText, Sense, Shape, Ui, Vec2,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ui_http::HttpBackend;
use ui_model::{Backend, HealthReport, HealthRule, HealthStatus, IndicatorInfo, Role, Status, TimedSeries};

const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Health,
    Server,
}

pub(crate) struct LiveApp {
    backend: HttpBackend,
    api_url: String,
    lang: Lang,
    /// language the fonts were last installed for
    fonts_for: Option<Lang>,
    figure: figure::Figure,
    mode: figure::Mode,
    role: Role,
    tab: Tab,
    system: Option<String>,
    metric: Option<String>,
    inputs: HashMap<String, String>,
    notices: VecDeque<String>,
    toast: Option<(String, Instant)>,
    lite: bool,
    ticked: Instant,
    started: Instant,
    screenshot: Option<String>,
    screenshot_requested: bool,
}

impl LiveApp {
    pub(crate) fn new(backend: HttpBackend, options: Opts) -> Self {
        let lang = options.lang.as_deref().and_then(Lang::parse).unwrap_or(Lang::Ru);
        let mut figure = figure::Figure::new().with_live_labels();
        figure.set_russian(lang == Lang::Ru);
        Self {
            backend,
            api_url: options.api_url.clone().unwrap_or_default(),
            lang,
            fonts_for: None,
            figure,
            mode: match options.mode.as_deref() {
                Some("skeleton") => figure::Mode::Skeleton,
                Some("xray") => figure::Mode::XRay,
                _ => figure::Mode::Organs,
            },
            role: if options.role.as_deref() == Some("ground") { Role::Ground } else { Role::Crew },
            tab: if options.tab.as_deref() == Some("system") { Tab::Server } else { Tab::Health },
            system: options.system,
            metric: options.metric,
            inputs: HashMap::new(),
            notices: VecDeque::new(),
            toast: None,
            lite: options.lite,
            ticked: Instant::now(),
            started: Instant::now(),
            screenshot: options.shot,
            screenshot_requested: false,
        }
    }

    fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.figure.set_russian(lang == Lang::Ru);
    }

    fn indicators(&self) -> Vec<IndicatorInfo> {
        self.backend.systems().iter().flat_map(|system| self.backend.indicators(&system.id)).collect()
    }

    fn indicator(&self, id: &str) -> Option<IndicatorInfo> {
        self.indicators().into_iter().find(|indicator| indicator.id == id)
    }

    fn name(&self, indicator: &IndicatorInfo) -> String {
        self.lang.metric(&indicator.id, &indicator.name)
    }

    fn unit(&self, indicator: &IndicatorInfo) -> String {
        self.lang.unit(&indicator.unit, &indicator.id)
    }

    fn system_name(&self, id: &str) -> String {
        let fallback = self.backend.systems().into_iter().find(|system| system.id == id).map(|system| system.name).unwrap_or_else(|| id.to_string());
        self.lang.system(id, &fallback)
    }

    fn rule_status(&self, indicator: &str) -> HealthStatus {
        self.backend.health_rule(indicator).map_or(HealthStatus::NotEvaluated, |rule| rule.current_status())
    }

    /// "fresh", "stale", ... of the displayed value (untranslated key).
    fn quality(&self, indicator: &str) -> String {
        quality_word(&self.backend.measurement_detail(indicator))
    }

    /// The body system highlighted on the figure: the open metric's system, else the filter.
    fn selected_system(&self) -> String {
        if let Some(indicator) = self.metric.as_deref().and_then(|id| self.indicator(id)) {
            return indicator.system;
        }
        self.system.clone().unwrap_or_default()
    }

    fn open_system(&mut self, system: String) {
        self.tab = Tab::Health;
        if self.metric.is_none() && self.system.as_ref() == Some(&system) {
            self.system = None;
        } else {
            self.system = Some(system);
        }
        self.metric = None;
    }

    /// Keep messages untranslated; they are translated when drawn, so a language switch applies to them too.
    fn notice(&mut self, text: String) {
        self.toast = Some((text.clone(), Instant::now()));
        self.notices.push_front(text);
        self.notices.truncate(12);
    }

    // ------------------------------------------------------------------ frame
    fn top_bar(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let overview = self.backend.overview();
        let connecting = !overview.link_up && self.backend.connection_label().starts_with("Connecting");
        let profile = self.backend.selected_crew();
        let pending = self.backend.pending_writes();
        egui::TopBottomPanel::top("live-top")
            .exact_height(48.0)
            .frame(Frame::new().fill(SURFACE).stroke(stroke(1.0, LINE)).inner_margin(Margin::symmetric(14, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("ASTROCHECK").font(display(14.0)).strong().extra_letter_spacing(2.6).color(TXT));
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(tracked(if profile.is_empty() { l.t("No profile", "Нет профиля") } else { &profile }, 11.0, TXT));
                    ui.separator();
                    let (color, text) = if overview.link_up {
                        (OK, l.t("Link OK", "Связь есть"))
                    } else if connecting {
                        (WARN, l.t("Connecting", "Подключение"))
                    } else {
                        (CRIT, l.t("Offline", "Нет связи"))
                    };
                    led(ui, color);
                    ui.label(tracked(text, 11.0, color));
                    ui.label(RichText::new(&self.api_url).font(mono(11.0)).color(MUT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut lang = self.lang;
                        if segmented(ui, &mut lang, &[(Lang::En, "EN"), (Lang::Ru, "RU")]) {
                            self.set_lang(lang);
                        }
                        ui.add_space(6.0);
                        let mut role = self.role;
                        if segmented(ui, &mut role, &[(Role::Ground, "Ground"), (Role::Crew, "Crew")]) {
                            self.role = role;
                            self.metric = None;
                        }
                        if pending > 0 {
                            ui.add_space(6.0);
                            ui.label(tracked(l.t("Sending", "Отправка"), 10.5, ACCENT));
                            ui.spinner();
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let report = self.backend.health_report();
        let overview = self.backend.overview();
        let pending = self.backend.pending_writes();
        let rules = report.evaluation.as_ref().map(|e| (e.evaluated_metric_count, e.total_metric_count));
        let toast = self.toast.as_ref().map(|(message, _)| l.message(message));
        egui::TopBottomPanel::bottom("live-status")
            .exact_height(26.0)
            .frame(Frame::new().fill(DEEP).stroke(stroke(1.0, LINE)).inner_margin(Margin::symmetric(14, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let text = |t: String, c: Color32| RichText::new(t).font(mono(11.0)).color(c);
                    ui.label(text("API".into(), MUT));
                    ui.label(text(if overview.link_up { "OK" } else { l.t("DOWN", "НЕТ") }.into(), if overview.link_up { OK } else { CRIT }));
                    ui.label(text(format!("  {}", l.t("HEALTH", "ОЦЕНКА")), MUT));
                    ui.label(text(l.health(report.status).to_uppercase(), health_color(report.status)));
                    if let Some((evaluated, total)) = rules {
                        ui.label(text(format!("  {} {evaluated}/{total}", l.t("RULES", "ПРАВИЛА")), MUT));
                    }
                    ui.label(text(format!("  {} {}", l.t("METRICS", "ПОКАЗАТЕЛИ"), overview.total_indicators), MUT));
                    ui.label(text(format!("  {} {pending}", l.t("PENDING", "В ОЧЕРЕДИ")), if pending > 0 { ACCENT } else { MUT }));
                    ui.label(text(format!("  {}", l.t("PROTOTYPE RULES, NOT A DIAGNOSIS", "ДЕМО-ПРАВИЛА, НЕ ДИАГНОЗ")), MUT));
                    if let Some(message) = toast {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(message).size(12.0).color(ACCENT).strong()).truncate());
                        });
                    }
                });
            });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        let time = ctx.input(|input| input.time);
        let animate = !self.lite;
        egui::SidePanel::left("live-left")
            .exact_width(384.0)
            .resizable(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    caption(ui, l.t("Anatomy", "Анатомия"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut mode = self.mode;
                        let modes = [(figure::Mode::XRay, l.t("X-ray", "Рентген")), (figure::Mode::Skeleton, l.t("Skeleton", "Скелет")), (figure::Mode::Organs, l.t("Organs", "Органы"))];
                        if segmented(ui, &mut mode, &modes) {
                            self.mode = mode;
                        }
                    });
                });
                ui.add_space(4.0);
                let selected = self.selected_system();
                let height = (ui.available_height() - 4.0).max(300.0);
                let clicked = self.figure.show(ui, vec2(ui.available_width(), height), &|system| self.backend.system_status(system), &selected, self.mode, time, animate);
                if let Some(system) = clicked {
                    self.open_system(system);
                }
            });
    }

    fn central(&mut self, ctx: &egui::Context) {
        let l = self.lang;
        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::same(14))).show(ctx, |ui| {
            self.alert_strip(ui);
            ui.horizontal(|ui| {
                let main = if self.role == Role::Crew { l.t("Health", "Здоровье") } else { l.t("Telemetry", "Телеметрия") };
                for (tab, name) in [(Tab::Health, main), (Tab::Server, l.t("Server link", "Связь с сервером"))] {
                    if tab_btn(ui, name, self.tab == tab) {
                        self.tab = tab;
                    }
                }
            });
            let rect = ui.min_rect();
            ui.painter().hline(rect.left()..=ui.max_rect().right(), rect.bottom() + 1.0, stroke(1.0, LINE));
            ui.add_space(10.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::Health => {
                    self.crumbs(ui);
                    if let Some(id) = self.metric.clone() {
                        self.metric_view(ui, &id);
                    } else if let Some(system) = self.system.clone() {
                        self.system_view(ui, &system);
                    } else if self.role == Role::Crew {
                        self.crew_overview(ui);
                    } else {
                        self.ground_overview(ui);
                    }
                }
                Tab::Server => self.server_tab(ui),
            });
        });
    }

    /// One strip above the tabs: the server link when it is down, otherwise an urgent rule result.
    fn alert_strip(&mut self, ui: &mut Ui) {
        let l = self.lang;
        if !self.backend.overview().link_up {
            let label = self.backend.connection_label();
            let connecting = label.starts_with("Connecting");
            let color = if connecting { WARN } else { CRIT };
            card().fill(alpha(color, 0.12)).line(alpha(color, 0.7)).margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    led(ui, color);
                    let title = if connecting { l.t("CONNECTING", "ПОДКЛЮЧЕНИЕ") } else { l.t("SERVER OFFLINE", "СЕРВЕР НЕДОСТУПЕН") };
                    ui.label(RichText::new(title).size(12.0).strong().color(color));
                    ui.label(RichText::new(&self.api_url).font(mono(12.0)).color(TXT));
                });
                ui.label(RichText::new(l.message(&label)).size(12.0).color(TXT));
            });
            ui.add_space(8.0);
            return;
        }
        let report = self.backend.health_report();
        if report.status != HealthStatus::Critical {
            return;
        }
        let urgent: Vec<IndicatorInfo> = self.indicators().into_iter().filter(|indicator| self.rule_status(&indicator.id) == HealthStatus::Critical).collect();
        let mut open: Option<String> = None;
        card().fill(alpha(CRIT, 0.12)).line(alpha(CRIT, 0.7)).margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                led(ui, CRIT);
                ui.label(RichText::new(l.t("URGENT ACTION", "СРОЧНО")).size(12.0).strong().color(CRIT));
                if let Some(first) = urgent.first() {
                    let value = self.backend.latest(&first.id).map(|r| format!("  {} {}", l.num(r.value), self.unit(first))).unwrap_or_default();
                    ui.label(RichText::new(format!("{}{}", self.name(first), value)).size(13.0).strong());
                    if urgent.len() > 1 {
                        ui.label(RichText::new(format!("+{} {}", urgent.len() - 1, l.t("more", "ещё"))).size(12.0).color(MUT));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(l.t("Review", "Открыть")).clicked() {
                            open = Some(first.id.clone());
                        }
                    });
                }
            });
            ui.label(RichText::new(l.message(&report.message)).size(12.0).color(Color32::from_rgb(0xf3, 0xc6, 0xc8)));
        });
        ui.add_space(8.0);
        if let Some(id) = open {
            self.tab = Tab::Health;
            self.metric = Some(id);
        }
    }

    fn crumbs(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let profile = self.backend.selected_crew();
        let root = if self.role == Role::Crew || profile.is_empty() {
            l.t("Overview", "Обзор").to_string()
        } else {
            format!("{}  \u{203A}  {profile}", l.t("Crew status", "Экипаж"))
        };
        let mut to: Option<(Option<String>, Option<String>)> = None;
        ui.horizontal(|ui| {
            if link_label(ui, &root, self.system.is_none() && self.metric.is_none()) {
                to = Some((None, None));
            }
            let separator = |ui: &mut Ui| {
                ui.label(RichText::new("\u{203A}").color(MUT));
            };
            if let Some(indicator) = self.metric.as_deref().and_then(|id| self.indicator(id)) {
                separator(ui);
                if link_label(ui, &self.system_name(&indicator.system), false) {
                    to = Some((Some(indicator.system.clone()), None));
                }
                separator(ui);
                ui.label(RichText::new(self.name(&indicator)).size(12.5).color(MUT));
            } else if let Some(system) = &self.system {
                separator(ui);
                ui.label(RichText::new(self.system_name(system)).size(12.5).color(MUT));
            }
        });
        ui.add_space(6.0);
        if let Some((system, metric)) = to {
            self.system = system;
            self.metric = metric;
        }
    }

    // ------------------------------------------------------------------ crew overview
    fn crew_overview(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let report = self.backend.health_report();
        let overview = self.backend.overview();
        let profile = self.backend.selected_crew();
        let pending = self.backend.pending_writes();
        let indicators = self.indicators();
        let gap = 10.0;
        let width = ((ui.available_width() - 2.0 * gap) / 3.0).floor();
        let mut task_change: Option<(String, bool)> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            vcard(ui, width, 164.0, |ui| {
                caption(ui, l.t("Health guidance", "Оценка сервера"));
                ui.add_space(4.0);
                let color = health_color(report.status);
                let (evaluated, total) = report.evaluation.as_ref().map_or((0, overview.total_indicators), |e| (e.evaluated_metric_count, e.total_metric_count));
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
                let right = format!("{evaluated} / {total}");
                let right_width = ui.fonts(|fonts| fonts.layout_no_wrap(right.clone(), mono(14.0), TXT)).size().x;
                let status = one_line(ui, &l.health(report.status).to_uppercase(), mono(16.0), color, row.width() - right_width - 12.0);
                let painter = ui.painter();
                painter.galley(pos2(row.left(), row.center().y - 8.0 - status.size().y / 2.0), status, color);
                painter.text(pos2(row.left(), row.center().y + 12.0), Align2::LEFT_CENTER, l.t("SERVER RULE SET", "ПРАВИЛА СЕРВЕРА"), mono(9.5), MUT);
                painter.text(pos2(row.right(), row.center().y - 8.0), Align2::RIGHT_CENTER, right, mono(14.0), TXT);
                painter.text(pos2(row.right(), row.center().y + 12.0), Align2::RIGHT_CENTER, l.t("EVALUATED", "ОЦЕНЕНО"), mono(9.5), MUT);
                let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
                let fraction = if total > 0 { evaluated as f32 / total as f32 } else { 0.0 };
                seg_bar(ui.painter(), Rect::from_min_size(bar.min, vec2(bar.width(), 9.0)), fraction, total.max(1), color);
                ui.add_space(4.0);
                let at = report
                    .evaluated_at
                    .as_deref()
                    .map(|t| format!("{} {}", l.t("Evaluated", "Оценено"), l.timestamp(t)))
                    .unwrap_or_else(|| l.t("Waiting for the server evaluation", "Ожидается оценка сервера").into());
                ui.label(RichText::new(at).size(11.0).color(MUT));
                ui.label(RichText::new(l.t("Prototype rules, not a diagnosis", "Демо-правила, не диагноз")).size(11.0).color(MUT));
            });
            vcard(ui, width, 164.0, |ui| {
                caption(ui, l.t("Daily tasks", "Задачи на сегодня"));
                let tasks = self.backend.tasks();
                let enabled = overview.link_up && pending == 0;
                if tasks.is_empty() {
                    ui.label(RichText::new(l.t("No tasks received from the server yet.", "Сервер ещё не прислал задачи.")).size(12.0).color(MUT));
                }
                for task in &tasks {
                    let mut checked = task.completed_at.is_some();
                    if ui.add_enabled(enabled, egui::Checkbox::new(&mut checked, RichText::new(&task.title).size(12.5))).changed() {
                        task_change = Some((task.id.clone(), checked));
                    }
                }
                ui.add_space(4.0);
                let done = tasks.iter().filter(|task| task.completed_at.is_some()).count();
                ui.label(RichText::new(format!("{done} / {} {}", tasks.len(), l.t("done today", "выполнено"))).font(mono(11.5)).color(MUT));
            });
            vcard(ui, width, 164.0, |ui| {
                caption(ui, l.t("Data link", "Канал данных"));
                ui.add(egui::Label::new(RichText::new(&self.api_url).font(mono(12.5))).truncate());
                let fresh = indicators.iter().filter(|i| self.quality(&i.id) == "fresh").count();
                for (key, value) in [
                    (l.t("Profile", "Профиль"), if profile.is_empty() { "-".to_string() } else { profile.clone() }),
                    (l.t("Fresh data", "Свежие"), format!("{fresh} {} {} {}", l.t("/", "из"), indicators.len(), l.t("metrics", ""))),
                    (l.t("Pending", "В очереди"), format!("{pending} {}", l.t("writes", "записей"))),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([86.0, 18.0], egui::Label::new(RichText::new(key).size(12.0).color(MUT)));
                        ui.label(RichText::new(value).font(mono(12.5)));
                    });
                }
                ui.add_space(2.0);
                let (text, color) = if overview.link_up { (l.t("Shared server", "Общий сервер"), OK) } else { (l.t("Offline cache", "Сохранённые данные"), WARN) };
                ui.horizontal(|ui| pill(ui, text, color));
            });
        });
        if let Some((id, completed)) = task_change {
            match self.backend.set_task_completed(&id, completed) {
                Ok(()) => self.notice("Task change sent; waiting for confirmation.".into()),
                Err(error) => self.notice(error),
            }
        }
        ui.add_space(10.0);
        guidance_card(ui, l, &report);
        ui.add_space(10.0);

        // body systems + findings, equal heights
        let systems = self.backend.systems();
        let mut findings: Vec<(IndicatorInfo, HealthRule)> = indicators
            .iter()
            .filter_map(|indicator| {
                let rule = self.backend.health_rule(&indicator.id)?;
                matches!(rule.current_status(), HealthStatus::Attention | HealthStatus::Critical).then(|| (indicator.clone(), rule))
            })
            .collect();
        findings.sort_by_key(|(_, rule)| rule.current_status() != HealthStatus::Critical);
        let row_height = (systems.len() as f32 * 30.0).max(findings.len().min(5) as f32 * 58.0) + 38.0;
        let mut go: Option<(Option<String>, Option<String>)> = None;
        ui.spacing_mut().item_spacing.x = gap;
        ui.columns(2, |columns| {
            card().show(&mut columns[0], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(row_height);
                caption(ui, l.t("Body systems", "Системы организма"));
                ui.add_space(4.0);
                for system in &systems {
                    let (label, color) = system_label(l, self.backend.system_status(&system.id));
                    let count = self.backend.indicators(&system.id).len();
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::ZERO, SURFACE2);
                    }
                    let status = label.to_uppercase();
                    let status_width = ui.fonts(|fonts| fonts.layout_no_wrap(status.clone(), mono(10.0), color)).size().x;
                    let count_text = format!("{count} {}", if count == 1 { l.t("metric", "пок.") } else { l.t("metrics", "пок.") });
                    let name = one_line(ui, &self.system_name(&system.id), FontId::proportional(12.5), TXT, rect.width() - status_width - 110.0);
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(Rect::from_center_size(rect.left_center() + vec2(10.0, 0.0), vec2(7.0, 7.0)), CornerRadius::ZERO, color);
                    painter.galley(pos2(rect.left() + 22.0, rect.center().y - name.size().y / 2.0), name, TXT);
                    painter.text(rect.right_center() - vec2(status_width + 22.0, 0.0), Align2::RIGHT_CENTER, count_text, mono(10.0), MUT);
                    painter.text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, status, mono(10.0), color);
                    if response.clicked() {
                        go = Some((Some(system.id.clone()), None));
                    }
                }
            });
            card().show(&mut columns[1], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(row_height);
                caption(ui, l.t("Findings", "Сигналы правил"));
                ui.add_space(4.0);
                if findings.is_empty() {
                    ui.label(RichText::new(l.t("No rule alerts. This does not mean that all organs are healthy.", "Срабатываний нет. Это не означает, что все органы здоровы.")).size(12.5).color(MUT));
                }
                for (indicator, rule) in findings.iter().take(5) {
                    let color = health_color(rule.current_status());
                    let value = self.backend.latest(&indicator.id).map(|r| format!("  {} {}", l.num(r.value), self.unit(indicator))).unwrap_or_default();
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::ZERO, SURFACE2);
                    }
                    let width = rect.width() - 22.0;
                    let lines = [
                        one_line(ui, &format!("{}{}", self.name(indicator), value), FontId::proportional(12.5), TXT, width),
                        one_line(ui, l.health(rule.current_status()), FontId::proportional(11.0), color, width),
                        one_line(ui, &rule.suggested_action, FontId::proportional(10.5), MUT, width),
                    ];
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, 46.0)), CornerRadius::ZERO, color);
                    for (galley, y) in lines.into_iter().zip([14.0, 31.0, 46.0]) {
                        painter.galley(rect.min + vec2(14.0, y - galley.size().y / 2.0), galley, TXT);
                    }
                    if response.clicked() {
                        go = Some((None, Some(indicator.id.clone())));
                    }
                }
            });
        });
        ui.add_space(10.0);
        caption(ui, l.t("Metrics", "Показатели"));
        ui.add_space(4.0);
        self.tiles(ui, &indicators);
        if let Some((system, metric)) = go {
            self.system = system;
            self.metric = metric;
        }
    }

    fn tiles(&mut self, ui: &mut Ui, indicators: &[IndicatorInfo]) {
        let gap = 8.0;
        let columns = if ui.available_width() >= 880.0 { 4 } else if ui.available_width() >= 620.0 { 3 } else { 2 };
        let width = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32 - 1.0;
        let mut clicked: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            for indicator in indicators {
                let tile = Tile {
                    name: self.name(indicator),
                    unit: self.unit(indicator),
                    value: self.backend.latest(&indicator.id).map(|r| self.lang.num(r.value)),
                    quality: self.quality(&indicator.id),
                    status: self.rule_status(&indicator.id),
                };
                if live_tile(ui, self.lang, vec2(width, 100.0), &tile, &self.backend.data_series(&indicator.id)).clicked() {
                    clicked = Some(indicator.id.clone());
                }
            }
        });
        if let Some(id) = clicked {
            self.metric = Some(id);
        }
    }

    // ------------------------------------------------------------------ system and metric
    fn system_view(&mut self, ui: &mut Ui, id: &str) {
        let l = self.lang;
        let (label, color) = system_label(l, self.backend.system_status(id));
        let indicators = self.backend.indicators(id);
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.system_name(id)).font(display(22.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| pill(ui, label, color));
        });
        ui.add_space(4.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let text = if indicators.is_empty() {
                l.t("No measurements are connected for this body system yet.", "Для этой системы пока нет подключённых измерений.")
            } else {
                l.t(
                    "The status comes from the server's health rules for the metrics below. Data freshness is shown on each tile and is separate from health guidance.",
                    "Статус берётся из правил сервера для показателей ниже. Свежесть данных указана на каждой плитке и не является оценкой здоровья.",
                )
            };
            ui.label(RichText::new(text).size(12.5).color(MUT));
        });
        if !indicators.is_empty() {
            ui.add_space(10.0);
            caption(ui, l.t("Metrics", "Показатели"));
            ui.add_space(4.0);
            self.tiles(ui, &indicators);
        }
    }

    fn metric_view(&mut self, ui: &mut Ui, id: &str) {
        let l = self.lang;
        let Some(indicator) = self.indicator(id) else {
            ui.label(RichText::new(format!("{} '{id}'.", l.t("Unknown indicator", "Неизвестный показатель"))).color(CRIT));
            if ui.button(l.t("Back to overview", "К обзору")).clicked() {
                self.metric = None;
            }
            return;
        };
        let (name, unit) = (self.name(&indicator), self.unit(&indicator));
        let latest = self.backend.latest(id);
        let detail = self.backend.measurement_detail(id);
        let quality = quality_word(&detail);
        let rule = self.backend.health_rule(id);
        let status = rule.as_ref().map_or(HealthStatus::NotEvaluated, |r| r.current_status());
        let color = health_color(status);
        let series = self.backend.data_series(id);

        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).font(display(22.0)));
            pill(ui, l.short(status), color);
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(latest.map(|r| l.num(r.value)).unwrap_or_else(|| "-".into())).font(mono(40.0)));
            ui.label(RichText::new(&unit).font(mono(14.0)).color(MUT));
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(tracked(&l.quality(&quality), 10.5, quality_color(&quality)));
                ui.label(RichText::new(l.provenance(&detail)).size(11.5).color(MUT));
            });
        });
        ui.add_space(4.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("Last seven days, UTC", "Последние 7 дней, UTC"));
            ui.add_space(4.0);
            telemetry_chart(ui, l, &series, &unit, 230.0);
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("Log a new measurement", "Новое измерение"));
            ui.add_space(2.0);
            self.entry(ui, &indicator);
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("Health guidance", "Оценка правила"));
            ui.add_space(2.0);
            match &rule {
                Some(rule) => rule_block(ui, l, rule),
                None => {
                    ui.label(RichText::new(l.t("The server has not published a health rule for this metric yet.", "Сервер пока не опубликовал правило для этого показателя.")).size(12.5).color(MUT));
                }
            }
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("About this measurement", "Об измерении"));
            let (why, how): (&str, Vec<String>) = match l {
                Lang::En => (&indicator.why, indicator.how.clone()),
                Lang::Ru => (
                    "Хранится на общем сервере; оценка правил и свежесть данных показываются отдельно.",
                    vec![format!("Введите измеренное значение в единицах «{unit}». Время ввода записывается как время измерения."), "Дождитесь подтверждения сервера, прежде чем закрывать приложение.".into()],
                ),
            };
            ui.label(RichText::new(why).size(12.5).color(MUT));
            ui.add_space(2.0);
            ui.label(RichText::new(l.t("How to measure", "Как измерить")).size(12.0).strong());
            for (n, step) in how.iter().enumerate() {
                ui.label(RichText::new(format!("{}. {}", n + 1, step)).size(12.5).color(MUT));
            }
        });
    }

    fn entry(&mut self, ui: &mut Ui, indicator: &IndicatorInfo) {
        let l = self.lang;
        let unit = self.unit(indicator);
        let enabled = self.backend.overview().link_up && self.backend.pending_writes() == 0;
        let mut submit = false;
        ui.horizontal(|ui| {
            let text = self.inputs.entry(indicator.id.clone()).or_default();
            let input = ui.add_enabled(enabled, egui::TextEdit::singleline(text).desired_width(130.0).font(mono(14.0)).hint_text(l.t("Value", "Значение")));
            ui.label(RichText::new(&unit).font(mono(12.0)).color(MUT));
            submit = ui.add_enabled(enabled && !text.trim().is_empty(), primary(l.t("Save to server", "Сохранить"))).clicked()
                || (enabled && input.lost_focus() && ui.input(|state| state.key_pressed(egui::Key::Enter)));
        });
        if submit {
            let text = self.inputs.get(&indicator.id).map(String::as_str).unwrap_or("");
            match parse_value(text, l) {
                Ok(value) => match self.backend.submit_measurement(&indicator.id, value) {
                    Ok(()) => self.notice(format!("{} {}{}", l.t("Sending", "Отправка:"), self.name(indicator), l.t("; waiting for server confirmation.", "; ждём подтверждения сервера."))),
                    Err(error) => self.notice(error),
                },
                Err(error) => self.notice(error.into()),
            }
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new(l.t(
                "Recorded as a manual measurement with the current time. The server validates and stores it; success is shown only after its confirmation.",
                "Записывается как ручное измерение с текущим временем. Сервер проверяет и сохраняет его; успех показывается только после подтверждения.",
            ))
            .size(11.0)
            .color(MUT),
        );
    }

    // ------------------------------------------------------------------ ground: stacked telemetry
    fn ground_overview(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let report = self.backend.health_report();
        guidance_card(ui, l, &report);
        ui.add_space(10.0);
        caption(ui, l.t("Stacked channels, last seven days, UTC", "Все каналы, последние 7 дней, UTC"));
        ui.add_space(4.0);
        let mut open: Option<String> = None;
        for indicator in self.indicators() {
            let status = self.rule_status(&indicator.id);
            let color = health_color(status);
            let value = self.backend.latest(&indicator.id).map(|r| l.num(r.value)).unwrap_or_else(|| "-".into());
            let quality = self.quality(&indicator.id);
            let series = self.backend.data_series(&indicator.id);
            let (name, unit) = (self.name(&indicator), self.unit(&indicator));
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    led(ui, color);
                    if link_label(ui, &name, false) {
                        open = Some(indicator.id.clone());
                    }
                    ui.label(RichText::new(&value).font(mono(13.0)).color(TXT));
                    ui.label(RichText::new(&unit).font(mono(11.0)).color(MUT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        pill(ui, l.short(status), color);
                        ui.label(tracked(&l.quality(&quality), 9.5, quality_color(&quality)));
                    });
                });
                telemetry_chart(ui, l, &series, &unit, 120.0);
            });
            ui.add_space(8.0);
        }
        if let Some(id) = open {
            self.metric = Some(id);
        }
    }

    // ------------------------------------------------------------------ server link
    fn server_tab(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let health = self.backend.health();
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("Connection", "Подключение"));
            ui.label(RichText::new(l.message(&self.backend.connection_label())).size(12.5));
            ui.add_space(4.0);
            for check in &health.checks {
                ui.horizontal(|ui| {
                    led(ui, if check.ok { OK } else { CRIT });
                    ui.label(RichText::new(l.message(&check.name)).size(12.5).strong());
                    ui.label(RichText::new(l.message(&check.detail)).size(12.0).color(MUT));
                });
            }
            ui.add_space(2.0);
            ui.label(RichText::new(l.message(&health.platform)).font(mono(11.0)).color(MUT));
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, l.t("Connection and save messages", "Сообщения о связи и сохранении"));
            if self.notices.is_empty() {
                ui.label(RichText::new(l.t("No messages yet.", "Сообщений пока нет.")).size(12.5).color(MUT));
            }
            for notice in &self.notices {
                ui.label(RichText::new(l.message(notice)).size(12.5));
            }
        });
    }

    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.as_ref() else {
            return;
        };
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
            && ((self.backend.overview().link_up && series_ready && elapsed >= Duration::from_secs(3)) || elapsed >= Duration::from_secs(12))
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
            if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5)) {
                self.toast = None;
            }
        }
        if self.fonts_for != Some(self.lang) {
            theme::set_russian_text(ctx, self.lang == Lang::Ru);
            self.fonts_for = Some(self.lang);
            ctx.request_repaint();
        }
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.left_panel(ctx);
        self.central(ctx);
        self.capture(ctx);
        // Repaint budget as in the prototype: 10 fps while focused and animating, otherwise once a second.
        let focused = ctx.input(|input| input.focused);
        let interval = if self.screenshot.is_some() || self.backend.pending_writes() > 0 {
            250
        } else if !self.lite && focused {
            100
        } else {
            1000
        };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }
}

// ---------------------------------------------------------------------- helpers

fn health_color(status: HealthStatus) -> Color32 {
    match status {
        HealthStatus::NoAlert => Color32::from_rgb(89, 199, 139),
        HealthStatus::Attention => WARN,
        HealthStatus::Critical => CRIT,
        HealthStatus::InsufficientData | HealthStatus::NotEvaluated => MUT,
    }
}

fn system_label(l: Lang, status: Status) -> (&'static str, Color32) {
    let health = match status {
        Status::Normal => HealthStatus::NoAlert,
        Status::Watch => HealthStatus::Attention,
        Status::Act => HealthStatus::Critical,
        Status::Unassessed | Status::NoData => HealthStatus::NotEvaluated,
    };
    (l.short(health), health_color(health))
}

/// Data quality key of the displayed value ("fresh", "stale", ...), the first part of the measurement detail.
fn quality_word(detail: &str) -> String {
    let mut parts = detail.split(';').map(str::trim);
    let first = parts.next().unwrap_or("");
    if first == "Offline / cached" {
        parts.next().unwrap_or("").to_string()
    } else {
        first.to_string()
    }
}

fn quality_color(quality: &str) -> Color32 {
    if quality == "fresh" {
        MUT
    } else {
        NONE
    }
}

/// Text on one line, cut with an ellipsis at `width`.
fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping { max_width: width.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('\u{2026}') };
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn source_links(ui: &mut Ui, urls: &[String]) {
    for url in urls {
        if url.starts_with("https://") || url.starts_with("http://") {
            ui.hyperlink_to(RichText::new(url).size(11.5), url);
        } else {
            ui.label(RichText::new(url).size(11.5).color(MUT));
        }
    }
}

/// Server guidance for the whole profile: summary plus the folded reference sections.
fn guidance_card(ui: &mut Ui, l: Lang, report: &HealthReport) {
    let color = health_color(report.status);
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, l.t("Server guidance", "Рекомендации сервера"));
        let response = ui.horizontal_top(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(l.health(report.status)).size(13.0).strong().color(color));
                ui.label(RichText::new(l.message(&report.message)).size(12.5));
                if let Some(health) = &report.evaluation {
                    if report.status == health.status && health.rules.iter().all(|rule| !rule.status.is_evaluated() || rule.current) {
                        ui.label(RichText::new(&health.explanation).size(12.5).color(MUT));
                    }
                }
            });
        });
        let rect = response.response.rect;
        ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius::ZERO, color);
        let Some(health) = &report.evaluation else { return };
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new(l.t("When to get urgent help", "Когда нужна срочная помощь")).size(12.5))
            .id_salt("urgent-help")
            .default_open(report.status == HealthStatus::Critical)
            .show(ui, |ui| {
                ui.label(RichText::new(&health.emergency_guidance).size(12.5).color(CRIT));
                ui.label(RichText::new(l.t("The application does not send messages to the crew or ground automatically.", "Приложение не отправляет сообщения экипажу или на Землю автоматически.")).size(11.0).color(MUT));
            });
        egui::CollapsingHeader::new(RichText::new(l.t("Spaceflight hazards and prevention", "Факторы полёта и профилактика")).size(12.5)).id_salt("hazards").show(ui, |ui| {
            for topic in &health.education {
                ui.separator();
                ui.label(RichText::new(&topic.title).size(12.5).strong());
                ui.label(RichText::new(&topic.explanation).size(12.5).color(MUT));
                ui.label(RichText::new(&topic.suggested_action).size(12.5));
                source_links(ui, &topic.source_urls);
            }
        });
        egui::CollapsingHeader::new(RichText::new(l.t("Scope and limitations", "Границы применения")).size(12.5)).id_salt("scope").show(ui, |ui| {
            ui.label(RichText::new(&health.disclaimer).size(12.5).color(MUT));
            ui.label(RichText::new(format!("{} {}  \u{00B7}  {}", l.t("Rule set", "Набор правил"), health.rule_set_version, l.t("demonstration guidance", "демонстрационные рекомендации"))).font(mono(10.5)).color(MUT));
        });
    });
}

/// One metric rule: result, reason, escalation and next step beside a status bar; rule data folded.
fn rule_block(ui: &mut Ui, l: Lang, rule: &HealthRule) {
    let status = rule.current_status();
    let color = health_color(status);
    let urgent = rule.current && rule.status == HealthStatus::Critical;
    let response = ui.horizontal_top(|ui| {
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(l.health(status)).size(13.0).strong().color(color));
            if !rule.current {
                ui.label(RichText::new(format!("{}: {}", l.t("Previous result", "Предыдущий результат"), l.health(rule.status))).size(12.0).color(MUT));
            }
            if let Some(reason) = &rule.unavailable_reason {
                ui.label(RichText::new(l.message(reason)).size(12.0).color(MUT));
            }
            ui.label(RichText::new(&rule.explanation).size(12.5));
            ui.label(RichText::new(l.escalation(rule.escalation)).size(12.5).strong().color(if urgent { CRIT } else { TXT }));
            ui.label(RichText::new(format!("\u{2022} {}", rule.suggested_action)).size(12.5));
            if !rule.flight_context.is_empty() {
                ui.label(RichText::new(&rule.flight_context).size(12.0).color(MUT));
            }
        });
    });
    let rect = response.response.rect;
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius::ZERO, color);
    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new(format!("{}: {}", l.t("Rule and sources", "Правило и источники"), rule.title)).size(12.5))
        .id_salt(&rule.rule_id)
        .show(ui, |ui| {
            let until = rule.valid_until.as_deref().map(|t| l.timestamp(t)).unwrap_or_else(|| l.t("not evaluated", "не оценено").into());
            ui.label(RichText::new(format!("{} / {}  \u{00B7}  {} {until}", rule.rule_id, rule.rule_version, l.t("valid until", "действует до"))).font(mono(10.5)).color(MUT));
            source_links(ui, &rule.source_urls);
        });
}

/// What a metric tile shows, already in the window's language.
struct Tile {
    name: String,
    unit: String,
    value: Option<String>,
    quality: String,
    status: HealthStatus,
}

/// Metric tile in the prototype's shape: name, large value, data quality, rule result, sparkline.
fn live_tile(ui: &mut Ui, l: Lang, size: Vec2, tile: &Tile, series: &TimedSeries) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let alert = matches!(tile.status, HealthStatus::Attention | HealthStatus::Critical);
    let color = health_color(tile.status);
    let name = one_line(ui, &tile.name, FontId::proportional(11.5), MUT, size.x - 28.0);
    let value = tile.value.clone().unwrap_or_else(|| "-".into());
    let value_width = ui.fonts(|fonts| fonts.layout_no_wrap(value.clone(), mono(26.0), TXT)).size().x;
    let unit_width = ui.fonts(|fonts| fonts.layout_no_wrap(tile.unit.clone(), mono(10.5), MUT)).size().x;
    let painter = ui.painter_at(rect.expand(1.0));
    let border = if response.hovered() { alpha(ACCENT, 0.6) } else { LINE };
    painter.add(panel_shape(rect, CUT, if response.hovered() { SURFACE2 } else { SURFACE }, stroke(1.0, border)));
    painter.galley(pos2(rect.left() + 14.0, rect.top() + 16.0 - name.size().y / 2.0), name, MUT);
    let y = rect.top() + 46.0;
    painter.text(pos2(rect.left() + 14.0, y), Align2::LEFT_CENTER, &value, mono(26.0), TXT);
    painter.text(pos2(rect.left() + 18.0 + value_width, y + 5.0), Align2::LEFT_CENTER, &tile.unit, mono(10.5), MUT);
    painter.text(rect.min + vec2(14.0, size.y - 16.0), Align2::LEFT_CENTER, l.quality(&tile.quality).to_uppercase(), mono(9.5), quality_color(&tile.quality));
    if tile.status != HealthStatus::NoAlert {
        painter.text(pos2(rect.right() - 12.0, rect.bottom() - 16.0), Align2::RIGHT_CENTER, l.short(tile.status).to_uppercase(), mono(9.5), color);
    }
    // the sparkline takes what the value and unit leave free, up to 94 px
    let spark_left = (rect.left() + 28.0 + value_width + unit_width).max(rect.right() - 108.0);
    if rect.right() - 14.0 - spark_left >= 40.0 {
        let spark = Rect::from_min_max(pos2(spark_left, rect.top() + 32.0), pos2(rect.right() - 14.0, rect.top() + 64.0));
        series_spark(&painter, spark, series, if alert { color } else { OK });
    }
    if alert {
        painter.rect_filled(Rect::from_min_size(rect.min + vec2(1.0, 1.0), vec2(3.0, size.y - 2.0)), CornerRadius::ZERO, color);
    }
    response
}

/// Sparkline over the series' UTC window; gaps stay gaps.
fn series_spark(painter: &Painter, rect: Rect, series: &TimedSeries, color: Color32) {
    let segments = chart_segments(series);
    let values: Vec<f64> = segments.iter().flatten().map(|(_, value)| *value).collect();
    if values.len() < 2 || series.to_ms <= series.from_ms {
        return;
    }
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low) * 0.12).max(high.abs() * 0.01).max(1e-6);
    let (low, high) = (low - pad, high + pad);
    if !(high - low).is_finite() {
        return;
    }
    let span = (series.to_ms - series.from_ms) as f64;
    let x = |time: i64| rect.left() + ((time - series.from_ms) as f64 / span) as f32 * rect.width();
    let y = |value: f64| rect.bottom() - ((value - low) / (high - low)) as f32 * rect.height();
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke(1.0, alpha(OK, 0.25)));
    for segment in &segments {
        let points: Vec<Pos2> = segment.iter().map(|(time, value)| pos2(x(*time), y(*value))).collect();
        if points.len() > 1 {
            painter.add(Shape::line(points, stroke(1.3, alpha(color, 0.9))));
        } else if let Some(point) = points.first() {
            // isolated sample between gaps: a dot, never a connecting line
            painter.rect_filled(Rect::from_center_size(*point, vec2(2.5, 2.5)), CornerRadius::ZERO, alpha(color, 0.9));
        }
    }
    if let Some((time, value)) = segments.last().and_then(|segment| segment.last()) {
        painter.rect_filled(Rect::from_center_size(pos2(x(*time), y(*value)), vec2(5.0, 5.0)), CornerRadius::ZERO, color);
    }
}

/// A typed value. Russian input may use a decimal comma, as the window shows one.
fn parse_value(text: &str, l: Lang) -> Result<f64, &'static str> {
    let text = text.trim();
    let normalized = if l == Lang::Ru { text.replacen(',', ".", 1) } else { text.to_string() };
    let value: f64 = normalized.parse().map_err(|_| l.t("Enter a number using a decimal point.", "Введите число, например 72,5."))?;
    if !value.is_finite() {
        return Err(l.t("Enter a finite number.", "Введите конечное число."));
    }
    Ok(value)
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

/// UTC telemetry plot in the prototype's chart style: deep background, mono scale, day ticks,
/// square sample markers and a hover read-out.
fn telemetry_chart(ui: &mut Ui, l: Lang, series: &TimedSeries, unit: &str, height: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, DEEP);
    let segments = chart_segments(series);
    let values: Vec<_> = segments.iter().flatten().map(|(_, value)| *value).collect();
    if values.is_empty() || series.to_ms <= series.from_ms {
        painter.text(rect.center(), Align2::CENTER_CENTER, l.t("NO SAMPLES IN THIS INTERVAL", "НЕТ ЗАМЕРОВ ЗА ЭТОТ ПЕРИОД"), mono(10.5), MUT);
    } else {
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        // The API accepts every finite positive numeric measurement. Normalize before
        // subtracting so even very large accepted values cannot overflow the plot.
        let scale = low.abs().max(high.abs()).max(1.0);
        let (low, high) = (low / scale, high / scale);
        let pad = ((high - low) * 0.12).max(0.005);
        let (low, high) = (low - pad, high + pad);
        let plot = Rect::from_min_max(pos2(rect.left() + 52.0, rect.top() + 10.0), pos2(rect.right() - 12.0, rect.bottom() - 20.0));
        let x = |time: i64| plot.left() + ((time - series.from_ms) as f64 / (series.to_ms - series.from_ms) as f64) as f32 * plot.width();
        let normalized_y = |value: f64| plot.bottom() - ((value - low) / (high - low)) as f32 * plot.height();
        let y = |value: f64| normalized_y(value / scale);
        let small = mono(9.5);
        for (i, value) in [low, (low + high) / 2.0, high].into_iter().enumerate() {
            let line_y = normalized_y(value);
            if i > 0 {
                painter.line_segment([pos2(plot.left(), line_y), pos2(plot.right(), line_y)], stroke(1.0, alpha(LINE, 0.7)));
            }
            painter.line_segment([pos2(plot.left() - 4.0, line_y), pos2(plot.left(), line_y)], stroke(1.0, LINE_HI));
            let raw = value * scale;
            let label = if !raw.is_finite() {
                "-".into()
            } else if raw.abs() >= 1e6 {
                format!("{raw:.1e}")
            } else {
                l.decimal(format!("{raw:.1}"))
            };
            painter.text(pos2(plot.left() - 7.0, line_y), Align2::RIGHT_CENTER, label, small.clone(), MUT);
        }
        // UTC midnight ticks along the bottom, labelled when there is room
        let days = (series.to_ms - series.from_ms) / DAY_MS;
        let labelled = days > 0 && plot.width() / days as f32 >= 46.0;
        let mut midnight = series.from_ms.div_euclid(DAY_MS) * DAY_MS;
        if midnight < series.from_ms {
            midnight += DAY_MS;
        }
        while midnight <= series.to_ms {
            let tick_x = x(midnight);
            painter.line_segment([pos2(tick_x, plot.bottom()), pos2(tick_x, plot.bottom() + 3.0)], stroke(1.0, LINE_HI));
            if labelled && tick_x > plot.left() + 20.0 && tick_x < plot.right() - 20.0 {
                painter.text(pos2(tick_x, rect.bottom() - 8.0), Align2::CENTER_CENTER, l.day(midnight), small.clone(), MUT);
            }
            midnight += DAY_MS;
        }
        let trace = Color32::from_rgb(0xc9, 0xd6, 0xe6);
        for segment in &segments {
            let points: Vec<_> = segment.iter().map(|(time, value)| pos2(x(*time), y(*value))).collect();
            if points.len() > 1 {
                painter.add(Shape::line(points.clone(), stroke(1.5, trace)));
            }
            if points.len() <= 60 {
                for point in points {
                    painter.rect_filled(Rect::from_center_size(point, vec2(4.0, 4.0)), CornerRadius::ZERO, trace);
                }
            }
        }
        if let Some((time, value)) = segments.last().and_then(|segment| segment.last()) {
            painter.rect_filled(Rect::from_center_size(pos2(x(*time), y(*value)), vec2(6.0, 6.0)), CornerRadius::ZERO, ACCENT);
        }
        if let Some(pointer) = response.hover_pos() {
            if let Some((time, value)) = segments.iter().flatten().min_by(|a, b| (x(a.0) - pointer.x).abs().total_cmp(&(x(b.0) - pointer.x).abs())) {
                painter.line_segment([pos2(x(*time), plot.top()), pos2(x(*time), plot.bottom())], stroke(1.0, MUT));
                response.on_hover_text(format!("{}\n{} {}", l.date_time(*time), l.num(*value), unit));
            }
        }
    }
    if !series.note.is_empty() {
        ui.label(RichText::new(l.message(&series.note)).size(10.5).color(MUT));
    }
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
                TimePoint { at_ms: 1_000, value: Some(70.0) },
                TimePoint { at_ms: 2_000, value: Some(72.0) },
                TimePoint { at_ms: 3_000, value: None },
                TimePoint { at_ms: 4_000, value: Some(71.0) },
            ],
            note: String::new(),
        };
        assert_eq!(chart_segments(&series), vec![vec![(1_000, 70.0), (2_000, 72.0)], vec![(4_000, 71.0)]]);
        assert_eq!(Lang::En.date_time(0), "01-01 00:00 UTC");
        assert_eq!(Lang::Ru.date_time(0), "01.01 00:00 UTC");
    }

    #[test]
    fn manual_input_preserves_fractional_values_and_rejects_nonfinite() {
        assert_eq!(parse_value(" 72.25 ", Lang::En), Ok(72.25));
        for invalid in ["", "NaN", "inf", "1,5", "pulse"] {
            assert!(parse_value(invalid, Lang::En).is_err());
        }
        // Russian shows a decimal comma, so it accepts one; everything else is still rejected.
        assert_eq!(parse_value("72,25", Lang::Ru), Ok(72.25));
        assert_eq!(parse_value("72.25", Lang::Ru), Ok(72.25));
        for invalid in ["", "NaN", "inf", "1,5,5", "пульс"] {
            assert!(parse_value(invalid, Lang::Ru).is_err());
        }
    }

    #[test]
    fn quality_word_is_the_first_detail_part() {
        let detail = "stale; source: simulator; measured: 2026-11-15T02:10:00Z; data quality is separate from health guidance";
        assert_eq!(quality_word(detail), "stale");
        assert_eq!(quality_word("Offline / cached; fresh; source: manual-01"), "fresh");
        assert_eq!(quality_word("No data loaded"), "No data loaded");
    }
}
