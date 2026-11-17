#!/usr/bin/env python3
"""Render a captioned slideshow video (WebM, VP9 + Opus) from a WebVTT file, and a poster image.
Each cue becomes a slide showing its text, so the transcript matches the picture exactly.
Usage: make_video.py captions.vtt out.webm poster.webp "Title"     (needs ffmpeg with libvpx-vp9 and libopus)
Bit-exact flags and one encoder thread keep the output reproducible for the same ffmpeg build."""
import re, subprocess, sys
vtt, out, poster, title = sys.argv[1:5]
font = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
cues = []
for m in re.finditer(r"(\d\d):(\d\d):(\d\d)\.(\d{3}) --> (\d\d):(\d\d):(\d\d)\.(\d{3})\n(.+?)(?:\n\n|\Z)", open(vtt).read(), re.S):
    g = m.groups()
    s = int(g[0]) * 3600 + int(g[1]) * 60 + int(g[2]) + int(g[3]) / 1000
    e = int(g[4]) * 3600 + int(g[5]) * 60 + int(g[6]) + int(g[7]) / 1000
    cues.append((s, e, " ".join(g[8].split())))
dur = cues[-1][1]
def q(t):
    return t.replace("\\", "\\\\").replace(":", "\\:").replace("'", "’").replace(",", "\\,")
f = [f"drawtext=fontfile={font}:text='{q(title)}':fontsize=34:fontcolor=white:x=(w-tw)/2:y=60"]
for i, (s, e, t) in enumerate(cues):
    f.append(f"drawtext=fontfile={font}:text='{q(t)}':fontsize=30:fontcolor=white:x=(w-tw)/2:y=(h-th)/2:enable='between(t,{s},{e - 0.001})'")
    f.append(f"drawtext=fontfile={font}:text='Step {i + 1} of {len(cues)}':fontsize=22:fontcolor=0xb0c4de:x=(w-tw)/2:y=h-80:enable='between(t,{s},{e - 0.001})'")
f.append(f"drawtext=fontfile={font}:text='%{{pts\\:hms}}':fontsize=18:fontcolor=0x9aa5b1:x=w-tw-20:y=20")
cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "lavfi", "-i", f"color=c=0x1d3557:s=960x540:r=25:d={dur}",
       "-f", "lavfi", "-i", f"sine=frequency=220:sample_rate=48000:duration={dur}",
       "-filter_complex", f"[0:v]{','.join(f)}[v];[1:a]volume=0.05[a]", "-map", "[v]", "-map", "[a]",
       "-c:v", "libvpx-vp9", "-b:v", "300k", "-g", "50", "-keyint_min", "50", "-row-mt", "0", "-threads", "1",
       "-c:a", "libopus", "-b:a", "32k", "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact",
       "-metadata", f"title={title}", out]
subprocess.run(cmd, check=True)
subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", "8", "-i", out, "-frames:v", "1", "-c:v", "libwebp", "-quality", "70",
                "-fflags", "+bitexact", poster], check=True)
print(f"wrote {out} ({dur:.0f} s, {len(cues)} cues) and {poster}")
