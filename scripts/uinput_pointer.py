#!/usr/bin/env python3
"""Drives the real pointer and modifier keys through /dev/uinput, for recording the demo (no evdev needed).

Reads commands from stdin, one per line, coordinates in screen pixels (SCREEN_W x SCREEN_H, default 3840x2160):
  warp X Y            jump to X,Y
  move X Y [MS]       glide to X,Y over MS milliseconds (default 600)
  down | up           left button
  key ctrl|shift|alt down|up
  wait MS
  print TEXT          write TEXT to stdout (lets a caller sync a screenshot to the script)
"""
import fcntl, os, struct, sys, time

W, H = int(os.environ.get("SCREEN_W", 3840)), int(os.environ.get("SCREEN_H", 2160))
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
BTN_LEFT, ABS_X, ABS_Y = 0x110, 0, 1
KEYS = {"ctrl": 29, "shift": 42, "alt": 56}
UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT = 0x40045564, 0x40045565, 0x40045567
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502

fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
for ev in (EV_KEY, EV_ABS, EV_SYN):
    fcntl.ioctl(fd, UI_SET_EVBIT, ev)
for k in (BTN_LEFT, *KEYS.values()):
    fcntl.ioctl(fd, UI_SET_KEYBIT, k)
for a in (ABS_X, ABS_Y):
    fcntl.ioctl(fd, UI_SET_ABSBIT, a)
absmax = [0] * 64; absmax[ABS_X] = absmax[ABS_Y] = 65535
dev = struct.pack("80sHHHHi", b"cfm-demo-pointer", 3, 1, 1, 1, 0) + struct.pack("64i", *absmax) + struct.pack("64i", *[0] * 64) + struct.pack("64i", *[0] * 64) + struct.pack("64i", *[0] * 64)
os.write(fd, dev)
fcntl.ioctl(fd, UI_DEV_CREATE)
time.sleep(1.0)

def emit(t, c, v):
    os.write(fd, struct.pack("llHHi", 0, 0, t, c, v))

def sync(): emit(EV_SYN, 0, 0)

x = y = 0.0
def put(px, py):
    global x, y
    x, y = px, py
    emit(EV_ABS, ABS_X, int(px * 65535 / W)); emit(EV_ABS, ABS_Y, int(py * 65535 / H)); sync()

def glide(tx, ty, ms):
    sx, sy = x, y
    steps = max(1, int(ms / 12))
    for i in range(1, steps + 1):
        t = i / steps
        t = t * t * (3 - 2 * t)     # ease in and out
        put(sx + (tx - sx) * t, sy + (ty - sy) * t)
        time.sleep(ms / 1000 / steps)

try:
    for line in sys.stdin:
        a = line.split()
        if not a or a[0].startswith("#"): continue
        if a[0] == "warp": put(float(a[1]), float(a[2]))
        elif a[0] == "move": glide(float(a[1]), float(a[2]), float(a[3]) if len(a) > 3 else 600)
        elif a[0] in ("down", "up"): emit(EV_KEY, BTN_LEFT, 1 if a[0] == "down" else 0); sync()
        elif a[0] == "key": emit(EV_KEY, KEYS[a[1]], 1 if a[2] == "down" else 0); sync()
        elif a[0] == "wait": time.sleep(float(a[1]) / 1000)
        elif a[0] == "print": print(" ".join(a[1:]), flush=True)
finally:
    for k in KEYS.values(): emit(EV_KEY, k, 0)
    emit(EV_KEY, BTN_LEFT, 0); sync()
    fcntl.ioctl(fd, UI_DEV_DESTROY)
