#!/usr/bin/env python3
"""Builds the user manual: one self-contained HTML file.

    python3 tools/build_manual.py [--label LABEL] [--out FILE]

It fills docs/manual/manual.template.html with the version from Cargo.toml and
generates the rudiment reference from docs/manual/rudiments.json (the core crate writes it and tests it), so the
manual cannot drift from what the program offers. Standard library only.
"""
import argparse
import html
import json
import re
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "docs" / "manual" / "manual.template.html"
FAMILIES = {"Roll": "Roll rudiments", "Diddle": "Diddle rudiments", "Flam": "Flam rudiments", "Drag": "Drag rudiments"}


def style() -> str:
    """The stylesheet shared by the manual and the developer guide."""
    return (ROOT / "docs" / "style.css").read_text(encoding="utf-8")


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


def rudiment_tables() -> str:
    # Written by `cargo run -p roll-model-core --example dump_rudiments`; a test of the core crate fails
    # when the file is not the catalogue, so this does not read Rust source.
    found = json.loads((ROOT / "docs" / "manual" / "rudiments.json").read_text(encoding="utf-8"))
    if len(found) != 40:
        sys.exit(f"expected 40 rudiments in docs/manual/rudiments.json, found {len(found)}")
    parts = []
    for family, title in FAMILIES.items():
        rows = []
        for r in found:
            number, name, pattern, about = r["number"], r["name"], r["pattern"], r["about"]
            if r["family"] != family:
                continue
            star = ' <span class="nard" title="One of the original NARD Standard 26">*</span>' if r["nard"] else ""
            rows.append(
                f'<tr data-name="{html.escape(name.lower())}"><td>{number}</td><td>{html.escape(name)}{star}</td>'
                f'<td class="sticking">{sticking_html(pattern)}</td>'
                f"<td>{r['beats']}</td><td>{html.escape(about)}</td></tr>"
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
        "{{STYLE}}": style(),
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
