//! In-memory demo backend: lets the interfaces run today, before the real backend exists.
//! Mimics the behaviour the real one is expected to have (due dates, blackout,
//! store-and-forward queue with priorities, corruption detection) but stores nothing on disk.

use crate::backend::Backend;
use crate::content::*;
use crate::rules;
use crate::types::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let u1 = self.next().max(1e-12);
        let u2 = self.next();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

struct QItem {
    id: u64,
    ind: String,
    val: f64,
    pri: Priority,
    state: QState,
    sent: Option<Instant>,
}

/// State of a crew member who is not the currently selected one.
struct Stash {
    hist: HashMap<String, Vec<Reading>>,
}
struct Member {
    id: &'static str,
    name: &'static str,
    title: &'static str,
    contact_base: u64,
    stash: Option<Stash>,
}

pub struct DemoBackend {
    members: Vec<Member>,
    cur: usize,
    acked: Vec<(usize, String)>,
    chat: Vec<(usize, Message)>,
    blackout_since: Option<Instant>,
    c: Content,
    day: u32,
    env: usize,
    blackout: bool,
    hist: HashMap<String, Vec<Reading>>,
    queue: Vec<QItem>,
    next_qid: u64,
    events: Vec<String>,
    msgs: Vec<String>,
    fault: Option<Instant>,
    started: Instant,
}

fn seed_values(ind: &IndDef, env_dose: f64, day: u32, rng: &mut Rng) -> Vec<Reading> {
    let (mut b, mut dr, mut n) = (ind.seed[0], ind.seed[1], ind.seed[2]);
    if ind.id == "dose" {
        b = env_dose;
        dr = 0.0;
        n = b * 0.1;
    }
    let days: Vec<u32> = if ind.freq == 1 {
        (1..day).filter(|d| *d != 9).collect() // one missed day
    } else {
        let mut v = vec![];
        let mut d = day as i64 - ind.freq as i64;
        while d >= 1 {
            v.push(d as u32);
            d -= ind.freq as i64;
        }
        v.reverse();
        v
    };
    days.into_iter()
        .map(|d| Reading { day: d, value: rules::snap(ind, b + dr * d as f64 + n * rng.gauss()) })
        .collect()
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoBackend {
    pub fn new() -> Self {
        let mut b = DemoBackend {
            members: vec![
                Member { id: "reyes", name: "Cdr. Reyes", title: "Commander", contact_base: 61, stash: None },
                Member { id: "okafor", name: "Dr. Okafor", title: "Medical officer", contact_base: 242, stash: None },
                Member { id: "chen", name: "Eng. Chen", title: "Flight engineer", contact_base: 118, stash: None },
                Member { id: "novak", name: "Sci. Novak", title: "Mission scientist", contact_base: 205, stash: None },
            ],
            cur: 0,
            acked: vec![],
            chat: vec![],
            blackout_since: None,
            c: Content::load(),
            day: 15,
            env: 0,
            blackout: false,
            hist: HashMap::new(),
            queue: vec![],
            next_qid: 1,
            events: vec![],
            msgs: vec![],
            fault: None,
            started: Instant::now(),
        };
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let dose = b.c.envs[0].dose;
        for ind in &b.c.inds {
            let v = seed_values(ind, dose, b.day, &mut rng);
            b.hist.insert(ind.id.clone(), v);
        }
        for i in 1..b.members.len() {
            let s = b.build_stash(i);
            b.members[i].stash = Some(s);
        }
        b.event("BOOT: demo backend started (in-memory)");
        b
    }

    /// Demo crew: 0 = fatigue (the seeded default), 1 = rising stress, 2 = low SpO2 event, 3 = all normal.
    fn build_stash(&self, i: usize) -> Stash {
        let mut rng = Rng(0xABCD_1234 + i as u64 * 7919);
        let dose = self.c.envs[self.env].dose;
        let mut hist: HashMap<String, Vec<Reading>> = HashMap::new();
        for ind in &self.c.inds {
            hist.insert(ind.id.clone(), seed_values(ind, dose, self.day, &mut rng));
        }
        let set_last = |hist: &mut HashMap<String, Vec<Reading>>, id: &str, vals: &[f64]| {
            if let Some(h) = hist.get_mut(id) {
                let n = h.len();
                for (k, v) in vals.iter().rev().enumerate() {
                    if k < n {
                        h[n - 1 - k].value = *v;
                    }
                }
            }
        };
        match i {
            1 => {
                set_last(&mut hist, "stress", &[5.0, 6.0, 7.0, 8.0]);
                set_last(&mut hist, "sleep", &[6.3, 5.8, 5.6]);
                set_last(&mut hist, "hr", &[74.0, 78.0]);
            }
            2 => {
                set_last(&mut hist, "spo2", &[91.0]);
                set_last(&mut hist, "hr", &[92.0]);
                set_last(&mut hist, "rr", &[24.0]);
                set_last(&mut hist, "headache", &[6.0]);
            }
            3 => {
                for ind in &self.c.inds {
                    let mid = (ind.ok[0] + ind.ok[1]) / 2.0;
                    let w = (ind.ok[1] - ind.ok[0]) * 0.15;
                    if let Some(h) = hist.get_mut(&ind.id) {
                        for r in h.iter_mut() {
                            r.value = rules::snap(ind, mid + w * rng.gauss());
                        }
                    }
                }
            }
            _ => {}
        }
        Stash { hist }
    }

    fn contact_age(&self, m: &Member) -> u64 {
        match self.blackout_since {
            Some(t) if self.blackout => m.contact_base + t.elapsed().as_secs() * 60, // demo clock x60
            _ => m.contact_base % 300 + (self.envd().rtt / 2.0) as u64,
        }
    }

    fn ind(&self, id: &str) -> &IndDef {
        self.c.inds.iter().find(|i| i.id == id).expect("known indicator id")
    }
    fn envd(&self) -> &EnvDef {
        &self.c.envs[self.env]
    }
    fn stamp(&self) -> String {
        let s = self.started.elapsed().as_secs();
        format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    }
    fn event(&mut self, m: &str) {
        let line = format!("{}  {}", self.stamp(), m);
        self.events.insert(0, line);
        self.events.truncate(50);
    }
    fn rtt_real(&self) -> f64 {
        (self.envd().rtt / 60.0).max(1.5) // demo clock x60
    }
    fn due_count(&self) -> usize {
        self.c.inds.iter().filter(|i| self.is_due(&i.id)).count()
    }
    fn all_statuses(&self) -> Vec<Status> {
        self.c.inds.iter().map(|i| self.indicator_status(&i.id)).collect()
    }
}

impl Backend for DemoBackend {
    fn overview(&self) -> Overview {
        let e = self.envd();
        Overview {
            mission_day: self.day,
            env_id: e.id.clone(),
            env_name: e.name.clone(),
            score: rules::score(&self.all_statuses()),
            link_up: !self.blackout,
            autonomous: self.blackout || e.rtt >= 60.0,
            rtt_s: e.rtt,
            due_count: self.due_count(),
            total_indicators: self.c.inds.len(),
        }
    }

    fn systems(&self) -> Vec<SystemInfo> {
        self.c
            .systems
            .iter()
            .map(|s| SystemInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                function: s.function.clone(),
                hazards: s.haz.iter().map(|k| self.hazard_label(k)).collect(),
                effect: s.effect.clone(),
                why: s.why.clone(),
            })
            .collect()
    }

    fn indicators(&self, system: &str) -> Vec<IndicatorInfo> {
        self.c
            .inds
            .iter()
            .filter(|i| i.sys == system)
            .map(|i| IndicatorInfo {
                id: i.id.clone(),
                system: i.sys.clone(),
                name: i.name.clone(),
                unit: i.unit.clone(),
                kind: i.kind,
                min: i.min,
                max: i.max,
                step: i.step,
                freq_days: i.freq,
                why: i.why.clone(),
                how: i.how.clone(),
            })
            .collect()
    }

    fn system_status(&self, system: &str) -> Status {
        self.c
            .inds
            .iter()
            .filter(|i| i.sys == system)
            .map(|i| self.indicator_status(&i.id))
            .max()
            .unwrap_or(Status::NoData)
    }

    fn indicator_status(&self, id: &str) -> Status {
        match self.latest(id) {
            Some(r) => rules::status(self.ind(id), r.value, self.envd()),
            None => Status::NoData,
        }
    }

    fn latest(&self, id: &str) -> Option<Reading> {
        self.hist.get(id).and_then(|h| h.last().copied())
    }

    fn history(&self, id: &str) -> Vec<Reading> {
        self.hist.get(id).cloned().unwrap_or_default()
    }

    fn thresholds(&self, id: &str) -> Thresholds {
        rules::thresholds(self.ind(id), self.envd())
    }

    fn evaluate(&self, id: &str, value: f64) -> Evaluation {
        let ind = self.ind(id);
        let st = rules::status(ind, value, self.envd());
        Evaluation {
            status: st,
            target: rules::range_text(ind, self.envd()),
            advice: rules::advice(ind, st, self.envd(), self.blackout),
        }
    }

    fn is_due(&self, id: &str) -> bool {
        let f = self.ind(id).freq;
        self.latest(id).map_or(true, |r| self.day.saturating_sub(r.day) >= f)
    }

    fn environments(&self) -> Vec<EnvInfo> {
        self.c.envs.iter().map(env_info).collect()
    }
    fn environment(&self) -> EnvInfo {
        env_info(self.envd())
    }
    fn hazard_label(&self, key: &str) -> String {
        self.c.hazards.get(key).map(|s| plain(s)).unwrap_or_else(|| key.to_string())
    }

    fn queue(&self, max_rows: usize) -> (QueueSummary, Vec<QueueRow>) {
        let mut s = QueueSummary::default();
        for q in &self.queue {
            match q.state {
                QState::Queued => s.queued += 1,
                QState::InFlight => s.in_flight += 1,
                QState::Acked => s.acked += 1,
            }
        }
        let rows = self
            .queue
            .iter()
            .rev()
            .take(max_rows)
            .map(|q| {
                let i = self.ind(&q.ind);
                QueueRow {
                    id: q.id,
                    label: format!("{}: {} {}", i.name, rules::fmt(q.val), i.unit),
                    priority: q.pri,
                    state: q.state,
                }
            })
            .collect();
        (s, rows)
    }

    fn health(&self) -> Health {
        let vectors = [("hr", 70.0, Status::Normal), ("hr", 125.0, Status::Act), ("spo2", 93.0, Status::Watch), ("temp", 37.8, Status::Watch)];
        let env0 = &self.c.envs[0];
        let vec_ok = vectors.iter().filter(|(id, v, st)| rules::status(self.ind(id), *v, env0) == *st).count();
        let sane = self.c.inds.iter().all(|i| self.hist[&i.id].iter().all(|r| r.value >= i.min && r.value <= i.max && r.day <= self.day));
        let ck = |name: &str, ok: bool, d: &str| Check { name: name.into(), ok, detail: d.into() };
        Health {
            checks: vec![
                ck("Database integrity", true, "integrity_check ok"),
                ck(
                    "Row CRC32 + journal agree",
                    self.fault.is_none(),
                    if self.fault.is_none() { "all records verified" } else { "corruption detected - repair in progress" },
                ),
                ck("Threshold table consistent", true, &format!("{} indicators checked", self.c.inds.len())),
                ck("Rules engine reference vectors", vec_ok == vectors.len(), &format!("{}/{} deterministic results", vec_ok, vectors.len())),
                ck("Data sanity: ranges and mission clock", sane, if sane { "all records valid" } else { "invalid records" }),
                ck("Network", true, "not required - nothing is sent anywhere"),
            ],
            events: self.events.clone(),
            platform: format!("{} / {}", std::env::consts::ARCH, std::env::consts::OS),
        }
    }

    fn blackout(&self) -> bool {
        self.blackout
    }

    fn log(&mut self, id: &str, value: f64) -> LogResult {
        let (v, st) = {
            let ind = self.ind(id);
            let v = rules::snap(ind, value);
            (v, rules::status(ind, v, self.envd()))
        };
        let pri = match st {
            Status::Act => Priority::Urgent,
            Status::Watch => Priority::Watch,
            _ => Priority::Routine,
        };
        let day = self.day;
        let h = self.hist.entry(id.to_string()).or_default();
        if let Some(r) = h.iter_mut().find(|r| r.day == day) {
            r.value = v;
        } else {
            h.push(Reading { day, value: v });
        }
        self.queue.push(QItem { id: self.next_qid, ind: id.to_string(), val: v, pri, state: QState::Queued, sent: None });
        self.next_qid += 1;
        if self.queue.len() > 200 {
            self.queue.remove(0);
        }
        LogResult { status: st, value: v }
    }

    fn set_environment(&mut self, env_id: &str) {
        if let Some(i) = self.c.envs.iter().position(|e| e.id == env_id) {
            self.env = i;
            let day = self.day;
            let dose_def = self.ind("dose");
            let mut rng = Rng(0x5EED_5EED);
            let mut v = seed_values(dose_def, self.c.envs[i].dose, day, &mut rng);
            if let Some(t) = self.hist["dose"].iter().find(|r| r.day == day).copied() {
                v.push(t);
            }
            self.hist.insert("dose".into(), v);
            let n = self.c.envs[i].name.clone();
            self.event(&format!("Environment: {}", n));
        }
    }

    fn set_blackout(&mut self, on: bool) {
        self.blackout = on;
        self.blackout_since = if on { Some(Instant::now()) } else { None };
        self.event(if on { "LINK DOWN: blackout, autonomous mode" } else { "LINK UP: flushing queue, urgent first" });
    }

    fn run_selftest(&mut self) {
        let h = self.health();
        let ok = h.checks.iter().filter(|c| c.ok).count();
        self.event(&format!("BIT: {}/{} passed", ok, h.checks.len()));
    }

    fn export(&mut self) -> Result<String, String> {
        let dir = std::env::temp_dir().join("astrocheck-exports");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let bundles: Vec<_> = self
            .queue
            .iter()
            .map(|q| {
                let pri = ["routine", "watch", "urgent"][q.pri as usize];
                serde_json::json!({ "id": q.id, "indicator": q.ind, "value": q.val, "priority": pri })
            })
            .collect();
        let body = serde_json::json!({
            "format": "astrocheck-downlink/0.1", "environment": self.envd().id,
            "missionDay": self.day, "bundles": bundles,
        });
        let path = dir.join(format!("downlink-day{}.json", self.day));
        std::fs::write(&path, serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }

    fn tick(&mut self) -> bool {
        let now = Instant::now();
        let mut ch = false;
        if let Some(t) = self.fault {
            if now.duration_since(t) >= Duration::from_secs(3) {
                self.fault = None;
                self.event("SCRUB: 1 corrupted row restored from journal");
                self.msgs.push("Data corruption detected and repaired automatically".into());
                ch = true;
            }
        }
        if !self.blackout {
            let mut idx: Vec<usize> = (0..self.queue.len()).filter(|&i| self.queue[i].state == QState::Queued).collect();
            idx.sort_by_key(|&i| (std::cmp::Reverse(self.queue[i].pri as u8), self.queue[i].id));
            for i in idx.into_iter().take(3) {
                self.queue[i].state = QState::InFlight;
                self.queue[i].sent = Some(now);
                ch = true;
            }
            let rtt = Duration::from_secs_f64(self.rtt_real());
            for q in self.queue.iter_mut() {
                if q.state == QState::InFlight && q.sent.map_or(false, |t| now.duration_since(t) >= rtt) {
                    q.state = QState::Acked;
                    ch = true;
                }
            }
        }
        ch
    }

    fn crew(&self) -> Vec<CrewMember> {
        self.members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let hist = if i == self.cur { &self.hist } else { &m.stash.as_ref().expect("stash").hist };
                let st: Vec<Status> = self
                    .c
                    .inds
                    .iter()
                    .map(|ind| hist.get(&ind.id).and_then(|h| h.last()).map_or(Status::NoData, |r| rules::status(ind, r.value, self.envd())))
                    .collect();
                CrewMember {
                    id: m.id.into(),
                    name: m.name.into(),
                    title: m.title.into(),
                    status: st.iter().copied().max().unwrap_or(Status::NoData),
                    score: rules::score(&st),
                    last_contact_s: self.contact_age(m),
                    open_alerts: st.iter().filter(|s| **s >= Status::Watch).count(),
                }
            })
            .collect()
    }

    fn selected_crew(&self) -> String {
        self.members[self.cur].id.to_string()
    }

    fn select_crew(&mut self, id: &str) {
        let Some(i) = self.members.iter().position(|m| m.id == id) else { return };
        if i == self.cur {
            return;
        }
        let out = Stash { hist: std::mem::take(&mut self.hist) };
        self.members[self.cur].stash = Some(out);
        let s = self.members[i].stash.take().expect("stash present");
        self.hist = s.hist;
        self.cur = i;
    }

    fn alerts(&self) -> Vec<AlertRow> {
        let mut v: Vec<AlertRow> = self
            .c
            .inds
            .iter()
            .enumerate()
            .filter_map(|(k, ind)| {
                let r = self.hist.get(&ind.id)?.last()?;
                let st = rules::status(ind, r.value, self.envd());
                if st < Status::Watch {
                    return None;
                }
                Some(AlertRow {
                    id: k as u64,
                    indicator: ind.id.clone(),
                    indicator_name: ind.name.clone(),
                    system: ind.sys.clone(),
                    value: r.value,
                    unit: ind.unit.clone(),
                    status: st,
                    day: r.day,
                    acked: self.acked.iter().any(|(m, i)| *m == self.cur && *i == ind.id),
                    headline: rules::advice(ind, st, self.envd(), self.blackout).headline,
                })
            })
            .collect();
        v.sort_by(|a, b| b.status.cmp(&a.status).then(clinical_rank(&a.indicator).cmp(&clinical_rank(&b.indicator))));
        v
    }

    fn ack_alert(&mut self, alert_id: u64) {
        if let Some(ind) = self.c.inds.get(alert_id as usize) {
            let id = ind.id.clone();
            if !self.acked.iter().any(|(m, i)| *m == self.cur && *i == id) {
                self.acked.push((self.cur, id.clone()));
                let who = self.members[self.cur].name;
                self.event(&format!("ACK: {} - {}", who, id));
            }
        }
    }

    fn send_message(&mut self, text: &str) {
        let (day, who) = (self.day, self.members[self.cur].name);
        self.chat.push((self.cur, Message { from_ground: true, text: text.to_string(), day }));
        let one_way = self.envd().rtt / 2.0;
        let eta = if one_way < 60.0 { format!("{} s", rules::fmt(one_way)) } else { format!("{} min", (one_way / 60.0).round()) };
        self.event(&format!("UPLINK: message to {} queued", who));
        self.msgs.push(format!("Message to {} queued, arrives in about {}", who, eta));
    }

    fn messages(&self) -> Vec<Message> {
        self.chat.iter().filter(|(m, _)| *m == self.cur).map(|(_, x)| x.clone()).collect()
    }

    fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.msgs)
    }

    fn demo_next_day(&mut self) {
        self.day += 1;
        let d = self.day;
        self.event(&format!("Mission day {}", d));
    }
    fn demo_solar_storm(&mut self) {
        let v = self.envd().dose * 14.0;
        self.log("dose", v);
        self.event("TEST: solar particle event simulated");
    }
    fn demo_inject_fault(&mut self) -> String {
        self.fault = Some(Instant::now());
        self.event("TEST: bit flip injected into database");
        "database: one value corrupted (CRC mismatch), repair in a few seconds".into()
    }
    fn demo_reset(&mut self) {
        *self = DemoBackend::new();
    }
}

fn env_info(e: &EnvDef) -> EnvInfo {
    EnvInfo {
        id: e.id.clone(),
        name: e.name.clone(),
        gravity: e.gravity.clone(),
        comms: e.comms.clone(),
        rtt_s: e.rtt,
        typical_dose: e.dose,
        escalation: e.esc.clone(),
        notes: e.notes.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_state_matches_the_reference_demo() {
        let b = DemoBackend::new();
        let o = b.overview();
        assert_eq!(o.mission_day, 15);
        assert!(o.due_count > 0);
        assert_eq!(b.systems().len(), 7);
        assert_eq!(b.environments().len(), 5);
        assert!(o.score.is_some());
    }

    #[test]
    fn urgent_is_sent_first_and_blackout_holds_everything() {
        let mut b = DemoBackend::new();
        b.set_blackout(true);
        b.log("hr", 70.0);
        let r = b.log("spo2", 88.0);
        assert_eq!(r.status, Status::Act);
        assert!(!b.tick());
        assert_eq!(b.queue(10).0.in_flight, 0);
        b.set_blackout(false);
        assert!(b.tick());
        let (s, rows) = b.queue(10);
        assert_eq!(s.in_flight, 2);
        assert!(rows.iter().any(|r| r.priority == Priority::Urgent && r.state == QState::InFlight));
    }

    #[test]
    fn full_check_clears_due_list() {
        let mut b = DemoBackend::new();
        let ids: Vec<String> = b.c.inds.iter().filter(|i| b.is_due(&i.id)).map(|i| i.id.clone()).collect();
        for id in ids {
            let t = b.thresholds(&id);
            b.log(&id, (t.ok.0 + t.ok.1) / 2.0);
        }
        assert_eq!(b.overview().due_count, 0);
    }

    #[test]
    fn ground_roster_alerts_and_messages() {
        let mut b = DemoBackend::new();
        let crew = b.crew();
        assert_eq!(crew.len(), 4);
        assert_eq!(crew[2].status, Status::Act); // Chen: low SpO2
        assert_eq!(crew[3].status, Status::Normal); // Novak: all normal
        b.select_crew("chen");
        assert_eq!(b.selected_crew(), "chen");
        let a = b.alerts();
        let spo2 = a.iter().find(|x| x.indicator == "spo2").expect("spo2 finding");
        assert_eq!(spo2.status, Status::Act);
        assert!(!spo2.acked);
        b.ack_alert(spo2.id);
        assert!(b.alerts().iter().find(|x| x.indicator == "spo2").unwrap().acked);
        b.send_message("Repeat SpO2 in 1 hour");
        assert_eq!(b.messages().len(), 1);
        b.select_crew("novak");
        assert!(b.messages().is_empty());
        assert!(b.alerts().is_empty());
        b.select_crew("reyes");
        assert!(!b.alerts().is_empty());
    }

    #[test]
    fn blackout_makes_ground_data_stale() {
        let mut b = DemoBackend::new();
        let before = b.crew()[0].last_contact_s;
        b.set_blackout(true);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(b.crew()[0].last_contact_s > before + 30);
    }

    #[test]
    fn preview_evaluation_uses_blackout_text() {
        let mut b = DemoBackend::new();
        assert!(b.evaluate("spo2", 88.0).advice.escalate.unwrap().contains("flight surgeon"));
        b.set_blackout(true);
        assert!(b.evaluate("spo2", 88.0).advice.escalate.unwrap().contains("onboard"));
    }
}
