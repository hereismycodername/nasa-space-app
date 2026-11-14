# AstroCheck UI (Rust)

Two interfaces, one contract, for two audiences:

| Audience | What they get |
|---|---|
| **Crew** (the astronaut) | Self-assessment: readiness score, rings for today, contributors per body system, insights in plain language, metric tiles with sparklines and deviation from *their own* baseline, metric detail with chart + statistics, daily check-in, reaction-time test, XP/streak. |
| **Ground** (flight surgeon / NASA team) | Crew roster with score and *data age* (STALE during blackout), alert triage with acknowledge, clinical table (latest / baseline / delta / status / trend), metric detail per crew member, message composer (uplink) with one-way light-delay estimate. |

Both exist as a **window** (`astrocheck-gui`, egui) and a **terminal** program (`astrocheck-tui`, ratatui;
works over SSH / serial console / on boards without a display). Switch role at runtime (GUI: Crew|Ground control
in the top bar, TUI: key `r`).

Real anatomy: a human figure with skin, skeleton (skull, spine, ribs, clavicles, pelvis, arm and leg bones),
and organs (brain with gyri, eyes, lungs with lobes and bronchi, heart with great vessels, liver, stomach, intestines,
spleen). Layers: *Organs / Skeleton / X-ray*. Organ outlines and the hotspot markers show status (green / amber / red);
labels with leader lines are clickable. In the terminal the same figure is drawn with braille dots.

Screens: see `docs/*.png` (window) and `docs/terminal-*.txt` (terminal frames).

## Layout

```
crates/ui-model   the contract + data. NO UI code.
    backend.rs      trait Backend  <- implement this on the real backend
    types.rs        plain owned data types (Status, Reading, Evaluation, CrewMember, AlertRow, ...)
    anatomy.rs      the human figure as vector data (skin, skeleton, organs, labels), triangulation, hit-testing
    demo.rs         DemoBackend: in-memory, 4 crew members, so the UIs run today
    content.rs, rules.rs   demo content (thresholds, texts) from data/content.json
crates/ui-tui     terminal UI (lib + bin astrocheck-tui), braille.rs rasteriser
crates/ui-gui     window UI (bin astrocheck-gui): figure.rs, widgets.rs, theme.rs, main.rs
data/content.json   thresholds, advice texts, environments (demo only; the real backend owns its own)
docs/             screenshots and terminal frames
```

## Connecting the real backend

1. `impl ui_model::Backend for YourBackend` (see `crates/ui-model/src/backend.rs`, every method is documented).
   * Required: the read side (`overview`, `systems`, `indicators`, `latest`, `history`, `thresholds`, `evaluate`, ...),
     the write side (`log`, `set_environment`, `set_blackout`, ...) and `tick` / `take_messages`.
   * Optional (default no-ops): `crew`, `select_crew`, `alerts`, `ack_alert`, `send_message`, `messages` - without them
     the UI shows a single crew member and an empty ground roster; `demo_*` methods.
2. In `crates/ui-tui/src/main.rs` and `crates/ui-gui/src/main.rs` replace `DemoBackend::new()` with your backend.
3. The interfaces never block: all `Backend` calls must be cheap and synchronous; do slow work on your own thread
   and return the cached state. The UIs call `tick()` about once per second and redraw only when needed.

All numbers in `evaluate` / `thresholds` / advice texts are produced by the backend. The UI only displays them.

## Build and run

```bash
cargo run --release -p ui-tui          # terminal, needs 80x24 and a UTF-8 terminal
cargo run --release -p ui-gui          # window,   needs OpenGL (GL 2.1 / GLES 2) and X11 or Wayland
cargo test  --workspace                # 10 model tests + 9 terminal tests (renders frames headless)
```

Window options: `--role crew|ground`, `--env iss|orion|moon|mars|transit`, `--lite` (no animation),
`--size WxH`, and for visual checks `--screenshot out.png --metric ID --system ID --crew ID --mode organs|skeleton|xray
--tab main|comms|system --blackout --storm`.
Terminal on a console without braille glyphs (Linux framebuffer, `TERM=linux`): an ASCII sketch is used automatically,
or force it with `ASTROCHECK_ASCII=1`.

Windows needs a C toolchain for Rust (MSVC Build Tools, or the GNU toolchain + MinGW). If the project sits in a deep
folder, point `CARGO_TARGET_DIR` at a short path (Windows 260-character path limit).
`[profile.release]` is tuned for size (`opt-level = "s"`, LTO, `panic = "abort"`, stripped).

## Measured (this PC, Windows 11, release builds, idle)

| | binary | idle CPU | RAM |
|---|---|---|---|
| `astrocheck-tui` | 0.71 MB | 0.13 % of one core | 6 MB |
| `astrocheck-gui` (`--lite`) | 3.5 MB | about 2 % of one core | 84 MB |
| `astrocheck-gui` (animated) | 3.5 MB | about 4 % of one core | 84 MB |

The window repaints about once a second when idle (10 fps with the heartbeat / breathing animation while the window is
focused). Most of its CPU and RAM here is the *software* OpenGL path of this machine; on a board with a GPU driver it is
expected to differ - measure on the target. The terminal version is the lightweight one.

## Honest limits

* The anatomy is a schematic medical illustration authored in code (`anatomy.rs`), not a licensed atlas. To use official
  artwork replace `anatomy::parts()`; nothing else depends on how the shapes were made.
* Thresholds, advice texts and the demo crew are illustrative. Not a medical device.
* Developed and tested on Windows 11 only. Not yet run on a Raspberry Pi or any other board; the GUI needs the usual
  GL / X11 / Wayland runtime libraries there.
* The demo backend keeps everything in memory; persistence, integrity checks and the downlink queue belong to the real backend.
