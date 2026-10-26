#!/usr/bin/env python3
"""Writes THIRD_PARTY_NOTICES.txt: the licence texts of everything Roll Model is built from.

    python3 tools/build_notices.py [--out FILE]

The dependency list comes from `cargo metadata --locked` (all platforms), the texts
from the licence and notice files shipped inside each crate, including the fonts
that egui bundles. Identical texts are printed once, with the packages they belong
to. Needed because MIT, BSD, Apache and font licences require their notices to
accompany binary copies. Standard library only.
"""
import argparse
import json
import subprocess
import sys
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAMES = ("license", "licence", "copying", "notice", "unlicense", "copyright", "authors")
# Crates that carry extra licence texts in subfolders (bundled fonts).
EXTRA = {"epaint_default_fonts": ["fonts/OFL.txt", "fonts/UFL.txt", "fonts/emoji-icon-font-mit-license.txt", "fonts/Hack-Regular.txt"]}


def metadata() -> dict:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT, check=True, capture_output=True, text=True, encoding="utf-8",
    ).stdout
    return json.loads(out)


def licence_files(pkg: dict) -> list[Path]:
    folder = Path(pkg["manifest_path"]).parent
    found = [
        p for p in sorted(folder.iterdir())
        if p.is_file() and p.name.lower().startswith(NAMES) and p.suffix.lower() in ("", ".md", ".txt")
    ]
    for rel in EXTRA.get(pkg["name"], []):
        p = folder / rel
        if p.is_file():
            found.append(p)
    return found


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=str(ROOT / "target" / "notices" / "THIRD_PARTY_NOTICES.txt"))
    args = ap.parse_args()

    meta = metadata()
    texts: "OrderedDict[str, list[str]]" = OrderedDict()   # text -> packages
    without: list[str] = []
    count = 0
    for pkg in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
        if pkg["name"] == "roll-model":
            continue
        count += 1
        label = f'{pkg["name"]} {pkg["version"]} ({pkg.get("license") or "see package"})'
        files = licence_files(pkg)
        if not files:
            who = ", ".join(pkg.get("authors") or []) or "see repository"
            without.append(label + "\n      authors: " + who + "; " + (pkg.get("repository") or "no repository given"))
        for f in files:
            body = f.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n").strip()
            if body:
                texts.setdefault(body, []).append(label)

    lines = [
        "Third-party notices for Roll Model",
        "=" * 34,
        "",
        "Roll Model is built from the open-source components below. Each is used under",
        "its own licence, the text of which follows. Roll Model's own licence is in the",
        "file LICENSE.",
        "",
        f"{count} components. Components under a dual licence (for example MIT OR Apache-2.0)",
        "may be used under either one.",
        "",
        "Fonts bundled by the egui library: Hack (MIT-style licence), Noto Emoji (SIL OFL 1.1),",
        "Ubuntu Light (Ubuntu Font Licence 1.0) and emoji-icon-font (MIT). Their licence",
        "texts are included below under the package epaint_default_fonts.",
        "",
    ]
    for body, packages in texts.items():
        lines += ["-" * 78, "Used by:"]
        lines += [f"  {p}" for p in sorted(set(packages))]
        lines += ["", body, ""]
    if without:
        lines += [
            "-" * 78,
            "These packages ship no separate licence file; they are used under the licence",
            "they declare (the SPDX identifier in brackets). The standard texts of the MIT and",
            "Apache-2.0 licences appear above; the copyright holders are the authors named here",
            "and in the package's repository:",
        ]
        lines += [f"  {p}" for p in without]
        lines.append("")
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    print(f"wrote {out}: {count} components, {len(texts)} distinct texts, {len(without)} without a licence file")
    if not texts:
        sys.exit("no licence texts found")


if __name__ == "__main__":
    main()
