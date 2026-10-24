#!/usr/bin/env python3
"""Builds ~/Videos/CaptainFM_Trailer03.mp4 (4K, 60 fps, 51 s, 120 BPM) from the live scenes that scripts/record-demo03.sh
records, plus the hook and outro drawn with cairo (gfx.py) and a synthesized soundtrack (audio.py).

    python3 scripts/trailer03/make.py                  full render
    PREVIEW=1 python3 scripts/trailer03/make.py        quarter size, 30 fps, to the work folder
    python3 scripts/trailer03/make.py clip search      only one piece (hook, c1..c8, race, outro, trans, audio, final)

Every feature clip is a real recording of CaptainFM; the film adds zooms, captions, key caps, a sonar ring and the
wave-wipe between clips, nothing else."""
import os, subprocess, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gfx
from scenes import Scene

PREVIEW = bool(os.environ.get("PREVIEW"))
W, H, FPS = (960, 540, 30) if PREVIEW else (3840, 2160, 60)
Q = W / 3840
WORK = os.path.expanduser("~/.cache/cfm-trailer03" + ("-preview" if PREVIEW else ""))
OUT = os.path.join(WORK, "preview.mp4") if PREVIEW else os.path.expanduser("~/Videos/CaptainFM_Trailer03.mp4")
MONO_BOLD = "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Bold.ttf"
HANDLE = 0.2          # each clip runs this long past its slot on both sides, for the transition
TRANS = 2 * HANDLE
os.makedirs(WORK, exist_ok=True)

# slot starts and lengths (seconds); a beat is 0.5 s at 120 BPM
SECTIONS = [("hook", 0.0, 4.0)] + [(f"c{i + 1}", 4.0 + 5.0 * i, 5.0) for i in range(8)] + [("race", 44.0, 3.0), ("outro", 47.0, 4.0)]
TOTAL = 51.0

def run(cmd, **kw):
    cmd = [str(c) for c in cmd]
    r = subprocess.run(cmd, **kw)
    if r.returncode != 0:
        raise SystemExit("failed: " + " ".join(cmd[:12]) + " ...")

def ffmpeg(*args, **kw):
    run(["ffmpeg", "-v", "error", "-y", *args], **kw)

X264 = ["-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", "yuv420p", "-g", str(FPS), "-r", str(FPS)]

# ---- expressions --------------------------------------------------------------------------------------------------

def keyframes(keys, idx):
    """An ffmpeg expression in t that glides smoothly through keys = [(t, z, cx, cy)] using component `idx`."""
    expr = f"{keys[-1][idx]:.5f}"
    for a, b in reversed(list(zip(keys, keys[1:]))):
        u = f"clip((t-{a[0]:.4f})/{b[0] - a[0]:.4f}\\,0\\,1)"
        smooth = f"({u}*{u}*(3-2*{u}))"
        expr = f"if(lt(t\\,{b[0]:.4f})\\,{a[idx]:.5f}+({b[idx] - a[idx]:.5f})*{smooth}\\,{expr})"
    return f"if(lt(t\\,{keys[0][0]:.4f})\\,{keys[0][idx]:.5f}\\,{expr})"

class Clip:
    """One feature clip: pieces of a recording joined and sped up, a zoom path, a caption and overlays."""

    def __init__(self, name, caption, tag, pieces, zoom, keys=(), rings=(), keycaps=(), labels=(), watches=(), head=True, tail=True):
        self.name, self.caption, self.tag = name, caption, tag
        self.pieces, self.zoom = pieces, zoom
        self.keycaps, self.rings, self.labels, self.watches = keycaps, rings, labels, watches
        self.head, self.tail = head, tail

    def length(self, slot):
        return (HANDLE if self.head else 0) + slot + (HANDLE if self.tail else 0)

    def to_clip_time(self, src_time):
        """Where a moment of the source recording lands on this clip's own timeline (None if it is not in it)."""
        t0 = 0.0
        for p in self.pieces:
            if p["start"] <= src_time <= p["start"] + p["dur"]:
                return t0 + (src_time - p["start"]) / p["speed"]
            t0 += p["dur"] / p["speed"]
        return None

def wipe_expr():
    """xfade custom expression: the next clip sweeps in from the left behind a wavy, soft edge (P runs 1 -> 0).
    (No st()/ld(): the filter runs on several threads and they would share that variable.)"""
    edge = "((W*1.14)*(1-P)-0.07*W+0.03*W*sin(Y/H*18.85+(1-P)*11))"
    d = f"clip(({edge}-X)/(0.014*W)\\,0\\,1)"
    return f"A*(1-{d})+B*{d}"

# ---- the clips ------------------------------------------------------------------------------------------------------

def scenes():
    return {n: Scene(n) for n in ["launch", "search", "keys", "split", "cargo", "spyglass", "dnd", "wm", "race"]}

def piece(sc, start, dur, speed=1.0):
    return {"src": sc.video, "start": start, "dur": dur, "speed": speed}

def keycap_events(sc, clip_map, wanted, gap=0.55):
    """(clip time, labels) for the tokens in `wanted`, each shown until the next key or `gap` seconds."""
    ev = []
    for t, tok in sc.keys:
        if tok in wanted:
            ct = clip_map(t)
            if ct is not None:
                ev.append((ct, tok))
    out = []
    for i, (t, tok) in enumerate(ev):
        end = ev[i + 1][0] - 0.02 if i + 1 < len(ev) else t + gap
        out.append((t, min(t + gap, end), gfx.key_label(tok)))
    return out

def build_clips():
    S = scenes()
    clips = {}
    L = 5.4

    def window(sc, slot_start, speed=1.0, slot=5.0):
        """One piece covering the slot, extended by the handles."""
        return [piece(sc, slot_start - HANDLE * speed, (slot + 2 * HANDLE) * speed, speed)]

    # 1 launch: the window pops in while the wipe reveals it
    sc = S["launch"]
    p = window(sc, sc.t_app + 0.4)
    c = Clip("c1", "Set course", "01 / 08", p, [(0, 1.0, .5, .5), (0.9, 1.2, .40, .32), (5.4, 1.34, .55, .38)])
    clips["c1"] = c

    # 2 search: sonar rings from the search box when ctrl+f opens it, and again when the matches arrive
    sc = S["search"]
    p = window(sc, 2.75)
    c = Clip("c2", "Sonar. Find anything.", "02 / 08", p, [(0, 1.0, .5, .5), (0.7, 1.55, .20, .2), (3.6, 1.4, .33, .3), (5.4, 1.12, .45, .42)])
    t_open = c.to_clip_time(sc.key_time("<C-f>"))
    t_first = c.to_clip_time(sc.key_time("=i"))
    c.rings = [(t_open, 0.11, 0.035), (t_open + 0.55, 0.11, 0.035), (t_first + 0.35, 0.11, 0.035)]
    c.keycaps = keycap_events(sc, c.to_clip_time, {"<C-f>", "<Down>"})
    clips["c2"] = c

    # 3 keys: arrows, then the palette
    sc = S["keys"]
    p = window(sc, sc.t_app + 0.4)
    c = Clip("c3", "Hands on the helm", "03 / 08", p, [(0, 1.0, .5, .5), (1.6, 1.10, .3, .35), (2.5, 1.32, .5, .28), (4.6, 1.30, .5, .3), (5.4, 1.12, .45, .4)])
    c.keycaps = keycap_events(sc, c.to_clip_time, {"<Down>", "<Up>", "<Left>", "<Right>", "<C-p>", "!accept"})
    clips["c3"] = c

    # 4 split panes and tabs, a little faster so the whole story fits
    sc = S["split"]
    p = window(sc, sc.key_time("<C-\\>") - 0.45, speed=1.2)
    c = Clip("c4", "Two decks, one captain", "04 / 08", p, [(0, 1.0, .5, .5), (1.5, 1.14, .5, .3), (3.5, 1.2, .5, .25), (5.4, 1.06, .5, .4)])
    c.keycaps = keycap_events(sc, c.to_clip_time, {"<C-\\>", "<Tab>", "<C-t>", "<C-Tab>"}, gap=0.7)
    clips["c4"] = c

    # 5 cargo: select four, move them to the other pane
    sc = S["cargo"]
    p = window(sc, sc.key_time("<C-\\>") - 0.45)
    c = Clip("c5", "Load the cargo", "05 / 08", p, [(0, 1.0, .5, .5), (1.5, 1.22, .38, .42), (3.6, 1.3, .38, .45), (5.4, 1.34, .30, .68)])
    c.keycaps = keycap_events(sc, c.to_clip_time, {"<C-\\>", "<Tab>", "<A-Up>", "<S-Down>", "<F8>"}, gap=0.6)
    clips["c5"] = c

    # 6 spyglass: image, code, video, with the zoom parked on the preview pane
    sc = S["spyglass"]
    p = window(sc, sc.t_app + 0.4)
    c = Clip("c6", "Spyglass", "06 / 08", p, [(0, 1.0, .5, .5), (0.8, 1.5, .76, .30), (4.9, 1.62, .80, .30), (5.4, 1.62, .80, .30)])
    c.keycaps = keycap_events(sc, c.to_clip_time, {"<Down>", "<C-m>"}, gap=0.6)
    clips["c6"] = c

    # 7 drag and drop, then the context menu; the pointer script's timeline is converted to recording time
    sc = S["dnd"]
    t0 = sc.meta["ptr"] + 1.0            # the script's first command runs after the uinput device is made
    p = [piece(sc, 5.4, 5.2, 2.0), piece(sc, 12.2, 3.1, 3.1 / 2.8)]   # drag (2.85 s) + menu (2.55 s) = 5.4
    c = Clip("c7", "All hands on deck", "07 / 08", p, [(0, 1.0, .5, .5), (0.5, 1.45, .42, .27), (2.6, 1.45, .42, .27), (3.3, 1.4, .55, .42), (5.4, 1.4, .55, .42)])
    clips["c7"] = c

    # 8 window manager: CaptainFM tiled beside a terminal and a browser
    sc = S["wm"]
    p = window(sc, 5.8)
    c = Clip("c8", "Fits any fleet", "08 / 08", p, [(0, 1.0, .5, .5), (5.4, 1.1, .42, .46)])
    clips["c8"] = c

    # speed: CaptainFM and a typical file manager open the same 100,000-file folder at the same moment
    sc = S["race"]
    speed = 2.5
    t_launch = sc.proc               # both programs were started at this moment
    src0 = t_launch                  # the stopwatches count from here
    shown = t_launch + 0.65          # the film starts as the two windows open
    p = [piece(sc, shown - HANDLE * speed, (3.0 + 2 * HANDLE) * speed, speed)]
    c = Clip("race", "Full speed ahead.", None, p, [(0, 1.0, .5, .5), (3.4, 1.0, .5, .5)])
    c.labels = [("CaptainFM", 0.25, 0.115, gfx.TEAL), ("Typical file manager", 0.75, 0.115, gfx.BRASS)]
    # stopwatches in real seconds since launch; each stops when its list is on screen (measured on the recording)
    c.watches = [(0.25, 0.19, 0.9, src0, speed), (0.75, 0.19, 6.8, src0, speed)]
    clips["race"] = c
    return clips

# ---- rendering ----------------------------------------------------------------------------------------------------

def render_clip(c, slot):
    L = c.length(slot)
    gdir = os.path.join(WORK, "assets")
    os.makedirs(gdir, exist_ok=True)
    inputs, chains = [], []
    # source pieces
    for i, p in enumerate(c.pieces):
        inputs += ["-ss", f"{max(0.0, p['start']):.3f}", "-t", f"{p['dur']:.3f}", "-i", p["src"]]
        chains.append(f"[{i}:v]setpts=(PTS-STARTPTS)/{p['speed']:.5f},fps={FPS},scale={W}:{H}:flags=bicubic,setsar=1,format=yuv420p[p{i}]")
    n = len(c.pieces)
    # a recording that ends a little early holds its last frame
    chains.append("".join(f"[p{i}]" for i in range(n)) + (f"concat=n={n}:v=1:a=0," if n > 1 else "") + "tpad=stop_mode=clone:stop_duration=2[base]")
    # zoom: scale up with time, then place on a canvas so the focus point stays in frame
    z, cx, cy = (keyframes(c.zoom, k) for k in (1, 2, 3))
    chains.append(f"[base]scale=w='trunc({W}*({z})/2)*2':h='trunc({H}*({z})/2)*2':eval=frame:flags=bicubic[big]")
    chains.append(f"color=c=0x0b1d2e:s={W}x{H}:r={FPS},format=yuv420p[canvas]")
    chains.append(f"[canvas][big]overlay=x='min(0\\,max({W}-w\\,{W}/2-({cx})*w))':y='min(0\\,max({H}-h\\,{H}/2-({cy})*h))':shortest=1[v0]")
    last = "v0"
    idx = n
    extra = []
    def add_image(path, rate=True):
        nonlocal idx
        inputs.extend(["-loop", "1", "-framerate", str(FPS), "-t", f"{L:.3f}", "-i", path])
        idx += 1
        return idx - 1

    # sonar rings: expanding teal circles around a point, one input per ring
    ring = os.path.join(gdir, "ring.png")
    gfx.ring_png(ring, int(1200 * Q) // 2 * 2)
    for (t0, fx, fy) in [(r[0], r[1], r[2]) for r in c.rings]:
        i = add_image(ring)
        dur = 1.5
        size = int(1200 * Q)
        chains.append(
            f"[{i}:v]format=rgba,setpts=PTS+{t0:.3f}/TB,"
            f"scale=w='max(2\\,trunc({size}*(0.12+4.6*clip((t-{t0:.3f})/{dur}\\,0\\,1))/2)*2)':h=-2:eval=frame,"
            f"fade=t=in:st={t0:.3f}:d=0.1:alpha=1,fade=t=out:st={t0 + 0.35:.3f}:d={dur - 0.35:.3f}:alpha=1[r{i}]")
        nxt = f"v{i}"
        chains.append(f"[{last}][r{i}]overlay=x='{fx * W:.1f}-w/2':y='{fy * H:.1f}-h/2':enable='between(t,{t0:.3f},{t0 + dur:.3f})':eof_action=pass[{nxt}]")
        last = nxt

    # key caps
    for k, (a, b, labels) in enumerate(c.keycaps):
        path = os.path.join(gdir, f"{c.name}_key{k}.png")
        gfx.keycaps_png(path, W, H, labels)
        i = add_image(path)
        nxt = f"v{i}"
        chains.append(f"[{last}][{i}:v]overlay=x=0:y='{12 * Q:.1f}*(1-min(1\\,(t-{a:.3f})/0.1))':enable='between(t,{a:.3f},{b:.3f})':eof_action=pass[{nxt}]")
        last = nxt

    # labels and stopwatches for the speed comparison
    for k, (text, fx, fy, color) in enumerate(c.labels):
        path = os.path.join(gdir, f"{c.name}_label{k}.png")
        gfx.label_png(path, W, H, text, fx, fy, color)
        i = add_image(path)
        nxt = f"v{i}"
        chains.append(f"[{last}][{i}:v]overlay=0:0:eof_action=pass[{nxt}]")
        last = nxt
    for k, (fx, fy, stop, src0, speed) in enumerate(c.watches):
        # elapsed real seconds since launch: source time minus launch time, source time = piece start + t * speed
        value = f"max(0\\,min({c.pieces[0]['start']:.3f}+t*{speed}-{src0:.3f}\\,{stop}))"
        nxt = f"w{k}"
        chains.append(
            f"[{last}]drawtext=fontfile={MONO_BOLD}:text='%{{eif\\:floor({value})\\:d}}.%{{eif\\:mod(floor({value}*10)\\,10)\\:d}} s':"
            f"fontsize={H * 0.062:.1f}:fontcolor=0xeaf4f4:x={fx * W:.1f}-text_w/2:y={fy * H:.1f}:box=1:boxcolor=0x0b1d2e@0.82:boxborderw={H * 0.012:.1f}[{nxt}]")
        last = nxt

    # caption
    cap = os.path.join(gdir, f"{c.name}_cap.png")
    gfx.caption_png(cap, W, H, c.caption, c.tag)
    i = add_image(cap)
    a = (HANDLE if c.head else 0) + 0.3
    b = L - (HANDLE if c.tail else 0) - 0.55
    chains.append(f"[{i}:v]format=rgba,fade=t=in:st={a:.3f}:d=0.25:alpha=1,fade=t=out:st={b:.3f}:d=0.25:alpha=1[cap]")
    chains.append(f"[{last}][cap]overlay=x=0:y='{H * 0.02:.1f}*(1-min(1\\,max(0\\,(t-{a:.3f})/0.3)))':eof_action=pass,format=yuv420p[out]")

    script = os.path.join(WORK, f"{c.name}.filter")
    open(script, "w").write(";\n".join(chains))
    out = os.path.join(WORK, f"{c.name}.mp4")
    ffmpeg(*inputs, "-/filter_complex", script, "-map", "[out]", "-t", f"{L:.3f}", *X264, out)
    return out

def render_hook():
    out = os.path.join(WORK, "hook.mp4")
    dur = 4.0 + HANDLE
    p = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgra", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                          *X264, out], stdin=subprocess.PIPE)
    for frame in gfx.render_frames(W, H, FPS, dur, gfx.hook_frame):
        p.stdin.write(frame)
    p.stdin.close(); p.wait()
    return out

def render_outro():
    out = os.path.join(WORK, "outro.mp4")
    dur = 4.0 + HANDLE
    p = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgra", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                          *X264, out], stdin=subprocess.PIPE)
    shift = HANDLE      # the first 0.2 s are the handle: hold the first frame while the wipe reveals it
    def frame(ctx, w, h, t):
        gfx.outro_frame(ctx, w, h, max(0.0, t - shift))
    for fr in gfx.render_frames(W, H, FPS, dur, frame):
        p.stdin.write(fr)
    p.stdin.close(); p.wait()
    return out

def render_transitions(lengths):
    names = [s[0] for s in SECTIONS]
    for a, b in zip(names, names[1:]):
        out = os.path.join(WORK, f"t_{a}_{b}.mp4")
        ffmpeg("-ss", f"{lengths[a] - TRANS:.3f}", "-t", f"{TRANS}", "-i", os.path.join(WORK, a + ".mp4"),
               "-t", f"{TRANS}", "-i", os.path.join(WORK, b + ".mp4"),
               "-filter_complex", f"[0:v][1:v]xfade=transition=custom:duration={TRANS}:offset=0:expr='{wipe_expr()}',format=yuv420p[v]",
               "-map", "[v]", *X264, out)

def assemble(lengths, audio):
    names = [s[0] for s in SECTIONS]
    # order: body0, trans0, body1, trans1 ... as separate inputs
    seq = []
    inputs = []
    k = 0
    for i, name in enumerate(names):
        start = 0.0 if i == 0 else TRANS
        end = lengths[name] if i == len(names) - 1 else lengths[name] - TRANS
        inputs += ["-ss", f"{start:.3f}", "-t", f"{end - start:.3f}", "-i", os.path.join(WORK, name + ".mp4")]
        seq.append(f"[{k}:v]setpts=PTS-STARTPTS[s{k}]"); k += 1
        if i < len(names) - 1:
            inputs += ["-i", os.path.join(WORK, f"t_{name}_{names[i + 1]}.mp4")]
            seq.append(f"[{k}:v]setpts=PTS-STARTPTS[s{k}]"); k += 1
    graph = ";".join(seq) + ";" + "".join(f"[s{j}]" for j in range(k)) + f"concat=n={k}:v=1:a=0,fps={FPS}[v]"
    inputs += ["-i", audio]
    ffmpeg(*inputs, "-filter_complex", graph, "-map", "[v]", "-map", f"{k}:a",
           "-c:v", "libx264", "-preset", "medium" if not PREVIEW else "veryfast", "-crf", "15" if not PREVIEW else "20",
           "-profile:v", "high", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "320k", "-movflags", "+faststart", "-t", str(TOTAL), OUT)

def main():
    todo = sys.argv[1:] or ["all"]
    everything = "all" in todo
    clips = build_clips()
    lengths = {"hook": 4.0 + HANDLE, "outro": HANDLE + 4.0}
    for name, c in clips.items():
        slot = dict((s[0], s[2]) for s in SECTIONS)[name]
        lengths[name] = c.length(slot)
    if everything or "hook" in todo:
        render_hook()
    for name, c in clips.items():
        if everything or name in todo or "clips" in todo:
            slot = dict((s[0], s[2]) for s in SECTIONS)[name]
            print("clip", name, flush=True)
            render_clip(c, slot)
    if everything or "outro" in todo:
        render_outro()
    if everything or "trans" in todo:
        render_transitions(lengths)
    if everything or "audio" in todo:
        import audio
        audio.make(os.path.join(WORK, "audio.wav"), clips, SECTIONS, HANDLE, TOTAL)
    if everything or "final" in todo:
        assemble(lengths, os.path.join(WORK, "audio.wav"))
        print("wrote", OUT)

if __name__ == "__main__":
    main()
