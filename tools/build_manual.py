#!/usr/bin/env python3
"""Builds the user manual: one self-contained HTML file.

    python3 tools/build_manual.py [--label LABEL] [--out FILE]

It fills docs/manual/manual.template.html with the version from Cargo.toml and
generates the rudiment reference from the patterns in src/rudiments.rs, so the
manual cannot drift from what the program offers. Standard library only.
"""
import argparse
import html
import math
import re
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "docs" / "manual" / "manual.template.html"
UNITS = {"S16": 0.25, "S8": 0.5, "T16": 1.0 / 6.0}
FAMILIES = {"Roll": "Roll rudiments", "Diddle": "Diddle rudiments", "Flam": "Flam rudiments", "Drag": "Drag rudiments"}
RUD = re.compile(
    r'rud\((\d+),\s*"([^"]+)",\s*Family::(\w+),\s*(true|false),\s*(\w+),\s*"([^"]*)",\s*"([^"]*)"\)'
)


def version() -> str:
    for line in (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        m = re.match(r'version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    sys.exit("no version in Cargo.toml")


def sticking_html(pattern: str) -> str:
    """The pattern as the sticking is written: grace notes lower case, accents bold."""
    out = []
    for token in pattern.split():
        body = token.split(":")[0]
        accent = body.endswith(">")
        body = body.rstrip(">")
        text = "buzz" if body == "Z" else html.escape(body)
        out.append(f"<b>{text}</b>" if accent else text)
    return " ".join(out)


def beats(unit: float, pattern: str) -> int:
    end = 0.0
    for token in pattern.split():
        n = float(token.split(":")[1]) if ":" in token else 1.0
        end += n * unit
    return max(1, math.ceil(end - 1e-9))


def rudiment_tables() -> str:
    source = (ROOT / "src" / "rudiments.rs").read_text(encoding="utf-8")
    found = RUD.findall(source)
    if len(found) != 40:
        sys.exit(f"expected 40 rudiments in src/rudiments.rs, found {len(found)}")
    parts = []
    for family, title in FAMILIES.items():
        rows = []
        for number, name, fam, nard, unit, pattern, about in found:
            if fam != family:
                continue
            star = ' <span class="nard" title="One of the original NARD Standard 26">*</span>' if nard == "true" else ""
            rows.append(
                f'<tr data-name="{html.escape(name.lower())}"><td>{number}</td><td>{html.escape(name)}{star}</td>'
                f'<td class="sticking">{sticking_html(pattern)}</td>'
                f"<td>{beats(UNITS[unit], pattern)}</td><td>{html.escape(about)}</td></tr>"
            )
        parts.append(
            f'<h3 id="rudiments-{family.lower()}">{title}</h3>\n<table class="rudiments">'
            "<thead><tr><th>#</th><th>Rudiment</th><th>Sticking (right lead)</th><th>Beats</th><th>What it is</th></tr></thead>"
            "<tbody>\n" + "\n".join(rows) + "\n</tbody></table>"
        )
    return "\n".join(parts)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--label", default="", help="release label shown on the title page, e.g. v0.6.0 or preview")
    ap.add_argument("--out", default=str(ROOT / "target" / "manual" / "roll-model-manual.html"))
    args = ap.parse_args()

    text = TEMPLATE.read_text(encoding="utf-8")
    ver = version()
    label = args.label or f"v{ver}"
    replacements = {
        "{{VERSION}}": html.escape(ver),
        "{{LABEL}}": html.escape(label),
        "{{DATE}}": date.today().isoformat(),
        "{{RUDIMENT_TABLES}}": rudiment_tables(),
    }
    for key, value in replacements.items():
        if key not in text:
            sys.exit(f"{key} is not in the template")
        text = text.replace(key, value)
    left = re.findall(r"\{\{[A-Z_]+\}\}", text)
    if left:
        sys.exit(f"unfilled placeholders: {sorted(set(left))}")
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {out} ({len(text) // 1024} KiB, version {ver}, label {label})")


if __name__ == "__main__":
    main()
