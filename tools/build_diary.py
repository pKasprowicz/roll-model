#!/usr/bin/env python3
"""Builds the development diary of the site: entries, the roadmap and a feed.

    python3 tools/build_diary.py [--out DIR] [--issues-json FILE]

The entries are Markdown files in docs/diary/ (see docs/diary/README.md). The roadmap is not
written by hand: it is made from the GitHub issues, by their `roadmap:now`, `roadmap:next`
and `roadmap:later` labels (and `idea`), so it cannot drift from the issue list. DIR gets
diary/index.html, diary/roadmap/index.html, diary/<entry>/index.html and diary/feed.xml.
Standard library only; `gh` is needed for the roadmap (--issues-json reads a saved list instead).
"""
import argparse
import html
import json
import re
import subprocess
import sys
from datetime import date, datetime, timezone
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import build_manual  # noqa: E402  (its version() and style() are shared)

ROOT = build_manual.ROOT
DIARY = ROOT / "docs" / "diary"
SHELL = DIARY / "template.html"

HORIZONS = [
    ("roadmap:now", "Now", "What is being worked on."),
    ("roadmap:next", "Next", "What comes after that."),
    ("roadmap:later", "Later", "Wanted, with no order yet."),
]
IDEA = "idea"


class Entry:
    def __init__(self, slug: str, title: str, day: date, summary: str, issues: list, body: str):
        self.slug, self.title, self.day, self.summary, self.issues, self.body = slug, title, day, summary, issues, body


def repository() -> str:
    for line in (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        m = re.match(r'repository\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1).rstrip("/").removesuffix(".git")
    sys.exit("no repository in Cargo.toml")


def site_url(repo: str) -> str:
    """Where Pages serves the site: https://OWNER.github.io/NAME/."""
    m = re.match(r"https://github\.com/([^/]+)/([^/]+)$", repo)
    return f"https://{m.group(1).lower()}.github.io/{m.group(2)}/" if m else repo + "/"


# Entries.

def parse_entry(path: Path) -> Entry:
    """A diary file: a block of `key: value` lines between `---` lines, then the Markdown text."""
    text = path.read_text(encoding="utf-8").replace("\r\n", "\n")
    m = re.match(r"---\n(.*?)\n---\n(.*)\Z", text, re.S)
    if not m:
        raise ValueError(f"{path.name}: it must begin with a --- block (title, date, summary)")
    meta = {}
    for line in m.group(1).split("\n"):
        if line.strip() and not line.lstrip().startswith("#"):
            key, _, value = line.partition(":")
            meta[key.strip()] = value.strip()
    for key in ("title", "date", "summary"):
        if not meta.get(key):
            raise ValueError(f"{path.name}: `{key}:` is missing")
    try:
        day = date.fromisoformat(meta["date"])
    except ValueError:
        raise ValueError(f"{path.name}: the date `{meta['date']}` is not YYYY-MM-DD") from None
    if not path.stem.startswith(meta["date"]):
        raise ValueError(f"{path.name}: the file name must begin with its date, {meta['date']}")
    issues = [int(n) for n in re.findall(r"\d+", meta.get("issues", ""))]
    body = m.group(2).strip()
    if not body:
        raise ValueError(f"{path.name}: there is no text")
    return Entry(path.stem, meta["title"], day, meta["summary"], issues, body)


def load_entries(folder: Path = DIARY) -> list:
    """Every entry, the newest first."""
    entries = [parse_entry(p) for p in sorted(folder.glob("*.md")) if p.name.lower() != "readme.md"]
    entries.sort(key=lambda e: (e.day, e.slug), reverse=True)
    return entries


# Markdown: the small part of it that the entries use.

def inline(text: str, repo: str) -> str:
    """Code, bold, italic, links and issue numbers (#107) of one line of text."""
    spans = []

    def keep(markup: str) -> str:
        spans.append(markup)
        return f"\x00{len(spans) - 1}\x00"

    text = re.sub(r"`([^`]+)`", lambda m: keep(f"<code>{html.escape(m.group(1))}</code>"), text)
    text = re.sub(
        r"\[([^\]]+)\]\(([^)\s]+)\)",
        lambda m: keep(f'<a href="{html.escape(m.group(2), quote=True)}">{html.escape(m.group(1))}</a>'),
        text,
    )
    text = html.escape(text, quote=False)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", text)
    text = re.sub(r"(?<![\w&/])#(\d+)\b", lambda m: f'<a href="{repo}/issues/{m.group(1)}">#{m.group(1)}</a>', text)
    return re.sub(r"\x00(\d+)\x00", lambda m: spans[int(m.group(1))], text)


def markdown(text: str, repo: str) -> str:
    """Paragraphs, ## headings, lists, quotes, code blocks and the inline marks of `inline`."""
    out, lines, i = [], text.split("\n"), 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            i += 1
        elif line.startswith("```"):
            i += 1
            code = []
            while i < len(lines) and not lines[i].startswith("```"):
                code.append(lines[i])
                i += 1
            i += 1
            out.append("<pre><code>" + html.escape("\n".join(code)) + "</code></pre>")
        elif re.match(r"#{2,4} ", line):
            level = len(line) - len(line.lstrip("#"))
            out.append(f"<h{level}>{inline(line[level + 1:].strip(), repo)}</h{level}>")
            i += 1
        elif re.match(r"(- |\* )", line) or re.match(r"\d+\. ", line):
            ordered = bool(re.match(r"\d+\. ", line))
            marker = r"\d+\. " if ordered else r"(?:- |\* )"
            items = []
            while i < len(lines) and re.match(marker, lines[i]):
                item = re.sub(marker, "", lines[i], count=1)
                i += 1
                while i < len(lines) and lines[i].startswith("  ") and lines[i].strip():
                    item += " " + lines[i].strip()
                    i += 1
                items.append(f"<li>{inline(item, repo)}</li>")
            tag = "ol" if ordered else "ul"
            out.append(f"<{tag}>" + "".join(items) + f"</{tag}>")
        elif line.startswith(">"):
            quote = []
            while i < len(lines) and lines[i].startswith(">"):
                quote.append(lines[i].lstrip("> ").rstrip())
                i += 1
            out.append(f'<div class="callout">{inline(" ".join(quote), repo)}</div>')
        else:
            para = []
            while i < len(lines) and lines[i].strip() and not re.match(r"(```|#{2,4} |- |\* |\d+\. |>)", lines[i]):
                para.append(lines[i].strip())
                i += 1
            out.append(f"<p>{inline(' '.join(para), repo)}</p>")
    return "\n".join(out)


# The roadmap: made from the issues.

def fetch_issues(saved: str = "") -> "list | None":
    """The issues of the repository, or None when they cannot be read (a warning goes to stderr)."""
    try:
        if saved:
            return json.loads(Path(saved).read_text(encoding="utf-8"))
        result = subprocess.run(
            ["gh", "issue", "list", "--state", "all", "--limit", "500", "--json", "number,title,labels,state,closedAt,url"],
            check=True, capture_output=True, text=True, encoding="utf-8",
        )
        return json.loads(result.stdout)
    except (OSError, subprocess.CalledProcessError, ValueError) as e:
        print(f"warning: the roadmap cannot be made, the issues were not read ({e})", file=sys.stderr)
        return None


def label_names(issue: dict) -> set:
    return {label["name"] for label in issue.get("labels", [])}


def plan(issues: list) -> dict:
    """The roadmap as lists: a key for each horizon, `ideas` and `done` (closed, newest first)."""
    result = {label: [] for label, _, _ in HORIZONS}
    result["ideas"], result["done"] = [], []
    for issue in sorted(issues, key=lambda i: i["number"]):
        names = label_names(issue)
        horizon = next((label for label, _, _ in HORIZONS if label in names), None)
        if issue["state"].upper() == "CLOSED":
            if horizon or IDEA in names:
                result["done"].append(issue)
        elif horizon:
            result[horizon].append(issue)
        elif IDEA in names:
            result["ideas"].append(issue)
    result["done"].sort(key=lambda i: i.get("closedAt") or "", reverse=True)
    result["done"] = result["done"][:12]
    return result


def issue_item(issue: dict, repo: str) -> str:
    flags = ""
    if "needs-device" in label_names(issue):
        flags = ' <span class="pill">waits for a device</span>'
    elif "needs-decision" in label_names(issue):
        flags = ' <span class="pill">waits for a decision</span>'
    title = html.escape(re.sub(r"^[A-Z]+-\d+:\s*", "", issue["title"]))
    return f'<li><a href="{repo}/issues/{issue["number"]}">#{issue["number"]}</a> {title}{flags}</li>'


def roadmap_body(issues: "list | None", repo: str) -> str:
    if issues is None:
        return "<p>The roadmap could not be read from GitHub when this page was built. It is made from the issues, so see the <a href=\"%s/issues\">issue list</a>.</p>" % repo
    lists = plan(issues)
    parts = [
        "<p>This page is made from the issues of the project by their labels, so it is as current as the issue list. "
        "There are no dates: <em>Now</em> is what is being worked on, <em>Next</em> follows it, <em>Later</em> is wanted but not ordered.</p>"
    ]
    for label, title, blurb in HORIZONS:
        items = "".join(issue_item(i, repo) for i in lists[label]) or "<li><em>nothing</em></li>"
        parts.append(f"<h2>{title}</h2><p>{blurb}</p><ul class=\"roadmap\">{items}</ul>")
    if lists["ideas"]:
        parts.append("<h2>Ideas</h2><p>Suggested, not decided.</p><ul class=\"roadmap\">" + "".join(issue_item(i, repo) for i in lists["ideas"]) + "</ul>")
    if lists["done"]:
        parts.append("<h2>Recently done</h2><ul class=\"roadmap\">" + "".join(issue_item(i, repo) for i in lists["done"]) + "</ul>")
    parts.append(
        f'<p>Have an idea? <a href="{repo}/issues/new?template=idea.yml">Suggest it</a>; it becomes an issue, and the ones that are taken up appear here.</p>'
    )
    return "\n".join(parts)


# Pages.

def page(title: str, description: str, body: str, base: str, label: str) -> str:
    shell = SHELL.read_text(encoding="utf-8")
    values = {
        "{{TITLE}}": html.escape(title),
        "{{DESCRIPTION}}": html.escape(description, quote=True),
        "{{STYLE}}": build_manual.style(),
        "{{BASE}}": base,
        "{{BODY}}": body,
        "{{LABEL}}": html.escape(label),
    }
    for key, value in values.items():
        if key not in shell:
            sys.exit(f"{key} is not in {SHELL.name}")
        shell = shell.replace(key, value)
    return shell


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def entry_list(entries: list) -> str:
    if not entries:
        return "<p><em>No entries yet.</em></p>"
    rows = []
    for e in entries:
        rows.append(
            f'<li><span class="when">{e.day.isoformat()}</span> <a href="{e.slug}/">{html.escape(e.title)}</a>'
            f'<br><span class="sum">{html.escape(e.summary)}</span></li>'
        )
    return '<ul class="entries">' + "".join(rows) + "</ul>"


def feed(entries: list, repo: str, base_url: str) -> str:
    """An Atom feed of the entries, the newest first."""
    def stamp(day: date) -> str:
        return datetime(day.year, day.month, day.day, 12, tzinfo=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    updated = stamp(entries[0].day) if entries else stamp(date.today())
    out = [
        '<?xml version="1.0" encoding="utf-8"?>',
        '<feed xmlns="http://www.w3.org/2005/Atom">',
        "<title>Roll Model: development diary</title>",
        f'<link href="{base_url}diary/" rel="alternate"/>',
        f'<link href="{base_url}diary/feed.xml" rel="self"/>',
        f"<id>{base_url}diary/</id>",
        f"<updated>{updated}</updated>",
        "<author><name>Roll Model</name></author>",
    ]
    for e in entries:
        url = f"{base_url}diary/{e.slug}/"
        out += [
            "<entry>",
            f"<title>{html.escape(e.title)}</title>",
            f'<link href="{url}"/>',
            f"<id>{url}</id>",
            f"<updated>{stamp(e.day)}</updated>",
            f"<summary>{html.escape(e.summary)}</summary>",
            f'<content type="html">{html.escape(markdown(e.body, repo))}</content>',
            "</entry>",
        ]
    out.append("</feed>")
    return "\n".join(out) + "\n"


def build(out: Path, label: str, issues: "list | None", entries: "list | None" = None) -> list:
    """Writes the diary into `out`/diary and returns the paths written."""
    repo = repository()
    entries = load_entries() if entries is None else entries
    root = out / "diary"
    written = []

    def emit(path: Path, text: str) -> None:
        write(path, text)
        written.append(path)

    intro = (
        "<p>What is being built, what changed in each version, and what may come. "
        f'See the <a href="roadmap/">roadmap</a>, or follow the <a href="feed.xml">feed</a>.</p>'
    )
    emit(root / "index.html", page("Development diary", "What is new in Roll Model and what is coming.", f"<h1>Development diary</h1>{intro}{entry_list(entries)}", "../", label))
    emit(root / "roadmap" / "index.html", page("Roadmap", "What Roll Model is working on next.", f"<h1>Roadmap</h1>{roadmap_body(issues, repo)}", "../../", label))
    for e in entries:
        related = ""
        if e.issues:
            related = "<p class=\"related\">Issues: " + ", ".join(f'<a href="{repo}/issues/{n}">#{n}</a>' for n in e.issues) + "</p>"
        body = f'<p class="when">{e.day.isoformat()}</p><h1>{html.escape(e.title)}</h1>{markdown(e.body, repo)}{related}'
        emit(root / e.slug / "index.html", page(e.title, e.summary, body, "../../", label))
    emit(root / "feed.xml", feed(entries, repo, site_url(repo)))
    return written


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=str(ROOT / "target" / "site"), help="the folder the site is written to")
    ap.add_argument("--label", default="", help="label shown on the pages (default: v plus the Cargo.toml version)")
    ap.add_argument("--issues-json", default="", help="a saved `gh issue list --json ...` instead of asking GitHub")
    args = ap.parse_args()
    try:
        written = build(Path(args.out), args.label or f"v{build_manual.version()}", fetch_issues(args.issues_json))
    except ValueError as e:
        sys.exit(f"diary: {e}")
    print(f"wrote {len(written)} files of the diary to {Path(args.out) / 'diary'}")


if __name__ == "__main__":
    main()
