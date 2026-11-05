#!/usr/bin/env python3
"""Require every named application icon to be explicitly embedded."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "crates/glim-app/src"
assets = (SOURCE / "app/assets.rs").read_text()
declaration = re.search(r"icon_assets!\(\s*ViewerIcons,\s*\[(.*?)\]\s*\)", assets, re.S)
if declaration is None:
    raise SystemExit("Cannot find the ViewerIcons resource declaration")
registered = set(re.findall(r"\b[A-Z][A-Za-z0-9_]*\b", declaration[1]))
used = {}
for path in SOURCE.rglob("*.rs"):
    for icon in re.findall(r"\bIconName::([A-Z][A-Za-z0-9_]*)", path.read_text()):
        used.setdefault(icon, set()).add(str(path.relative_to(ROOT)))

missing = sorted(set(used) - registered)
if missing:
    raise SystemExit("Icons referenced but not embedded:\n" + "\n".join(
        f"  {icon}: {', '.join(sorted(used[icon]))}" for icon in missing
    ))

for path in (ROOT / "assets/navigation").glob("*.svg"):
    ET.parse(path)
print(f"Icon resources OK: {len(used)} referenced names are explicitly embedded.")
