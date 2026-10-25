#!/usr/bin/env python3
"""Emit a GitHub Actions error annotation containing (the tail of) a log file.

Annotations are readable through the API without downloading the job logs.
Usage: annotate.py <logfile> <title> [max_chars] [error|warning|notice]
"""
import re
import sys

path, title = sys.argv[1], sys.argv[2]
limit = int(sys.argv[3]) if len(sys.argv) > 3 else 6000
level = sys.argv[4] if len(sys.argv) > 4 else "error"

try:
    text = open(path, encoding="utf-8", errors="replace").read()
except OSError as exc:
    text = f"<cannot read {path}: {exc}>"

text = re.sub(r"\x1b\[[0-9;]*m", "", text)
text = "\n".join(
    line
    for line in text.splitlines()
    if line.strip() and not line.lstrip().startswith(("Downloaded", "Downloading", "Compiling", "Fresh"))
)
# Errors are at the end of a build log: keep the tail, not the head.
text = text[-limit:]
text = text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
print(f"::{level} title={title}::{text}")
