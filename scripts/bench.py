#!/usr/bin/env python3
"""Rough timing of a release build: CPU time and peak memory for a few startup cases, plus the
[timing] lines CFM_DEBUG prints (folder listing, deep-search index build and search).

    scripts/bench.py [folder-with-many-files]

CFM_PROFILE=dist picks the `dist` build. Runs headless with scratch config/cache folders. The wall time includes the 1.5 s CFM_SHOT delay,
so read the cpu column and the [timing] lines instead."""
import os, resource, subprocess, sys, tempfile, time

def run(path, keys, label, scratch):
    env = dict(os.environ, QT_QPA_PLATFORM="offscreen", CFM_SIZE="1200x700", CFM_KEYS=keys,
               CFM_SHOT=os.path.join(scratch, "shot.png"), CFM_DEBUG="1",
               XDG_CONFIG_HOME=scratch + "/cfg", XDG_DATA_HOME=scratch + "/data", XDG_CACHE_HOME=scratch + "/cache")
    env.pop("WAYLAND_DISPLAY", None)
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.time()
    p = subprocess.run(["./target/%s/captainfm" % os.environ.get("CFM_PROFILE", "release"), path], env=env, capture_output=True, text=True)
    wall = time.time() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime
    print(f"{label:24s} wall {wall:5.2f}s  cpu {cpu:5.2f}s  peak RSS so far {after.ru_maxrss / 1024:5.0f} MB")
    for line in p.stderr.splitlines():
        # The index build prints a search line per refresh; keep the first and the final ones.
        if "[timing]" in line and ("indexed" in line or "list_dir" in line):
            print("   ", line.strip())

home = os.path.expanduser("~")
many = sys.argv[1] if len(sys.argv) > 1 else home
with tempfile.TemporaryDirectory() as scratch:
    os.makedirs(scratch + "/cfg", exist_ok=True)
    run(scratch + "/cfg", "j", "empty folder", scratch)
    run(many, "j", "folder: " + many[-14:], scratch)
    run(home, "@search.drive =config !accept", "drive index + search", scratch)
