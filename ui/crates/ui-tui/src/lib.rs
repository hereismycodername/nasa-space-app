//! Terminal interface for crew and ground. Uses only the 16 ANSI colours, so it looks right on a
//! Linux console, over SSH and over a serial line. Redraws only when something changed.

mod braille;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    widgets::{Block, Cell, HighlightSpacing, List, ListItem, ListState, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap},
};
use std::collections::HashSet;
use std::time::{Duration, Instant};
// explicit imports: ratatui's prelude also exports a `Backend` trait
use ui_model::{fmt_num, Backend, EventKind, IndicatorInfo, Priority, QState, Role, Status, SystemInfo};

const TEMPLATES: [&str; 4] = [
    "Repeat the measurement in 1 hour.",
    "Check cabin CO2 and ventilation.",
    "Rest, hydrate, and re-test later today.",
    "Schedule a private medical conference.",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Systems,
    Indicators,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Main,
    Comms,
    Health,
    Help,
}
enum Node {
    Crew(String),
    Sys(String),
    Ind(String),
}
struct TRow {
    node: Node,
    indent: usize,
    label: String,
    right: String,
    st: Status,
    open: Option<bool>,
    watched: bool,
}
struct Edit {
    ind: String,
    text: String,
}

pub struct App<B: Backend> {
    pub b: B,
    systems: Vec<SystemInfo>,
    role: Role,
    sys: usize,
    ind: usize,
    alert_idx: usize,
    pane: Pane,
    view: View,
    edit: Option<Edit>,
    compose: Option<String>,
    tpl: usize,
    checkin: Vec<String>,
    toast: Option<(String, Instant)>,
    scroll: u16,
    tree_cur: usize,
    watch: Vec<String>,
    watch_touched: bool,
    watch_crew: String,
    flipped: HashSet<String>,
    pub quit: bool,
    pub dirty: bool,
}

fn sc(s: Status) -> Color {
    match s {
        // quiet display: normal is neutral, colour is reserved for departures from normal
        Status::Normal => Color::Gray,
        Status::Watch => Color::Yellow,
        Status::Act => Color::Red,
        Status::NoData => Color::Cyan,
    }
}
fn word(s: Status) -> &'static str {
    match s {
        Status::Normal => "NORMAL",
        Status::Watch => "WATCH",
        Status::Act => "ACT NOW",
        Status::NoData => "NO DATA",
    }
}
fn rtt(s: f64) -> String {
    if s < 60.0 {
        format!("{} s", fmt_num(s))
    } else {
        format!("{} min", (s / 60.0).round())
    }
}
fn baseline(v: &[ui_model::Reading]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let n = v.len().min(7);
    Some(v[..n].iter().map(|r| r.value).sum::<f64>() / n as f64)
}
fn spark(vals: &[f64]) -> String {
    const G: [char; 8] = ['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];
    let (mn, mx) = vals.iter().fold((f64::MAX, f64::MIN), |a, v| (a.0.min(*v), a.1.max(*v)));
    vals.iter().map(|v| if mx - mn < 1e-9 { G[3] } else { G[(((v - mn) / (mx - mn)) * 7.0).round() as usize] }).collect()
}
/// Short names that fit a narrow list (full names are shown in the detail panel).
fn short(id: &str, full: &str) -> String {
    match id {
        "brain" => "Brain & Mind",
        "eyes" => "Eyes & Vision",
        "heart" => "Heart",
        "lungs" => "Lungs",
        "gut" => "Metabolism",
        "msk" => "Bones & Muscles",
        "immune" => "Whole body",
        _ => return trunc(full, 15),
    }
    .to_string()
}
fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{}~", t)
    }
}

/// ASCII body sketch for consoles without braille glyphs (ASTROCHECK_ASCII=1 or TERM=linux).
const FIGURE: [&str; 20] = [
    ".-----.",
    "/BBBBBBB\\",
    "| BBBBBBB |",
    "|  E   E  |",
    "\\_______/",
    ".---|     |---.",
    "/MMM |LLLHLLL| MMM\\",
    "|MMM |LLLHLLL| MMM|",
    "|MM  |LLLLLLL|  MM|",
    "|MM  |LLLLLLL|  MM|",
    "|MM  | GGGGG |  MM|",
    "|MM  |GGGGGGG|  MM|",
    "|MM  |GGGGGGG|  MM|",
    "'.MM  \\_____/  MM.'",
    "|MM|   |MM|",
    "|MM|   |MM|",
    "|MM|   |MM|",
    "|MM|   |MM|",
    "|MM|   |MM|",
    "'--'   '--'",
];
fn figure_cell(c: char) -> (Option<&'static str>, char) {
    match c {
        'B' => (Some("brain"), '\u{2592}'),
        'E' => (Some("eyes"), 'o'),
        'L' => (Some("lungs"), '\u{2593}'),
        'H' => (Some("heart"), '\u{2665}'),
        'G' => (Some("gut"), '\u{2591}'),
        'M' => (Some("msk"), '\u{2588}'),
        ' ' => (None, ' '),
        o => (Some("immune"), o),
    }
}

impl<B: Backend> App<B> {
    pub fn new(b: B) -> Self {
        let systems = b.systems();
        let mut app = App { b, systems, role: Role::Crew, sys: 2, ind: 0, alert_idx: 0, pane: Pane::Systems, view: View::Main, edit: None, compose: None, tpl: 0, checkin: vec![], toast: None, scroll: 0, tree_cur: 0, watch: vec![], watch_touched: false, watch_crew: String::new(), flipped: HashSet::new(), quit: false, dirty: true };
        app.sync_watch();
        app
    }
    fn say(&mut self, m: impl Into<String>) {
        self.toast = Some((m.into(), Instant::now()));
        self.dirty = true;
    }
    fn cur_sys(&self) -> &SystemInfo {
        &self.systems[self.sys]
    }
    fn cur_inds(&self) -> Vec<IndicatorInfo> {
        self.b.indicators(&self.cur_sys().id)
    }
    fn all_inds(&self) -> Vec<IndicatorInfo> {
        self.systems.iter().flat_map(|s| self.b.indicators(&s.id)).collect()
    }
    /// Flattened object tree: crew > body systems > indicators (only the selected crew member is expanded).
    fn tree_rows(&self) -> Vec<TRow> {
        let sel = self.b.selected_crew();
        let link_up = self.b.overview().link_up;
        let mut v = vec![];
        for c in self.b.crew() {
            let selected = c.id == sel;
            let score = c.score.map(|s| s.to_string()).unwrap_or("-".into());
            // without a link every crew member's data is stale right away
            let right = if !link_up || c.last_contact_s > 3600 { format!("STALE {}", score) } else { score };
            v.push(TRow { node: Node::Crew(c.id.clone()), indent: 0, label: c.name.clone(), right, st: c.status, open: Some(selected), watched: false });
            if !selected {
                continue;
            }
            for s in &self.systems {
                let st = self.b.system_status(&s.id);
                let key = format!("sys:{}", s.id);
                let open = (st >= Status::Watch) != self.flipped.contains(&key);
                v.push(TRow { node: Node::Sys(s.id.clone()), indent: 2, label: short(&s.id, &s.name), right: String::new(), st, open: Some(open), watched: false });
                if !open {
                    continue;
                }
                for i in self.b.indicators(&s.id) {
                    let ist = self.b.indicator_status(&i.id);
                    let val = self.b.latest(&i.id).map(|r| format!("{} {}", fmt_num(r.value), i.unit)).unwrap_or("-".into());
                    let watched = self.watch.contains(&i.id);
                    v.push(TRow { node: Node::Ind(i.id.clone()), indent: 4, label: i.name.clone(), right: val, st: ist, open: None, watched });
                }
            }
        }
        v
    }

    /// Default telemetry channels: the selected crew member's most severe findings.
    fn sync_watch(&mut self) {
        let sel = self.b.selected_crew();
        if self.watch_crew != sel {
            self.watch_crew = sel;
            self.watch_touched = false;
        }
        if !self.watch_touched {
            let mut v: Vec<String> = self.b.alerts().into_iter().map(|a| a.indicator).take(3).collect();
            if v.is_empty() {
                v = vec!["hr".into(), "spo2".into()];
            }
            self.watch = v;
        }
    }

    fn crew_name(&self) -> String {
        let id = self.b.selected_crew();
        self.b.crew().into_iter().find(|c| c.id == id).map(|c| c.name).unwrap_or_else(|| "crew member".into())
    }

    /// Call once per second.
    pub fn tick(&mut self) {
        self.sync_watch();
        if self.b.tick() {
            self.dirty = true;
        }
        for m in self.b.take_messages() {
            self.say(m);
        }
        if let Some((_, t)) = &self.toast {
            if t.elapsed() > Duration::from_secs(4) {
                self.toast = None;
                self.dirty = true;
            }
        }
    }

    // ------------------------------------------------------------------ keys
    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind == KeyEventKind::Release {
            return;
        }
        self.dirty = true;
        self.sync_watch();
        if self.compose.is_some() {
            return self.compose_key(k);
        }
        if self.edit.is_some() {
            return self.edit_key(k);
        }
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.view = if self.view == View::Help { View::Main } else { View::Help },
            KeyCode::Char('g') => self.view = if self.view == View::Comms { View::Main } else { View::Comms },
            KeyCode::Char('h') => self.view = if self.view == View::Health { View::Main } else { View::Health },
            KeyCode::Char('r') => {
                self.role = if self.role == Role::Crew { Role::Ground } else { Role::Crew };
                self.view = View::Main;
                self.pane = Pane::Systems;
                self.alert_idx = 0;
                self.say(if self.role == Role::Ground { "Ground console" } else { "Crew view" });
            }
            KeyCode::Char('m') if self.role == Role::Ground => {
                self.compose = Some(String::new());
            }
            KeyCode::Char('m') => self.view = View::Main,
            KeyCode::Char(c @ ('[' | ']')) if self.role == Role::Ground && self.view == View::Main => {
                let crew = self.b.crew();
                if !crew.is_empty() {
                    let cur = self.b.selected_crew();
                    let i = crew.iter().position(|c| c.id == cur).unwrap_or(0) as i32;
                    let n = (i + if c == ']' { 1 } else { -1 }).rem_euclid(crew.len() as i32) as usize;
                    self.b.select_crew(&crew[n].id);
                    self.alert_idx = 0;
                    self.tree_cur = 0;
                }
            }
            KeyCode::Char('a') if self.role == Role::Ground => {
                let events = self.b.events(50);
                match events.get(self.alert_idx).and_then(|e| e.alert_id.map(|id| (id, e.source.clone()))) {
                    Some((id, name)) => {
                        self.b.ack_alert(id);
                        self.say(format!("Acknowledged: {}", name));
                    }
                    None => self.say("Select a finding in the event log (Tab) to acknowledge it"),
                }
            }
            KeyCode::Char('b') => {
                let on = !self.b.blackout();
                self.b.set_blackout(on);
                self.say(if on { "Blackout: autonomous mode" } else { "Link restored: queue is being sent" });
            }
            KeyCode::Char('e') => {
                let envs = self.b.environments();
                let cur = self.b.environment().id;
                let i = envs.iter().position(|e| e.id == cur).unwrap_or(0);
                let next = envs[(i + 1) % envs.len()].clone();
                self.b.set_environment(&next.id);
                self.say(format!("Environment: {}", next.name));
            }
            KeyCode::Char('n') => self.b.demo_next_day(),
            KeyCode::Char('s') => {
                self.b.demo_solar_storm();
                self.say("Solar particle event simulated");
            }
            KeyCode::Char('f') => {
                let m = self.b.demo_inject_fault();
                self.say(m);
            }
            KeyCode::Char('t') => {
                self.b.run_selftest();
                self.view = View::Health;
            }
            KeyCode::Char('x') => match self.b.export() {
                Ok(p) => self.say(format!("Exported: {}", p)),
                Err(e) => self.say(format!("Export failed: {}", e)),
            },
            KeyCode::Char('c') if self.role == Role::Crew => {
                self.checkin = self.all_inds().into_iter().filter(|i| self.b.is_due(&i.id)).map(|i| i.id).rev().collect();
                if self.checkin.is_empty() {
                    self.say("Nothing due right now");
                } else {
                    self.next_checkin();
                }
            }
            KeyCode::Esc => {
                if self.view != View::Main {
                    self.view = View::Main
                } else if self.pane == Pane::Indicators {
                    self.pane = Pane::Systems
                }
            }
            _ if self.view != View::Main => {}
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Tab if self.role == Role::Ground => self.pane = if self.pane == Pane::Systems { Pane::Indicators } else { Pane::Systems },
            KeyCode::Tab | KeyCode::Right => self.pane = Pane::Indicators,
            KeyCode::Left => self.pane = Pane::Systems,
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(3),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(3),
            KeyCode::Enter => match (self.role, self.pane) {
                (Role::Ground, Pane::Systems) => self.tree_enter(),
                (_, Pane::Systems) => self.pane = Pane::Indicators,
                (Role::Crew, Pane::Indicators) => {
                    if let Some(i) = self.cur_inds().get(self.ind) {
                        self.begin_edit(&i.id.clone());
                    }
                }
                (Role::Ground, Pane::Indicators) => {
                    if let Some(i) = self.b.events(50).get(self.alert_idx).and_then(|e| e.indicator.clone()) {
                        self.watch_touched = true;
                        if !self.watch.contains(&i) {
                            self.watch.push(i);
                        }
                    }
                }
            },
            _ => {}
        }
    }

    fn move_sel(&mut self, d: i32) {
        self.scroll = 0;
        if self.role == Role::Ground {
            match self.pane {
                Pane::Systems => {
                    let n = self.tree_rows().len() as i32;
                    if n > 0 {
                        self.tree_cur = (self.tree_cur as i32 + d).rem_euclid(n) as usize;
                    }
                }
                Pane::Indicators => {
                    let n = self.b.events(50).len() as i32;
                    if n > 0 {
                        self.alert_idx = (self.alert_idx as i32 + d).rem_euclid(n) as usize;
                    }
                }
            }
            return;
        }
        match self.pane {
            Pane::Systems => {
                self.sys = (self.sys as i32 + d).rem_euclid(self.systems.len() as i32) as usize;
                self.ind = 0;
            }
            Pane::Indicators => {
                let n = self.cur_inds().len() as i32;
                self.ind = (self.ind as i32 + d).rem_euclid(n) as usize;
            }
        }
    }

    fn tree_enter(&mut self) {
        let rows = self.tree_rows();
        let Some(row) = rows.get(self.tree_cur.min(rows.len().saturating_sub(1))) else { return };
        match &row.node {
            Node::Crew(id) => {
                let id = id.clone();
                self.b.select_crew(&id);
                self.alert_idx = 0;
            }
            Node::Sys(id) => {
                let key = format!("sys:{}", id);
                if !self.flipped.remove(&key) {
                    self.flipped.insert(key);
                }
            }
            Node::Ind(id) => {
                let id = id.clone();
                self.watch_touched = true;
                if let Some(p) = self.watch.iter().position(|w| *w == id) {
                    self.watch.remove(p);
                } else {
                    self.watch.push(id);
                }
            }
        }
    }

    fn begin_edit(&mut self, id: &str) {
        for (si, s) in self.systems.iter().enumerate() {
            if let Some(ii) = self.b.indicators(&s.id).iter().position(|i| i.id == id) {
                self.sys = si;
                self.ind = ii;
                self.pane = Pane::Indicators;
                self.view = View::Main;
            }
        }
        let t = self.b.thresholds(id);
        let v = self.b.latest(id).map(|r| r.value).unwrap_or((t.ok.0 + t.ok.1) / 2.0);
        self.edit = Some(Edit { ind: id.to_string(), text: fmt_num(v) });
    }
    fn next_checkin(&mut self) {
        match self.checkin.pop() {
            Some(id) => self.begin_edit(&id),
            None => self.say("Check-in finished"),
        }
    }

    fn compose_key(&mut self, k: KeyEvent) {
        let mut text = self.compose.take().unwrap_or_default();
        match k.code {
            KeyCode::Char(c) => text.push(c),
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Tab => {
                text = TEMPLATES[self.tpl % TEMPLATES.len()].to_string();
                self.tpl += 1;
            }
            KeyCode::Esc => return,
            KeyCode::Enter => {
                let t = text.trim().to_string();
                if !t.is_empty() {
                    self.b.send_message(&t);
                    for m in self.b.take_messages() {
                        self.say(m);
                    }
                }
                return;
            }
            _ => {}
        }
        self.compose = Some(text);
    }

    fn edit_key(&mut self, k: KeyEvent) {
        let ind = self.edit.as_ref().unwrap().ind.clone();
        let info = self.all_inds().into_iter().find(|i| i.id == ind).unwrap();
        let e = self.edit.as_mut().unwrap();
        match k.code {
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' || c == '-' => e.text.push(c),
            KeyCode::Backspace => {
                e.text.pop();
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Char('+') | KeyCode::Char('=') => {
                let cur: f64 = e.text.replace(',', ".").parse().unwrap_or(0.0);
                let d = if k.code == KeyCode::Down { -info.step } else { info.step };
                e.text = fmt_num((cur + d).clamp(info.min, info.max));
            }
            KeyCode::Esc => {
                self.edit = None;
                self.checkin.clear();
            }
            KeyCode::Enter => {
                if let Ok(v) = e.text.replace(',', ".").parse::<f64>() {
                    self.edit = None;
                    let r = self.b.log(&ind, v);
                    self.say(format!("{}: {} {} - {}{}", info.name, fmt_num(r.value), info.unit, word(r.status), String::new()));
                    for m in self.b.take_messages() {
                        self.say(m);
                    }
                    if !self.checkin.is_empty() {
                        self.next_checkin();
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ drawing
    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        if area.width < 80 || area.height < 24 {
            f.render_widget(Paragraph::new(format!("Terminal too small ({}x{}).\nAstroCheck needs at least 80x24.", area.width, area.height)), area);
            return;
        }
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(10), Constraint::Length(1), Constraint::Length(1)]).split(area);
        self.header(f, rows[0]);
        match self.view {
            View::Main if self.role == Role::Ground => self.ground_view(f, rows[1]),
            View::Main => self.main_view(f, rows[1]),
            View::Comms => self.comms_view(f, rows[1]),
            View::Health => self.health_view(f, rows[1]),
            View::Help => self.help_view(f, rows[1]),
        }
        self.keys_line(f, rows[2]);
        self.status_line(f, rows[3]);
    }

    fn header(&self, f: &mut Frame, a: Rect) {
        let o = self.b.overview();
        let sep = Span::styled(" | ", Style::new().fg(Color::DarkGray));
        let link = if !o.link_up {
            Span::styled("LINK DOWN", Style::new().fg(Color::Red).bold())
        } else if o.autonomous {
            Span::styled(format!("AUTONOMOUS  RTT {}", rtt(o.rtt_s)), Style::new().fg(Color::Yellow).bold())
        } else {
            Span::styled(format!("LINK OK  RTT {}", rtt(o.rtt_s)), Style::new().fg(Color::Green))
        };
        let score = o.score.map(|s| s.to_string()).unwrap_or("-".into());
        let sc_col = match o.score {
            Some(s) if s >= 85 => Color::Green,
            Some(s) if s >= 65 => Color::Yellow,
            _ => Color::Red,
        };
        let ground = self.role == Role::Ground;
        let who = if ground { self.crew_name() } else { format!("Mission day {}", o.mission_day) };
        let line = Line::from(vec![
            Span::styled(" ASTROCHECK ", Style::new().fg(Color::Black).bg(Color::Cyan).bold()),
            Span::styled(if ground { " GROUND " } else { " CREW " }, Style::new().fg(Color::Black).bg(if ground { Color::Yellow } else { Color::Green }).bold()),
            Span::raw(format!(" Day {}", o.mission_day)),
            sep.clone(),
            Span::raw("Score "),
            Span::styled(score, Style::new().fg(sc_col).bold()),
            sep.clone(),
            link,
            sep.clone(),
            Span::raw(o.env_name.split(" (").next().unwrap_or(&o.env_name).to_string()),
            sep,
            Span::raw(who),
        ]);
        f.render_widget(Paragraph::new(line), a);
    }

    fn left_split(&self, a: Rect) -> (Rect, Rect, bool) {
        let lw = if a.width >= 100 { 32 } else { 29 };
        let cols = Layout::horizontal([Constraint::Length(lw), Constraint::Min(40)]).split(a);
        (cols[0], cols[1], cols[0].height >= 31)
    }

    fn main_view(&self, f: &mut Frame, a: Rect) {
        let (l, r, show_fig) = self.left_split(a);
        if show_fig {
            let parts = Layout::vertical([Constraint::Min(22), Constraint::Length(9)]).split(l);
            self.figure(f, parts[0], &self.cur_sys().id);
            self.sys_list(f, parts[1]);
        } else {
            self.sys_list(f, l);
        }
        let inds = self.cur_inds();
        let th = (inds.len() as u16 + 3).min(r.height.saturating_sub(6));
        let right = Layout::vertical([Constraint::Length(th), Constraint::Min(5)]).split(r);
        self.ind_table(f, right[0], &inds);
        self.detail(f, right[1], &inds);
    }

    fn ground_view(&self, f: &mut Frame, a: Rect) {
        let fig_w = if a.width >= 118 { 28 } else { 0 };
        let tree_w = if a.width >= 100 { 36 } else { 30 };
        let cols = Layout::horizontal([Constraint::Length(tree_w), Constraint::Min(40), Constraint::Length(fig_w)]).split(a);
        self.object_tree(f, cols[0]);
        let ev_h = (self.b.events(50).len() as u16 + 3).clamp(5, 11).min(cols[1].height.saturating_sub(8));
        let mid = Layout::vertical([Constraint::Min(6), Constraint::Length(ev_h)]).split(cols[1]);
        self.telemetry(f, mid[0]);
        self.event_log(f, mid[1]);
        if fig_w > 0 {
            self.figure(f, cols[2], "");
        }
    }

    fn object_tree(&self, f: &mut Frame, a: Rect) {
        let rows = self.tree_rows();
        let w = a.width.saturating_sub(6) as usize;
        let items: Vec<ListItem> = rows
            .iter()
            .map(|r| {
                let mark = match r.open {
                    Some(true) => "v ",
                    Some(false) => "> ",
                    None => "  ",
                };
                let left = format!("{}{}{}", " ".repeat(r.indent), mark, r.label);
                let rw = r.right.chars().count();
                let lw = w.saturating_sub(rw + 4);
                let name_style = if matches!(r.node, Node::Crew(_)) { Style::new().bold() } else { Style::new() };
                ListItem::new(Line::from(vec![
                    Span::styled(if r.watched { "\u{258E}" } else { " " }, Style::new().fg(Color::Cyan)),
                    Span::styled("\u{25CF} ", Style::new().fg(sc(r.st))),
                    Span::styled(format!("{:<width$}", trunc(&left, lw), width = lw), name_style),
                    Span::styled(r.right.clone(), Style::new().fg(Color::DarkGray)),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(format!(" OBJECTS  {}/{} ", self.tree_cur.min(rows.len().saturating_sub(1)) + 1, rows.len())).border_style(self.focus_style(Pane::Systems)))
            .highlight_style(self.cursor_style(Pane::Systems))
            .highlight_symbol(self.cursor_mark(Pane::Systems))
            .highlight_spacing(HighlightSpacing::Always);
        let cur = self.tree_cur.min(rows.len().saturating_sub(1));
        f.render_stateful_widget(list, a, &mut ListState::default().with_selected(Some(cur)));
        self.scroll(f, a, rows.len(), cur, 0);
    }

    /// Stacked telemetry: one header line and one trend line per watched channel, limit status per sample.
    fn telemetry(&self, f: &mut Frame, a: Rect) {
        let o = self.b.overview();
        let block = Block::bordered().title(format!(" TELEMETRY  {}  day {} ", self.crew_name(), o.mission_day)).border_style(Style::new().fg(Color::DarkGray));
        let inner = block.inner(a);
        f.render_widget(block, a);
        let mut lines: Vec<Line> = vec![];
        let per = 3usize;
        let max_ch = (inner.height as usize / per).max(1);
        if self.watch.is_empty() {
            lines.push(Line::from(Span::styled("No channels. Move to an indicator in OBJECTS and press Enter to plot it.", Style::new().fg(Color::DarkGray))));
        }
        let width = inner.width as usize;
        for id in self.watch.iter().take(max_ch) {
            let Some(ind) = self.all_inds().into_iter().find(|i| i.id == *id) else { continue };
            let hist = self.b.history(id);
            let st = self.b.indicator_status(id);
            let latest = hist.last().map(|r| format!("{} {}", fmt_num(r.value), ind.unit)).unwrap_or("-".into());
            let base = baseline(&hist).map(|v| fmt_num((v * 100.0).round() / 100.0)).unwrap_or("-".into());
            lines.push(Line::from(vec![
                Span::styled("\u{25CF} ", Style::new().fg(sc(st))),
                Span::styled(ind.name.clone(), Style::new().bold()),
                Span::raw("  "),
                Span::styled(latest, Style::new().fg(sc(st)).bold()),
                Span::raw("  "),
                Span::styled(if st == Status::Normal { String::new() } else { word(st).to_string() }, Style::new().fg(sc(st)).bold()),
                Span::styled(format!("   baseline {}", base), Style::new().fg(Color::DarkGray)),
            ]));
            let pts: Vec<(f64, Status)> = hist.iter().rev().take(width.max(1).min(40)).rev().map(|r| (r.value, self.b.evaluate(id, r.value).status)).collect();
            let vals: Vec<f64> = pts.iter().map(|p| p.0).collect();
            let sp: Vec<char> = spark(&vals).chars().collect();
            let rep_n = if pts.is_empty() { 1 } else { (width / pts.len()).max(1) };
            let mut spans: Vec<Span> = vec![];
            for (i, ch) in sp.iter().enumerate() {
                spans.push(Span::styled(ch.to_string().repeat(rep_n), Style::new().fg(sc(pts[i].1))));
            }
            lines.push(Line::from(spans));
            let (d0, d1) = (hist.first().map(|r| r.day).unwrap_or(0), hist.last().map(|r| r.day).unwrap_or(0));
            lines.push(Line::from(Span::styled(format!("day {:<w$}day {}", d0, d1, w = width.saturating_sub(10).max(1)), Style::new().fg(Color::DarkGray))));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn event_log(&self, f: &mut Frame, a: Rect) {
        let events = self.b.events(50);
        let rows: Vec<Row> = events
            .iter()
            .map(|e| {
                let (sev, col) = match e.kind {
                    EventKind::Finding => (word(e.severity).to_string(), sc(e.severity)),
                    EventKind::Uplink => ("UPLINK".to_string(), Color::Cyan),
                    EventKind::Crew => ("CREW".to_string(), Color::Cyan),
                };
                Row::new(vec![
                    Cell::from(Span::styled(format!("\u{25CF} {}", sev), Style::new().fg(col))),
                    Cell::from(trunc(&e.source, 24)),
                    Cell::from(e.text.clone()),
                    Cell::from(e.value.clone().unwrap_or_default()),
                    Cell::from(format!("day {}", e.day)),
                    Cell::from(match e.kind {
                        EventKind::Finding if e.acked => Span::styled("ACK", Style::new().fg(Color::DarkGray)),
                        EventKind::Finding => Span::styled("open", Style::new().fg(Color::Yellow)),
                        _ => Span::raw(""),
                    }),
                ])
            })
            .collect();
        let empty = events.is_empty();
        let table = Table::new(rows, [Constraint::Length(11), Constraint::Length(24), Constraint::Min(20), Constraint::Length(11), Constraint::Length(7), Constraint::Length(5)])
            .header(Row::new(vec!["SEVERITY", "SOURCE", "EVENT", "VALUE", "DAY", ""]).style(Style::new().fg(Color::DarkGray).bold()))
            .block(Block::bordered().title(" EVENT LOG   Tab = focus   a = acknowledge   m = message crew ").border_style(self.focus_style(Pane::Indicators)))
            .row_highlight_style(self.cursor_style(Pane::Indicators))
            .highlight_symbol(self.cursor_mark(Pane::Indicators))
            .highlight_spacing(HighlightSpacing::Always);
        let cur = if empty { 0 } else { self.alert_idx.min(events.len() - 1) };
        f.render_stateful_widget(table, a, &mut TableState::default().with_selected(if empty { None } else { Some(cur) }));
        self.scroll(f, a, events.len(), cur, 1);
    }

    fn focus_style(&self, p: Pane) -> Style {
        if self.pane == p { Style::new().fg(Color::Cyan).bold() } else { Style::new().fg(Color::DarkGray) }
    }

    /// The cursor row: a marker in the margin and a grey bar across the row (status colours stay readable).
    /// It is drawn in every pane, brighter in the pane that has the keyboard focus.
    fn cursor_style(&self, p: Pane) -> Style {
        if self.pane == p { Style::new().bg(Color::DarkGray).add_modifier(Modifier::BOLD) } else { Style::new().add_modifier(Modifier::UNDERLINED) }
    }
    fn cursor_mark(&self, p: Pane) -> &'static str {
        if self.pane == p { "\u{25B6} " } else { "\u{2022} " }
    }

    /// Scroll bar on the right border, only when the content does not fit.
    fn scroll(&self, f: &mut Frame, a: Rect, total: usize, pos: usize, header: u16) {
        let visible = a.height.saturating_sub(2 + header) as usize;
        if total > visible && a.height > 4 {
            let area = Rect { x: a.x, y: a.y + 1 + header, width: a.width, height: a.height - 2 - header };
            let mut st = ScrollbarState::new(total.saturating_sub(visible)).position(pos.saturating_sub(visible / 2).min(total.saturating_sub(visible)));
            f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None).thumb_symbol("\u{2588}").track_symbol(Some("\u{2502}")), area, &mut st);
        }
    }

    fn figure(&self, f: &mut Frame, a: Rect, selected: &str) {
        let block = Block::bordered().title(" BODY ").border_style(Style::new().fg(Color::DarkGray));
        let inner = block.inner(a);
        let lines: Vec<Line> = if braille::use_braille() {
            braille::render(inner.width as usize, inner.height as usize, &|s| self.b.system_status(s), selected)
                .into_iter()
                .map(|row| {
                    Line::from(
                        row.into_iter()
                            .map(|c| {
                                let mut st = Style::new().fg(c.color);
                                if c.bold {
                                    st = st.bold();
                                }
                                Span::styled(c.ch.to_string(), st)
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect()
        } else {
            FIGURE
                .iter()
                .map(|row| {
                    let padded = format!("{:^21}", row);
                    Line::from(
                        padded
                            .chars()
                            .map(|c| {
                                let (sys, g) = figure_cell(c);
                                match sys {
                                    None => Span::raw(" "),
                                    Some(s) => {
                                        let st = self.b.system_status(s);
                                        let mut style = Style::new().fg(sc(st));
                                        if s == selected {
                                            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                                        }
                                        if s == "immune" && s != selected {
                                            style = style.add_modifier(Modifier::DIM);
                                        }
                                        Span::styled(g.to_string(), style)
                                    }
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                    .centered()
                })
                .collect()
        };
        f.render_widget(Paragraph::new(lines).block(block), a);
    }

    fn sys_list(&self, f: &mut Frame, a: Rect) {
        let w = a.width.saturating_sub(6) as usize;
        let items: Vec<ListItem> = self
            .systems
            .iter()
            .map(|s| {
                let st = self.b.system_status(&s.id);
                let name = trunc(&short(&s.id, &s.name), w.saturating_sub(10));
                ListItem::new(Line::from(vec![
                    Span::styled("\u{25CF} ", Style::new().fg(sc(st))),
                    Span::raw(format!("{:<width$}", name, width = w.saturating_sub(9))),
                    Span::styled(format!("{:>7}", word(st)), Style::new().fg(sc(st))),
                ]))
            })
            .collect();
        let list = List::new(items).block(Block::bordered().title(" SYSTEMS ").border_style(self.focus_style(Pane::Systems)))
            .highlight_style(self.cursor_style(Pane::Systems))
            .highlight_symbol(self.cursor_mark(Pane::Systems))
            .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(list, a, &mut ListState::default().with_selected(Some(self.sys)));
        self.scroll(f, a, self.systems.len(), self.sys, 0);
    }

    fn ind_table(&self, f: &mut Frame, a: Rect, inds: &[IndicatorInfo]) {
        let wide = a.width >= 70;
        let rows: Vec<Row> = inds
            .iter()
            .map(|i| {
                let st = self.b.indicator_status(&i.id);
                let val = self.b.latest(&i.id).map(|r| format!("{} {}", fmt_num(r.value), i.unit)).unwrap_or("-".into());
                let hist: Vec<f64> = self.b.history(&i.id).iter().rev().take(14).rev().map(|r| r.value).collect();
                let mut cells = vec![
                    Cell::from(format!("{}{}", i.name, if self.b.is_due(&i.id) { " *" } else { "" })),
                    Cell::from(val),
                    Cell::from(Span::styled(format!("\u{25CF} {}", word(st)), Style::new().fg(sc(st)))),
                ];
                if wide {
                    cells.push(Cell::from(Span::styled(spark(&hist), Style::new().fg(sc(st)))));
                }
                Row::new(cells)
            })
            .collect();
        let widths: Vec<Constraint> = if wide { vec![Constraint::Min(22), Constraint::Length(12), Constraint::Length(10), Constraint::Length(14)] } else { vec![Constraint::Min(18), Constraint::Length(10), Constraint::Length(10)] };
        let head = if wide { vec!["INDICATOR", "LAST", "STATUS", "14-DAY"] } else { vec!["INDICATOR", "LAST", "STATUS"] };
        let table = Table::new(rows, widths)
            .header(Row::new(head).style(Style::new().fg(Color::DarkGray).bold()))
            .block(Block::bordered().title(" INDICATORS  (* = due) ").border_style(self.focus_style(Pane::Indicators)))
            .row_highlight_style(self.cursor_style(Pane::Indicators))
            .highlight_symbol(self.cursor_mark(Pane::Indicators))
            .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(table, a, &mut TableState::default().with_selected(Some(self.ind)));
        self.scroll(f, a, inds.len(), self.ind, 1);
    }

    fn detail(&self, f: &mut Frame, a: Rect, inds: &[IndicatorInfo]) {
        let mut l: Vec<Line> = vec![];
        let dim = Style::new().fg(Color::DarkGray);
        let title;
        if self.pane == Pane::Systems || inds.is_empty() {
            let s = self.cur_sys();
            title = format!(" {} ", s.name);
            let st = self.b.system_status(&s.id);
            l.push(Line::from(Span::styled(format!("Status: {}", word(st)), Style::new().fg(sc(st)).bold())));
            l.push(Line::from(vec![Span::styled("Function: ", dim), Span::raw(s.function.clone())]));
            l.push(Line::from(vec![Span::styled("Hazards:  ", dim), Span::styled(s.hazards.join(" | "), Style::new().fg(Color::Magenta))]));
            l.push(Line::from(vec![Span::styled("Effect:   ", dim), Span::raw(s.effect.clone())]));
            l.push(Line::from(vec![Span::styled("Why:      ", dim), Span::raw(s.why.clone())]));
            if let Some(n) = self.b.environment().notes.get(&s.id) {
                l.push(Line::from(vec![Span::styled("Here:     ", dim), Span::styled(n.clone(), Style::new().fg(Color::Cyan))]));
            }
        } else {
            let i = &inds[self.ind.min(inds.len() - 1)];
            title = format!(" {} ", i.name);
            let latest = self.b.latest(&i.id);
            // while typing a new value, the advice follows what is typed (live preview)
            let typed = self.edit.as_ref().filter(|e| e.ind == i.id).and_then(|e| e.text.replace(',', ".").parse::<f64>().ok());
            let ev = self.b.evaluate(&i.id, typed.or(latest.map(|r| r.value)).unwrap_or(0.0));
            let st = if typed.is_some() { ev.status } else { self.b.indicator_status(&i.id) };
            l.push(Line::from(vec![
                Span::styled(format!("{} {}", typed.or(latest.map(|r| r.value)).map(fmt_num).unwrap_or("-".into()), i.unit), Style::new().bold()),
                Span::raw("   "),
                Span::styled(word(st), Style::new().fg(sc(st)).bold()),
                Span::styled(format!("   target {}   Enter = log new value", ev.target), dim),
            ]));
            l.push(Line::from(vec![Span::styled("Why: ", dim), Span::raw(i.why.clone())]));
            l.push(Line::from(Span::styled("How to measure:", dim)));
            for (n, h) in i.how.iter().enumerate() {
                l.push(Line::from(format!("  {}. {}", n + 1, h)));
            }
            if latest.is_some() || typed.is_some() {
                l.push(Line::from(""));
                l.push(Line::from(Span::styled(ev.advice.headline.clone(), Style::new().fg(sc(st)).bold())));
                for a in &ev.advice.actions {
                    l.push(Line::from(format!("  - {}", a)));
                }
                if let Some(e) = &ev.advice.escalate {
                    l.push(Line::from(Span::styled(e.clone(), Style::new().fg(Color::Red).bold())));
                }
            }
        }
        let p = Paragraph::new(l).block(Block::bordered().title(title).border_style(Style::new().fg(Color::DarkGray))).wrap(Wrap { trim: true }).scroll((self.scroll, 0));
        f.render_widget(p, a);
    }

    fn comms_view(&self, f: &mut Frame, a: Rect) {
        let o = self.b.overview();
        let (s, rows) = self.b.queue(a.height.saturating_sub(8) as usize);
        let env = self.b.environment();
        let l = vec![
            Line::from(vec![
                Span::raw("Mode: "),
                if o.autonomous { Span::styled("AUTONOMOUS - onboard protocol is primary", Style::new().fg(Color::Yellow).bold()) } else { Span::styled("GROUND-ASSISTED", Style::new().fg(Color::Green).bold()) },
            ]),
            Line::from(format!("Link: {}    Round trip: {}    Comms here: {}", if o.link_up { "UP" } else { "DOWN" }, rtt(o.rtt_s), env.comms)),
            Line::from(format!("Queued {}   In flight {}   Delivered {}   (urgent first; nothing is lost during blackout or restart)", s.queued, s.in_flight, s.acked)),
            Line::from(""),
        ];
        let head = Row::new(vec!["PRIORITY", "BUNDLE", "STATE"]).style(Style::new().fg(Color::DarkGray).bold());
        let trs: Vec<Row> = rows
            .iter()
            .map(|r| {
                let (pw, pc) = match r.priority {
                    Priority::Urgent => ("URGENT", Color::Red),
                    Priority::Watch => ("watch", Color::Yellow),
                    Priority::Routine => ("routine", Color::Green),
                };
                let sw = match r.state {
                    QState::Queued => "queued",
                    QState::InFlight => "in flight",
                    QState::Acked => "ack from ground",
                };
                Row::new(vec![Cell::from(Span::styled(pw, Style::new().fg(pc).bold())), Cell::from(r.label.clone()), Cell::from(sw)])
            })
            .collect();
        let parts = Layout::vertical([Constraint::Length(l.len() as u16), Constraint::Min(3)]).split(a);
        f.render_widget(Paragraph::new(l), parts[0]);
        f.render_widget(
            Table::new(trs, [Constraint::Length(10), Constraint::Min(30), Constraint::Length(16)])
                .header(head)
                .block(Block::bordered().title(" DOWNLINK QUEUE   b = blackout on/off   x = export bundle ").border_style(Style::new().fg(Color::DarkGray))),
            parts[1],
        );
    }

    fn health_view(&self, f: &mut Frame, a: Rect) {
        let h = self.b.health();
        let ok = h.checks.iter().filter(|c| c.ok).count();
        let parts = Layout::vertical([Constraint::Length(h.checks.len() as u16 + 4), Constraint::Min(3)]).split(a);
        let mut l = vec![Line::from(vec![
            Span::raw("Self-test "),
            Span::styled(format!("{}/{}", ok, h.checks.len()), Style::new().fg(if ok == h.checks.len() { Color::Green } else { Color::Red }).bold()),
            Span::styled(format!("    platform {}", h.platform), Style::new().fg(Color::DarkGray)),
        ])];
        l.push(Line::from(""));
        for c in &h.checks {
            l.push(Line::from(vec![
                Span::styled(if c.ok { "[ OK ] " } else { "[FAIL] " }, Style::new().fg(if c.ok { Color::Green } else { Color::Red }).bold()),
                Span::raw(c.name.clone()),
                Span::styled(format!(" - {}", c.detail), Style::new().fg(Color::DarkGray)),
            ]));
        }
        f.render_widget(Paragraph::new(l).block(Block::bordered().title(" SYSTEM HEALTH   t = run self-test   f = inject fault (demo) ").border_style(Style::new().fg(Color::DarkGray))), parts[0]);
        let ev: Vec<Line> = h.events.iter().take(parts[1].height.saturating_sub(2) as usize).map(|e| Line::from(Span::styled(e.clone(), Style::new().fg(Color::Gray)))).collect();
        f.render_widget(Paragraph::new(ev).block(Block::bordered().title(" EVENT LOG ").border_style(Style::new().fg(Color::DarkGray))), parts[1]);
    }

    fn help_view(&self, f: &mut Frame, a: Rect) {
        let k = |key: &str, d: &str| Line::from(vec![Span::styled(format!("{:<16}", key), Style::new().fg(Color::Cyan).bold()), Span::raw(d.to_string())]);
        let l = vec![
            k("r", "switch between CREW view (self-assessment) and GROUND console (flight surgeon)"),
            k("Up/Down  j/k", "move in the current list"),
            k("Tab / Right", "go to the second list      Left / Esc: back"),
            k("Enter", "crew: open systems, log a new value      ground: open crew / system, plot or unplot an indicator"),
            k("  while typing", "digits . -   Up/Down = +/- one step   Enter = save   Esc = cancel"),
            k("c", "crew: daily check-in, every measurement that is due"),
            k("[  ]", "ground: previous / next crew member"),
            k("a  m", "ground: acknowledge the selected finding (Tab to the event log) / write a message to the crew member"),
            k("  while writing", "Tab = insert a template      Enter = send (uplink)      Esc = cancel"),
            k("g / h", "downlink queue / system health      Esc: back to the main view"),
            k("b", "simulate blackout on / off (queue keeps everything)"),
            k("e", "next environment: station, cislunar, Moon, Mars surface, Mars transit"),
            k("x  t", "export the downlink bundle (JSON)  /  run the self-test"),
            k("PgUp/PgDn", "scroll the text panel"),
            k("n s f", "demo: next day / solar storm / inject data fault"),
            k("q", "quit"),
            Line::from(""),
            Line::from(Span::styled("Prototype for decision support - not a medical device. Follow mission medical protocols.", Style::new().fg(Color::DarkGray))),
        ];
        f.render_widget(Paragraph::new(l).block(Block::bordered().title(" KEYS ").border_style(Style::new().fg(Color::DarkGray))), a);
    }

    fn keys_line(&self, f: &mut Frame, a: Rect) {
        if let Some(text) = &self.compose {
            let sp = vec![
                Span::styled(format!(" MESSAGE to {} ", self.crew_name()), Style::new().fg(Color::Black).bg(Color::Yellow).bold()),
                Span::raw(format!(" {}_", text)),
                Span::styled("   Enter=send Tab=template Esc=cancel", Style::new().fg(Color::DarkGray)),
            ];
            f.render_widget(Paragraph::new(Line::from(sp)), a);
            return;
        }
        if let Some(e) = &self.edit {
            let Some(info) = self.all_inds().into_iter().find(|i| i.id == e.ind) else { return };
            let ev = e.text.replace(',', ".").parse::<f64>().ok().map(|v| self.b.evaluate(&e.ind, v));
            let mut sp = vec![Span::styled(format!(" LOG {} [{}] ", info.name, info.unit), Style::new().fg(Color::Black).bg(Color::Cyan).bold()), Span::raw(format!(" {}_ ", e.text))];
            if let Some(ev) = ev {
                sp.push(Span::styled(format!(" {} ", word(ev.status)), Style::new().fg(Color::Black).bg(sc(ev.status)).bold()));
                sp.push(Span::styled(format!(" target {} ", ev.target), Style::new().fg(Color::DarkGray)));
            }
            sp.push(Span::styled(" Enter=save Esc=cancel Up/Dn=step", Style::new().fg(Color::DarkGray)));
            f.render_widget(Paragraph::new(Line::from(sp)), a);
            return;
        }
        let keys: Vec<(&str, &str)> = if self.role == Role::Ground {
            vec![("Up/Dn", "move"), ("Tab", "pane"), ("Enter", "plot"), ("[ ]", "crew"), ("a", "ack"), ("m", "message"), ("r", "crew view"), ("g", "downlink"), ("h", "health"), ("?", "help"), ("q", "quit")]
        } else {
            vec![("Up/Dn", "move"), ("Tab", "pane"), ("Enter", "log"), ("c", "check-in"), ("b", "blackout"), ("e", "env"), ("r", "ground"), ("g", "comms"), ("?", "help"), ("q", "quit")]
        };
        let mut sp = vec![];
        for (k, d) in keys {
            sp.push(Span::styled(format!(" {} ", k), Style::new().fg(Color::Black).bg(Color::Gray)));
            sp.push(Span::raw(format!("{} ", d)));
        }
        f.render_widget(Paragraph::new(Line::from(sp)), a);
    }

    fn status_line(&self, f: &mut Frame, a: Rect) {
        let h = self.b.health();
        let (q, _) = self.b.queue(0);
        let ok = h.checks.iter().filter(|c| c.ok).count();
        let o = self.b.overview();
        let good = |b: bool| if b { Style::new().fg(Color::Green) } else { Style::new().fg(Color::Red).bold() };
        let crc_ok = h.checks.iter().find(|c| c.name.contains("CRC")).map_or(true, |c| c.ok);
        let mut sp = vec![
            Span::styled(" DB ", Style::new().fg(Color::DarkGray)),
            Span::styled("OK", good(h.checks.first().map_or(true, |c| c.ok))),
            Span::styled("  CRC ", Style::new().fg(Color::DarkGray)),
            Span::styled(if crc_ok { "OK" } else { "REPAIR" }, good(crc_ok)),
            Span::styled("  BIT ", Style::new().fg(Color::DarkGray)),
            Span::styled(format!("{}/{}", ok, h.checks.len()), good(ok == h.checks.len())),
            Span::styled("  QUEUE ", Style::new().fg(Color::DarkGray)),
            Span::raw(format!("{}q {}f {}a", q.queued, q.in_flight, q.acked)),
            Span::styled("  DUE ", Style::new().fg(Color::DarkGray)),
            Span::raw(format!("{}/{}", o.due_count, o.total_indicators)),
        ];
        if let Some((m, _)) = &self.toast {
            sp.push(Span::styled(format!("   > {}", m), Style::new().fg(Color::Cyan).bold()));
        }
        f.render_widget(Paragraph::new(Line::from(sp)).style(Style::new().bg(Color::Black)), a);
    }
}

/// Run the interface until the user quits. Redraws only when something changed.
pub fn run<B: Backend>(b: B) -> std::io::Result<()> {
    run_as(b, Role::Crew)
}

/// Same, starting in the given role (the user can still switch with `r`).
pub fn run_as<B: Backend>(b: B, role: Role) -> std::io::Result<()> {
    let mut term = ratatui::init();
    let mut app = App::new(b);
    app.role = role;
    let mut last = Instant::now();
    let res = (|| -> std::io::Result<()> {
        loop {
            if app.dirty {
                term.draw(|f| app.draw(f))?;
                app.dirty = false;
            }
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => app.on_key(k),
                    Event::Resize(..) => app.dirty = true,
                    _ => {}
                }
            }
            if last.elapsed() >= Duration::from_secs(1) {
                app.tick();
                last = Instant::now();
            }
            if app.quit {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyEventKind, KeyModifiers};
    use ui_model::DemoBackend;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent { code: c, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: ratatui::crossterm::event::KeyEventState::NONE }
    }
    fn dump(app: &App<DemoBackend>, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| app.draw(f)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn ascii_figure_rows_are_symmetric_and_fit() {
        for r in FIGURE {
            assert!(r.chars().count() % 2 == 1, "row must have odd width to centre: {r:?}");
            assert!(r.chars().count() <= 21);
        }
    }

    #[test]
    fn braille_anatomy_draws_a_body() {
        let app = App::new(DemoBackend::new());
        let cells = braille::render(26, 26, &|s| app.b.system_status(s), "heart");
        let dots: usize = cells.iter().flatten().filter(|c| c.ch != ' ').count();
        assert!(dots > 120, "body should cover many cells, got {dots}");
        // the body is centred: the first and last columns stay empty
        assert!(cells.iter().all(|r| r[0].ch == ' '));
    }

    #[test]
    fn crew_view_renders_at_80x24_and_120x44() {
        let app = App::new(DemoBackend::new());
        let s = dump(&app, 80, 24);
        println!("{s}\n{}", "=".repeat(80));
        assert!(s.contains("ASTROCHECK") && s.contains("CREW") && s.contains("SYSTEMS") && s.contains("INDICATORS"));
        let s = dump(&app, 120, 44);
        println!("{s}");
        assert!(s.contains("BODY"));
    }

    #[test]
    fn ground_console_shows_roster_triage_and_clinical_table() {
        let mut app = App::new(DemoBackend::new());
        app.on_key(key(KeyCode::Char('r')));
        let s = dump(&app, 110, 40);
        println!("{s}");
        assert!(s.contains("GROUND") && s.contains("OBJECTS") && s.contains("TELEMETRY") && s.contains("EVENT LOG"));
        assert!(s.contains("Eng. Chen") && s.contains("Sci. Novak"));
        let s80 = dump(&app, 80, 24);
        println!("{s80}");
        assert!(s80.contains("EVENT LOG") && s80.contains("OBJECTS"));
    }

    #[test]
    fn ground_can_select_crew_acknowledge_and_message() {
        let mut app = App::new(DemoBackend::new());
        app.on_key(key(KeyCode::Char('r')));
        app.on_key(key(KeyCode::Char(']')));
        app.on_key(key(KeyCode::Char(']'))); // reyes -> okafor -> chen
        assert_eq!(app.b.selected_crew(), "chen");
        app.on_key(key(KeyCode::Tab)); // focus the event log
        let first = app.b.alerts()[0].clone();
        assert_eq!(first.indicator, "spo2", "life-critical finding is listed first");
        app.on_key(key(KeyCode::Char('a')));
        assert!(app.b.alerts()[0].acked);
        app.on_key(key(KeyCode::Char('m')));
        app.on_key(key(KeyCode::Tab)); // insert template
        assert!(dump(&app, 110, 40).contains("MESSAGE to Eng. Chen"));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.b.messages().len(), 1);
        app.on_key(key(KeyCode::Char('r')));
        assert_eq!(app.role, Role::Crew);
    }

    #[test]
    fn too_small_terminal_is_handled() {
        let app = App::new(DemoBackend::new());
        assert!(dump(&app, 60, 15).contains("too small"));
    }

    #[test]
    fn logging_a_value_through_keys_changes_status() {
        let mut app = App::new(DemoBackend::new());
        for k in [KeyCode::Down, KeyCode::Tab] {
            app.on_key(key(k));
        } // heart -> lungs, then into its indicators
        let before = app.b.indicator_status("spo2");
        app.on_key(key(KeyCode::Enter)); // start editing SpO2
        for _ in 0..3 {
            app.on_key(key(KeyCode::Backspace));
        }
        for c in "88".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        let s = dump(&app, 100, 30);
        assert!(s.contains("ACT NOW"), "live preview should show the status while typing");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.b.indicator_status("spo2"), Status::Act);
        assert_ne!(before, Status::Act);
    }

    #[test]
    fn views_render() {
        let mut app = App::new(DemoBackend::new());
        for (k, needle) in [('g', "DOWNLINK QUEUE"), ('h', "SYSTEM HEALTH"), ('?', "KEYS")] {
            app.on_key(key(KeyCode::Esc));
            app.on_key(key(KeyCode::Char(k)));
            let s = dump(&app, 100, 30);
            assert!(s.contains(needle), "{needle}");
        }
    }

    #[test]
    fn blackout_key_switches_mode() {
        let mut app = App::new(DemoBackend::new());
        app.on_key(key(KeyCode::Char('b')));
        assert!(dump(&app, 100, 30).contains("LINK DOWN"));
    }
}

#[cfg(test)]
mod ground_tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ui_model::DemoBackend;

    #[test]
    fn tree_plots_and_unplots_an_indicator() {
        let mut app = App::new(DemoBackend::new());
        app.role = Role::Ground;
        app.b.select_crew("chen");
        app.sync_watch();
        assert!(app.watch.contains(&"spo2".to_string()));
        let rows = app.tree_rows();
        let i = rows.iter().position(|r| matches!(&r.node, Node::Ind(id) if id == "spo2")).expect("spo2 is listed (lungs are open for Chen)");
        app.tree_cur = i;
        app.tree_enter();
        assert!(!app.watch.contains(&"spo2".to_string()));
        app.tree_enter();
        assert!(app.watch.contains(&"spo2".to_string()));
        let mut t = Terminal::new(TestBackend::new(130, 44)).unwrap();
        t.draw(|f| app.draw(f)).unwrap();
    }
}
