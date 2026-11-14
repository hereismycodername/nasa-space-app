# AstroCheck UI (Rust)

The main application now ports this layout and anatomy to React (`../src/features/`).
Run the Rust API and React app from the repository root; that client uses the real
BE-05 endpoints. The native programs in this directory remain a standalone demo
with `DemoBackend`; they are not HTTP clients for the server yet.

Two interfaces, one contract, for two audiences:

| Audience | What they get |
|---|---|
| **Crew** (the astronaut) | Self-assessment: readiness index, daily task list, status per body system, findings in plain language, metric tiles with sparklines and deviation from *their own* baseline, metric detail with chart + statistics, daily check-in, reaction-time test (PVT). No gamification: no levels, points or streaks. |
| **Ground** (flight surgeon / NASA team) | Console laid out like [NASA Open MCT](https://nasa.github.io/openmct/): object tree (crew > body system > indicator, status dots, click to plot, double-click for detail), stacked telemetry plots with limit bands and a time conductor (7D / 14D / All), event log with acknowledge, crew status strip with data age (STALE during blackout), the selected crew member's body figure, and an uplink message composer with one-way light-delay estimate. |

Both exist as a **window** (`astrocheck-gui`, egui) and a **terminal** program (`astrocheck-tui`, ratatui;
works over SSH / serial console / on boards without a display). Switch role at runtime (GUI: Crew|Ground control
in the top bar, TUI: key `r`).

Real anatomy: the figure is a frontal projection of the CT-based adult male meshes of BodyParts3D (skin, skull, spine,
ribs, clavicles, scapulae, pelvis, arm and leg bones; brain, eyes, trachea, lungs, thymus, heart, liver, gallbladder, stomach,
pancreas, kidneys, intestines, bladder), so sizes and positions are measured, not drawn. Layers: *Organs / Skeleton / X-ray*.
Organ outlines and the hotspot markers show status; labels with leader lines are clickable. In the terminal the same figure
is drawn with braille dots. Regenerate the data with `tools/gen_anatomy.py` (needs the BodyParts3D download, see the script).

Screens: see `docs/*.png` (window) and `docs/terminal-*.txt` (terminal frames).

## Design: NASA style, from NASA's own material

* **Typography (NASA Horizon Design System, [nasa/hds-core](https://github.com/nasa/hds-core)):** Inter for headings,
  Public Sans for body text, DM Mono for numbers, units and technical labels. Fonts are embedded
  (`crates/ui-gui/assets/fonts`, SIL Open Font License; licence texts included).
* **Colour:** Horizon blue as the single accent, neutral greys. Following NASA's crew-interface guidance
  ("use color sparingly", "quiet/dark" displays, standard colour meanings) the screen is *quiet*: NORMAL is neutral,
  colour appears only on departures: WATCH amber, ACT NOW red, NO DATA cyan. Healthy metric tiles carry no status bar,
  and the crew overview shows only what needs attention by default ("Show all metrics" is one click away).
* **Always visible:** a status bar with key parameters (DB / CRC / self-test / queue / due) and a one-line alert strip.
* **Standard conventions:** no custom decorative icons, the crew never has to do arithmetic (baseline and delta are
  computed), ground view is denser than the crew view (tables, trends) as on NASA ground displays.
* The exact Artemis GUI Standard (M2M-30040), Orion Display Format Standard (MPCV 72242) and ISS DGCS (SSP 50313) are not
  public; the status colours above follow general aerospace practice and must be checked against them before real use.
  Brand red must not be used on dark backgrounds (NASA brand rule), so alerts use a functional red.

Sources: [NASA Brand Guidelines](https://www.nasa.gov/nasa-brand-center/brand-guidelines),
[NASA Horizon Design System](https://github.com/nasa/hds-core),
[NASA CLDP "Crew Interfaces" (2024)](https://ntrs.nasa.gov/api/citations/20240003566/downloads/CLD%20HSI%20Workshop%20Crew%20Interfaces.pdf),
[NASA Open MCT](https://nasa.github.io/openmct/about-open-mct).

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

* The anatomy is one real adult male (BodyParts3D, 99 % reduced meshes, frontal view only, no spleen or lymph nodes in the
  data set, hands and feet are skin only). It is not a patient-specific model. To use other artwork replace `anatomy::parts()`;
  nothing else depends on how the shapes were made.
* Thresholds, advice texts and the demo crew are illustrative. Not a medical device.
* Developed and tested on Windows 11 only. Not yet run on a Raspberry Pi or any other board; the GUI needs the usual
  GL / X11 / Wayland runtime libraries there.
* The demo backend keeps everything in memory; persistence, integrity checks and the downlink queue belong to the real backend.

## Credits and licences

* Anatomy data: "BodyParts3D, (c) The Database Center for Life Science licensed under CC Attribution 4.0 International"
  ([licence](https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html)). The mesh files themselves carry the older
  CC Attribution-Share Alike 2.1 Japan header, so `crates/ui-model/src/anatomy_data.rs` (derived data) is shared under the
  same terms.
* Fonts: Inter, Public Sans, DM Mono (SIL Open Font License, texts in `crates/ui-gui/assets/fonts`).
