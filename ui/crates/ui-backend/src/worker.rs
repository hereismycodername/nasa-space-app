//! The database thread. All SQLite work happens here, so the interface never waits for the disk:
//! the interface sends commands over an unbounded channel (a send never blocks) and picks up results in `tick()`.
//!
//! Besides executing commands the worker watches `PRAGMA data_version`: when another process (another
//! interface on the same database file) commits, it reloads a snapshot and sends it to the interface.

use crate::store;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use tokio::sync::mpsc as tmpsc;
use ui_model::{DemoBackend, Snapshot};

pub enum Cmd {
    Log { crew: String, ind: String, value: f64 },
    Ack { crew: String, ind: String },
    Msg { crew: String, text: String, day: u32 },
    Env(String),
    Blackout(bool),
    SkipDay,
    Reset { seed: bool },
    /// reload the snapshot now and send it (used by tests and after a failed write)
    Resync,
    Stop,
}

pub enum Evt {
    Snapshot(Box<Snapshot>),
    Failed(String),
}

/// Counters shared with the interface thread (plain atomics: reading them costs nothing).
#[derive(Default)]
pub struct Shared {
    pub pending: AtomicU64,
    pub writes: AtomicU64,
    pub duplicates: AtomicU64,
    pub failures: AtomicU64,
    pub reloads: AtomicU64,
    pub p50_us: AtomicU64,
    pub p95_us: AtomicU64,
    pub max_us: AtomicU64,
    pub last_reload_us: AtomicU64,
    /// 1 = last PRAGMA quick_check was ok
    pub integrity_ok: AtomicU64,
    pub integrity_checks: AtomicU64,
}

pub struct Open {
    pub path: PathBuf,
    pub seed: bool,
}

pub struct Handle {
    pub tx: tmpsc::UnboundedSender<Cmd>,
    pub rx: mpsc::Receiver<Evt>,
    pub shared: Arc<Shared>,
    pub join: Option<std::thread::JoinHandle<()>>,
}

/// Opens (and migrates, and if asked seeds) the database on a new thread and returns once the first snapshot is ready.
pub fn spawn(open: Open) -> Result<(Handle, Snapshot, Vec<(String, String, String)>), String> {
    let (tx, rx) = tmpsc::unbounded_channel::<Cmd>();
    let (etx, erx) = mpsc::channel::<Evt>();
    let (rtx, rrx) = mpsc::channel::<Result<(Snapshot, Vec<(String, String, String)>), String>>();
    let shared = Arc::new(Shared::default());
    let sh = shared.clone();
    let join = std::thread::Builder::new()
        .name("astrocheck-db".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_time().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = rtx.send(Err(e.to_string()));
                    return;
                }
            };
            rt.block_on(run(open, rx, etx, rtx, sh));
        })
        .map_err(|e| e.to_string())?;
    match rrx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok((snap, crew))) => Ok((Handle { tx, rx: erx, shared, join: Some(join) }, snap, crew)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("database did not open within 30 s".into()),
    }
}

async fn init(pool: &SqlitePool, open: &Open) -> Result<(Snapshot, Vec<(String, String, String)>), String> {
    store::migrate(pool).await?;
    let (seed_day, rows, demo_crew) = DemoBackend::demo_rows();
    store::ensure_crew(pool, &demo_crew).await?;
    store::init_clock(pool, if open.seed { seed_day as i64 - 1 } else { 0 }).await?;
    if open.seed {
        store::seed(pool, &rows).await?;
    }
    Ok((store::snapshot(pool).await?, store::crew(pool).await?))
}

async fn run(open: Open, mut rx: tmpsc::UnboundedReceiver<Cmd>, etx: mpsc::Sender<Evt>, rtx: mpsc::Sender<Result<(Snapshot, Vec<(String, String, String)>), String>>, sh: Arc<Shared>) {
    let pool = match store::open_pool(&open.path).await {
        Ok(p) => p,
        Err(e) => {
            let _ = rtx.send(Err(format!("open {}: {}", open.path.display(), e)));
            return;
        }
    };
    match init(&pool, &open).await {
        Ok(v) => {
            let _ = rtx.send(Ok(v));
        }
        Err(e) => {
            let _ = rtx.send(Err(e));
            return;
        }
    }
    drop(rtx);
    sh.integrity_ok.store(1, Relaxed);
    let session = format!("ui-{}-{}", std::process::id(), chrono::Utc::now().timestamp_millis());
    let mut seq: i64 = 0;
    let mut skew = store::skew(&pool).await.unwrap_or(0);
    let mut seen = store::data_version(&pool).await.unwrap_or(0);
    let mut samples: Vec<u32> = Vec::with_capacity(512);
    let mut tick = tokio::time::interval(Duration::from_millis(400));
    let mut last_check = Instant::now();

    macro_rules! reload {
        () => {{
            let t = Instant::now();
            match store::snapshot(&pool).await {
                Ok(s) => {
                    sh.last_reload_us.store(t.elapsed().as_micros() as u64, Relaxed);
                    sh.reloads.fetch_add(1, Relaxed);
                    skew = store::skew(&pool).await.unwrap_or(skew);
                    let _ = etx.send(Evt::Snapshot(Box::new(s)));
                }
                Err(e) => {
                    sh.failures.fetch_add(1, Relaxed);
                    let _ = etx.send(Evt::Failed(format!("reload: {e}")));
                }
            }
        }};
    }

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(first) = cmd else { break };
                let mut batch = vec![first];
                while let Ok(c) = rx.try_recv() { batch.push(c); }
                let mut resync = false;
                let mut stop = false;
                for cmd in batch {
                    sh.pending.fetch_sub(1, Relaxed);
                    let t = Instant::now();
                    let now = store::virtual_now(skew);
                    let res: Result<(), String> = match cmd {
                        Cmd::Log { crew, ind, value } => {
                            let r = store::ingest(&pool, &crew, &session, seq, &ind, value, now, now).await;
                            seq += 1;
                            match r {
                                Ok(created) => {
                                    sh.writes.fetch_add(1, Relaxed);
                                    if !created { sh.duplicates.fetch_add(1, Relaxed); }
                                    let us = t.elapsed().as_micros().min(u32::MAX as u128) as u32;
                                    if samples.len() == 512 { samples.remove(0); }
                                    samples.push(us);
                                    let mut s = samples.clone();
                                    s.sort_unstable();
                                    sh.p50_us.store(s[s.len() / 2] as u64, Relaxed);
                                    sh.p95_us.store(s[(s.len() * 95 / 100).min(s.len() - 1)] as u64, Relaxed);
                                    sh.max_us.fetch_max(us as u64, Relaxed);
                                    Ok(())
                                }
                                Err(e) => Err(format!("write {ind}: {e}")),
                            }
                        }
                        Cmd::Ack { crew, ind } => store::ack(&pool, &crew, &ind, now).await,
                        Cmd::Msg { crew, text, day } => store::message(&pool, &crew, true, &text, day, now).await,
                        Cmd::Env(id) => store::meta_set(&pool, "env", &id).await,
                        Cmd::Blackout(on) => store::meta_set(&pool, "blackout", if on { "1" } else { "0" }).await,
                        Cmd::SkipDay => {
                            skew += store::DAY_MS;
                            store::meta_set(&pool, "clock_skew_ms", &skew.to_string()).await
                        }
                        Cmd::Reset { seed } => {
                            let r = async {
                                store::wipe(&pool).await?;
                                let (seed_day, rows, _) = DemoBackend::demo_rows();
                                store::init_clock(&pool, if seed { seed_day as i64 - 1 } else { 0 }).await?;
                                if seed { store::seed(&pool, &rows).await?; }
                                skew = 0;
                                seq = 0;
                                Ok::<(), String>(())
                            }.await;
                            resync = true;
                            r
                        }
                        Cmd::Resync => { resync = true; Ok(()) }
                        Cmd::Stop => { stop = true; Ok(()) }
                    };
                    if let Err(e) = res {
                        sh.failures.fetch_add(1, Relaxed);
                        let _ = etx.send(Evt::Failed(e));
                        resync = true; // the interface's optimistic state may now differ from the database
                    }
                }
                if resync { reload!(); }
                if stop { break; }
            }
            _ = tick.tick() => {
                if let Ok(v) = store::data_version(&pool).await {
                    if v != seen { seen = v; reload!(); }
                }
                if last_check.elapsed() >= Duration::from_secs(60) {
                    last_check = Instant::now();
                    let ok = store::quick_check(&pool).await.unwrap_or(false);
                    sh.integrity_ok.store(ok as u64, Relaxed);
                    sh.integrity_checks.fetch_add(1, Relaxed);
                }
            }
        }
    }
    pool.close().await;
}
