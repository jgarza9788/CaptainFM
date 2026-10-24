"""Reads what scripts/record-demo03.sh recorded: when the app showed up in each video and when each key was fed."""
import os, re, subprocess
import numpy as np

DEMO = os.environ.get("DEMO", os.path.expanduser("~/Videos/cfm-demo03"))

def first_change(video, thresh=6.0, max_s=8.0):
    """Seconds into `video` when the picture first changes a lot from its first frame (the app appearing)."""
    cmd = ["ffmpeg", "-v", "error", "-t", str(max_s), "-i", video, "-vf", "fps=30,scale=96:54,format=gray", "-f", "rawvideo", "-"]
    raw = subprocess.run(cmd, capture_output=True).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 54, 96).astype(np.float32)
    # The first moments can still be settling (the recorder starting); compare with a calm frame from 0.7 s in.
    base = frames[21]
    for i, f in enumerate(frames):
        if i > 21 and np.abs(f - base).mean() > thresh:
            return i / 30.0
    return None

def keys(log):
    out = []
    for line in open(log, errors="replace"):
        m = re.search(r"\[keys\] (\d+) (\S+)", line)
        if m:
            out.append((int(m.group(1)) / 1000.0, m.group(2)))
    return out

def first_frame_ms(log):
    for line in open(log, errors="replace"):
        m = re.search(r"first frame (\d+) ms", line)
        if m:
            return int(m.group(1))
    return 500

def meta(name):
    d = {}
    for line in open(os.path.join(DEMO, name + ".meta")):
        k, v = line.split()
        d[k] = float(v)
    return d

class Scene:
    def __init__(self, name):
        self.name = name
        self.video = os.path.join(DEMO, name + ".mp4")
        log = os.path.join(DEMO, name + ".log")
        self.meta = meta(name) if os.path.exists(os.path.join(DEMO, name + ".meta")) else {}
        self.t_app = first_change(self.video)
        # The process started this long before its first frame was drawn (plus about a frame for the compositor).
        self.proc = self.t_app - first_frame_ms(log) / 1000.0 - 0.03 if os.path.exists(log) else self.t_app
        self.keys = [(self.proc + t, k) for t, k in keys(log)] if os.path.exists(log) else []

    def key_time(self, token, nth=0):
        hits = [t for t, k in self.keys if k == token]
        return hits[nth]

if __name__ == "__main__":
    for n in ["launch", "search", "keys", "split", "cargo", "spyglass", "dnd", "wm", "race"]:
        s = Scene(n)
        print(f"{n:9s} app at {s.t_app:.2f}s  process start {s.proc:.2f}s  meta {s.meta}")
        print("   ", " ".join(f"{t:.2f}:{k}" for t, k in s.keys[:40]))
