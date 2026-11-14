// Windows release build: a window program, no console (otherwise Windows Terminal opens an extra tab)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod figure;
mod theme;
mod widgets;

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Ui};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::*;
use ui_model::*;
use widgets::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Main,
    Comms,
    System,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    D7,
    D14,
    All,
}
#[derive(Clone, PartialEq)]
enum Nav {
    Overview,
    System(String),
    Metric(String),
}
enum Pvt {
    Idle,
    Wait { until: f64 },
    Go { start: f64 },
    Early,
    Done { mean: f64 },
}

#[derive(Default)]
struct Opts {
    lite: bool,
    shot: Option<String>,
    tab: Option<String>,
    system: Option<String>,
    metric: Option<String>,
    env: Option<String>,
    role: Option<String>,
    crew: Option<String>,
    mode: Option<String>,
    blackout: bool,
    checkin: bool,
    pvt: bool,
    storm: bool,
    size: Option<(f32, f32)>,
    pos: Option<(f32, f32)>,
}

struct GuiApp<B: Backend> {
    b: B,
    systems: Vec<SystemInfo>,
    inds: Vec<IndicatorInfo>,
    role: Role,
    nav: Nav,
    tab: Tab,
    range: Range,
    inputs: HashMap<String, f64>,
    toast: Option<(String, Instant)>,
    last_tick: Instant,
    opts: Opts,
    frames: u32,
    stat_t: Instant,
    stat_n: u32,
    causes: HashMap<String, u32>,
    fig: figure::Figure,
    mode: figure::Mode,
    compose: String,
    show_all: bool,
    checkin_open: bool,
    pvt_open: bool,
    pvt: Pvt,
    pvt_trials: Vec<f64>,
    rng: u64,
}

// ------------------------------------------------------------------------------ small helpers
fn age_text(s: u64) -> String {
    if s < 60 {
        format!("{} s ago", s)
    } else if s < 3600 {
        format!("{} min ago", s / 60)
    } else if s < 86_400 {
        format!("{} h {} min ago", s / 3600, (s % 3600) / 60)
    } else {
        format!("{} d {} h ago", s / 86_400, (s % 86_400) / 3600)
    }
}

fn link_label(ui: &mut Ui, text: &str, active: bool) -> bool {
    let r = ui.add(egui::Label::new(RichText::new(text).size(12.5).color(if active { ACCENT } else { TXT }).strong()).sense(Sense::click()));
    r.clicked()
}

fn short_name(full: &str) -> String {
    full.split(':').next().unwrap_or(full).to_string()
}

fn readiness(score: Option<u8>) -> (&'static str, Color32) {
    match score {
        Some(s) if s >= 90 => ("Optimal", ACCENT),
        Some(s) if s >= 75 => ("Good", ACCENT),
        Some(s) if s >= 55 => ("Fair", WARN),
        Some(_) => ("Attention", CRIT),
        None => ("No data", NONE),
    }
}

// ------------------------------------------------------------------------------ app
impl<B: Backend> GuiApp<B> {
    fn new(b: B, opts: Opts) -> Self {
        let systems = b.systems();
        let inds: Vec<IndicatorInfo> = systems.iter().flat_map(|s| b.indicators(&s.id)).collect();
        let mut app = GuiApp {
            b,
            systems,
            inds,
            role: Role::Crew,
            nav: Nav::Overview,
            tab: Tab::Main,
            range: Range::D14,
            inputs: HashMap::new(),
            toast: None,
            last_tick: Instant::now(),
            opts,
            frames: 0,
            stat_t: Instant::now(),
            stat_n: 0,
            causes: HashMap::new(),
            fig: figure::Figure::new(),
            mode: figure::Mode::Organs,
            compose: String::new(),
            show_all: false,
            checkin_open: false,
            pvt_open: false,
            pvt: Pvt::Idle,
            pvt_trials: vec![],
            rng: 0x2545_F491_4F6C_DD1D,
        };
        if let Some(e) = app.opts.env.clone() {
            app.b.set_environment(&e);
        }
        if app.opts.blackout {
            app.b.set_blackout(true);
        }
        if app.opts.storm {
            app.b.demo_solar_storm();
        }
        if app.opts.role.as_deref() == Some("ground") {
            app.role = Role::Ground;
        }
        if let Some(c) = app.opts.crew.clone() {
            app.b.select_crew(&c);
        }
        app.tab = match app.opts.tab.as_deref() {
            Some("comms") => Tab::Comms,
            Some("system") | Some("health") => Tab::System,
            _ => Tab::Main,
        };
        app.mode = match app.opts.mode.as_deref() {
            Some("skeleton") => figure::Mode::Skeleton,
            Some("xray") => figure::Mode::XRay,
            _ => figure::Mode::Organs,
        };
        if let Some(s) = app.opts.system.clone() {
            app.nav = Nav::System(s);
        }
        if let Some(m) = app.opts.metric.clone() {
            app.nav = Nav::Metric(m);
        }
        app.checkin_open = app.opts.checkin;
        app.pvt_open = app.opts.pvt;
        app.reset_inputs();
        app
    }

    fn ind(&self, id: &str) -> Option<IndicatorInfo> {
        self.inds.iter().find(|i| i.id == id).cloned()
    }
    fn sys_name(&self, id: &str) -> String {
        self.systems.iter().find(|s| s.id == id).map(|s| short_name(&s.name)).unwrap_or_default()
    }
    fn selected_sys(&self) -> String {
        match &self.nav {
            Nav::Overview => String::new(),
            Nav::System(s) => s.clone(),
            Nav::Metric(i) => self.ind(i).map(|x| x.system).unwrap_or_default(),
        }
    }
    fn go_system(&mut self, id: &str) {
        self.nav = Nav::System(id.to_string());
        self.tab = Tab::Main;
    }
    fn go_metric(&mut self, id: &str) {
        self.nav = Nav::Metric(id.to_string());
        self.tab = Tab::Main;
    }
    fn reset_inputs(&mut self) {
        self.inputs.clear();
        for i in &self.inds {
            let t = self.b.thresholds(&i.id);
            let v = self.b.latest(&i.id).map(|r| r.value).unwrap_or((t.ok.0 + t.ok.1) / 2.0);
            self.inputs.insert(i.id.clone(), v);
        }
    }
    fn say(&mut self, m: impl Into<String>) {
        self.toast = Some((m.into(), Instant::now()));
    }
    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
    fn sys_score(&self, sys: &str) -> Option<u32> {
        let v: Vec<u32> = self
            .inds
            .iter()
            .filter(|i| i.system == sys)
            .filter_map(|i| match self.b.indicator_status(&i.id) {
                Status::Normal => Some(100),
                Status::Watch => Some(62),
                Status::Act => Some(25),
                Status::NoData => None,
            })
            .collect();
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<u32>() / v.len() as u32)
        }
    }
    /// Indicators outside the normal range, most severe first.
    fn issues(&self) -> Vec<(IndicatorInfo, Status, f64)> {
        let mut v: Vec<(IndicatorInfo, Status, f64)> = self
            .inds
            .iter()
            .filter_map(|i| {
                let st = self.b.indicator_status(&i.id);
                (st >= Status::Watch).then(|| (i.clone(), st, self.b.latest(&i.id).map(|r| r.value).unwrap_or(0.0)))
            })
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(clinical_rank(&a.0.id).cmp(&clinical_rank(&b.0.id))));
        v
    }
    fn crew_name(&self) -> String {
        let id = self.b.selected_crew();
        self.b.crew().into_iter().find(|c| c.id == id).map(|c| c.name).unwrap_or_else(|| "Crew member".into())
    }

    // ------------------------------------------------------------------ chrome
    fn top_bar(&mut self, ctx: &egui::Context) {
        let o = self.b.overview();
        egui::TopBottomPanel::top("top")
            .exact_height(48.0)
            .frame(Frame::new().fill(SURFACE).stroke(stroke(1.0, LINE)).inner_margin(Margin::symmetric(14, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("ASTROCHECK").font(display(15.0)).strong().color(TXT));
                    ui.add_space(4.0);
                    ui.separator();
                    let envs = self.b.environments();
                    let mut chosen: Option<String> = None;
                    egui::ComboBox::from_id_salt("env").width(240.0).selected_text(RichText::new(&o.env_name).size(13.0)).show_ui(ui, |ui| {
                        for e in &envs {
                            if ui.selectable_label(e.id == o.env_id, &e.name).clicked() {
                                chosen = Some(e.id.clone());
                            }
                        }
                    });
                    if let Some(id) = chosen {
                        self.b.set_environment(&id);
                        self.reset_inputs();
                    }
                    ui.separator();
                    ui.label(RichText::new(format!("MISSION DAY {}", o.mission_day)).size(12.0).strong());
                    ui.separator();
                    let (col, txt) = if !o.link_up {
                        (CRIT, "LINK DOWN".to_string())
                    } else if o.autonomous {
                        (WARN, format!("AUTONOMOUS  RTT {}", rtt_text(o.rtt_s)))
                    } else {
                        (OK, format!("LINK OK  RTT {}", rtt_text(o.rtt_s)))
                    };
                    led(ui, col);
                    ui.label(RichText::new(txt).size(12.0).color(col).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.role == Role::Crew && ui.add(primary("Daily check-in")).clicked() {
                            self.checkin_open = true;
                        }
                        ui.menu_button("Demo", |ui| {
                            if ui.button("Next mission day").clicked() {
                                self.b.demo_next_day();
                                self.reset_inputs();
                                ui.close_menu();
                            }
                            if ui.button("Simulate solar storm").clicked() {
                                self.b.demo_solar_storm();
                                self.reset_inputs();
                                self.go_metric("dose");
                                ui.close_menu();
                            }
                            if ui.button("Inject data fault").clicked() {
                                let m = self.b.demo_inject_fault();
                                self.say(m);
                                ui.close_menu();
                            }
                            if ui.button("Reset demo data").clicked() {
                                self.b.demo_reset();
                                self.reset_inputs();
                                self.nav = Nav::Overview;
                                ui.close_menu();
                            }
                        });
                        let mut role = self.role;
                        if segmented(ui, &mut role, &[(Role::Ground, "Ground"), (Role::Crew, "Crew")]) {
                            self.role = role;
                            self.tab = Tab::Main;
                            self.nav = Nav::Overview;
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let h = self.b.health();
        let (q, _) = self.b.queue(0);
        let o = self.b.overview();
        let okc = h.checks.iter().filter(|c| c.ok).count();
        let crc_ok = h.checks.iter().find(|c| c.name.contains("CRC")).map_or(true, |c| c.ok);
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(Frame::new().fill(DEEP).stroke(stroke(1.0, LINE)).inner_margin(Margin::symmetric(14, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let mono = |t: String, c: Color32| RichText::new(t).monospace().size(11.0).color(c);
                    ui.label(mono("DB".into(), MUT));
                    ui.label(mono("OK".into(), OK));
                    ui.label(mono("  CRC".into(), MUT));
                    ui.label(mono(if crc_ok { "OK".into() } else { "REPAIR".into() }, if crc_ok { OK } else { CRIT }));
                    ui.label(mono("  BIT".into(), MUT));
                    ui.label(mono(format!("{}/{}", okc, h.checks.len()), if okc == h.checks.len() { OK } else { CRIT }));
                    ui.label(mono(format!("  QUEUE {}q {}f {}a", q.queued, q.in_flight, q.acked), MUT));
                    ui.label(mono(format!("  DUE {}/{}", o.due_count, o.total_indicators), MUT));
                    ui.label(mono(format!("  {}", h.platform), MUT));
                    if let Some((m, _)) = &self.toast {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(m.clone()).size(12.0).color(ACCENT).strong());
                        });
                    }
                });
            });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        let animate = !self.opts.lite;
        egui::SidePanel::left("left")
            .exact_width(372.0)
            .resizable(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.role == Role::Ground {
                        ui.label(RichText::new(self.crew_name()).size(15.0).strong());
                    } else {
                        caption(ui, "Anatomy");
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut m = self.mode;
                        if segmented(ui, &mut m, &[(figure::Mode::XRay, "X-ray"), (figure::Mode::Skeleton, "Skeleton"), (figure::Mode::Organs, "Organs")]) {
                            self.mode = m;
                        }
                    });
                });
                ui.add_space(4.0);
                let sel = self.selected_sys();
                let h = (ui.available_height() - 4.0).max(300.0);
                let clicked = self.fig.show(ui, vec2(ui.available_width(), h), &|s| self.b.system_status(s), &sel, self.mode, time, animate);
                if let Some(c) = clicked {
                    self.go_system(&c);
                }
            });
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::same(14))).show(ctx, |ui| {
            self.alert_strip(ui);
            ui.horizontal(|ui| {
                let (a, b) = if self.role == Role::Crew { ("Health", "Comms & autonomy") } else { ("Crew status", "Downlink") };
                for (t, name) in [(Tab::Main, a), (Tab::Comms, b), (Tab::System, "System health")] {
                    if tab_btn(ui, name, self.tab == t) {
                        self.tab = t;
                    }
                }
            });
            let r = ui.min_rect();
            ui.painter().hline(r.left()..=ui.max_rect().right(), r.bottom() + 1.0, stroke(1.0, LINE));
            ui.add_space(10.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::Main => {
                    self.crumbs(ui);
                    match self.nav.clone() {
                        Nav::Overview => {
                            if self.role == Role::Crew {
                                self.crew_overview(ui)
                            } else {
                                self.ground_console(ui)
                            }
                        }
                        Nav::System(s) => self.system_view(ui, &s),
                        Nav::Metric(i) => self.metric_view(ui, &i),
                    }
                }
                Tab::Comms => self.comms_tab(ui),
                Tab::System => self.system_tab(ui),
            });
        });
    }

    fn alert_strip(&mut self, ui: &mut Ui) {
        let issues = self.issues();
        let acts: Vec<&(IndicatorInfo, Status, f64)> = issues.iter().filter(|i| i.1 == Status::Act).collect();
        let Some(first) = acts.first() else { return };
        let o = self.b.overview();
        let mut open: Option<String> = None;
        Frame::new().fill(alpha(CRIT, 0.10)).stroke(stroke(1.0, alpha(CRIT, 0.55))).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                led(ui, CRIT);
                ui.label(RichText::new("ACTION NEEDED").size(12.0).strong().color(CRIT));
                ui.label(RichText::new(format!("{}  {} {}", first.0.name, fmt_num(first.2), first.0.unit)).size(13.0).strong());
                if acts.len() > 1 {
                    ui.label(RichText::new(format!("+{} more", acts.len() - 1)).size(12.0).color(MUT));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Review").clicked() {
                        open = Some(first.0.id.clone());
                    }
                });
            });
            let msg = if self.role == Role::Ground {
                format!("{}: acknowledge the finding and send guidance to the crew. Light delay one way: about {}.", self.crew_name(), rtt_text(o.rtt_s / 2.0))
            } else if !o.link_up {
                "Link blackout: you are autonomous. Follow the onboard medical protocol. The alert is queued and will be sent automatically when the link returns.".to_string()
            } else {
                self.b.evaluate(&first.0.id, first.2).advice.escalate.unwrap_or_default()
            };
            ui.label(RichText::new(msg).size(12.0).color(Color32::from_rgb(0xf3, 0xc6, 0xc8)));
        });
        ui.add_space(8.0);
        if let Some(id) = open {
            self.go_metric(&id);
        }
    }

    fn crumbs(&mut self, ui: &mut Ui) {
        let root = if self.role == Role::Crew { "Overview".to_string() } else { format!("Crew status  \u{203A}  {}", self.crew_name()) };
        let mut to: Option<Nav> = None;
        ui.horizontal(|ui| {
            if link_label(ui, &root, self.nav == Nav::Overview) {
                to = Some(Nav::Overview);
            }
            let sep = |ui: &mut Ui| ui.label(RichText::new("\u{203A}").color(MUT));
            match &self.nav {
                Nav::Overview => {}
                Nav::System(s) => {
                    sep(ui);
                    ui.label(RichText::new(self.sys_name(s)).size(12.5).color(MUT));
                }
                Nav::Metric(i) => {
                    if let Some(ind) = self.ind(i) {
                        sep(ui);
                        if link_label(ui, &self.sys_name(&ind.system), false) {
                            to = Some(Nav::System(ind.system.clone()));
                        }
                        sep(ui);
                        ui.label(RichText::new(&ind.name).size(12.5).color(MUT));
                    }
                }
            }
        });
        ui.add_space(6.0);
        if let Some(n) = to {
            self.nav = n;
        }
    }

    // ------------------------------------------------------------------ crew: overview
    fn crew_overview(&mut self, ui: &mut Ui) {
        let o = self.b.overview();
        let env = self.b.environment();
        let gap = 10.0;
        let cw = (ui.available_width() - 2.0 * gap) / 3.0 - 1.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // readiness
            vcard(ui, cw, 150.0, |ui| {
                caption(ui, "Readiness index");
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(108.0, 108.0), Sense::hover());
                    let (label, col) = readiness(o.score);
                    ring(ui.painter(), rect.center(), 46.0, 9.0, o.score.unwrap_or(0) as f32 / 100.0, col);
                    ui.painter().text(rect.center() + vec2(0.0, -4.0), Align2::CENTER_CENTER, o.score.map(|s| s.to_string()).unwrap_or("-".into()), mono(30.0), TXT);
                    ui.painter().text(rect.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, label.to_uppercase(), mono(9.0), col);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Relative to personal baseline").size(12.0).color(MUT));
                        ui.label(RichText::new(format!("{} of {} indicators in range", self.inds.len() - self.issues().len(), self.inds.len())).size(12.5));
                    });
                });
            });
            // daily tasks: plain list, no rings
            vcard(ui, cw, 150.0, |ui| {
                caption(ui, "Daily tasks");
                let val = |id: &str| self.b.latest(id).map(|r| r.value).unwrap_or(0.0);
                let (ex, sl) = (val("exercise"), val("sleep"));
                let done = o.total_indicators - o.due_count;
                for (k, v, ok) in [
                    ("Check-in", format!("{} / {}", done, o.total_indicators), o.due_count == 0),
                    ("Exercise", format!("{} / 120 min", fmt_num(ex)), ex >= 120.0),
                    ("Sleep", format!("{} / 8 h", fmt_num(sl)), sl >= 7.0),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([86.0, 18.0], egui::Label::new(RichText::new(k).size(12.0).color(MUT)));
                        ui.label(RichText::new(v).font(mono(12.5)).color(if ok { OK } else { TXT }));
                    });
                }
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{} due now", o.due_count)).size(11.5).color(MUT));
            });
            // mission
            vcard(ui, cw, 150.0, |ui| {
                caption(ui, "Environment");
                ui.add(egui::Label::new(RichText::new(env.name.split(" (").next().unwrap_or(&env.name)).size(14.0).strong()).truncate());
                for (k, v) in [("Gravity", env.gravity.clone()), ("Comms", env.comms.clone()), ("Radiation", format!("{} mSv/day typical", fmt_num(env.typical_dose)))] {
                    ui.add(egui::Label::new(RichText::new(format!("{}   {}", k, v)).size(11.5).color(MUT)).truncate());
                }
                ui.horizontal(|ui| pill(ui, if o.autonomous { "Autonomous" } else { "Ground-assisted" }, if o.autonomous { WARN } else { OK }));
            });
        });
        ui.add_space(10.0);

        // contributors + insights
        let mut go: Option<Nav> = None;
        let issues = self.issues();
        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| {
                ui.set_width(ui.available_width());
                caption(ui, "Body systems");
                ui.add_space(4.0);
                for s in &self.systems {
                    let st = self.b.system_status(&s.id);
                    let score = self.sys_score(&s.id);
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    let p = ui.painter();
                    if resp.hovered() {
                        p.rect_filled(rect, CornerRadius::same(6), SURFACE2);
                    }
                    p.text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, short_name(&s.name), FontId::proportional(12.5), TXT);
                    let bar = Rect::from_min_size(pos2(rect.left() + 170.0, rect.center().y - 3.5), vec2(rect.width() - 170.0 - 52.0, 7.0));
                    p.rect_filled(bar, CornerRadius::same(4), DEEP);
                    let f = score.unwrap_or(0) as f32 / 100.0;
                    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, bar.height())), CornerRadius::same(4), status_color(st));
                    p.text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, score.map(|v| v.to_string()).unwrap_or("-".into()), FontId::proportional(13.0), status_color(st));
                    if resp.clicked() {
                        go = Some(Nav::System(s.id.clone()));
                    }
                }
            });
            card().show(&mut cols[1], |ui| {
                ui.set_width(ui.available_width());
                caption(ui, "Findings");
                ui.add_space(4.0);
                if issues.is_empty() {
                    ui.label(RichText::new("No findings. All indicators are within range.").size(12.5).color(MUT));
                }
                for (ind, st, v) in issues.iter().take(5) {
                    let ev = self.b.evaluate(&ind.id, *v);
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                    let p = ui.painter_at(rect);
                    if resp.hovered() {
                        p.rect_filled(rect, CornerRadius::same(6), SURFACE2);
                    }
                    p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, 46.0)), CornerRadius::same(2), status_color(*st));
                    p.text(rect.min + vec2(14.0, 14.0), Align2::LEFT_CENTER, format!("{}  {} {}", ind.name, fmt_num(*v), ind.unit), FontId::proportional(12.5), TXT);
                    p.text(rect.min + vec2(14.0, 31.0), Align2::LEFT_CENTER, &ev.advice.headline, FontId::proportional(11.0), status_color(*st));
                    if let Some(a) = ev.advice.actions.first() {
                        p.text(rect.min + vec2(14.0, 46.0), Align2::LEFT_CENTER, a, FontId::proportional(10.5), MUT);
                    }
                    if resp.clicked() {
                        go = Some(Nav::Metric(ind.id.clone()));
                    }
                }
            });
        });
        ui.add_space(10.0);

        let msgs = self.b.messages();
        if !msgs.is_empty() {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                caption(ui, "Messages from flight surgeon");
                for m in msgs.iter().rev().take(3) {
                    ui.label(RichText::new(format!("Day {}   {}", m.day, m.text)).size(12.5));
                }
            });
            ui.add_space(10.0);
        }

        // quiet display: by default show only what needs attention (or is due); everything else is one click away
        let attention: Vec<String> = self.inds.iter().filter(|i| self.b.indicator_status(&i.id) >= Status::Watch).map(|i| i.id.clone()).collect();
        ui.horizontal(|ui| {
            caption(ui, if self.show_all { "All metrics" } else { "Needs attention" });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button(if self.show_all { "Show only attention" } else { "Show all metrics" }).clicked() {
                    self.show_all = !self.show_all;
                }
            });
        });
        ui.add_space(4.0);
        let ids: Vec<String> = if self.show_all || attention.is_empty() { self.inds.iter().map(|i| i.id.clone()).collect() } else { attention };
        self.tiles(ui, &ids);
        if let Some(n) = go {
            self.nav = n;
        }
    }

    fn tiles(&mut self, ui: &mut Ui, ids: &[String]) {
        let gap = 8.0;
        let cols = if ui.available_width() >= 880.0 { 4 } else if ui.available_width() >= 620.0 { 3 } else { 2 };
        let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32 - 1.0;
        let mut clicked: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            for id in ids {
                let Some(ind) = self.ind(id) else { continue };
                let hist = self.b.history(id);
                if tile(ui, vec2(w, 100.0), &ind, &hist, self.b.thresholds(id), self.b.indicator_status(id), self.b.is_due(id), false).clicked() {
                    clicked = Some(id.clone());
                }
            }
        });
        if let Some(c) = clicked {
            self.go_metric(&c);
        }
    }

    // ------------------------------------------------------------------ system + metric views
    fn system_view(&mut self, ui: &mut Ui, sys_id: &str) {
        let Some(sys) = self.systems.iter().find(|s| s.id == sys_id).cloned() else { return };
        let st = self.b.system_status(&sys.id);
        let env = self.b.environment();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&sys.name).font(display(22.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| pill(ui, st.name(), status_color(st)));
        });
        ui.add_space(4.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row = |ui: &mut Ui, k: &str, v: &str, c: Color32| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(k).size(12.5).strong());
                    ui.label(RichText::new(v).size(12.5).color(c));
                });
            };
            row(ui, "What it does:", &sys.function, MUT);
            row(ui, "Spaceflight hazards:", &sys.hazards.join("   |   "), Color32::from_rgb(0xb8, 0xa4, 0xf0));
            row(ui, "How spaceflight affects it:", &sys.effect, MUT);
            row(ui, "Why check:", &sys.why, MUT);
            if let Some(n) = env.notes.get(&sys.id) {
                ui.add_space(4.0);
                Frame::new().fill(alpha(ACCENT, 0.10)).stroke(stroke(1.0, alpha(ACCENT, 0.45))).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("{}: {}", env.name, n)).size(12.0));
                });
            }
        });
        ui.add_space(10.0);
        caption(ui, "Metrics");
        ui.add_space(4.0);
        let ids: Vec<String> = self.inds.iter().filter(|i| i.system == sys.id).map(|i| i.id.clone()).collect();
        self.tiles(ui, &ids);
    }

    fn metric_view(&mut self, ui: &mut Ui, id: &str) {
        let Some(ind) = self.ind(id) else { return };
        let hist = self.b.history(id);
        let th = self.b.thresholds(id);
        let day = self.b.overview().mission_day;
        let latest = self.b.latest(id);
        let mut v = *self.inputs.get(id).unwrap_or(&0.0);
        let crew = self.role == Role::Crew;
        let shown = if crew { v } else { latest.map(|r| r.value).unwrap_or(0.0) };
        let ev = self.b.evaluate(id, shown);
        let st = self.b.indicator_status(id);
        let col = status_color(if crew { ev.status } else { st });

        ui.horizontal(|ui| {
            ui.label(RichText::new(&ind.name).font(display(22.0)));
            pill(ui, if crew { ev.status.name() } else { st.name() }, col);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut r = self.range;
                if segmented(ui, &mut r, &[(Range::All, "All"), (Range::D14, "14D"), (Range::D7, "7D")]) {
                    self.range = r;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(latest.map(|r| fmt_num(r.value)).unwrap_or("-".into())).font(mono(40.0)));
            ui.label(RichText::new(&ind.unit).font(mono(14.0)).color(MUT));
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("target {}", ev.target)).size(12.0).color(MUT));
                if let (Some(l), Some(b)) = (latest, baseline(&hist)) {
                    let d = l.value - b;
                    ui.label(RichText::new(format!("{}{} vs your baseline {}", if d >= 0.0 { "+" } else { "" }, fmt_num((d * 100.0).round() / 100.0), fmt_num((b * 100.0).round() / 100.0))).size(12.0).color(if st == Status::Normal { MUT } else { status_color(st) }));
                }
            });
        });
        ui.add_space(4.0);
        let window = match self.range {
            Range::D7 => Some(7),
            Range::D14 => Some(14),
            Range::All => None,
        };
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            chart(ui, &hist, th, ind.min, ind.max, day, 230.0, window, &ind.unit);
        });
        ui.add_space(8.0);
        // statistics for the selected range
        let d0 = match window {
            Some(n) => day.saturating_sub(n - 1).max(1),
            None => 1,
        };
        let vals: Vec<f64> = hist.iter().filter(|r| r.day >= d0).map(|r| r.value).collect();
        if !vals.is_empty() {
            let (mn, mx) = (vals.iter().cloned().fold(f64::MAX, f64::min), vals.iter().cloned().fold(f64::MIN, f64::max));
            let avg = vals.iter().sum::<f64>() / vals.len() as f64;
            let stats = [
                ("Latest", latest.map(|r| fmt_num(r.value)).unwrap_or("-".into())),
                ("Baseline", baseline(&hist).map(|b| fmt_num((b * 100.0).round() / 100.0)).unwrap_or("-".into())),
                ("Average", fmt_num((avg * 100.0).round() / 100.0)),
                ("Min", fmt_num(mn)),
                ("Max", fmt_num(mx)),
            ];
            let gap = 8.0;
            let w = (ui.available_width() - gap * 4.0) / 5.0 - 1.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (k, val) in stats {
                    vcard(ui, w, 0.0, |ui| {
                        caption(ui, k);
                        ui.label(RichText::new(format!("{} {}", val, ind.unit)).font(mono(14.0)));
                    });
                }
            });
            ui.add_space(8.0);
        }

        // input (crew) or acknowledge (ground)
        let mut log_clicked = false;
        let mut ack: Option<u64> = None;
        let mut advise = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if crew {
                caption(ui, "Log a new measurement");
                ui.horizontal(|ui| {
                    match ind.kind {
                        Kind::Scale => {
                            ui.add(egui::Slider::new(&mut v, ind.min..=ind.max).step_by(ind.step).integer());
                        }
                        Kind::Num => {
                            if ui.button("-").clicked() {
                                v = (v - ind.step).max(ind.min);
                            }
                            ui.add(egui::DragValue::new(&mut v).range(ind.min..=ind.max).speed(ind.step).suffix(format!(" {}", ind.unit)).max_decimals(2));
                            if ui.button("+").clicked() {
                                v = (v + ind.step).min(ind.max);
                            }
                        }
                    }
                    if ui.add(primary("Log today")).clicked() {
                        log_clicked = true;
                    }
                    if id == "pvt" && ui.button("Run test").clicked() {
                        self.pvt_open = true;
                    }
                });
                ui.add_space(4.0);
            } else {
                caption(ui, "Flight surgeon actions");
                ui.horizontal(|ui| {
                    if let Some(a) = self.b.alerts().into_iter().find(|a| a.indicator == id) {
                        if ui.add_enabled(!a.acked, egui::Button::new(if a.acked { "Acknowledged" } else { "Acknowledge finding" })).clicked() {
                            ack = Some(a.id);
                        }
                    } else {
                        ui.label(RichText::new("No open finding for this metric.").size(12.0).color(MUT));
                    }
                    if ui.add(primary("Send guidance to crew")).clicked() {
                        advise = true;
                    }
                });
                ui.add_space(4.0);
            }
            let a = &ev.advice;
            let r = ui.horizontal_top(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&a.headline).size(13.0).strong().color(col));
                    for x in &a.actions {
                        ui.label(RichText::new(format!("\u{2022} {}", x)).size(12.5));
                    }
                    if let Some(e) = &a.escalate {
                        ui.label(RichText::new(e).size(12.5).strong().color(CRIT));
                    }
                });
            });
            let rr = r.response.rect;
            ui.painter().rect_filled(Rect::from_min_size(rr.min, vec2(3.0, rr.height())), CornerRadius::same(1), col);
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, "About this measurement");
            ui.label(RichText::new(format!("Why: {}", ind.why)).size(12.5).color(MUT));
            ui.add_space(2.0);
            ui.label(RichText::new("How to measure").size(12.0).strong());
            for (n, h) in ind.how.iter().enumerate() {
                ui.label(RichText::new(format!("{}. {}", n + 1, h)).size(12.5).color(MUT));
            }
        });
        self.inputs.insert(id.to_string(), v);
        if log_clicked {
            let r = self.b.log(id, v);
            self.inputs.insert(id.to_string(), r.value);
            self.say(format!("{}: {} {} - {}{}", ind.name, fmt_num(r.value), ind.unit, r.status.name(), String::new()));
        }
        if let Some(a) = ack {
            self.b.ack_alert(a);
            self.say("Finding acknowledged");
        }
        if advise {
            self.compose = format!("Regarding {}: ", ind.name);
            self.nav = Nav::Overview;
        }
    }

    // ------------------------------------------------------------------ ground console
    fn ground_console(&mut self, ui: &mut Ui) {
        let crew = self.b.crew();
        let sel = self.b.selected_crew();
        let o = self.b.overview();
        if crew.is_empty() {
            ui.label(RichText::new("This backend reports a single crew member.").color(MUT));
        }
        // roster
        let gap = 8.0;
        let n = crew.len().max(1) as f32;
        let w = (ui.available_width() - gap * (n - 1.0)) / n - 1.0;
        let mut pick: Option<String> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for c in &crew {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 92.0), Sense::click());
                let p = ui.painter_at(rect);
                let selected = c.id == sel;
                p.rect_filled(rect, CornerRadius::same(10), if resp.hovered() { SURFACE2 } else { SURFACE });
                p.rect_stroke(rect, CornerRadius::same(10), stroke(if selected { 1.6 } else { 1.0 }, if selected { ACCENT } else { LINE }), egui::StrokeKind::Inside);
                let col = status_color(c.status);
                p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(3.0, 68.0)), CornerRadius::same(2), col);
                p.text(rect.min + vec2(14.0, 18.0), Align2::LEFT_CENTER, &c.name, FontId::proportional(14.0), TXT);
                p.text(rect.min + vec2(14.0, 36.0), Align2::LEFT_CENTER, &c.title, FontId::proportional(11.0), MUT);
                let stale = c.last_contact_s > 3600;
                let ct = if stale { format!("STALE  {}", age_text(c.last_contact_s)) } else { format!("contact {}", age_text(c.last_contact_s)) };
                p.text(rect.min + vec2(14.0, 56.0), Align2::LEFT_CENTER, ct, FontId::proportional(10.5), if stale { WARN } else { MUT });
                p.text(rect.min + vec2(14.0, 74.0), Align2::LEFT_CENTER, if c.open_alerts == 0 { "no open findings".to_string() } else { format!("{} open finding{}", c.open_alerts, if c.open_alerts == 1 { "" } else { "s" }) }, FontId::proportional(10.5), if c.open_alerts == 0 { MUT } else { col });
                let rc = pos2(rect.right() - 32.0, rect.top() + 34.0);
                ring(&p, rc, 20.0, 5.0, c.score.unwrap_or(0) as f32 / 100.0, col);
                p.text(rc, Align2::CENTER_CENTER, c.score.map(|s| s.to_string()).unwrap_or("-".into()), mono(13.0), TXT);
                if resp.clicked() {
                    pick = Some(c.id.clone());
                }
            }
        });
        if let Some(id) = pick {
            self.b.select_crew(&id);
            self.reset_inputs();
        }
        ui.add_space(10.0);

        let alerts = self.b.alerts();
        let mut ack: Option<u64> = None;
        let mut open: Option<String> = None;
        ui.columns(2, |cols| {
            // triage
            card().show(&mut cols[0], |ui| {
                ui.set_width(ui.available_width());
                caption(ui, "Alert triage");
                ui.add_space(4.0);
                if alerts.is_empty() {
                    ui.label(RichText::new("No open findings for this crew member.").size(12.5).color(MUT));
                }
                for a in &alerts {
                    let c = status_color(a.status);
                    ui.horizontal(|ui| {
                        led(ui, c);
                        if ui.add(egui::Label::new(RichText::new(&a.indicator_name).size(12.5).strong()).sense(Sense::click())).clicked() {
                            open = Some(a.indicator.clone());
                        }
                        ui.label(RichText::new(format!("{} {}", fmt_num(a.value), a.unit)).size(12.5).color(c));
                        ui.label(RichText::new(format!("day {}", a.day)).size(11.0).color(MUT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(!a.acked, egui::Button::new(if a.acked { "ACK'D" } else { "Acknowledge" })).clicked() {
                                ack = Some(a.id);
                            }
                        });
                    });
                }
            });
            // clinical table
            card().show(&mut cols[1], |ui| {
                ui.set_width(ui.available_width());
                caption(ui, "Clinical summary");
                ui.add_space(4.0);
                egui::Grid::new("clinical").num_columns(5).spacing([14.0, 7.0]).striped(true).show(ui, |ui| {
                    for h in ["Indicator", "Latest", "Baseline", "Delta", "Status"] {
                        ui.label(RichText::new(h.to_uppercase()).size(9.5).color(MUT).strong());
                    }
                    ui.end_row();
                    for i in &self.inds {
                        let st = self.b.indicator_status(&i.id);
                        let c = status_color(st);
                        let hist = self.b.history(&i.id);
                        if ui.add(egui::Label::new(RichText::new(&i.name).size(12.0)).sense(Sense::click())).clicked() {
                            open = Some(i.id.clone());
                        }
                        let latest = hist.last().map(|r| r.value);
                        ui.label(RichText::new(latest.map(fmt_num).unwrap_or("-".into())).font(mono(12.0)));
                        let b = baseline(&hist);
                        ui.label(RichText::new(b.map(|v| fmt_num((v * 100.0).round() / 100.0)).unwrap_or("-".into())).font(mono(12.0)).color(MUT));
                        let d = latest.zip(b).map(|(l, b)| l - b);
                        ui.label(RichText::new(d.map(|d| format!("{}{}", if d >= 0.0 { "+" } else { "" }, fmt_num((d * 100.0).round() / 100.0))).unwrap_or("-".into())).font(mono(12.0)).color(if st == Status::Normal { MUT } else { c }));
                        ui.label(RichText::new(st.name().to_uppercase()).size(10.0).color(c).strong());
                        ui.end_row();
                    }
                });
            });
        });
        if let Some(a) = ack {
            self.b.ack_alert(a);
        }
        if let Some(i) = open {
            self.go_metric(&i);
        }
        ui.add_space(10.0);

        // composer
        let name = self.crew_name();
        let msgs = self.b.messages();
        let mut compose = std::mem::take(&mut self.compose);
        let mut send = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, &format!("Message to {}", name));
            ui.horizontal_wrapped(|ui| {
                for t in ["Repeat the measurement in 1 hour.", "Check cabin CO2 and ventilation.", "Rest, hydrate, and re-test later today.", "Schedule a private medical conference."] {
                    if ui.small_button(t).clicked() {
                        compose = t.to_string();
                    }
                }
            });
            ui.add(egui::TextEdit::multiline(&mut compose).desired_rows(2).desired_width(f32::INFINITY).hint_text("Type guidance for the crew member"));
            ui.horizontal(|ui| {
                if ui.add_enabled(!compose.trim().is_empty(), primary("Send (uplink)")).clicked() {
                    send = true;
                }
                ui.label(RichText::new(format!("Estimated delivery: about {} (one-way light time).", rtt_text(o.rtt_s / 2.0))).size(11.5).color(MUT));
            });
            for m in msgs.iter().rev().take(4) {
                ui.label(RichText::new(format!("Day {}   {}", m.day, m.text)).size(12.0).color(MUT));
            }
        });
        if send {
            self.b.send_message(compose.trim());
            compose.clear();
        }
        self.compose = compose;
    }

    // ------------------------------------------------------------------ comms / system tabs
    fn comms_tab(&mut self, ui: &mut Ui) {
        let o = self.b.overview();
        let (s, rows) = self.b.queue(12);
        let env = self.b.environment();
        ui.horizontal(|ui| {
            pill(ui, if o.autonomous { "Autonomous: onboard protocol is primary" } else { "Ground-assisted" }, if o.autonomous { WARN } else { OK });
            pill(ui, if o.link_up { "Link up" } else { "Link down" }, if o.link_up { OK } else { CRIT });
            pill(ui, &format!("RTT {}", rtt_text(o.rtt_s)), ACCENT);
        });
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Comms at this location: {}. Urgent data goes first; nothing is lost during a blackout or a restart.", env.comms)).size(12.0).color(MUT));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (n, v, c) in [("Queued", s.queued, WARN), ("In flight", s.in_flight, ACCENT), ("Delivered", s.acked, OK)] {
                vcard(ui, 140.0, 0.0, |ui| {
                    ui.label(RichText::new(v.to_string()).size(26.0).strong().color(c));
                    caption(ui, n);
                });
            }
        });
        ui.add_space(10.0);
        caption(ui, "Downlink queue");
        ui.add_space(4.0);
        for r in rows {
            let (pc, pt) = match r.priority {
                Priority::Urgent => (CRIT, "URGENT"),
                Priority::Watch => (WARN, "watch"),
                Priority::Routine => (OK, "routine"),
            };
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(6), SURFACE);
            ui.painter().rect_stroke(rect, CornerRadius::same(6), stroke(1.0, LINE), egui::StrokeKind::Inside);
            ui.painter().text(pos2(rect.left() + 12.0, rect.center().y), Align2::LEFT_CENTER, pt.to_uppercase(), FontId::proportional(10.5), pc);
            ui.painter().text(pos2(rect.left() + 92.0, rect.center().y), Align2::LEFT_CENTER, &r.label, FontId::proportional(12.5), TXT);
            let (sw, sc) = match r.state {
                QState::Queued => ("queued", WARN),
                QState::InFlight => ("in flight", ACCENT),
                QState::Acked => ("ack from ground", MUT),
            };
            ui.painter().text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, sw, FontId::proportional(11.5), sc);
            ui.add_space(2.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(if o.link_up { "Simulate blackout (solar conjunction)" } else { "Restore link" }).clicked() {
                let on = o.link_up;
                self.b.set_blackout(on);
                self.say(if on { "Blackout: AstroCheck continues autonomously" } else { "Link restored: queue is being sent" });
            }
            if ui.button("Export downlink bundle (JSON)").clicked() {
                match self.b.export() {
                    Ok(p) => self.say(format!("Exported: {}", p)),
                    Err(e) => self.say(format!("Export failed: {}", e)),
                }
            }
        });
    }

    fn system_tab(&mut self, ui: &mut Ui) {
        let h = self.b.health();
        let ok = h.checks.iter().filter(|c| c.ok).count();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Self-test {}/{}", ok, h.checks.len())).size(22.0).strong().color(if ok == h.checks.len() { OK } else { CRIT }));
            ui.label(RichText::new(format!("platform {}", h.platform)).size(12.0).color(MUT));
        });
        ui.add_space(6.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            for c in &h.checks {
                ui.horizontal(|ui| {
                    led(ui, if c.ok { OK } else { CRIT });
                    ui.label(RichText::new(&c.name).size(13.0));
                    ui.label(RichText::new(format!("- {}", c.detail)).size(12.0).color(MUT));
                });
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Run self-test").clicked() {
                self.b.run_selftest();
                self.say("Self-test finished");
            }
            if ui.button("Inject data fault (demo)").clicked() {
                let m = self.b.demo_inject_fault();
                self.say(m);
            }
        });
        ui.add_space(8.0);
        caption(ui, "Event log");
        ui.add_space(4.0);
        Frame::new().fill(DEEP).stroke(stroke(1.0, LINE)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for e in h.events.iter().take(14) {
                ui.label(RichText::new(e).monospace().size(11.0).color(MUT));
            }
        });
    }

    // ------------------------------------------------------------------ modals
    fn modals(&mut self, ctx: &egui::Context) {
        if self.checkin_open {
            let mut open = true;
            let mut save = false;
            egui::Window::new("Daily check-in").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).default_width(640.0).show(ctx, |ui| {
                ui.label(RichText::new("Set each value to your real measurement, then save.").size(12.0).color(MUT));
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    let due: Vec<IndicatorInfo> = self.inds.iter().filter(|i| self.b.is_due(&i.id)).cloned().collect();
                    if due.is_empty() {
                        ui.label("Nothing due right now.");
                    }
                    for i in due {
                        let mut v = *self.inputs.get(&i.id).unwrap_or(&0.0);
                        let ev = self.b.evaluate(&i.id, v);
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(vec2(210.0, 22.0), Layout::left_to_right(Align::Center), |ui| {
                                ui.label(RichText::new(&i.name).size(13.0));
                            });
                            match i.kind {
                                Kind::Scale => {
                                    ui.add(egui::Slider::new(&mut v, i.min..=i.max).step_by(i.step).integer());
                                }
                                Kind::Num => {
                                    ui.add(egui::DragValue::new(&mut v).range(i.min..=i.max).speed(i.step).suffix(format!(" {}", i.unit)));
                                }
                            }
                            pill(ui, ev.status.name(), status_color(ev.status));
                        });
                        self.inputs.insert(i.id, v);
                    }
                });
                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(primary("Save check-in")).clicked() {
                        save = true;
                    }
                });
            });
            if save {
                let due: Vec<String> = self.inds.iter().filter(|i| self.b.is_due(&i.id)).map(|i| i.id.clone()).collect();
                let n = due.len();
                for id in due {
                    let v = *self.inputs.get(&id).unwrap_or(&0.0);
                    self.b.log(&id, v);
                }
                self.say(format!("{} measurements saved", n));
                open = false;
            }
            self.checkin_open = open;
        }
        if self.pvt_open {
            self.pvt_window(ctx);
        }
    }

    fn pvt_window(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Pvt::Wait { until } = self.pvt {
            if now >= until {
                self.pvt = Pvt::Go { start: now };
            }
        }
        let mut open = true;
        let mut clicked = false;
        let mut log_mean: Option<f64> = None;
        egui::Window::new("Reaction-time test (PVT)").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(RichText::new("Press the box as soon as it turns green. 5 trials.").size(12.5).color(MUT));
            let (txt, bg) = match &self.pvt {
                Pvt::Idle => (if self.pvt_trials.is_empty() { "Press to start".to_string() } else { format!("{} ms  -  press for next trial", self.pvt_trials.last().unwrap().round()) }, Color32::from_rgb(0x1b, 0x2a, 0x5e)),
                Pvt::Wait { .. } => (format!("Trial {}/5  -  wait for green...", self.pvt_trials.len() + 1), Color32::from_rgb(0x5b, 0x22, 0x30)),
                Pvt::Go { .. } => ("RESPOND".to_string(), Color32::from_rgb(0x1f, 0x9d, 0x68)),
                Pvt::Early => ("Early response - press to repeat this trial".to_string(), Color32::from_rgb(0x1b, 0x2a, 0x5e)),
                Pvt::Done { mean } => (format!("Average {} ms", mean.round()), Color32::from_rgb(0x1b, 0x2a, 0x5e)),
            };
            let (rect, resp) = ui.allocate_exact_size(vec2(480.0, 170.0), Sense::click());
            ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, txt, FontId::proportional(if matches!(self.pvt, Pvt::Go { .. }) { 34.0 } else { 18.0 }), Color32::WHITE);
            if resp.clicked() {
                clicked = true;
            }
            if !self.pvt_trials.is_empty() {
                ui.label(RichText::new(format!("Reaction times: {} ms", self.pvt_trials.iter().map(|t| format!("{}", t.round())).collect::<Vec<_>>().join(", "))).size(12.0).color(MUT));
            }
            if let Pvt::Done { mean } = self.pvt {
                let ev = self.b.evaluate("pvt", mean);
                pill(ui, ev.status.name(), status_color(ev.status));
                if ui.add(primary("Log result")).clicked() {
                    log_mean = Some(mean);
                }
            }
        });
        if clicked {
            match self.pvt {
                Pvt::Idle | Pvt::Early => {
                    let wait = 1.2 + self.rand() * 2.8;
                    self.pvt = Pvt::Wait { until: now + wait };
                }
                Pvt::Wait { .. } => self.pvt = Pvt::Early,
                Pvt::Go { start } => {
                    self.pvt_trials.push((now - start) * 1000.0);
                    self.pvt = if self.pvt_trials.len() >= 5 { Pvt::Done { mean: self.pvt_trials.iter().sum::<f64>() / 5.0 } } else { Pvt::Idle };
                }
                Pvt::Done { .. } => {}
            }
        }
        if let Some(m) = log_mean {
            let r = self.b.log("pvt", m);
            self.inputs.insert("pvt".into(), r.value);
            self.say(format!("Reaction time logged: {} ms", fmt_num(r.value)));
            open = false;
        }
        if matches!(self.pvt, Pvt::Wait { .. } | Pvt::Go { .. }) {
            ctx.request_repaint();
        }
        if !open {
            self.pvt_open = false;
            self.pvt = Pvt::Idle;
            self.pvt_trials.clear();
        }
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.opts.shot.clone() else { return };
        ctx.request_repaint();
        if self.frames == 14 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let img = ctx.input(|i| i.events.iter().find_map(|e| match e { egui::Event::Screenshot { image, .. } => Some(image.clone()), _ => None }));
        if let Some(img) = img {
            let [w, h] = img.size;
            match image::save_buffer(&path, img.as_raw(), w as u32, h as u32, image::ColorType::Rgba8) {
                Ok(_) => println!("screenshot saved: {} ({}x{})", path, w, h),
                Err(e) => eprintln!("screenshot failed: {}", e),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.frames > 240 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl<B: Backend> eframe::App for GuiApp<B> {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        self.frames += 1;
        if std::env::var_os("ASTROCHECK_STATS").is_some() {
            for c in ctx.repaint_causes() {
                *self.causes.entry(c.to_string()).or_insert(0) += 1;
            }
            if self.stat_t.elapsed() >= Duration::from_secs(5) {
                println!("update() calls in last 5s: {}", self.frames - self.stat_n);
                let mut v: Vec<_> = self.causes.drain().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1));
                for (c, n) in v.iter().take(6) {
                    println!("   {:>4} x {}", n, c);
                }
                self.stat_t = Instant::now();
                self.stat_n = self.frames;
            }
        }
        if self.last_tick.elapsed() >= Duration::from_secs(1) {
            self.b.tick();
            for m in self.b.take_messages() {
                self.say(m);
            }
            self.last_tick = Instant::now();
            if let Some((_, t)) = &self.toast {
                if t.elapsed() > Duration::from_secs(4) {
                    self.toast = None;
                }
            }
        }
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.left_panel(ctx);
        self.central(ctx);
        self.modals(ctx);
        // Repaint budget: 10 fps while focused and animating, otherwise once a second.
        let focused = ctx.input(|i| i.focused);
        ctx.request_repaint_after(Duration::from_millis(if !self.opts.lite && focused { 100 } else { 1000 }));
        self.screenshot(ctx);
    }
}

fn main() -> eframe::Result {
    let mut o = Opts::default();
    let mut a = std::env::args().skip(1);
    while let Some(x) = a.next() {
        match x.as_str() {
            "--lite" => o.lite = true,
            "--screenshot" => o.shot = a.next(),
            "--tab" => o.tab = a.next(),
            "--system" => o.system = a.next(),
            "--metric" => o.metric = a.next(),
            "--env" => o.env = a.next(),
            "--role" => o.role = a.next(),
            "--crew" => o.crew = a.next(),
            "--mode" => o.mode = a.next(),
            "--blackout" => o.blackout = true,
            "--checkin" => o.checkin = true,
            "--pvt" => o.pvt = true,
            "--storm" => o.storm = true,
            "--size" => {
                if let Some(s) = a.next() {
                    let p: Vec<f32> = s.split('x').filter_map(|v| v.parse().ok()).collect();
                    if p.len() == 2 {
                        o.size = Some((p[0], p[1]));
                    }
                }
            }
            "--pos" => {
                if let Some(s) = a.next() {
                    let p: Vec<f32> = s.split(',').filter_map(|v| v.parse().ok()).collect();
                    if p.len() == 2 {
                        o.pos = Some((p[0], p[1]));
                    }
                }
            }
            "--help" | "-h" => {
                println!("astrocheck-gui [--lite] [--role crew|ground] [--pos X,Y] [--env iss|orion|moon|mars|transit] [--size WxH]\n  visual checks: --screenshot FILE.png --tab main|comms|system --system ID --metric ID --crew ID --mode organs|skeleton|xray --blackout --storm --checkin --pvt");
                return Ok(());
            }
            _ => {}
        }
    }
    let (w, h) = o.size.unwrap_or((1360.0, 800.0));
    let title = if o.role.as_deref() == Some("ground") { "AstroCheck - Ground" } else { "AstroCheck - Crew" };
    let mut vb = egui::ViewportBuilder::default().with_inner_size([w, h]).with_min_inner_size([1000.0, 600.0]).with_title(title);
    if let Some((x, y)) = o.pos {
        vb = vb.with_position([x, y]);
    }
    let options = eframe::NativeOptions { viewport: vb, ..Default::default() };
    eframe::run_native(
        "AstroCheck",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            // Swap DemoBackend for the real backend: anything that implements ui_model::Backend.
            Ok(Box::new(GuiApp::new(DemoBackend::new(), o)))
        }),
    )
}
