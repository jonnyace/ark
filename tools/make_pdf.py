#!/usr/bin/env python3
"""Write a small text PDF (Helvetica, real text layer) from a JSON list of pages, with no third-party libraries.
Usage: make_pdf.py pages.json out.pdf   (pages.json: [{"title": str, "lines": [str, ...]}, ...])
Deterministic: the same input gives the same bytes (no dates, fixed ID), so packs stay reproducible."""
import json, sys

def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")

def wrap(text, width=88):
    out, line = [], ""
    for w in text.split():
        if len(line) + len(w) + 1 > width:
            out.append(line); line = w
        else:
            line = (line + " " + w).strip()
    if line: out.append(line)
    return out or [""]

pages = json.load(open(sys.argv[1]))
objs = {}
objs[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
objs[3] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
objs[4] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
kids = []
n = 5
for i, p in enumerate(pages, 1):
    ops = ["BT", "/F2 18 Tf", "72 740 Td", f"({esc(p['title'])}) Tj", "/F1 11 Tf", "0 -30 Td", "14 TL"]
    for para in p["lines"]:
        for l in wrap(para):
            ops.append(f"({esc(l)}) Tj T*")
        ops.append("T*")
    ops += ["ET", "BT", "/F1 9 Tf", "290 40 Td", f"(Page {i} of {len(pages)}) Tj", "ET"]
    stream = "\n".join(ops).encode("cp1252")
    objs[n] = b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream"
    objs[n + 1] = (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
                   b"/Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>" % n)
    kids.append(n + 1)
    n += 2
objs[2] = b"<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join(f"{k} 0 R" for k in kids).encode(), len(kids))
info = n
objs[info] = b"<< /Title (Emergency Supplies Checklist - Ark sample) /Producer (ark-library tools/make_pdf.py) >>"
out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
offsets = {}
for k in sorted(objs):
    offsets[k] = len(out)
    out += b"%d 0 obj\n" % k + objs[k] + b"\nendobj\n"
xref = len(out)
out += b"xref\n0 %d\n0000000000 65535 f \n" % (max(objs) + 1)
for k in range(1, max(objs) + 1):
    out += b"%010d 00000 n \n" % offsets[k]
out += (b"trailer\n<< /Size %d /Root 1 0 R /Info %d 0 R /ID [<6c626e2d73616d706c652d706466> <6c626e2d73616d706c652d706466>] >>\n"
        b"startxref\n%d\n%%%%EOF\n" % (max(objs) + 1, info, xref))
open(sys.argv[2], "wb").write(out)
print(f"wrote {sys.argv[2]}: {len(pages)} pages, {len(out)} bytes")
