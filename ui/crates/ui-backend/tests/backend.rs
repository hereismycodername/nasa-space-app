use std::path::PathBuf;
use std::time::{Duration, Instant};
use ui_backend::{Options, SqliteBackend};
use ui_model::{Backend, DemoBackend, Status};

struct TempDb(PathBuf);
impl TempDb {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("astrocheck-test-{}-{}", name, uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDb(dir.join("test.db"))
    }
    fn opts(&self) -> Options {
        Options::new(&self.0)
    }
}
impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}
fn uuid_like() -> String {
    format!("{}-{:?}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

/// Tick until `f` is true or the timeout passes.
fn wait(b: &mut SqliteBackend, f: impl Fn(&SqliteBackend) -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(30) {
        b.tick();
        if f(b) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn seeded_database_reproduces_the_demo_crew_and_statuses() {
    let db = TempDb::new("seed");
    let b = SqliteBackend::open(db.opts()).unwrap();
    let demo = DemoBackend::new();
    let (cb, cd) = (b.crew(), demo.crew());
    assert_eq!(cb.len(), 4);
    for (x, y) in cb.iter().zip(cd.iter()) {
        assert_eq!((x.id.as_str(), x.status, x.score), (y.id.as_str(), y.status, y.score), "{}", x.id);
    }
    assert_eq!(b.overview().mission_day, demo.overview().mission_day);
    assert_eq!(b.indicator_status("sleep"), demo.indicator_status("sleep"));
}

#[test]
fn a_logged_value_survives_a_restart() {
    let db = TempDb::new("restart");
    {
        let mut b = SqliteBackend::open(db.opts()).unwrap();
        let r = b.log("hr", 73.0);
        assert_eq!(r.status, Status::Normal);
        assert!(b.sync(Duration::from_secs(5)));
        assert_eq!(b.stats().failures, 0);
    }
    let b = SqliteBackend::open(db.opts()).unwrap();
    assert_eq!(b.latest("hr").unwrap().value, 73.0);
    assert_eq!(b.latest("hr").unwrap().day, b.overview().mission_day);
    // reopening must not seed twice: the demo history plus today's reading, nothing duplicated
    assert_eq!(b.history("hr").len(), DemoBackend::new().history("hr").len() + 1);
}

#[test]
fn findings_acks_and_messages_persist_per_crew_member() {
    let db = TempDb::new("ground");
    {
        let mut b = SqliteBackend::open(db.opts()).unwrap();
        b.select_crew("chen");
        let spo2 = b.alerts().into_iter().find(|a| a.indicator == "spo2").expect("Chen's low SpO2 is a finding");
        assert_eq!(spo2.status, Status::Act);
        b.ack_alert(spo2.id);
        b.send_message("Repeat SpO2 in 1 hour");
        assert!(b.sync(Duration::from_secs(5)));
    }
    let mut b = SqliteBackend::open(db.opts()).unwrap();
    b.select_crew("chen");
    assert!(b.alerts().into_iter().find(|a| a.indicator == "spo2").unwrap().acked);
    assert_eq!(b.messages().len(), 1);
    b.select_crew("novak");
    assert!(b.messages().is_empty());
    assert!(b.alerts().is_empty());
}

#[test]
fn next_day_and_environment_persist_and_new_readings_land_on_the_new_day() {
    let db = TempDb::new("clock");
    let day;
    {
        let mut b = SqliteBackend::open(db.opts()).unwrap();
        day = b.overview().mission_day;
        b.demo_next_day();
        b.set_environment("mars");
        b.log("hr", 70.0);
        assert!(b.sync(Duration::from_secs(5)));
        assert_eq!(b.stats().failures, 0, "a reading with a mission-clock time in the future must be accepted");
    }
    let b = SqliteBackend::open(db.opts()).unwrap();
    assert_eq!(b.overview().mission_day, day + 1);
    assert_eq!(b.overview().env_id, "mars");
    assert_eq!(b.latest("hr").unwrap().day, day + 1);
}

#[test]
fn two_interfaces_on_one_file_see_each_others_changes() {
    let db = TempDb::new("two");
    let mut crew = SqliteBackend::open(db.opts()).unwrap();
    let mut ground = SqliteBackend::open(db.opts()).unwrap();
    ground.select_crew("reyes");
    crew.select_crew("reyes");
    crew.log("spo2", 88.0); // a serious drop
    assert!(wait(&mut ground, |g| g.indicator_status("spo2") == Status::Act), "ground sees the new reading without any action");
    let alert = ground.alerts().into_iter().find(|a| a.indicator == "spo2").unwrap();
    ground.ack_alert(alert.id);
    ground.send_message("Stay calm, re-measure");
    assert!(wait(&mut crew, |c| c.messages().len() == 1), "crew receives the message");
    assert!(crew.alerts().iter().any(|a| a.indicator == "spo2" && a.acked));
}

#[test]
fn invalid_values_are_reported_not_lost_silently() {
    let db = TempDb::new("invalid");
    let mut b = SqliteBackend::open(db.opts()).unwrap();
    // the engine clamps to the indicator range, so a NaN is the only way to reach the storage rules
    b.log("hr", f64::NAN);
    assert!(wait(&mut b, |b| b.stats().failures == 1));
    assert!(b.take_messages().iter().any(|m| m.starts_with("Database:")));
    assert!(b.health().checks.iter().any(|c| c.name == "Storage writes" && !c.ok));
}

/// Numbers for the performance discussion: `cargo test -p ui-backend --release perf -- --nocapture`.
#[test]
fn perf_the_interface_thread_never_waits_for_the_disk() {
    let db = TempDb::new("perf");
    let t = Instant::now();
    let mut b = SqliteBackend::open(db.opts()).unwrap();
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;
    // reads are memory only
    let t = Instant::now();
    let mut sink = 0usize;
    for _ in 0..20_000 {
        sink += b.history("hr").len() + b.alerts().len();
    }
    let read_us = t.elapsed().as_secs_f64() * 1e6 / 20_000.0;
    // writes on the UI thread = engine update + channel send
    let ids = ["hr", "sbp", "temp", "sleep", "spo2", "rr", "glucose", "pvt", "headache", "stress"];
    let n = 5_000;
    let t = Instant::now();
    let mut worst = Duration::ZERO;
    for i in 0..n {
        let s = Instant::now();
        b.log(ids[i % ids.len()], 60.0 + (i % 30) as f64);
        worst = worst.max(s.elapsed());
    }
    let call_us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    let enq = t.elapsed();
    // the database thread drains the queue
    let drained = wait_drain(&mut b);
    let total = t.elapsed();
    let s = b.stats();
    println!("open+seed {:.0} ms | history()+alerts() {:.1} us | log() {:.1} us avg, {:.0} us worst (UI thread)", open_ms, read_us, call_us, worst.as_secs_f64() * 1e6);
    println!("{} writes queued in {:.0} ms, stored by {:.0} ms ({:.0} writes/s) | db p50 {} us p95 {} us max {} us | reload {} us", n, enq.as_secs_f64() * 1000.0, total.as_secs_f64() * 1000.0, n as f64 / total.as_secs_f64(), s.p50_us, s.p95_us, s.max_us, s.last_reload_us);
    assert!(drained && s.failures == 0 && s.writes as usize >= n);
    assert!(call_us < 200.0, "log() on the UI thread must stay cheap, was {call_us:.1} us");
    assert!(sink > 0);
}

fn wait_drain(b: &mut SqliteBackend) -> bool {
    wait(b, |b| b.stats().pending == 0 && b.stats().writes >= 5_000)
}
