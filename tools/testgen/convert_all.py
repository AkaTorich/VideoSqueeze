# Runs every test file (target/testfiles, made by `cargo run --release --example make_tests`) through every
# output format with VideoSqueeze.exe in its windowless mode, then reads each result back (--probe).
# Results go to target/testout/<time>. Run: python tools/testgen/convert_all.py [file filter]
import json
import os
import subprocess
import sys
import time

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
EXE = os.path.join(ROOT, "target", "release", "VideoSqueeze.exe")
SRC = os.path.join(ROOT, "target", "testfiles")
OUT = os.path.join(ROOT, "target", "testout", time.strftime("%H%M%S"))

COMBOS = [
    dict(format="mp4", videoMode="encode", sizeMode="quality"),
    dict(format="mkv", videoMode="encode", sizeMode="target", targetMb=0.5),
    dict(format="avi", videoMode="encode", sizeMode="bitrate", bitrateKbps=600, height=360, fps=15),
    dict(format="mov", videoMode="copy", sizeMode="quality"),
    dict(format="mkv", videoMode="copy", sizeMode="quality"),
    dict(format="avi", videoMode="copy", sizeMode="quality"),
    dict(format="mp4", videoMode="encode", sizeMode="quality", audio="aac", audioKbps=96, height=240),
    dict(format="gif", videoMode="encode", sizeMode="quality", height=200, fps=10),
    dict(format="audio", videoMode="encode", sizeMode="quality"),
    dict(format="m4a", videoMode="encode", sizeMode="quality"),
    dict(format="wav", videoMode="encode", sizeMode="quality"),
]

BASE = dict(qualityBpp=0.1, targetMb=1.0, bitrateKbps=800, height=0, fps=0, audio="auto", audioKbps=128)


def call(args):
    res = os.path.join(OUT, "r.json")
    subprocess.run([EXE] + args + ["--out", res], check=False)
    with open(res, encoding="utf-8") as f:
        return json.load(f)


def main():
    os.makedirs(OUT, exist_ok=True)
    flt = sys.argv[1] if len(sys.argv) > 1 else ""
    failures = 0
    for name in sorted(os.listdir(SRC)):
        if flt not in name:
            continue
        src = os.path.join(SRC, name)
        for i, combo in enumerate(COMBOS):
            s = dict(BASE, **combo)
            s["outDir"] = OUT
            s["suffix"] = "_%02d_%s_%s" % (i, s["format"], s["videoMode"])
            settings = os.path.join(OUT, "s.json")
            with open(settings, "w", encoding="utf-8") as f:
                json.dump(s, f)
            t0 = time.time()
            r = call(["--convert", src, "--settings", settings])
            dt = time.time() - t0
            if "error" in r:
                failures += 1
                print("%-22s %-28s %5.1fs  ERROR %s" % (name, s["suffix"], dt, r["error"]))
                continue
            p = call(["--probe", r["out_path"]]) if not r["out_path"].endswith(".gif") else {}
            if "error" in p:
                failures += 1
            v = p.get("video") or {}
            a = p.get("audio") or {}
            desc = "%7.1f KB  %4d fr | %s %sx%s %.2ffps %.2fs | %s %s" % (
                r["out_size"] / 1024, r["frames"], v.get("name", "-"), v.get("width", ""), v.get("height", ""),
                v.get("fps", 0) or 0, p.get("duration", 0) or 0, a.get("name", "-"), a.get("rate", ""))
            if "error" in p:
                desc += "  PROBE ERROR " + p["error"]
            notes = ("  [" + "; ".join(r["notes"]) + "]") if r["notes"] else ""
            print("%-22s %-28s %5.1fs  %s%s" % (name, s["suffix"], dt, desc, notes))
    print("failures:", failures, "->", OUT)


if __name__ == "__main__":
    main()
