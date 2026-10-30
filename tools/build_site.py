#!/usr/bin/env python3
"""Builds the documentation site that GitHub Pages serves.

    python3 tools/build_site.py [--label LABEL] [--out DIR]

DIR gets index.html (a start page that links to both documents), manual/index.html
(the user manual) and developer/index.html (the developer guide). The two documents
are made by build_manual.py and build_devguide.py, the scripts the release uses for
its files, so the site and the release archives hold the same pages. Standard
library only.
"""
import argparse
import html
import re
import subprocess
import sys
from datetime import date
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import build_manual  # noqa: E402  (its version() and style() are shared)

ROOT = build_manual.ROOT
TEMPLATE = ROOT / "docs" / "site" / "index.template.html"


def repository() -> str:
    """The repository URL from Cargo.toml, without a trailing slash or .git."""
    for line in (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        m = re.match(r'repository\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1).rstrip("/").removesuffix(".git")
    sys.exit("no repository in Cargo.toml")


def build_document(script: str, label: str, out: Path) -> None:
    command = [sys.executable, str(TOOLS / script), "--out", str(out)]
    if label:
        command += ["--label", label]
    subprocess.run(command, check=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--label", default="", help="label shown on the pages, e.g. v0.7.2 (default: v plus the Cargo.toml version)")
    ap.add_argument("--out", default=str(ROOT / "target" / "site"), help="the folder the site is written to")
    args = ap.parse_args()

    out = Path(args.out)
    build_document("build_manual.py", args.label, out / "manual" / "index.html")
    build_document("build_devguide.py", args.label, out / "developer" / "index.html")

    ver = build_manual.version()
    text = TEMPLATE.read_text(encoding="utf-8")
    replacements = {
        "{{LABEL}}": html.escape(args.label or f"v{ver}"),
        "{{DATE}}": date.today().isoformat(),
        "{{REPO}}": html.escape(repository()),
        "{{STYLE}}": build_manual.style(),
    }
    for key, value in replacements.items():
        if key not in text:
            sys.exit(f"{key} is not in the template")
        text = text.replace(key, value)
    left = re.findall(r"\{\{[A-Z_]+\}\}", text)
    if left:
        sys.exit(f"unfilled placeholders: {sorted(set(left))}")
    (out / "index.html").write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {out}: index.html, manual/index.html, developer/index.html (version {ver})")


if __name__ == "__main__":
    main()
