"""Drawing for trailer03 with cairo: the compass/helm logo, the hook and outro animations, captions, key caps, sonar rings.

Everything is drawn in 4K units and scaled by Q, so a preview can be rendered at a quarter of the size."""
import math, os
import cairo

TAU = math.tau
NAVY = (11 / 255, 29 / 255, 46 / 255)
DEEP = (7 / 255, 19 / 255, 31 / 255)
MID = (18 / 255, 48 / 255, 74 / 255)
TEAL = (31 / 255, 166 / 255, 160 / 255)
BRASS = (201 / 255, 162 / 255, 75 / 255)
FOAM = (234 / 255, 244 / 255, 244 / 255)
SANS = "Noto Sans"
MONO = "JetBrainsMono Nerd Font"

def smooth(u):
    u = max(0.0, min(1.0, u))
    return u * u * (3 - 2 * u)

def new_surface(w, h):
    s = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
    return s, cairo.Context(s)

def rgba(c, a=1.0):
    return (c[0], c[1], c[2], a)

def font(ctx, family, size, bold=False):
    ctx.select_font_face(family, cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD if bold else cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(size)

def text_width(ctx, s, spacing=0.0):
    return ctx.text_extents(s).x_advance + spacing * max(0, len(s) - 1)

def draw_text(ctx, s, x, y, color, anchor="center", spacing=0.0):
    """Text with its baseline at y; anchor is left, center or right of x. Letter spacing in pixels."""
    w = text_width(ctx, s, spacing)
    x0 = x - w / 2 if anchor == "center" else (x - w if anchor == "right" else x)
    ctx.set_source_rgba(*color)
    if spacing == 0:
        ctx.move_to(x0, y); ctx.show_text(s)
        return
    for ch in s:
        ctx.move_to(x0, y); ctx.show_text(ch)
        x0 += ctx.text_extents(ch).x_advance + spacing

def rounded(ctx, x, y, w, h, r):
    ctx.new_sub_path()
    ctx.arc(x + w - r, y + r, r, -TAU / 4, 0)
    ctx.arc(x + w - r, y + h - r, r, 0, TAU / 4)
    ctx.arc(x + r, y + h - r, r, TAU / 4, TAU / 2)
    ctx.arc(x + r, y + r, r, TAU / 2, 3 * TAU / 4)
    ctx.close_path()

# ---- the logo --------------------------------------------------------------------------------------------------

def draw_rose(ctx, cx, cy, R, rot, alpha=1.0):
    """Compass rose: a brass north point, teal east, south and west points, small diagonal points, a hub."""
    ctx.save()
    ctx.translate(cx, cy)
    ctx.rotate(rot)
    # small points first, so the big ones sit on top
    for k in range(4):
        a = TAU / 8 + k * TAU / 4
        ctx.save(); ctx.rotate(a)
        ctx.move_to(0, -0.46 * R); ctx.line_to(0.07 * R, -0.1 * R); ctx.line_to(-0.07 * R, -0.1 * R); ctx.close_path()
        ctx.set_source_rgba(*rgba(TEAL, 0.45 * alpha)); ctx.fill()
        ctx.restore()
    for k, (length, color) in enumerate(((0.74, BRASS), (0.58, TEAL), (0.58, TEAL), (0.58, TEAL))):
        ctx.save(); ctx.rotate(k * TAU / 4)
        # two halves in two tones give the point a fold
        ctx.move_to(0, -length * R); ctx.line_to(0.13 * R, -0.12 * R); ctx.line_to(0, -0.05 * R); ctx.close_path()
        ctx.set_source_rgba(*rgba(color, alpha)); ctx.fill()
        ctx.move_to(0, -length * R); ctx.line_to(-0.13 * R, -0.12 * R); ctx.line_to(0, -0.05 * R); ctx.close_path()
        ctx.set_source_rgba(color[0] * 0.62, color[1] * 0.62, color[2] * 0.62, alpha); ctx.fill()
        ctx.restore()
    ctx.restore()
    ctx.arc(cx, cy, 0.085 * R, 0, TAU)
    ctx.set_source_rgba(*rgba(NAVY, alpha)); ctx.fill_preserve()
    ctx.set_source_rgba(*rgba(BRASS, alpha)); ctx.set_line_width(0.028 * R); ctx.stroke()

def draw_helm(ctx, cx, cy, R, rot, alpha=1.0):
    """The helm wheel: a brass rim, a teal inner ring, eight spokes ending in handles."""
    ctx.save()
    ctx.translate(cx, cy)
    ctx.rotate(rot)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    for k in range(8):
        ctx.save(); ctx.rotate(k * TAU / 8)
        ctx.move_to(0, -0.80 * R); ctx.line_to(0, -1.24 * R)
        ctx.set_source_rgba(*rgba(BRASS, alpha)); ctx.set_line_width(0.085 * R); ctx.stroke()
        ctx.arc(0, -1.31 * R, 0.085 * R, 0, TAU)
        ctx.set_source_rgba(*rgba(BRASS, alpha)); ctx.fill()
        ctx.restore()
    ctx.arc(0, 0, R, 0, TAU)
    ctx.set_source_rgba(*rgba(BRASS, alpha)); ctx.set_line_width(0.075 * R); ctx.stroke()
    ctx.arc(0, 0, 0.84 * R, 0, TAU)
    ctx.set_source_rgba(*rgba(TEAL, 0.9 * alpha)); ctx.set_line_width(0.022 * R); ctx.stroke()
    ctx.restore()

def draw_logo(ctx, cx, cy, R, rose_rot=0.0, helm_rot=0.0, alpha=1.0):
    draw_helm(ctx, cx, cy, R, helm_rot, alpha)
    # North tick inside the rim: the rose locks onto it.
    ctx.move_to(cx, cy - 0.78 * R); ctx.line_to(cx - 0.05 * R, cy - 0.88 * R); ctx.line_to(cx + 0.05 * R, cy - 0.88 * R); ctx.close_path()
    ctx.set_source_rgba(*rgba(FOAM, alpha)); ctx.fill()
    draw_rose(ctx, cx, cy, R * 0.92, rose_rot, alpha)

# ---- backgrounds ----------------------------------------------------------------------------------------------

def draw_background(ctx, W, H, glow=0.0, gx=0.5, gy=0.4):
    g = cairo.LinearGradient(0, 0, 0, H)
    g.add_color_stop_rgb(0, *NAVY); g.add_color_stop_rgb(1, *DEEP)
    ctx.set_source(g); ctx.paint()
    if glow > 0:
        r = cairo.RadialGradient(W * gx, H * gy, 0, W * gx, H * gy, H * 0.62)
        r.add_color_stop_rgba(0, *rgba(TEAL, 0.22 * glow)); r.add_color_stop_rgba(1, *rgba(TEAL, 0))
        ctx.set_source(r); ctx.paint()

def draw_waves(ctx, W, H, t, amp=1.0, base=0.80, count=5, alpha=0.5, speed=1.0):
    """Layered sine waves along the bottom; `amp` scales how choppy they are."""
    for i in range(count):
        y0 = H * (base + 0.032 * i)
        a = H * 0.020 * amp * (1 - 0.1 * i)
        ctx.move_to(0, y0)
        steps = 160
        for s in range(steps + 1):
            x = W * s / steps
            y = y0 + a * math.sin(x / W * TAU * (1.4 + 0.35 * i) + t * speed * (0.9 + 0.25 * i) + i * 1.3)
            ctx.line_to(x, y)
        ctx.set_source_rgba(*rgba(TEAL if i % 2 == 0 else MID, alpha * (0.9 - 0.14 * i)))
        ctx.set_line_width(H * (0.0045 - 0.0004 * i))
        ctx.stroke()

def wordmark(ctx, cx, y, size, alpha=1.0):
    font(ctx, SANS, size, True)
    a, b = "Captain", "FM"
    wa, wb = text_width(ctx, a), text_width(ctx, b)
    x0 = cx - (wa + wb) / 2
    draw_text(ctx, a, x0, y, rgba(FOAM, alpha), "left")
    draw_text(ctx, b, x0 + wa, y, rgba(TEAL, alpha), "left")

# ---- animated sections -------------------------------------------------------------------------------------

def hook_frame(ctx, W, H, t):
    Q = W / 3840
    draw_background(ctx, W, H, glow=smooth(t / 2.0), gy=0.37)
    draw_waves(ctx, W, H, t * 1.2, amp=0.7 + 0.5 * smooth((t - 2.0) / 0.4) * 0, base=0.84, alpha=0.40 * smooth(t / 1.0))
    R = H * 0.19
    cx, cy = W / 2, H * 0.37
    lock = 2.0
    p = min(1.0, t / lock)
    rose = TAU * 2.5 * (1 - p) ** 3
    helm = -TAU * 0.45 * (1 - p) ** 3
    if t > lock:
        wob = math.exp(-7.0 * (t - lock))
        rose += 0.16 * math.sin(26 * (t - lock)) * wob
        helm += 0.05 * math.sin(20 * (t - lock)) * wob
    fade = smooth(t / 0.35)
    draw_logo(ctx, cx, cy, R, rose, helm, fade)
    # a brass ring spreads from the helm when the compass locks on north
    if t > lock:
        u = (t - lock) / 0.9
        if u < 1:
            ctx.arc(cx, cy, R * (1.0 + 1.5 * smooth(u)), 0, TAU)
            ctx.set_source_rgba(*rgba(BRASS, 0.55 * (1 - u))); ctx.set_line_width(H * 0.006 * (1 - u) + 1); ctx.stroke()
    # wordmark and tagline
    u = smooth((t - 2.15) / 0.5)
    if u > 0:
        wordmark(ctx, cx, H * 0.725 + H * 0.02 * (1 - u), H * 0.092, u)
    u = smooth((t - 2.75) / 0.5)
    if u > 0:
        font(ctx, MONO, H * 0.034)
        draw_text(ctx, "Take the helm of your files.", cx, H * 0.815 + H * 0.012 * (1 - u), rgba(BRASS, u), "center", spacing=H * 0.0022)

def outro_frame(ctx, W, H, t):
    draw_background(ctx, W, H, glow=1.0, gy=0.34)
    # the sea settles: big swells at first, then a calm line
    amp = 0.35 + 3.2 * math.exp(-t / 0.75)
    draw_waves(ctx, W, H, t * 1.6 * math.exp(-t / 2.5) + 0.8, amp=amp, base=0.86, alpha=0.55, speed=1.0)
    R = H * 0.17
    cx, cy = W / 2, H * 0.30
    draw_logo(ctx, cx, cy, R, 0.0, 0.0, 1.0)
    u = smooth((t - 0.15) / 0.5)
    wordmark(ctx, cx, H * 0.600 + H * 0.015 * (1 - u), H * 0.095, u)
    u = smooth((t - 0.45) / 0.5)
    font(ctx, MONO, H * 0.032)
    draw_text(ctx, "github.com/jgarza9788/CaptainFM", cx, H * 0.680 + H * 0.01 * (1 - u), rgba(TEAL, u), "center", spacing=H * 0.0015)
    caption(ctx, W, H, "Chart your course.", None, 0.84, smooth((t - 0.9) / 0.4), center_x=cx)

# ---- captions and overlays ----------------------------------------------------------------------------------

def caption(ctx, W, H, text, tag, cy_frac, alpha=1.0, center_x=None):
    """A navy pill with a teal edge and a brass anchor line; `tag` is a small mono label above the text."""
    size = H * 0.050
    font(ctx, SANS, size, True)
    tw = text_width(ctx, text)
    padx, pady = H * 0.040, H * 0.026
    w, h = tw + 2 * padx + H * 0.03, size * 1.2 + 2 * pady
    cx = center_x if center_x is not None else W / 2
    x, y = cx - w / 2, H * cy_frac - h / 2
    rounded(ctx, x, y, w, h, H * 0.018)
    ctx.set_source_rgba(*rgba(NAVY, 0.90 * alpha)); ctx.fill_preserve()
    ctx.set_source_rgba(*rgba(TEAL, 0.85 * alpha)); ctx.set_line_width(H * 0.0028); ctx.stroke()
    ctx.rectangle(x + H * 0.018, y + h * 0.22, H * 0.0065, h * 0.56)
    ctx.set_source_rgba(*rgba(BRASS, alpha)); ctx.fill()
    font(ctx, SANS, size, True)
    draw_text(ctx, text, cx + H * 0.012, y + h / 2 + size * 0.34, rgba(FOAM, alpha), "center")
    if tag:
        font(ctx, MONO, H * 0.0185)
        draw_text(ctx, tag, x + H * 0.04, y - H * 0.012, rgba(BRASS, alpha), "left", spacing=H * 0.0016)

def caption_png(path, W, H, text, tag):
    s, c = new_surface(W, H)
    caption(c, W, H, text, tag, 0.915)
    s.write_to_png(path)

KEY_LABELS = {
    "<Down>": ["↓"], "<Up>": ["↑"], "<Left>": ["←"], "<Right>": ["→"], "<Tab>": ["Tab"], "<CR>": ["Enter"], "!accept": ["Enter"],
    "<A-Up>": ["Alt", "↑"], "<S-Down>": ["Shift", "↓"], "<C-\\>": ["Ctrl", "\\"], "<C-Tab>": ["Ctrl", "Tab"],
}

def key_label(token):
    if token in KEY_LABELS:
        return KEY_LABELS[token]
    if token.startswith("<") and token.endswith(">"):
        body = token[1:-1]
        parts = body.split("-", 1)
        names = {"C": "Ctrl", "A": "Alt", "S": "Shift"}
        if len(parts) == 2 and parts[0] in names:
            return [names[parts[0]], parts[1].upper() if len(parts[1]) == 1 else parts[1]]
        return [body]
    return [token]

def keycaps_png(path, W, H, labels):
    """One or two key caps centred near the bottom of a full-size transparent frame."""
    s, c = new_surface(W, H)
    size = H * 0.034
    font(c, MONO, size, True)
    caps = []
    for lab in labels:
        w = max(H * 0.07, text_width(c, lab) + H * 0.04)
        caps.append((lab, w))
    gap = H * 0.022
    total = sum(w for _, w in caps) + gap * (len(caps) - 1)
    x = W / 2 - total / 2
    h = H * 0.07
    y = H * 0.80
    for lab, w in caps:
        rounded(c, x, y + H * 0.006, w, h, H * 0.012)
        c.set_source_rgba(0, 0, 0, 0.35); c.fill()
        rounded(c, x, y, w, h, H * 0.012)
        c.set_source_rgba(*rgba(NAVY, 0.92)); c.fill_preserve()
        c.set_source_rgba(*rgba(BRASS, 0.95)); c.set_line_width(H * 0.0032); c.stroke()
        font(c, MONO, size, True)
        draw_text(c, lab, x + w / 2, y + h / 2 + size * 0.35, rgba(FOAM), "center")
        x += w + gap
    s.write_to_png(path)

def ring_png(path, size):
    """A sonar ring: a thin teal circle with a soft glow, on a transparent square."""
    s, c = new_surface(size, size)
    cx = size / 2
    for width, alpha in ((size * 0.040, 0.10), (size * 0.022, 0.22), (size * 0.010, 0.85)):
        c.arc(cx, cx, size * 0.44, 0, TAU)
        c.set_source_rgba(*rgba(TEAL, alpha)); c.set_line_width(width); c.stroke()
    s.write_to_png(path)

def label_png(path, W, H, text, x_frac, y_frac, color, size_frac=0.034, mono=True):
    s, c = new_surface(W, H)
    font(c, MONO if mono else SANS, H * size_frac, True)
    tw = text_width(c, text)
    x, y = W * x_frac, H * y_frac
    padx, pady = H * 0.022, H * 0.014
    rounded(c, x - tw / 2 - padx, y - H * size_frac * 0.95 - pady, tw + 2 * padx, H * size_frac * 1.35 + 2 * pady, H * 0.012)
    c.set_source_rgba(*rgba(NAVY, 0.88)); c.fill_preserve()
    c.set_source_rgba(*rgba(color, 0.9)); c.set_line_width(H * 0.003); c.stroke()
    draw_text(c, text, x, y, rgba(FOAM), "center")
    s.write_to_png(path)

def render_frames(w, h, fps, seconds, frame_fn):
    """Yields raw BGRA frames from frame_fn(ctx, W, H, t) for seconds*fps frames."""
    for i in range(int(round(seconds * fps))):
        s, c = new_surface(w, h)
        frame_fn(c, w, h, i / fps)
        yield bytes(s.get_data())
