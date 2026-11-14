use ui_model::{DemoBackend, Role};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("astrocheck-tui [--ground]   terminal interface (demo backend)\n  --ground  start in the ground / flight-surgeon console (default: crew view)\n  inside: r switches role, ? shows all keys, q quits");
        return Ok(());
    }
    let role = if args.iter().any(|a| a == "--ground") { Role::Ground } else { Role::Crew };
    // Swap DemoBackend for the real backend here: anything that implements ui_model::Backend.
    ui_tui::run_as(DemoBackend::new(), role)
}
