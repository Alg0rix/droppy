# ARCHITECTURE — Droppy

Droppy is a living notch for Windows 10/11: a small black droplet hangs
from the top of the screen, shows live activities while collapsed, and
stretches open like a teardrop into quick pages.

Native Rust, no web view, no runtime. Zero CPU while nothing is moving.

## Crates

| Crate | Role |
|---|---|
| `crates/teardrop-core` | Pure logic. Springs, droplet geometry, LRC parsing, Pomodoro, water log, tasks, shelf, settings, clipboard ring, media/HUD/system models. Unit-tested on any OS. No Windows deps. |
| `crates/teardrop` | Windows app: Win32 + GDI now, Direct2D/DirectWrite next, WinRT for media. Owns the window, tray, services, persistence, selftest. |

## Threads / runtime model

- **UI thread**: Win32 message loop. While anything animates it switches to a
  frame loop: drain input, draw one frame, `DwmFlush()` (vsync of whatever
  refresh rate the monitor runs: 60/120/144/240 Hz). Physics advance on a fixed
  240 Hz step with a carried remainder, so speed is identical at every refresh
  rate. Ambient motion (equalizer, eyes) uses a 30 fps timer; idle = no frames.
- **1 s service tick**: system sample, Pomodoro, water reminder, volume poll,
  media position, SMTC adoption, lyric lookup.
- **Workers**: SMTC poller (MTA, 5 s). Seeks arrive over an `mpsc` channel:
  the UI thread only posts a target, the worker calls WinRT and re-polls, so a
  slow player never blocks animation. Lyric fetch runs per track, never on the
  UI thread.

## Rendering model

`ViewModel` + `UiState` + morph amount --`ui::layout::build`--> `Frame { Scene, hits }`.

- `ui/scene.rs`: retained draw list (rects, rings, polys, text, clip) with a
  builder-level translate + opacity so pages slide/fade as a unit. Pure data,
  unit-tested on any OS.
- `ui/layout.rs` + `ui/pages/`: header tabs (sliding accent pill), collapsed
  activities (music, focus ring, HUDs, eyes), one layout fn per page.
- `ui/state.rs`: hover glow, press, toggle knobs, page slide, compact width,
  track-change peek, water level, all eased with frame-rate independent smoothing.
- `render_d2d.rs`: replays a Scene with Direct2D + DirectWrite. The island is a
  **layered window with per-pixel alpha** (soft shadow, antialiased silhouette).
  Frames render on the GPU (D3D11 texture, GDI-compatible) and go to
  `UpdateLayeredWindow`; machines without a hardware device fall back to a CPU DIB.
  `--selftest` renders the same scenes through WIC to PNG.
- `droplet.rs` (core): flush-top silhouette. Rest states are exact rects; mid-morph
  the neck pinches at the attachment point and the bulb swells, driven by the
  `open` spring plus a `wobble` slosh spring.

## Interaction

Hit regions come from the same frame that was drawn (`Frame.hits`), so what you
see is what you click. Hover-intent opens after 170 ms (collapses 450 ms after
leaving unless pinned by a click); click, tray, hotkey or file drop pin it.
Clicking away closes a pinned island. Text entry (new task) uses `WM_CHAR`.
Shelf tiles are also OLE drag sources: after a 6 px press-and-move they call
`DoDragDrop` with a `CF_HDROP` payload. Only copy/link effects are offered, so
dragging never moves or deletes the original file; a click without movement
still opens it.

## Pages & services contract

Each page is `core` state + a thin Windows sampler:

- Music: SMTC snapshot → `media::Track`; click the seek bar to post a
  `TryChangePlaybackPositionAsync` seek to the media worker; lyrics via LRCLIB
  cache in `%LOCALAPPDATA%\Droppy\lyrics\`, `lyrics::line_at` + `line_progress`.
- Focus: `pomodoro::Pomodoro` state machine; island shows countdown.
- Water: `water::WaterLog` (+250 ml action, week history, `reminder_due`).
- Shelf: `shelf::Shelf` (drop files → pin, click/drag out).
- Tasks: `tasks::TaskList` tiny todos.
- Notes: `notes::NoteList` newest-first scratch notes; the first line is the
  row title, `title_of`/`preview_of` derive both, and the editor wraps text
  with `ui::pages::wrap_note_lines`. Autosaves on every keystroke.
- Clipboard: `clipboard::ClipboardRing` in-memory only; skips items flagged
  `ExcludeClipboardContentFromMonitorProcessing`; click a row to copy it back.
- Today: month calendar + `system::SystemSnapshot` (CPU/mem/disk/battery).
- Quick: `quick::QuickState` — Wi-Fi/Bluetooth tiles drive WinRT
  `Devices::Radios` on a worker (`radios.rs`, MTA, also reads the connected
  SSID); mic/output mute via Core Audio endpoints; the sliders press and
  drag onto `volume::set_scalar` / `brightness::set` (LCD `IOCTL_VIDEO_*`
  first, DDC/CI monitors as fallback); shortcut tiles open `ms-settings:`
  links for what has no public toggle.
- Settings: gear in the header (toggles, water goal, idle look; eye style,
  mood ring, opt-in weather and birthday when the look is Eyes).

## Eyes

`ui/eyes/` is a small expression engine, pure like the rest of `ui`:

- **Mood**: lasting state re-picked every frame from `Senses` (music + beat,
  idle time, pointer, clock/calendar, battery, CPU, disk, network, mic/camera,
  do-not-disturb, weather). Priority order lives in `pick_mood`.
- **React**: short one-offs queued by events (`Eyes::react`). Window events
  (lock, shell flash, WerFault window, Downloads watcher, clipboard image,
  volume, water, drops, pokes) queue them; they play once the face is visible.
- **React**: short one-offs queued by events (`Eyes::react`). Window events
  (lock, shell flash, WerFault window, Downloads watcher, clipboard image,
  volume, water, drops, pokes) queue them; they play once the face is visible.
  Idle ones come from `bits()`: a mood-weighted table plus `BITS`, a pool of
  ~100 gags and skits (typing on a laptop, reading a book, sipping coffee,
  juggling, a magic trick...) that fire unprompted while nothing is going on.
- **Pose**: numbers the painter draws (lids, gaze, brows, squash, glyphs,
  hats, particles). Lids are per-column spans so pupils clip without masks.
  `Extras::prop` + `prop_k` add a held item (`prop()` draws laptops, books,
  mugs, gamepads... in face space, so props squash and rock with the face).
- **Jiggle**: a small spring layered over the pose. Reactions, pokes, beats
  and level changes kick it (hop, squish, wobble), so motion overshoots
  and settles instead of easing in flat.
- **Dance**: while music plays the dancer picks a `Move` per phrase (4 or 8
  beats) from a pool per mood (Dance/Headbang/Vibe) weighted by energy;
  beat strength from the peak meter scales each accent, notes fly on the beat.
- **Gauge**: volume and brightness HUDs carry the face (`Spot::Hud`). It
  acts out the level live: watches the bar, eyes grow with volume, winces
  near max, clamps shut on mute; pupils dilate in the dark and squint (then
  shades) in bright light. Queued reactions wait until the HUD is gone.
  Volume changes arrive from an `IAudioEndpointVolumeCallback` (the 1 s poll
  stays as fallback); brightness from `GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS`
  power-setting notifications (built-in panels).
- Things done while the island is open (a note saved, a task removed) queue
  reactions that don't go stale while hidden, so they play on collapse.
  Every action has one: skips/seeks/calendar steps swipe, +/- and setting
  toggles nod or shake, undo looks sheepish (`Oops`), clearing sweeps,
  goals (all tasks, water goal, a focus block) `Celebrate`, joining a
  meeting salutes, the meeting alert gasps, a fast speed test zooms.
- Live, while collapsed: keystrokes anywhere (input with a still pointer)
  switch to `Typing`; a busy AI to `Thinking`; the last ten seconds of a
  focus timer tick-tock; the wheel over the island spins the pupils; a
  window switch (shell hook) makes the eyes glance toward it; CAPS LOCK
  shouts; long hovers giggle and leaving pouts; unplugging gasps, 10%
  battery faints. Pausing mid-dance is a record scratch.

- **Work**: `Mood::Work(AppCategory)` acts out the app in front
  (`work_pose` gaze pattern + `work_parts` particles): the ambient pick
  below the real states (music, typing, recording, sleep…), promoted
  above them only while a Focus block runs.
  `teardrop_core::apps` classifies it: an exe table, whole-word title hints
  (longest phrase wins) for browsers and app hosts, then the user's
  `app_themes` / `eye_themes` overrides from `settings.json`. `window/`
  samples the foreground window once a second and on every activation
  (only while the eyes are the idle look); just the exe name is kept in
  `Ambient.app`.
  New app: one string in `apps::EXES` or `TITLE_HINTS`. New theme: an
  `AppCategory` arm plus a pose and a particle line in `ui/eyes/`.

Signals come from `ViewModel.env` (`Ambient`), filled by `services_env.rs`
(idle via `GetLastInputInfo`, consent-store registry for mic/camera,
`SHQueryUserNotificationState`, `InternetGetConnectedState`) and the output
peak meter in `volume.rs`, all only while the eyes are the idle look. Weather
is opt-in (wttr.in, geolocated by IP). The face also replaces the equalizer
next to the artwork and the label next to the focus ring.

HUDs (`hud::Hud`) are transient collapsed-island notices: volume, brightness, charging,
headset battery, water reminder with one-click action.

## Data files

- `%APPDATA%\Droppy\`: `settings.json`, `water.json`, `tasks.json`,
  `notes.json`, `shelf.json` — kept on uninstall. AI Chat adds `chats.json`,
  `ai-secrets.json` (DPAPI ciphertext only) and `ai-models.json` (last synced
  model catalog per provider; safe to delete, it re-syncs on launch).
- `%LOCALAPPDATA%\Droppy\`: `droppy.log`, `lyrics\` cache — safe to delete.

## Testing without clicking around

`droppy.exe --selftest <dir>` renders ~26 states (collapsed activities, HUDs,
morph frames, every page, hover/empty/toast variants) to PNG through the real
painter, runs layout + logic checks and writes `report.json`. Non-zero exit on
failure. (Write to a fresh dir if an image viewer still holds old PNGs open.)

| Var | Effect |
|---|---|
| `TEARDROP_SELFTEST_DPI=144` | Render at 150% (default 192 = 2x) |
| `TEARDROP_PERF=1` | Log fps + render ms to `droppy.log` |
| `TEARDROP_CPU=1` | Force the CPU renderer |
