#!/usr/bin/env python3
"""Standard performance test. Every run builds the same synthetic home folder, runs the same cases the same
number of times, and writes a small report to docs/perf/ so runs on different days can be compared.

    scripts/perf.py                  # the dist build, 5 runs per case
    scripts/perf.py --profile release --runs 3
    scripts/perf.py --no-save        # print only

What makes runs comparable:
  * The fixture is deterministic (fixed names, fixed counts, empty files) and lives in a scratch folder that is
    also $HOME, and the index case searches that folder (not "whole drive", which walks the whole mount), so nothing from the machine is counted.
  * Headless (offscreen Qt), scratch config/data/cache folders, a size of 1200x700.
  * Each case runs RUNS times; the median and the best are reported. Compare medians.
  * The report records the commit, build profile, binary size, CPU, cores, RAM and kernel. Only compare runs
    from the same machine; the report says so when the previous run was somewhere else.
  * Bump FIXTURE_VERSION whenever the fixture or the cases change; runs with different versions are not compared.

Timings come from the [timing] lines CFM_DEBUG prints; CPU time and peak memory come from the kernel (wait4).
"""
import argparse, datetime, json, os, platform, re, shutil, statistics, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "perf")
FIXTURE_VERSION = 2
TREE_DIRS, TREE_FILES = 200, 250          # 50,000 files in a tree
FLAT_FILES = 20000                        # and one folder with 20,000 files
COPY_FILES, COPY_MB = 100, 2              # and 100 files of 2 MB (200 MB) to copy
EXTS = ["txt", "md", "rs", "png", "jpg", "mp3", "pdf", "zip"]


def make_fixture(home):
    tree = os.path.join(home, "tree")
    for d in range(TREE_DIRS):
        sub = os.path.join(tree, "dir%03d" % d, "inner")
        os.makedirs(sub)
        for f in range(TREE_FILES):
            folder = sub if f % 5 == 0 else os.path.dirname(sub)
            open(os.path.join(folder, "file%04d_%s.%s" % (f, "alpha beta gamma delta".split()[f % 4], EXTS[f % 8])), "w").close()
    flat = os.path.join(home, "flat")
    os.makedirs(flat)
    for f in range(FLAT_FILES):
        open(os.path.join(flat, "item%05d.%s" % (f, EXTS[f % 8])), "w").close()
    os.makedirs(os.path.join(home, "empty"))
    os.makedirs(os.path.join(home, "copydst"))
    src = os.path.join(home, "copysrc")
    os.makedirs(src)
    block = bytes((i * 7 + 3) % 251 for i in range(1 << 20))
    for f in range(COPY_FILES):
        with open(os.path.join(src, "blob%03d.bin" % f), "wb") as out:
            out.write(block * COPY_MB)
    return {"tree_files": TREE_DIRS * TREE_FILES, "flat_files": FLAT_FILES, "copy_mb": COPY_FILES * COPY_MB}


def duration_ms(text):
    """'1.23ms', '456.7µs', '0.22s' -> milliseconds."""
    m = re.match(r"([\d.]+)(ns|µs|us|ms|s)", text)
    v, unit = float(m.group(1)), m.group(2)
    return v * {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1000.0}[unit]


def run_once(binary, home, scratch, path, keys, patterns):
    keys = keys.replace("{home}", home)
    shutil.rmtree(os.path.join(home, "copydst"), ignore_errors=True)   # every copy starts into an empty folder
    os.makedirs(os.path.join(home, "copydst"))
    env = dict(os.environ, QT_QPA_PLATFORM="offscreen", CFM_SIZE="1200x700", CFM_KEYS=keys, CFM_DEBUG="1",
               CFM_SHOT=os.path.join(scratch, "shot.png"), HOME=home,
               XDG_CONFIG_HOME=scratch + "/cfg", XDG_DATA_HOME=scratch + "/data", XDG_CACHE_HOME=scratch + "/cache")
    env.pop("WAYLAND_DISPLAY", None)
    p = subprocess.Popen([binary, path], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    _, status, ru = os.wait4(p.pid, 0)
    err = p.stderr.read()
    p.returncode = 0
    out = {"cpu_s": ru.ru_utime + ru.ru_stime, "rss_mb": ru.ru_maxrss / 1024}
    for name, rx in patterns.items():
        found = re.findall(rx, err)
        if found:
            # An index build prints a search line per refresh; the last one is the settled value.
            out[name] = duration_ms(found[-1]) if not re.fullmatch(r"\d+", found[-1]) else float(found[-1])
    if "copy_ms" in out:
        out["copy_mb_per_s"] = COPY_FILES * COPY_MB / (out["copy_ms"] / 1000)
    return out


CASES = [
    # name, folder inside the fixture, keys, {metric: regex over stderr}
    ("startup (empty folder)", "empty", "j", {"qml_loaded_ms": r"QML loaded (\d+) ms"}),
    ("list 20,000 files", "flat", "j", {"list_dir_ms": r"list_dir 20000 entries: ([\d.]+\S+)"}),
    ("index 70,000+ files and search", "", "@search.folder =alpha !accept",
     {"index_ms": r"indexed \d+ entries under .* in ([\d.]+\S+)", "search_ms": r"search \"alpha\" over \d+ entries: ([\d.]+\S+)"}),
    ("copy 200 MB (100 files)", "copysrc", "@sel.all @file.copy @nav.jump ={home}/copydst !accept @file.paste",
     {"copy_ms": r"copied \d+ items, \d+ bytes: ([\d.]+\S+)"}),
]
HIGHER_IS_BETTER = {"copy_mb_per_s"}


def machine():
    cpu = ""
    try:
        cpu = next(l.split(":", 1)[1].strip() for l in open("/proc/cpuinfo") if l.startswith("model name"))
    except (OSError, StopIteration):
        pass
    mem = ""
    try:
        mem = "%.0f GB" % (int(next(l.split()[1] for l in open("/proc/meminfo") if l.startswith("MemTotal"))) / 1048576)
    except (OSError, StopIteration):
        pass
    return {"cpu": cpu, "cores": os.cpu_count(), "ram": mem, "kernel": platform.release(), "host": platform.node()}


def git(*args):
    try:
        return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True).stdout.strip()
    except OSError:
        return ""


def summarise(values):
    return {"median": statistics.median(values), "best": min(values), "worst": max(values)}


def previous(current):
    if not os.path.isdir(OUT):
        return None
    for name in sorted(os.listdir(OUT), reverse=True):
        if name.endswith(".json"):
            try:
                data = json.load(open(os.path.join(OUT, name)))
            except (OSError, ValueError):
                continue
            if data.get("fixture_version") == current["fixture_version"] and data["profile"] == current["profile"]:
                return data
    return None


def report(res, prev):
    L = ["# Performance run " + res["when"], ""]
    dirty = " (uncommitted changes)" if res["dirty"] else ""
    L += ["- Commit `%s`%s, profile `%s`, binary %.1f MB" % (res["commit"], dirty, res["profile"], res["binary_mb"]),
          "- %s, %s cores, %s RAM, kernel %s; fixture on %s" % (res["machine"]["cpu"], res["machine"]["cores"], res["machine"]["ram"], res["machine"]["kernel"], res["scratch"]),
          "- Fixture v%d: %d files in a tree, %d in one folder, %d MB to copy; %d runs per case, median shown" % (
              res["fixture_version"], res["fixture"]["tree_files"], res["fixture"]["flat_files"], res["fixture"]["copy_mb"], res["runs"])]
    same = prev and prev["machine"]["cpu"] == res["machine"]["cpu"] and prev["machine"]["host"] == res["machine"]["host"]
    if prev:
        L.append("- Compared with `%s` (%s)%s" % (prev["commit"], prev["when"], "" if same else ", **a different machine, so the change column means little**"))
    else:
        L.append("- No earlier run with the same fixture and profile to compare with")
    L += ["", "| Case | Metric | Median | Best | Worst | Change |", "|---|---|---:|---:|---:|---:|"]
    for case, metrics in res["cases"].items():
        for metric, s in metrics.items():
            unit = {"cpu_s": "s", "rss_mb": "MB", "copy_mb_per_s": "MB/s"}.get(metric, "ms")
            change = ""
            old = prev and prev["cases"].get(case, {}).get(metric)
            if old and old["median"]:
                pct = (s["median"] - old["median"]) / old["median"] * 100
                change = "%+.0f%%" % pct
            L.append("| %s | %s | %.2f %s | %.2f | %.2f | %s |" % (case, metric, s["median"], unit, s["best"], s["worst"], change))
    L += ["", "Lower is better, except copy_mb_per_s where higher is better. CPU time and memory repeat within about 10%; timings under 20 ms can swing by half between runs, so trust them only when the change is large and repeats.", ""]
    return "\n".join(L)


def main():
    ap = argparse.ArgumentParser(description="CaptainFM standard performance test")
    ap.add_argument("--profile", default=os.environ.get("CFM_PROFILE", "dist"))
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--no-save", action="store_true")
    ap.add_argument("--scratch", default=tempfile.gettempdir(), help="where the fixture is built; the copy speed is that disk's (default: the temp folder, often RAM)")
    a = ap.parse_args()

    binary = os.path.join(ROOT, "target", a.profile, "captainfm")
    if not os.path.exists(binary):
        sys.exit("No %s build; run ./build.sh (or cargo build --profile %s) first." % (a.profile, a.profile))

    now = datetime.datetime.now()
    res = {"when": now.strftime("%Y-%m-%d %H:%M"), "fixture_version": FIXTURE_VERSION, "profile": a.profile, "runs": a.runs,
           "commit": git("rev-parse", "--short", "HEAD"), "dirty": bool(git("status", "--porcelain", "--", "src", "qml", "Cargo.toml")),
           "binary_mb": os.path.getsize(binary) / 1048576, "machine": machine(), "cases": {}, "scratch": os.path.realpath(a.scratch)}

    # No underscores in the name: CFM_KEYS turns them into spaces inside "=text".
    scratch = os.path.join(a.scratch, "cfm-perf-%d" % os.getpid())
    os.makedirs(scratch)
    try:
        home = os.path.join(scratch, "home")
        t = time.time()
        res["fixture"] = make_fixture(home)
        print("fixture built in %.1f s" % (time.time() - t))
        for name, folder, keys, patterns in CASES:
            path = os.path.join(home, folder)
            run_once(binary, home, scratch, path, keys, patterns)  # warm-up: file cache, first-run config; not counted
            runs = [run_once(binary, home, scratch, path, keys, patterns) for _ in range(a.runs)]
            metrics = {k: summarise([r[k] for r in runs if k in r]) for k in runs[0] if all(k in r for r in runs)}
            missing = [k for k in patterns if k not in metrics]
            if missing:
                print("  warning: %s: no %s in the output" % (name, ", ".join(missing)))
            res["cases"][name] = metrics
            print("%-34s %s" % (name, "  ".join("%s %.2f" % (k, v["median"]) for k, v in metrics.items())))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    text = report(res, previous(res))
    print("\n" + text)
    if not a.no_save:
        os.makedirs(OUT, exist_ok=True)
        stem = now.strftime("%Y-%m-%d_%H%M") + "_" + (res["commit"] or "nogit") + "_" + a.profile
        json.dump(res, open(os.path.join(OUT, stem + ".json"), "w"), indent=1)
        open(os.path.join(OUT, stem + ".md"), "w").write(text)
        print("saved docs/perf/" + stem + ".{md,json}")


main()
