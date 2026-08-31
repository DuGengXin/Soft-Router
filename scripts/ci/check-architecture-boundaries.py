#!/usr/bin/env python3
"""Fail if gateway-* crates depend on a crate not in architecture-rules.json.

The manifest path is passed with forward slashes so cargo emits valid JSON on
both Windows and Unix hosts.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RULES = json.loads((Path(__file__).parent / "architecture-rules.json").read_text(encoding="utf-8"))
allowed = RULES["allowed"]

proc = subprocess.run(
    [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--manifest-path",
        (ROOT / "Cargo.toml").as_posix(),
        "--no-deps",
    ],
    check=True,
    capture_output=True,
    text=True,
)
# Some Windows cargo versions emit native backslashes in path strings without
# escaping them. Normalize only invalid JSON escapes before parsing; standard
# JSON escapes such as `\\"` and `\\uXXXX` remain untouched.
metadata_text = re.sub(r'\\(?!["\\/bfnrtu])', '/', proc.stdout)
meta = json.loads(metadata_text)
failed = False
for pkg in meta["packages"]:
    name = pkg["name"]
    if name not in allowed:
        continue
    permit = set(allowed[name])
    for dep in pkg["dependencies"]:
        dep_name = dep["name"]
        if dep_name.startswith("gateway-") and dep_name not in permit:
            print(f"{name} must not depend on {dep_name}", file=sys.stderr)
            failed = True
if failed:
    sys.exit(1)
print("PASSED")
