#!/usr/bin/env python3
"""Probe the lastbook-llm runner: one generation, timing and peak memory (VmHWM) from /proc.
Usage: runner_probe.py RUNNER MODEL [--threads N] [--gpu]   (--gpu: let llama.cpp offload to Vulkan instead of CPU only)"""
import json, subprocess, sys, time, threading
runner, model = sys.argv[1], sys.argv[2]
extra = [a for a in sys.argv[3:] if a != "--gpu"]
cpu = [] if "--gpu" in sys.argv[3:] else ["--cpu-only"]
seg = [["<|im_start|>system\n", True], ["Answer in one sentence using only the source. End the sentence with [S1].", False],
       ["<|im_end|>\n<|im_start|>user\n", True],
       ["Source [S1]: Bring water to a rolling boil for one minute to kill germs. At elevations above 6,500 feet, boil for three minutes.\n\nQuestion: How long should I boil water at high altitude?", False],
       ["<|im_end|>\n<|im_start|>assistant\n", True]]
t0 = time.time()
p = subprocess.Popen([runner, "--model", model, "--ctx", "4096", *cpu, *extra], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
peak = [0]
def watch():
    while p.poll() is None:
        try:
            for l in open(f"/proc/{p.pid}/status"):
                if l.startswith("VmHWM:"): peak[0] = max(peak[0], int(l.split()[1]))
        except OSError: pass
        time.sleep(0.2)
threading.Thread(target=watch, daemon=True).start()
ready = None; first = None; out = ""; ntok = 0
for line in p.stdout:
    j = json.loads(line)
    if j["ev"] == "ready":
        ready = time.time() - t0; print("ready", json.dumps(j)[:300]); t1 = time.time()
        p.stdin.write(json.dumps({"op": "gen", "id": 1, "segments": seg, "max_tokens": 80, "temp": 0.0, "seed": 1}) + "\n"); p.stdin.flush()
    elif j["ev"] == "fit": print("fit", json.dumps(j)[:300])
    elif j["ev"] == "tok":
        first = first or time.time() - t1; out += j["t"]; ntok += 1
    else:
        print(j["ev"], json.dumps(j)[:300]); 
        if j["ev"] in ("done", "error"): break
gen = time.time() - t1
p.stdin.write('{"op":"quit"}\n'); p.stdin.flush(); p.wait(10)
print("ANSWER:", out)
print(f"load_s={ready:.2f} first_token_s={first:.2f} gen_s={gen:.2f} tokens={ntok} tok_per_s={ntok/max(gen-first,1e-6):.1f} peak_rss_mb={peak[0]/1024:.0f}")
