use ui_model::{DemoBackend, Role};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("astrocheck-tui [--ground] [--db FILE] [--no-seed] [--demo]\n  --ground   start in the ground / flight-surgeon console (default: crew view)\n  --db FILE  SQLite database (default: ASTROCHECK_DB or the user data directory); several interfaces may share it\n  --no-seed  start with an empty database      --demo  in-memory demo data, nothing is stored\n  inside: r switches role, ? shows all keys, q quits");
        return Ok(());
    }
    let role = if args.iter().any(|a| a == "--ground") { Role::Ground } else { Role::Crew };
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if !args.iter().any(|a| a == "--demo") {
        let mut opts = ui_backend::Options::new(value("--db").map(std::path::PathBuf::from).unwrap_or_else(ui_backend::default_db_path));
        opts.seed = !args.iter().any(|a| a == "--no-seed");
        match ui_backend::SqliteBackend::open(opts) {
            Ok(b) => return ui_tui::run_as(b, role),
            Err(e) => eprintln!("database unavailable ({e}); running with demo data, nothing is stored"),
        }
    }
    ui_tui::run_as(DemoBackend::new(), role)
}
