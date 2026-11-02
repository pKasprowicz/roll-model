#!/usr/bin/env python3
"""Helpers for writing the development diary (docs/diary/).

    python3 tools/diary.py facts             what happened since the last entry
    python3 tools/diary.py new TITLE --summary TEXT [--issues 107,108] [--date YYYY-MM-DD] [--body-file FILE|-]
    python3 tools/diary.py check             read every entry and say what is wrong

`facts` collects what an entry can be written from: the releases and the commits since the last
entry and the issues closed since. `new` writes a file in the right place with the right name
and checks it. The text itself is for a person (or the diary-entry skill) to write.
Standard library only; `gh` is used for the closed issues when it is there.
"""
import argparse
import re
import subprocess
import sys
from datetime import date
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import build_diary  # noqa: E402

DIARY = build_diary.DIARY


def slugify(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")[:60].strip("-") or "entry"


def new_entry(title: str, summary: str, body: str, day: date, issues: list, folder: Path = DIARY) -> Path:
    """Writes `folder`/DATE-slug.md, refusing to replace a file, and reads it back as the build will."""
    path = folder / f"{day.isoformat()}-{slugify(title)}.md"
    if path.exists():
        raise ValueError(f"{path.name} exists already: change the title, or edit that file")
    head = [f"title: {title}", f"date: {day.isoformat()}", f"summary: {summary}"]
    if issues:
        head.append("issues: " + ", ".join(str(n) for n in issues))
    folder.mkdir(parents=True, exist_ok=True)
    path.write_text("---\n" + "\n".join(head) + "\n---\n" + body.strip() + "\n", encoding="utf-8", newline="\n")
    try:
        build_diary.parse_entry(path)
    except ValueError:
        path.unlink()
        raise
    return path


def run(command: list) -> str:
    try:
        return subprocess.run(command, check=True, capture_output=True, text=True, encoding="utf-8", cwd=build_diary.ROOT).stdout
    except (OSError, subprocess.CalledProcessError):
        return ""


def facts() -> str:
    entries = build_diary.load_entries()
    since = entries[0].day.isoformat() if entries else ""
    out = [f"Last entry: {entries[0].title} ({since})" if entries else "No entry yet."]
    changelog = (build_diary.ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    releases = re.findall(r"^## \[(\d[^\]]*)\] - (\d{4}-\d\d-\d\d)", changelog, re.M)
    out.append("\nReleases" + (f" on or after {since}" if since else "") + ":")
    out += [f"  {v} ({d})" for v, d in releases if not since or d >= since] or ["  none"]
    log = run(["git", "log", "develop", "--no-merges", "--pretty=%ad %s", "--date=short"] + ([f"--since={since}"] if since else []) + ["-n", "80"])
    out.append("\nCommits (newest first, at most 80):")
    out += [f"  {line}" for line in log.splitlines()] or ["  none found"]
    closed = run(["gh", "issue", "list", "--state", "closed", "--limit", "60", "--json", "number,title,closedAt", "--jq",
                  '.[] | select(.closedAt >= "%s") | "#\\(.number) \\(.title)"' % since])
    out.append("\nIssues closed since:")
    out += [f"  {line}" for line in closed.splitlines()] or ["  none found (or gh is not available)"]
    out.append("\nUnreleased in the changelog:")
    m = re.search(r"## \[Unreleased\]\n(.*?)(?=\n## \[)", changelog, re.S)
    out.append("  " + (m.group(1).strip().replace("\n", "\n  ") if m and m.group(1).strip() else "nothing"))
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("facts")
    sub.add_parser("check")
    n = sub.add_parser("new")
    n.add_argument("title")
    n.add_argument("--summary", required=True, help="one sentence, shown in the list and the feed")
    n.add_argument("--issues", default="", help="issue numbers, comma separated")
    n.add_argument("--date", default="", help="YYYY-MM-DD (default: today)")
    n.add_argument("--body-file", default="-", help="the Markdown text; - reads it from the standard input")
    args = ap.parse_args()
    try:
        if args.command == "facts":
            print(facts())
        elif args.command == "check":
            entries = build_diary.load_entries()
            print(f"{len(entries)} entries, all readable")
        else:
            body = sys.stdin.read() if args.body_file == "-" else Path(args.body_file).read_text(encoding="utf-8")
            day = date.fromisoformat(args.date) if args.date else date.today()
            issues = [int(x) for x in re.findall(r"\d+", args.issues)]
            print(new_entry(args.title, args.summary, body, day, issues))
    except ValueError as e:
        sys.exit(f"diary: {e}")


if __name__ == "__main__":
    main()
