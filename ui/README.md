# AstroCheck UI (Rust)

Both native programs now default to the same BE-06 HTTP API as React. Run
`npm run demo` from the repository root, then start the window or terminal client.
No native SQLite database or additional API server is created. A shared `ui-http`
adapter polls real measurements, server health guidance, seven-day UTC series and tasks on a background
thread. Writes are confirmed by the server before success is reported.

```bash
# From the repository root, in separate terminals:
cargo run --locked --manifest-path ui/Cargo.toml -p ui-gui
cargo run --locked --manifest-path ui/Cargo.toml -p ui-tui
# Another server / LAN host:
cargo run --locked --manifest-path ui/Cargo.toml -p ui-gui -- --api-url http://192.168.1.10:8080
```

`--api-url` overrides `ASTROCHECK_API_URL`, whose default is `http://127.0.0.1:8080`.
`--ground` starts TUI in Ground; GUI uses `--role ground`. Both support `--help`.
The GUI live window speaks Russian (default) or English: `--lang ru|en`, `ASTROCHECK_LANG`, or the
RU / EN switch in its top bar. Texts written by the server (rule explanations, actions, task titles)
are shown as received. The live window uses the same instrument-panel look as the `--demo` prototype.
The API owns the single `demo-astronaut` profile, measurements and task completion.
The clients discover supported metrics from the latest-measurements response: the
original six work on older servers; the additional six appear only when the
server advertises them. Live clients display source, measurement/receipt time and
server freshness separately from BE-08 health guidance. The server owns all health
rules; green means no configured rule alert, amber attention, red urgent action,
and grey means unevaluated or insufficient data. These are demonstration rules,
not diagnoses or validated astronaut limits. Rules with an expired validity time,
unmatched measurement ID, failed assessment refresh or offline connection cannot
remain current; prior guidance stays explicitly historical.

GUI metric cards and body-system labels reflect current rules. Expand a card for
next steps, escalation, flight context and source links; the health panel includes
urgent-symptom guidance and the five spaceflight hazards. In TUI, use `i` for the
selected metric, `h` for health guidance, and `f` for flight education; arrow keys
and PageUp/PageDown scroll these views. New urgent results automatically open the
health view. `Enter` edits a measurement, `Tab` switches to tasks, and `?` lists keys.
An older BE-06 server without health rules stays usable with neutral status.
Scores, baseline advice, uplink and ACK remain unavailable in live mode. API failures keep the client in live
mode with a visible error and cached data, never switch to fake measurements.
Polling is about 2 seconds for latest/tasks/assessments and 10 seconds for series.
Pending writes are in memory, not a durable offline outbox; await confirmation
before closing the client. Retry attempts reuse the exact measurement payload/key.
Task updates with a lost response are not automatically replayed: the existing
PATCH changes the completion timestamp. The client reports an unconfirmed result
and refreshes tasks from the server.
The window retains a 2D body diagram; rotatable 3D belongs to the React client.

Build both applications with
`cargo build --locked --release --manifest-path ui/Cargo.toml -p ui-gui -p ui-tui`.
Default output is `ui/target/release/astrocheck-gui` and `astrocheck-tui`
(`.exe` on Windows). `CARGO_TARGET_DIR` can override that location.
Run `npm run test:native-api` from the repository root for a real-API acceptance
check using isolated SQLite, two independent adapters and browser-contract requests.

## Explicit standalone demo

Use `--demo` to choose the original in-memory `DemoBackend`. It is independent
of the HTTP server. The following feature tour and reference screenshots describe
that richer design prototype, not the implemented capabilities of BE-06.

Two interfaces, one contract, for two audiences:

| Audience | What they get |
|---|---|
| **Crew** (the astronaut) | Self-assessment: readiness index, daily task list, status per body system, findings in plain language, metric tiles with sparklines and deviation from *their own* baseline, metric detail with chart + statistics, daily check-in, reaction-time test (PVT). No gamification: no levels, points or streaks. |
| **Ground** (flight surgeon / NASA team) | Console laid out like [NASA Open MCT](https://nasa.github.io/openmct/): object tree (crew > body system > indicator, status dots, click to plot, double-click for detail), stacked telemetry plots with limit bands and a time conductor (7D / 14D / All), event log with acknowledge, crew status strip with data age (STALE during blackout), the selected crew member's body figure, and an uplink message composer with one-way light-delay estimate. |

Both exist as a **window** (`astrocheck-gui`, egui) and a **terminal** program (`astrocheck-tui`, ratatui;
works over SSH / serial console / on boards without a display). Switch role at runtime (GUI: Crew|Ground control
in the top bar, TUI: key `r`).

Real anatomy: the window shows a shaded frontal render of the CT-based adult male meshes of BodyParts3D (skin; skull,
spine, ribs with costal cartilage, sternum, clavicles, scapulae, pelvis, arm and leg bones, hands, patellae, feet; brain,
eyes, airways, lungs, thymus, heart, aorta, venae cavae, pulmonary trunk, oesophagus, liver, gallbladder, stomach, pancreas,
kidneys, intestines, bladder), so sizes and positions are measured, not drawn. `tools/render_anatomy.py` rasterises every
part with a z-buffer, key and fill light, specular and screen-space occlusion (2x2 supersampled) into one sprite atlas;
the app colours it per layer: *Organs / Skeleton / X-ray* (the skin is a dark body behind and a glass surface in front).
A body system's status is one outline around its organs plus a light tint; square markers and leader-line labels are
clickable, and clicks are tested against the sprites' pixels. In the terminal the figure is drawn with braille dots from
the vector outlines of `tools/gen_anatomy.py`. Both scripts need the BodyParts3D download (see the scripts).

Screens: see `docs/*.png` (window) and `docs/terminal-*.txt` (terminal frames).

## Design: NASA style, from NASA's own material

* **Typography (NASA Horizon Design System, [nasa/hds-core](https://github.com/nasa/hds-core)):** Inter for headings,
  Public Sans for body text, DM Mono for numbers, units and technical labels. Fonts are embedded
  (`crates/ui-gui/assets/fonts`, SIL Open Font License; licence texts included).
* **Colour:** Horizon blue as the single accent, neutral greys. Following NASA's crew-interface guidance
  ("use color sparingly", "quiet/dark" displays, standard colour meanings) the screen is *quiet*: NORMAL is neutral,
  colour appears only on departures: WATCH amber, ACT NOW red, NO DATA cyan. Healthy metric tiles carry no status bar,
  and the crew overview shows only what needs attention by default ("Show all metrics" is one click away).
* **Shape language:** an instrument panel, not an app: square corners, hairline borders, one chamfered corner per panel,
  square status markers and lamps, segmented gauges instead of rings, tracked DM Mono captions, a height scale and R / L
  marks on the anatomy view (radiology convention: the patient's right is on the viewer's left). Translucent colours are
  mixed in gamma space (`theme::alpha`), so a 12 % tint really is 12 %.
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
crates/ui-http    shared BE-06 HTTP adapter, worker, DTOs and metric mapping
crates/ui-model   the contract + data. NO UI code.
    backend.rs      trait Backend  <- implement this on the real backend
    types.rs        plain owned data types (Status, Reading, Evaluation, CrewMember, AlertRow, ...)
    anatomy.rs      the human figure as vector data (skin, skeleton, organs, labels), triangulation, hit-testing
    demo.rs         DemoBackend: in-memory, 4 crew members, so the UIs run today
    content.rs, rules.rs   demo content (thresholds, texts) from data/content.json
crates/ui-tui     terminal UI (lib + bin astrocheck-tui), braille.rs rasteriser
crates/ui-gui     window UI (bin astrocheck-gui): figure.rs, widgets.rs, theme.rs, main.rs
    anatomy_sprites.rs + assets/anatomy_atlas.png   shaded anatomy sprites (generated by tools/render_anatomy.py)
data/content.json   thresholds, advice texts, environments (demo only; the real backend owns its own)
docs/             screenshots and terminal frames
```

## Backend extension point

`ui-http::HttpBackend` implements this contract for the main server. Its live-mode
methods expose connection state, provenance, timed series, pending sends, tasks and
health reports with client validity metadata. No health threshold is duplicated in the UI.
The original methods below remain the interface for standalone demo extensions.

1. `impl ui_model::Backend for YourBackend` (see `crates/ui-model/src/backend.rs`, every method is documented).
   * Required: the read side (`overview`, `systems`, `indicators`, `latest`, `history`, `thresholds`, `evaluate`, ...),
     the write side (`log`, `set_environment`, `set_blackout`, ...) and `tick` / `take_messages`.
   * Optional (default no-ops): `crew`, `select_crew`, `alerts`, `ack_alert`, `send_message`, `messages` - without them
     the UI shows a single crew member and an empty ground roster; `demo_*` methods.
2. Choose the backend in `crates/ui-tui/src/main.rs` or `crates/ui-gui/src/main.rs`.
   The default is `HttpBackend`; `DemoBackend::new()` is used only with `--demo`.
3. The interfaces never block: all `Backend` calls must be cheap and synchronous; do slow work on your own thread
   and return the cached state. The UIs call `tick()` about once per second and redraw only when needed.

All numbers in `evaluate` / `thresholds` / advice texts are produced by the backend. The UI only displays them.

## Build and run

```bash
# From the repository root:
cd ui
cargo run --release -p ui-tui -- --demo # standalone terminal prototype
cargo run --release -p ui-gui -- --demo # standalone window prototype
cargo test --locked --workspace        # HTTP adapter, model, GUI and terminal tests
```

Demo window options: `--role crew|ground`, `--env iss|orion|moon|mars|transit`, `--lite` (no animation),
`--size WxH`, and for visual checks `--screenshot out.png --metric ID --system ID --crew ID --mode organs|skeleton|xray
--tab main|comms|system --blackout --storm`.
Terminal on a console without braille glyphs (Linux framebuffer, `TERM=linux`): an ASCII sketch is used automatically,
or force it with `ASTROCHECK_ASCII=1`.
Ground telemetry scrolls with PageUp/PageDown so every selected channel remains reachable.
Its sample labels refer to the displayed readings; GUI stacked plots share one time range, including All.

Windows needs a C toolchain for Rust (MSVC Build Tools, or the GNU toolchain + MinGW). If the project sits in a deep
folder, point `CARGO_TARGET_DIR` at a short path (Windows 260-character path limit).
`[profile.release]` is tuned for size (`opt-level = "s"`, LTO, `panic = "abort"`, stripped).

## Original prototype measurements (Windows 11, release builds, idle)

These historical figures describe the standalone prototype before the HTTP
adapter was added. They are not measurements of the current shared-API binaries.

| | binary | idle CPU | RAM |
|---|---|---|---|
| `astrocheck-tui` | 0.71 MB | 0.13 % of one core | 6 MB |
| `astrocheck-gui` (window not focused, 1 repaint / s) | 6.9 MB (0.65 MB anatomy atlas included) | 1.6 % of one core | 92 MB |

The focused, animated window (10 fps) was about 4 % of one core before the anatomy became sprites; not re-measured since.

The window repaints about once a second when idle (10 fps with the heartbeat / breathing animation while the window is
focused). Most of its CPU and RAM here is the *software* OpenGL path of this machine; on a board with a GPU driver it is
expected to differ - measure on the target. The terminal version is the lightweight one.

## Honest limits

* The anatomy is one real adult male (BodyParts3D, 99 % reduced meshes, frontal view only, no spleen or lymph nodes in the
  data set). BodyParts3D models the lungs only as their airway and vessel trees, so the lung surface is rebuilt from those
  trees (closed outline raised as a dome, fissures between the lobes): its outline is close, not exact. It is not a
  patient-specific model. To use other artwork, regenerate the atlas (window) or replace `anatomy::parts()` (terminal).
* Thresholds, advice texts and the demo crew are illustrative. Not a medical device.
* Originally developed on Windows 11; the integrated workspace also compiles and passes model/terminal tests on macOS.
  Linux headless workspace tests are configured in CI. Not yet run on a Raspberry Pi or any other board; the GUI needs the usual
  GL / X11 / Wayland runtime libraries there.
* In `--demo` the downlink queue, the built-in test and the corruption demo are simulated. The live window uses the
  shaded anatomy and the instrument theme, but its screen is the simpler shared-server view (`live.rs`); the full crew /
  ground prototype runs only with `--demo`.

## Credits and licences

* Anatomy data: "BodyParts3D, (c) The Database Center for Life Science licensed under CC Attribution 4.0 International"
  ([licence](https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html)). The mesh files themselves carry the older
  CC Attribution-Share Alike 2.1 Japan header, so `crates/ui-model/src/anatomy_data.rs` (derived data) is shared under the
  same terms.
* Fonts: Inter, Public Sans, DM Mono (SIL Open Font License, texts in `crates/ui-gui/assets/fonts`).
