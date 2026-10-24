#!/usr/bin/env python3
"""Makes a big, believable folder tree of empty files for the search and speed scenes: python3 make-big-archive.py DIR.

DIR/<year>/<quarter>/... holds about 110,000 scans, photos and documents (with some invoices to find), and DIR/everything
holds 100,000 files in one folder (the speed comparison opens that one)."""
import os, random, sys

root = sys.argv[1]
random.seed(7)
clients = ["northwind", "harbor-co", "lighthouse", "meridian", "tidewater", "anchor-labs", "keelhaul", "bluewater"]
kinds = [("scan", "pdf"), ("IMG", "jpg"), ("notes", "txt"), ("report", "docx"), ("export", "csv"), ("receipt", "png")]
n = 0
for year in range(2018, 2027):
    for q in range(1, 5):
        for sub in ("scans", "photos", "documents"):
            d = os.path.join(root, str(year), f"Q{q}", sub)
            os.makedirs(d, exist_ok=True)
            for i in range(380):
                base, ext = random.choice(kinds)
                open(os.path.join(d, f"{base}-{year}{q:02d}-{i:04d}.{ext}"), "w").close(); n += 1
            for c in random.sample(clients, 3):
                open(os.path.join(d, f"invoice-{c}-{year}-{q * 3:02d}.pdf"), "w").close(); n += 1
flat = os.path.join(root, "everything")
os.makedirs(flat, exist_ok=True)
for i in range(100000):
    open(os.path.join(flat, f"file-{i:06d}.dat"), "w").close(); n += 1
print(n, "files")
