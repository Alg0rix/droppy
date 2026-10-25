"""Draw Droppy's brand files: 2D mascot, app icon, logo, and pose art.

Branding system tailored for Droppy:
- Mascot: "Droppy", a friendly 2D minimalist liquid teardrop character with expressive eyes.
- Logo: A vibrant azure/cyan squircle tile with the black island hanging from the top
  edge (playing music with album art and equalizer bars) and the white 2D Droppy mascot below.
- Poses: 2D mascot with each page's theme (music, focus, water, shelf, tasks, today).

Run: python tools/make_brand.py (needs Pillow)
"""

import math
import sys
from pathlib import Path
from PIL import Image, ImageDraw

if sys.stdout.encoding != 'utf-8':
    try:
        sys.stdout.reconfigure(encoding='utf-8')
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "docs" / "brand"
ART = BRAND / "art"
POSES = ART / "poses"
RES = ROOT / "crates" / "teardrop" / "res"
DOCS_IMG = ROOT / "docs" / "img"

BRAND_COLOR = "#0284C7"       # Vibrant Azure / Ocean Blue tile background
BRAND_LIGHT = "#38BDF8"       # Light Cyan accent
BRAND_DARK = "#0369A1"        # Deep Cerulean
ISLAND_COLOR = "#0A0A0A"      # Island notch black
EQ_COLOR = "#38BDF8"          # Island equalizer bars
COVER_SUN = "#FDE047"         # Mini album art sun
COVER_GRAD = [(0.0, "#38BDF8"), (0.5, "#6366F1"), (1.0, "#1E1B4B")]

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]

# ---------------------------------------------------------------------------
# 1. 2D Mascot Mark (SVG Paths & Coordinates)
# ---------------------------------------------------------------------------
# Droppy is defined in a standard [0, 100] coordinate space centered at (50, 50).
# The silhouette is a smooth, teardrop shape with a gently rounded apex and wide chubby base.
# Eyes are two cute vertical capsules / rounded rectangles.

def droplet_path_d(scale=1.0, tx=0.0, ty=0.0):
    """Returns SVG path data for the 2D droplet silhouette."""
    # Base control points relative to 100x100:
    # Apex at (50, 12)
    # Right curve: (50, 12) -> (68, 26) -> (86, 52) -> (86, 72) -> (78, 88) -> (50, 88)
    # Left curve: symmetric
    # Let's write standard cubic Bezier SVG path
    return (
        f"M {tx + 50*scale:.2f} {ty + 12*scale:.2f} "
        f"C {tx + 55*scale:.2f} {ty + 12*scale:.2f} {tx + 86*scale:.2f} {ty + 44*scale:.2f} {tx + 86*scale:.2f} {ty + 70*scale:.2f} "
        f"C {tx + 86*scale:.2f} {ty + 86*scale:.2f} {tx + 70*scale:.2f} {ty + 92*scale:.2f} {tx + 50*scale:.2f} {ty + 92*scale:.2f} "
        f"C {tx + 30*scale:.2f} {ty + 92*scale:.2f} {tx + 14*scale:.2f} {ty + 86*scale:.2f} {tx + 14*scale:.2f} {ty + 70*scale:.2f} "
        f"C {tx + 14*scale:.2f} {ty + 44*scale:.2f} {tx + 45*scale:.2f} {ty + 12*scale:.2f} {tx + 50*scale:.2f} {ty + 12*scale:.2f} Z"
    )

def eyes_svg(scale=1.0, tx=0.0, ty=0.0, fill="#FFFFFF", pupil_offset=(0, 0)):
    """SVG markup for Droppy's signature eyes."""
    dx, dy = pupil_offset
    # Left eye: center ~(38, 62), width 9, height 17
    # Right eye: center ~(62, 62), width 9, height 17
    w = 8.5 * scale
    h = 16.0 * scale
    r = w / 2.0
    lx = tx + (38 - 4.25 + dx) * scale
    ly = ty + (62 - 8.0 + dy) * scale
    rx = tx + (62 - 4.25 + dx) * scale
    ry = ty + (62 - 8.0 + dy) * scale
    return (
        f'<rect x="{lx:.2f}" y="{ly:.2f}" width="{w:.2f}" height="{h:.2f}" rx="{r:.2f}" fill="{fill}"/>\n'
        f'<rect x="{rx:.2f}" y="{ry:.2f}" width="{w:.2f}" height="{h:.2f}" rx="{r:.2f}" fill="{fill}"/>'
    )

def teardrop_mark_svg(color="#0284C7", eye_color="#FFFFFF", pupil_offset=(0,0)):
    """Standalone 2D Mascot Mark (Droppy)."""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">\n'
        f'  <path d="{droplet_path_d()}" fill="{color}"/>\n'
        f'  {eyes_svg(1.0, 0, 0, eye_color, pupil_offset)}\n'
        '</svg>\n'
    )

def teardrop_mark_white_svg():
    """White mascot mark with transparent eye cutouts using even-odd fill rule."""
    path = droplet_path_d()
    # Left eye path
    # rect x=33.75, y=54, w=8.5, h=16, r=4.25
    def capsule_path(cx, cy, w, h):
        r = w / 2
        x0, y0 = cx - r, cy - h/2
        x1, y1 = cx + r, cy + h/2
        return (
            f"M {x0} {y0+r} A {r} {r} 0 0 1 {x1} {y0+r} "
            f"L {x1} {y1-r} A {r} {r} 0 0 1 {x0} {y1-r} Z"
        )
    le = capsule_path(38, 62, 8.5, 16)
    re = capsule_path(62, 62, 8.5, 16)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">\n'
        f'  <path d="{path} {le} {re}" fill="#FFFFFF" fill-rule="evenodd"/>\n'
        '</svg>\n'
    )

# ---------------------------------------------------------------------------
# 2. Island Geometry (Top edge hanging notch)
# ---------------------------------------------------------------------------
def island_points(cx: float, y0: float, w: float, h: float, r: float, f: float) -> str:
    """Path data for the island: flat top flush on edge, flared corners, rounded bottom."""
    x0, x1 = cx - w / 2, cx + w / 2
    return (
        f"M{x0 - f:.1f} {y0:.1f} "
        f"Q{x0:.1f} {y0:.1f} {x0:.1f} {y0 + f:.1f} "
        f"L{x0:.1f} {y0 + h - r:.1f} "
        f"Q{x0:.1f} {y0 + h:.1f} {x0 + r:.1f} {y0 + h:.1f} "
        f"L{x1 - r:.1f} {y0 + h:.1f} "
        f"Q{x1:.1f} {y0 + h:.1f} {x1:.1f} {y0 + h - r:.1f} "
        f"L{x1:.1f} {y0 + f:.1f} "
        f"Q{x1:.1f} {y0:.1f} {x1 + f:.1f} {y0:.1f} Z"
    )

# ---------------------------------------------------------------------------
# 3. 2D App Logo SVG (Droppy Centered - No Notch)
# ---------------------------------------------------------------------------
def logo_svg(size=1024.0) -> str:
    s = float(size)
    inset = s * 0.047
    tile_size = s - 2 * inset
    tile_r = tile_size * 0.2245

    # Center white Droppy mascot inside the squircle tile
    mascot_w = tile_size * 0.72
    mascot_scale = mascot_w / 100.0
    mascot_tx = (s - mascot_w) / 2.0
    mascot_ty = (s - mascot_w) / 2.0

    d_path = droplet_path_d(mascot_scale, mascot_tx, mascot_ty)
    
    # White mark with transparent eyes:
    def capsule_path(cx, cy, w, h):
        r = w / 2
        x0, y0 = cx - r, cy - h/2
        x1, y1 = cx + r, cy + h/2
        return (
            f"M {x0:.2f} {y0+r:.2f} A {r:.2f} {r:.2f} 0 0 1 {x1:.2f} {y0+r:.2f} "
            f"L {x1:.2f} {y1-r:.2f} A {r:.2f} {r:.2f} 0 0 1 {x0:.2f} {y1-r:.2f} Z"
        )
    le = capsule_path(mascot_tx + 38 * mascot_scale, mascot_ty + 62 * mascot_scale, 8.5 * mascot_scale, 16 * mascot_scale)
    re = capsule_path(mascot_tx + 62 * mascot_scale, mascot_ty + 62 * mascot_scale, 8.5 * mascot_scale, 16 * mascot_scale)

    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s:g} {s:g}">\n'
        f'<title>Droppy</title>\n'
        f'<defs>\n'
        f'  <clipPath id="tile">\n'
        f'    <rect x="{inset:.1f}" y="{inset:.1f}" width="{tile_size:.1f}" height="{tile_size:.1f}" rx="{tile_r:.1f}"/>\n'
        f'  </clipPath>\n'
        f'  <linearGradient id="tile_bg" x1="0" y1="0" x2="0" y2="1">\n'
        f'    <stop offset="0" stop-color="{BRAND_LIGHT}"/>\n'
        f'    <stop offset="0.3" stop-color="{BRAND_COLOR}"/>\n'
        f'    <stop offset="1" stop-color="{BRAND_DARK}"/>\n'
        f'  </linearGradient>\n'
        f'</defs>\n'
        f'<g clip-path="url(#tile)">\n'
        f'  <rect x="0" y="0" width="{s:g}" height="{s:g}" fill="url(#tile_bg)"/>\n'
        f'</g>\n'
        f'<path d="{d_path} {le} {re}" fill="#FFFFFF" fill-rule="evenodd"/>\n'
        f'</svg>\n'
    )

# ---------------------------------------------------------------------------
# 4. Rasterize using Pillow for crisp PNG & ICO exports
# ---------------------------------------------------------------------------
def draw_logo_raster(size: int) -> Image.Image:
    """Draws pixel-perfect anti-aliased logo raster at given size using supersampling."""
    scale = 4
    s = size * scale
    inset = s * 0.047
    t = s - 2 * inset
    tile_r = t * 0.2245

    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Gradient background tile
    grad = Image.new("RGBA", (1, int(t)), (0, 0, 0, 0))
    c_top = (56, 189, 248)    # #38BDF8
    c_mid = (2, 132, 199)     # #0284C7
    c_bot = (3, 105, 161)     # #0369A1
    for y in range(int(t)):
        ratio = y / max(1, t - 1)
        if ratio < 0.3:
            sub = ratio / 0.3
            r = int(c_top[0] + (c_mid[0] - c_top[0]) * sub)
            g = int(c_top[1] + (c_mid[1] - c_top[1]) * sub)
            b = int(c_top[2] + (c_mid[2] - c_top[2]) * sub)
        else:
            sub = (ratio - 0.3) / 0.7
            r = int(c_mid[0] + (c_bot[0] - c_mid[0]) * sub)
            g = int(c_mid[1] + (c_bot[1] - c_mid[1]) * sub)
            b = int(c_bot[2] + (c_bot[2] - c_mid[2]) * sub)
        grad.putpixel((0, y), (r, g, b, 255))
    grad = grad.resize((int(t), int(t)), Image.BILINEAR)

    # Tile mask
    mask = Image.new("L", (int(t), int(t)), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, t - 1, t - 1], radius=tile_r, fill=255)
    img.paste(grad, (round(inset), round(inset)), mask)

    # Center white Droppy mascot
    mascot_w = t * 0.72
    m_scale = mascot_w / 100.0
    m_tx = (s - mascot_w) / 2.0
    m_ty = (s - mascot_w) / 2.0

    def bezier_point(p0, p1, p2, p3, steps=24):
        pts = []
        for st in range(steps + 1):
            u = st / steps
            x = (1-u)**3 * p0[0] + 3*(1-u)**2*u * p1[0] + 3*(1-u)*u**2 * p2[0] + u**3 * p3[0]
            y = (1-u)**3 * p0[1] + 3*(1-u)**2*u * p1[1] + 3*(1-u)*u**2 * p2[1] + u**3 * p3[1]
            pts.append((m_tx + x * m_scale, m_ty + y * m_scale))
        return pts

    d_pts = []
    d_pts.extend(bezier_point((50, 12), (55, 12), (86, 44), (86, 70)))
    d_pts.extend(bezier_point((86, 70), (86, 86), (70, 92), (50, 92)))
    d_pts.extend(bezier_point((50, 92), (30, 92), (14, 86), (14, 70)))
    d_pts.extend(bezier_point((14, 70), (14, 44), (45, 12), (50, 12)))

    drop_mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(drop_mask).polygon(d_pts, fill=255)

    eye_w = 8.5 * m_scale
    eye_h = 16.0 * m_scale
    eye_r = eye_w / 2
    lx = m_tx + (38 - 4.25) * m_scale
    ly = m_ty + (62 - 8.0) * m_scale
    rx = m_tx + (62 - 4.25) * m_scale
    ry = m_ty + (62 - 8.0) * m_scale
    ImageDraw.Draw(drop_mask).rounded_rectangle([lx, ly, lx + eye_w, ly + eye_h], radius=eye_r, fill=0)
    ImageDraw.Draw(drop_mask).rounded_rectangle([rx, ry, rx + eye_w, ry + eye_h], radius=eye_r, fill=0)

    white_layer = Image.new("RGBA", (s, s), (255, 255, 255, 255))
    img.paste(white_layer, (0, 0), drop_mask)

    return img.resize((size, size), Image.LANCZOS)
    # Sample cubic Beziers
    def bezier_point(p0, p1, p2, p3, steps=16):
        pts = []
        for st in range(steps + 1):
            u = st / steps
            x = (1-u)**3 * p0[0] + 3*(1-u)**2*u * p1[0] + 3*(1-u)*u**2 * p2[0] + u**3 * p3[0]
            y = (1-u)**3 * p0[1] + 3*(1-u)**2*u * p1[1] + 3*(1-u)*u**2 * p2[1] + u**3 * p3[1]
            pts.append((m_tx + x * m_scale, m_ty + y * m_scale))
        return pts

    # Right side
    d_pts.extend(bezier_point((50, 12), (55, 12), (86, 44), (86, 70)))
    d_pts.extend(bezier_point((86, 70), (86, 86), (70, 92), (50, 92)))
    # Left side
    d_pts.extend(bezier_point((50, 92), (30, 92), (14, 86), (14, 70)))
    d_pts.extend(bezier_point((14, 70), (14, 44), (45, 12), (50, 12)))

    # Droplet mask
    drop_mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(drop_mask).polygon(d_pts, fill=255)

    # Cut out eyes from the droplet mask
    eye_w = 8.5 * m_scale
    eye_h = 16.0 * m_scale
    eye_r = eye_w / 2
    lx = m_tx + (38 - 4.25) * m_scale
    ly = m_ty + (62 - 8.0) * m_scale
    rx = m_tx + (62 - 4.25) * m_scale
    ry = m_ty + (62 - 8.0) * m_scale
    ImageDraw.Draw(drop_mask).rounded_rectangle([lx, ly, lx + eye_w, ly + eye_h], radius=eye_r, fill=0)
    ImageDraw.Draw(drop_mask).rounded_rectangle([rx, ry, rx + eye_w, ry + eye_h], radius=eye_r, fill=0)

    # Paste white Droppy onto icon
    white_layer = Image.new("RGBA", (s, s), (255, 255, 255, 255))
    img.paste(white_layer, (0, 0), drop_mask)

    return img.resize((size, size), Image.LANCZOS)


# ---------------------------------------------------------------------------
# 5. Page Mascot Poses (2D Droppy in Action)
# ---------------------------------------------------------------------------
def pose_svg(title: str, prop_svg: str, eye_offset=(0, 0)) -> str:
    """Generate a cute 2D pose with Droppy interacting with a page prop."""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 160">\n'
        f'<title>Droppy - {title}</title>\n'
        f'<defs>\n'
        f'  <linearGradient id="body_grad" x1="0" y1="0" x2="0" y2="1">\n'
        f'    <stop offset="0" stop-color="{BRAND_LIGHT}"/>\n'
        f'    <stop offset="1" stop-color="{BRAND_COLOR}"/>\n'
        f'  </linearGradient>\n'
        f'</defs>\n'
        # Droppy Body centered at 80, 85, scale 1.25 -> tx=17.5, ty=22.5
        f'<g transform="translate(17.5, 22.5) scale(1.25)">\n'
        f'  <path d="{droplet_path_d()}" fill="url(#body_grad)"/>\n'
        f'  {eyes_svg(1.0, 0, 0, "#FFFFFF", eye_offset)}\n'
        f'</g>\n'
        f'{prop_svg}\n'
        f'</svg>\n'
    )

def generate_poses():
    # 1. Music: Droppy wearing sleek headphones & music notes
    music_prop = (
        '<!-- Headphones -->\n'
        '<path d="M 36 85 A 44 44 0 0 1 124 85" fill="none" stroke="#0F172A" stroke-width="6" stroke-linecap="round"/>\n'
        '<rect x="30" y="80" width="12" height="26" rx="6" fill="#0F172A"/>\n'
        '<rect x="118" y="80" width="12" height="26" rx="6" fill="#0F172A"/>\n'
        '<!-- Floating musical notes -->\n'
        '<path d="M 125 40 L 135 36 L 135 50 M 125 40 L 125 54 M 120 54 A 5 4 0 1 1 125 50 M 130 50 A 5 4 0 1 1 135 46" fill="#F43F5E" stroke="#F43F5E" stroke-width="2"/>\n'
    )
    (POSES / "droppy-music.svg").write_text(pose_svg("Music", music_prop, (1, -1)), encoding="utf-8")

    # 2. Focus: Pomodoro timer ring
    focus_prop = (
        '<!-- Focus Timer Badge -->\n'
        '<g transform="translate(105, 95)">\n'
        '  <circle cx="20" cy="20" r="18" fill="#FFFFFF" stroke="#F97316" stroke-width="4"/>\n'
        '  <circle cx="20" cy="20" r="14" fill="none" stroke="#FDBA74" stroke-width="3" stroke-dasharray="60 30"/>\n'
        '  <line x1="20" y1="20" x2="20" y2="10" stroke="#EA580C" stroke-width="3" stroke-linecap="round"/>\n'
        '  <line x1="20" y1="20" x2="27" y2="20" stroke="#EA580C" stroke-width="3" stroke-linecap="round"/>\n'
        '</g>\n'
    )
    (POSES / "droppy-focus.svg").write_text(pose_svg("Focus", focus_prop, (2, 2)), encoding="utf-8")

    # 3. Water: Glass with water ripples
    water_prop = (
        '<!-- Water Glass -->\n'
        '<g transform="translate(100, 95)">\n'
        '  <path d="M 6 4 L 10 32 Q 10 36 14 36 L 26 36 Q 30 36 30 32 L 34 4 Z" fill="#E0F2FE" stroke="#0284C7" stroke-width="3"/>\n'
        '  <path d="M 8 14 Q 20 18 32 14 L 30 32 Q 30 36 26 36 L 14 36 Q 10 36 10 32 Z" fill="#38BDF8"/>\n'
        '</g>\n'
    )
    (POSES / "droppy-water.svg").write_text(pose_svg("Water", water_prop, (2, 2)), encoding="utf-8")

    # 4. Shelf: File Folder
    shelf_prop = (
        '<!-- File Folder -->\n'
        '<g transform="translate(98, 98)">\n'
        '  <path d="M 2 8 L 14 8 L 18 12 L 36 12 Q 38 12 38 14 L 38 32 Q 38 34 36 34 L 2 34 Q 0 34 0 32 L 0 10 Q 0 8 2 8 Z" fill="#8B5CF6"/>\n'
        '  <rect x="6" y="4" width="22" height="14" rx="2" fill="#FFFFFF"/>\n'
        '  <path d="M 0 14 L 38 14 L 36 34 L 2 34 Z" fill="#A78BFA"/>\n'
        '</g>\n'
    )
    (POSES / "droppy-shelf.svg").write_text(pose_svg("Shelf", shelf_prop, (2, 2)), encoding="utf-8")

    # 5. Tasks: Checklist
    tasks_prop = (
        '<!-- Checklist Tile -->\n'
        '<g transform="translate(100, 95)">\n'
        '  <rect x="0" y="0" width="34" height="40" rx="4" fill="#FFFFFF" stroke="#22C55E" stroke-width="3"/>\n'
        '  <path d="M 6 12 L 10 16 L 18 8" fill="none" stroke="#22C55E" stroke-width="2.5" stroke-linecap="round"/>\n'
        '  <line x1="22" y1="12" x2="28" y2="12" stroke="#94A3B8" stroke-width="2.5" stroke-linecap="round"/>\n'
        '  <path d="M 6 24 L 10 28 L 18 20" fill="none" stroke="#22C55E" stroke-width="2.5" stroke-linecap="round"/>\n'
        '  <line x1="22" y1="24" x2="28" y2="24" stroke="#94A3B8" stroke-width="2.5" stroke-linecap="round"/>\n'
        '</g>\n'
    )
    (POSES / "droppy-tasks.svg").write_text(pose_svg("Tasks", tasks_prop, (2, 2)), encoding="utf-8")

    # 6. Today: Calendar / Clock
    today_prop = (
        '<!-- Calendar -->\n'
        '<g transform="translate(100, 95)">\n'
        '  <rect x="0" y="4" width="36" height="34" rx="4" fill="#FFFFFF" stroke="#3B82F6" stroke-width="3"/>\n'
        '  <rect x="0" y="4" width="36" height="10" rx="2" fill="#3B82F6"/>\n'
        '  <line x1="8" y1="0" x2="8" y2="6" stroke="#1D4ED8" stroke-width="3" stroke-linecap="round"/>\n'
        '  <line x1="28" y1="0" x2="28" y2="6" stroke="#1D4ED8" stroke-width="3" stroke-linecap="round"/>\n'
        '  <circle cx="18" cy="24" r="5" fill="#EF4444"/>\n'
        '</g>\n'
    )
    (POSES / "droppy-today.svg").write_text(pose_svg("Today", today_prop, (2, 2)), encoding="utf-8")


def main():
    print("Generating 2D brand files for Droppy...")
    BRAND.mkdir(parents=True, exist_ok=True)
    POSES.mkdir(parents=True, exist_ok=True)
    RES.mkdir(parents=True, exist_ok=True)
    DOCS_IMG.mkdir(parents=True, exist_ok=True)

    # 1. 2D Mascot Mark (Droppy)
    (BRAND / "teardrop-mark.svg").write_text(teardrop_mark_svg(BRAND_COLOR, "#FFFFFF"), encoding="utf-8")
    (BRAND / "teardrop-mark-white.svg").write_text(teardrop_mark_white_svg(), encoding="utf-8")
    (BRAND / "teardrop-mark-dark.svg").write_text(teardrop_mark_svg("#0F172A", "#38BDF8"), encoding="utf-8")
    print("✓ Wrote teardrop-mark.svg, teardrop-mark-white.svg, teardrop-mark-dark.svg")

    # Render 2D mascot raster
    def draw_mascot_raster(size=512, bg_transparent=True):
        scale = 4
        s = size * scale
        img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
        m_w = s * 0.76
        m_scale = m_w / 100.0
        m_tx = (s - m_w) / 2.0
        m_ty = (s - m_w) / 2.0

        def bezier_pts(p0, p1, p2, p3, steps=24):
            pts = []
            for st in range(steps + 1):
                u = st / steps
                x = (1-u)**3 * p0[0] + 3*(1-u)**2*u * p1[0] + 3*(1-u)*u**2 * p2[0] + u**3 * p3[0]
                y = (1-u)**3 * p0[1] + 3*(1-u)**2*u * p1[1] + 3*(1-u)*u**2 * p2[1] + u**3 * p3[1]
                pts.append((m_tx + x * m_scale, m_ty + y * m_scale))
            return pts

        d_pts = []
        d_pts.extend(bezier_pts((50, 12), (55, 12), (86, 44), (86, 70)))
        d_pts.extend(bezier_pts((86, 70), (86, 86), (70, 92), (50, 92)))
        d_pts.extend(bezier_pts((50, 92), (30, 92), (14, 86), (14, 70)))
        d_pts.extend(bezier_pts((14, 70), (14, 44), (45, 12), (50, 12)))

        drop_mask = Image.new("L", (s, s), 0)
        ImageDraw.Draw(drop_mask).polygon(d_pts, fill=255)

        # Gradient fill for droplet body
        grad = Image.new("RGBA", (1, s), (0, 0, 0, 0))
        for y in range(s):
            v = y / max(1, s - 1)
            cr = int(56 * (1 - v) + 2 * v)
            cg = int(189 * (1 - v) + 132 * v)
            cb = int(248 * (1 - v) + 199 * v)
            grad.putpixel((0, y), (cr, cg, cb, 255))
        grad = grad.resize((s, s), Image.BILINEAR)
        img.paste(grad, (0, 0), drop_mask)

        # Eyes: white pills
        draw = ImageDraw.Draw(img)
        eye_w = 8.5 * m_scale
        eye_h = 16.0 * m_scale
        eye_r = eye_w / 2
        lx = m_tx + (38 - 4.25) * m_scale
        ly = m_ty + (62 - 8.0) * m_scale
        rx = m_tx + (62 - 4.25) * m_scale
        ry = m_ty + (62 - 8.0) * m_scale
        draw.rounded_rectangle([lx, ly, lx + eye_w, ly + eye_h], radius=eye_r, fill=(255, 255, 255, 255))
        draw.rounded_rectangle([rx, ry, rx + eye_w, ry + eye_h], radius=eye_r, fill=(255, 255, 255, 255))

        return img.resize((size, size), Image.LANCZOS)

    mascot_512 = draw_mascot_raster(512)
    mascot_512.save(BRAND / "teardrop-mascot-512.png", optimize=True)
    mascot_512.save(DOCS_IMG / "teardrop-mascot-512.png", optimize=True)
    mascot_512.save(ROOT / "assets" / "teardrop-mascot-2d.png", optimize=True)
    print("✓ Wrote teardrop-mascot-512.png")

    # 2. 2D App Logo / Icon SVG
    (BRAND / "teardrop-logo.svg").write_text(logo_svg(1024), encoding="utf-8")
    print("✓ Wrote teardrop-logo.svg (1024x1024)")

    # 3. 2D Poses
    generate_poses()
    print("✓ Wrote 6 Droppy poses in docs/brand/art/poses/")

    # 4. Rasterize PNGs and Windows ICO
    logo_512 = draw_logo_raster(512)
    logo_512.save(BRAND / "teardrop-logo-512.png", optimize=True)
    logo_512.save(DOCS_IMG / "teardrop-logo-512.png", optimize=True)
    print("✓ Wrote teardrop-logo-512.png")

    icons = {n: draw_logo_raster(n) for n in ICO_SIZES}
    ico_path = RES / "teardrop.ico"
    icons[256].save(ico_path, format="ICO", sizes=[(n, n) for n in ICO_SIZES], append_images=[icons[n] for n in ICO_SIZES[:-1]])
    print(f"✓ Wrote Windows app/tray icon {ico_path} with sizes: {ICO_SIZES}")

    # Copy files to assets/
    assets_dir = ROOT / "assets"
    assets_dir.mkdir(exist_ok=True)
    logo_512.save(assets_dir / "teardrop-icon-2d.png", optimize=True)
    (assets_dir / "teardrop-logo.svg").write_text(logo_svg(1024), encoding="utf-8")
    (assets_dir / "teardrop-mark.svg").write_text(teardrop_mark_svg(BRAND_COLOR, "#FFFFFF"), encoding="utf-8")
    print("✓ Copied branding assets to assets/")

    # 5. Installer Artwork (Inno Setup & NSIS)
    installer_art = ROOT / "installer" / "art"
    installer_art.mkdir(parents=True, exist_ok=True)

    wizard_small_sizes = [58, 77, 97, 116, 124, 143, 159]
    for n in wizard_small_sizes:
        draw_logo_raster(n).save(installer_art / f"wizard-small-{n}.png", optimize=True)

    def make_wizard_banner(w: int, h: int) -> Image.Image:
        # Subtle light blue / cyan modern gradient
        banner = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        for y in range(h):
            ratio = y / max(1, h - 1)
            r = int(240 * (1 - ratio) + 224 * ratio)
            g = int(249 * (1 - ratio) + 242 * ratio)
            b = int(255 * (1 - ratio) + 254 * ratio)
            for x in range(w):
                banner.putpixel((x, y), (r, g, b, 255))
        # Place Droppy mascot in upper-middle
        mw = round(w * 0.72)
        m_img = draw_mascot_raster(mw)
        mx = (w - mw) // 2
        my = round(h * 0.44 - mw // 2)
        banner.alpha_composite(m_img, (mx, my))
        return banner

    wizard_sizes = [(202, 386), (336, 643), (430, 824)]
    for w, h in wizard_sizes:
        make_wizard_banner(w, h).convert("RGB").save(installer_art / f"wizard-{w}x{h}.png", optimize=True)

    # NSIS bitmaps (24-bit BMP)
    make_wizard_banner(164, 314).convert("RGB").save(installer_art / "nsis-welcome.bmp")
    nsis_hdr = Image.new("RGBA", (150, 57), (255, 255, 255, 255))
    logo48 = draw_logo_raster(48)
    nsis_hdr.alpha_composite(logo48, (150 - 48 - 6, (57 - 48) // 2))
    nsis_hdr.convert("RGB").save(installer_art / "nsis-header.bmp")

    # Inno Setup BMPs (compatible with all versions of Inno Setup)
    make_wizard_banner(164, 314).convert("RGB").save(installer_art / "wizard.bmp")
    small_hdr = Image.new("RGBA", (55, 55), (255, 255, 255, 255))
    small_hdr.alpha_composite(logo48, ((55 - 48) // 2, (55 - 48) // 2))
    small_hdr.convert("RGB").save(installer_art / "wizard-small.bmp")
    print("✓ Wrote installer artwork in installer/art/")

if __name__ == "__main__":
    main()
