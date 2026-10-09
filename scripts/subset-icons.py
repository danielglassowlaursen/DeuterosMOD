#!/usr/bin/env python3
"""Cuts the Lucide icon font down to the glyphs the client uses.

usage: scripts/subset-icons.py path/to/lucide.ttf

The full font comes from the `lucide-static` npm package (font/lucide.ttf).
The glyphs are the codepoints written as '\\u{XXXX}' in
crates/nullnet-client/src/icons.rs. Needs fonttools (pip install fonttools).
"""
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
icons = (root / "crates/nullnet-client/src/icons.rs").read_text()
codepoints = sorted(set(re.findall(r"'\\u\{([0-9A-Fa-f]+)\}'", icons)))
if len(sys.argv) != 2 or not codepoints:
    sys.exit(__doc__)
out = root / "crates/nullnet-client/assets/fonts/lucide-subset.ttf"
subprocess.run(
    [
        "pyftsubset",
        sys.argv[1],
        "--unicodes=" + ",".join(f"U+{c}" for c in codepoints),
        "--output-file=" + str(out),
        "--no-hinting",
        "--layout-features=",
    ],
    check=True,
)
print(f"{len(codepoints)} icons -> {out.relative_to(root)} ({out.stat().st_size} bytes)")
