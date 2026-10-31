#!/usr/bin/env python3
"""Writes the top of a GitHub release's notes from CHANGELOG.md.

    python3 tools/release_notes.py v0.8.0 [CHANGELOG.md]

The abstract is the paragraph under the version's heading, before its first `###`
(the 0.8.0 section starts with one). The output is that paragraph and a link to the
version's full section. A version without an abstract gives only the link, so a
release never fails for want of one. release.yml appends the rest of the body.
"""
import re
import sys
from pathlib import Path

REPO = "https://github.com/pKasprowicz/roll-model"


def section(text: str, version: str):
    """The heading line and the lines under it, up to the next `## [` heading."""
    lines = text.replace("\r\n", "\n").split("\n")
    start = next((i for i, l in enumerate(lines) if l.startswith(f"## [{version}]")), None)
    if start is None:
        return None, []
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## [")), len(lines))
    return lines[start], lines[start + 1:end]


def abstract(body: list) -> str:
    """The text before the first `###`, with its paragraph breaks."""
    intro = []
    for line in body:
        if line.startswith("### "):
            break
        intro.append(line)
    return "\n".join(intro).strip()


def anchor(heading: str) -> str:
    """GitHub's anchor for a heading: lower case, punctuation dropped, spaces to hyphens."""
    text = heading.lstrip("# ").lower()
    return re.sub(r"[^\w\- ]", "", text).replace(" ", "-")


def notes(tag: str, changelog: str) -> str:
    version = tag.lstrip("v")
    heading, body = section(changelog, version)
    if heading is None:
        return ""
    link = f"[What changed in {version}]({REPO}/blob/{tag}/CHANGELOG.md#{anchor(heading)})"
    intro = abstract(body)
    return f"{intro}\n\n{link}\n" if intro else f"{link}\n"


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    path = Path(sys.argv[2] if len(sys.argv) > 2 else "CHANGELOG.md")
    sys.stdout.buffer.write(notes(sys.argv[1], path.read_text(encoding="utf-8")).encode("utf-8"))
