#!/usr/bin/env python3
"""Grounded-answer check with a real model: each question must be answered with the expected fact, cite at least one
source, and every citation must resolve; out-of-library questions must be refused without asking the model.
Usage: LBN_DATA=... LBN_RUNNER=... tools/ai_eval.py [ark-pack] [--model PACK]
Prints one line per case and a summary; exit 1 if any case fails. Results depend on the model; record them in TESTING.md."""
import json, subprocess, sys, time
cli = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else "target/release/ark-pack"
model = sys.argv[sys.argv.index("--model") + 1] if "--model" in sys.argv else None
CASES = [
    ("How long should I boil water to make it safe at high elevation?", ["three minutes", "3 minutes"]),
    ("How many drops of 6 percent bleach should I add to a gallon of water?", ["8 drops", "eight drops"]),
    ("How much water should I store for each person?", ["one gallon", "1 gallon"]),
    ("How often should I replace stored tap water?", ["six months", "6 months"]),
    ("What light and radio items belong in an emergency kit?", ["flashlight"]),
    ("What is magnetic declination?", ["true", "geographic"]),
    ("What is the capital of Australia?", None),
]
fails = 0
for q, expect in CASES:
    cmd = [cli, "--json", "ask", q] + (["--model", model] if model else [])
    t = time.time()
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    dt = time.time() - t
    if r.returncode != 0:
        print(f"FAIL  {q}\n      error: {r.stderr.strip()[-300:]}"); fails += 1; continue
    v = json.loads(r.stdout)
    if expect is None:
        ok = v["outcome"] == "no_evidence"
        print(f"{'PASS' if ok else 'FAIL'}  {q}  -> {v['outcome']} ({dt:.1f}s)"); fails += not ok; continue
    a = v.get("answer") or {}
    text = a.get("text", "")
    has_fact = any(e.lower() in text.lower() for e in expect)
    cited = a.get("cited", [])
    resolved = all(json.loads(subprocess.run([cli, "--json", "resolve", c["citation"]], capture_output=True, text=True).stdout)["valid"] for c in cited)
    ok = has_fact and bool(cited) and resolved
    fails += not ok
    print(f"{'PASS' if ok else 'FAIL'}  {q}  ({dt:.1f}s, fact={has_fact}, cited={len(cited)}, resolved={resolved})\n      {text[:300]!r}")
print(f"\n{len(CASES) - fails}/{len(CASES)} passed")
sys.exit(1 if fails else 0)
