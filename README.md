<h1 align="center">Teardrop</h1>

<p align="center">
  <b>A living notch for Windows 10 and 11.</b><br>
  A small black droplet hangs from the top of your screen. It shows what's happening,<br>
  and when you point at it, it stretches open into quick pages.
</p>

<p align="center">
  <img src="docs/demo.gif" alt="Teardrop demo: the island opens, switches pages, closes and shows HUDs" width="720">
</p>

<p align="center">
  Native Rust &middot; no web view, no runtime &middot; under 1 MB &middot; GPU-drawn and vsync-locked (60 to 240 Hz)<br>
  No accounts, no tracking: your data stays on your PC.<br>
  <sub><a href="docs/demo.mp4">Full-quality demo (MP4, 60 fps)</a></sub>
</p>

---

## What it does

**Collapsed**, the island resizes itself to fit what's going on:

| | |
|---|---|
| <img src="docs/img/02-collapsed-music.png" width="270"> | **Music.** Artwork tile and a live equalizer. |
| <img src="docs/img/03-music-peek.png" width="380"> | **Track change.** The title peeks out for a few seconds, then tucks away. |
| <img src="docs/img/08-collapsed-focus.png" width="216"> | **Focus.** A progress ring and countdown, even when you're on another page. |
| <img src="docs/img/05-hud-volume.png" width="310"> | **Volume.** A level bar that follows your volume keys and mute. |
| <img src="docs/img/06-hud-water.png" width="350"> | **Reminders.** Click the island to log **+250 ml** (or press `W` anywhere). |
| <img src="docs/img/07-hud-charging.png" width="330"> | **Power and devices.** Charging, battery level, a headset arriving. |
| <img src="docs/img/04-collapsed-eyes.png" width="144"> | **Idle.** A clock, a dot, nothing at all, or eyes with a life of their own: they dance to music (on the real beat), doze off when you step away, react to your pointer, track changes, lock/unlock, downloads, screenshots and more. Five styles: round, cat, pixel, anime, robot. |

**Open it** by resting the pointer on it, clicking it, pressing `Win`+`Shift`+`T`, or dropping a file on it. It doesn't scale a rectangle: it necks in where it hangs from the screen edge, swells into a bulb, then settles into a card.

<p align="center">
  <img src="docs/img/morph.png" alt="Frames of the droplet stretching open" width="760">
</p>

Move away and a hover-opened island folds back up. Click inside it and it stays pinned until you press `Esc` or click elsewhere.

## Pages

Scroll the mouse wheel over the header, or use `←` `→`, to switch. Everything is clickable, with hover and press feedback.

<table>
  <tr>
    <td width="50%"><img src="docs/img/20-page-music.png"><br><b>Music</b> &mdash; live info from any app that uses Windows media controls, real prev / play / next, click the progress bar to seek, time-synced lyrics with a karaoke highlight.</td>
    <td width="50%"><img src="docs/img/21-page-focus.png"><br><b>Focus</b> &mdash; a Pomodoro ring that keeps counting in the collapsed island. Adjustable length, cycle dots.</td>
  </tr>
  <tr>
    <td><img src="docs/img/22-page-water.png"><br><b>Water</b> &mdash; an animated glass, quick +150 / +250 / +500, undo, and a 7-day chart against your goal.</td>
    <td><img src="docs/img/23-page-shelf.png"><br><b>Shelf</b> &mdash; drop files to keep them handy. Click to open, drag out to Explorer or another app, reveal in Explorer, or remove.</td>
  </tr>
  <tr>
    <td><img src="docs/img/24-page-tasks-typing.png"><br><b>Tasks</b> &mdash; type to add, click to tick off, hover to delete.</td>
    <td><img src="docs/img/25-page-clipboard.png"><br><b>Clipboard</b> &mdash; recent text, in memory only. Click a row to copy it back.</td>
  </tr>
  <tr>
    <td><img src="docs/img/26-page-today.png"><br><b>Today</b> &mdash; a month calendar plus CPU, memory, disk and battery meters.</td>
    <td><img src="docs/img/27-page-settings.png"><br><b>Settings</b> &mdash; behind the gear: hover-open, lyrics, reminders, history, sounds, fullscreen, start with Windows, water goal, idle look.</td>
  </tr>
</table>

### Keyboard

While the island is open and focused:

| Key | Action |
|---|---|
| `←` `→` / `Esc` | Switch page / close |
| `Space`, `N`, `B` | Music: play/pause, next, previous |
| `S`, `P`, `R` | Focus: start, pause/resume, reset |
| `W` | +250 ml water (works on any page) |
| `A` | New task (type, `Enter` to add, `Esc` to cancel) |
| `C` | Clear done tasks / clear clipboard history |
| `1`&ndash;`6` | Open a shelf file / copy a clipboard item |

## Smooth by design

- **Vsync-locked.** While anything moves, frames are paced by the compositor (`DwmFlush`), so it follows whatever your monitor runs: 60, 120, 144, 240 Hz.
- **GPU-drawn.** Direct2D renders on a D3D11 texture straight into a per-pixel-alpha layered window, so the shadow and edges are properly antialiased. A frame takes about 1 ms to draw. Machines without a GPU fall back to CPU rendering.
- **Fixed-step physics.** Springs advance at 240 Hz with a carried remainder, so motion speed is identical at every refresh rate and never judders.
- **Zero cost at rest.** Collapsed and idle means no frames at all. Equalizer and eyes use a gentle 30 fps timer.

## Privacy

Teardrop makes no accounts and sends no telemetry. The only network call is fetching synced lyrics from [LRCLIB](https://lrclib.net) for the track you're playing, cached locally, and you can switch it off in Settings. Clipboard history lives in memory only and skips items that password managers mark as private.

Your data is in `%APPDATA%\Teardrop\`; cache and log are in `%LOCALAPPDATA%\Teardrop\`. The cache and log are safe to delete any time.

## Install

Download **`teardrop-setup-<version>.exe`** from the [Releases](https://github.com/Alg0rix/teardrop/releases) page once a release is published. It installs per-user (no admin rights), adds a Start menu shortcut and launches Teardrop. There's also a portable `.exe` and zip if you'd rather not install. Builds are unsigned, so Windows SmartScreen warns on first run: choose **More info → Run anyway**.

Uninstall from *Settings → Apps*. It removes the app, cache and log and keeps your data.

No release yet? Build from source (below), or run `tools\package.ps1` for `dist\teardrop.exe`, a portable zip and, if [NSIS](https://nsis.sourceforge.io) is installed, the installer.

## Build

Cargo workspace: `teardrop-core` (pure logic, unit-tested on any OS) and `teardrop` (the Windows app).

On Windows with Rust stable and the MSVC toolchain:

```
cargo test --workspace
cargo build -p teardrop --release        # target\release\teardrop.exe
```

### Testing without clicking around

```
teardrop.exe --selftest dist\selftest    # a PNG of every state + report.json
teardrop.exe --demo docs                 # re-render demo.gif / demo.mp4 (needs ffmpeg)
```

`--selftest` runs the real renderer headless and exits non-zero if a layout or logic check fails. Environment variables:

| Var | Effect |
|---|---|
| `TEARDROP_SELFTEST_DPI=144` | Render screenshots at 150% (default 192 = 2x) |
| `TEARDROP_PERF=1` | Log fps and render time to `teardrop.log` |
| `TEARDROP_CPU=1` | Force the CPU renderer |

See [ARCHITECTURE.md](ARCHITECTURE.md) for threads, the rendering model, page contracts and data files.

## Status

Early. Developed and tested on Windows 11. The Shelf drags files out with OLE `DoDragDrop`, and the music progress bar seeks through Windows media controls.

## License

MIT. See [LICENSE](LICENSE).
