# Teardrop Brand & 2D Mascot Kit

Teardrop's brand files, app icon, logo, and 2D mascot: **Droppy**.

| File | What |
|---|---|
| `teardrop-mark.svg`, `teardrop-mark-white.svg`, `teardrop-mark-dark.svg` | The 2D mascot mark (Droppy) in brand azure, pure white cutout, and dark obsidian. |
| `teardrop-logo.svg`, `teardrop-logo-512.png` | The app logo: an azure squircle tile with the black island hanging from its top edge playing music (album cover and equalizer) and the white Droppy mascot below. |
| `teardrop-mascot-512.png` | High-resolution 2D mascot illustration. |
| `art/poses/droppy-*.svg` | Droppy with each page's theme: `droppy-music`, `droppy-focus`, `droppy-water`, `droppy-shelf`, `droppy-tasks`, `droppy-today`. |
| `crates/teardrop/res/teardrop.ico` | Multi-resolution Windows app, window and tray icon (16x16 to 256x256). |

### Drawing Droppy (The Mascot)

- **Shape**: A smooth, plump liquid teardrop with a soft curved apex and bouncy rounded base.
- **Face**: Two clean vertical capsule / pill eyes that look around and blink, matching Teardrop's idle feature. No mouth, nose, or unnecessary noise.
- **Colors**: Vibrant ocean azure (`#0284C7`) with light cyan accent (`#38BDF8`), or pure white when sitting inside the logo tile.

### Rebuilding Brand Assets

Run:
```bash
python tools/make_brand.py
```
*(Requires Python 3 and Pillow)*
