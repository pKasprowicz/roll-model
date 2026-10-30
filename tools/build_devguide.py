#!/usr/bin/env python3
"""Builds the developer guide: one self-contained HTML file.

    python3 tools/build_devguide.py [--label LABEL] [--out FILE]

It fills docs/developer/developer.template.html. Facts that change with the code are
never typed into the template; they are generated from the source:

  {{STATS}}, {{MODULES}}, {{DEPENDENCIES}}, {{RUDIMENT_DSL}}
  {{code:PATH::TEXT}} or {{code:PATH::TEXT::MAXLINES}}
        the Rust item whose first line contains TEXT, with its doc comments and
        attributes, syntax highlighted (cut after MAXLINES lines if given)
  {{lines:PATH::FIRST::LAST}}
        plain lines FIRST..LAST (1-based) of any text file

A snippet that cannot be found stops the build, so a refactoring that renames or
moves an item shows up as a failed release build with the name of the snippet to fix.
Standard library only.
"""
import argparse
import html
import re
import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_manual import ROOT, style, version  # noqa: E402

TEMPLATE = ROOT / "docs" / "developer" / "developer.template.html"

KEYWORDS = set(
    "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut "
    "pub ref return self Self static struct super trait true type unsafe use where while".split()
)
TOKEN = re.compile(
    r"""(?P<c>//[^\n]*)
      |(?P<s>"(?:\\.|[^"\\])*")
      |(?P<a>\#!?\[[^\]\n]*\])
      |(?P<l>'[a-z_]+\b(?!'))
      |(?P<n>\b\d[\d_]*(?:\.\d+)?(?:f32|f64|u8|u16|u32|u64|usize|i32|i64)?\b)
      |(?P<w>[A-Za-z_][A-Za-z0-9_]*!?)""",
    re.X,
)


def highlight(code: str) -> str:
    out, pos = [], 0
    for m in TOKEN.finditer(code):
        out.append(html.escape(code[pos:m.start()]))
        text, kind = m.group(), m.lastgroup
        esc = html.escape(text)
        if kind == "w":
            if text.endswith("!"):
                cls = "m"
            elif text in KEYWORDS:
                cls = "k"
            elif text[0].isupper():
                cls = "t"
            else:
                cls = None
        else:
            cls = {"c": "c", "s": "s", "a": "a", "l": "k", "n": "n"}[kind]
        out.append(f'<span class="{cls}">{esc}</span>' if cls else esc)
        pos = m.end()
    out.append(html.escape(code[pos:]))
    return "".join(out)


def read_lines(rel: str) -> list[str]:
    path = ROOT / rel
    if not path.is_file():
        sys.exit(f"snippet file not found: {rel}")
    return path.read_text(encoding="utf-8").replace("\r\n", "\n").split("\n")


def brace_delta(line: str) -> int:
    line = re.sub(r'"(?:\\.|[^"\\])*"', '""', line.split("//")[0])
    line = re.sub(r"'.'", "''", line)
    return line.count("{") - line.count("}")


def extract_item(rel: str, needle: str, max_lines: int | None) -> str:
    lines = read_lines(rel)
    start = next((i for i, l in enumerate(lines) if needle in l and not l.lstrip().startswith("//")), None)
    if start is None:
        sys.exit(f"snippet not found in {rel}: {needle!r}")
    first = start
    while first > 0 and lines[first - 1].lstrip().startswith(("///", "#[")):
        first -= 1
    depth, opened, end = 0, False, start
    for i in range(start, len(lines)):
        d = brace_delta(lines[i])
        if "{" in lines[i].split("//")[0]:
            opened = True
        depth += d
        end = i
        if (opened and depth <= 0) or (not opened and lines[i].rstrip().endswith(";")):
            break
    body = lines[first:end + 1]
    if max_lines and len(body) > max_lines:
        body = body[:max_lines] + [f"    // … ({len(body) - max_lines} more lines in {rel})"]
    indent = min((len(l) - len(l.lstrip()) for l in body if l.strip()), default=0)
    text = "\n".join(l[indent:] for l in body)
    return f'<pre class="code"><span class="src">{html.escape(rel)}</span><code>{highlight(text)}</code></pre>'


def extract_lines(rel: str, first: int, last: int) -> str:
    lines = read_lines(rel)[first - 1:last]
    return f'<pre class="code"><span class="src">{html.escape(rel)}</span><code>{html.escape(chr(10).join(lines))}</code></pre>'


def module_files() -> list[Path]:
    files = [p for root in ("src", "core/src", "core/tests") for p in (ROOT / root).rglob("*.rs")]
    return sorted(files, key=lambda p: (p.relative_to(ROOT).parts[0] != "src", p.parent.as_posix(), p.name))


def summary(text: str) -> str:
    docs = []
    for line in text.splitlines():
        if line.startswith("//!"):
            docs.append(line[3:].strip())
        elif docs:
            break
    joined = " ".join(d for d in docs if d)
    first = re.split(r"(?<=[.:])\s", joined, maxsplit=1)[0] if joined else ""
    return first or joined


def stats_and_modules() -> tuple[str, str]:
    rows, total_lines, total_tests = [], 0, 0
    for p in module_files():
        text = p.read_text(encoding="utf-8")
        n, tests = text.count("\n") + 1, text.count("#[test]")
        total_lines += n
        total_tests += tests
        rel = p.relative_to(ROOT).as_posix()
        rows.append(f"<tr><td><code>{rel}</code></td><td>{n}</td><td>{tests}</td><td>{html.escape(summary(text))}</td></tr>")
    table = (
        '<table class="compact"><thead><tr><th>File</th><th>Lines</th><th>Tests</th><th>What it is</th></tr></thead><tbody>\n'
        + "\n".join(rows)
        + "\n</tbody></table>"
    )
    stats = (
        f"<strong>{len(rows)}</strong> Rust files, <strong>{total_lines:,}</strong> lines, "
        f"<strong>{total_tests}</strong> unit tests (counted from the source when this guide was built)."
    )
    return stats, table


PURPOSE = {
    "eframe": "Window, event loop and the glue between egui and the operating system",
    "egui": "The immediate-mode GUI library everything is drawn with",
    "midir": "Cross-platform MIDI input and output (drum module, controller, MIDI playback)",
    "rfd": "Native open and save file dialogs",
    "zip": "Reads compressed MusicXML (<code>.mxl</code>)",
    "hound": "Reads the WAV files that replace built-in drum sounds",
    "cpal": "Audio output (optional feature <code>audio</code>)",
    "rtrb": "Wait-free queues between the scheduling threads and the audio callback",
}


def dependencies() -> str:
    rows, in_deps = [], False
    lines = []
    for manifest in (ROOT / "Cargo.toml", ROOT / "core" / "Cargo.toml"):
        lines += manifest.read_text(encoding="utf-8").splitlines() + ["["]
    for line in lines:
        if line.startswith("["):
            in_deps = line.strip() == "[dependencies]"
            continue
        m = re.match(r'([A-Za-z0-9_-]+)\s*=\s*(.*)', line)
        if in_deps and m and m.group(1) != "roll-model-core":
            name, spec = m.groups()
            v = re.search(r'"([^"]+)"', spec)
            optional = "optional = true" in spec
            rows.append(
                f"<tr><td><code>{name}</code></td><td>{html.escape(v.group(1) if v else '')}</td>"
                f"<td>{'optional' if optional else 'yes'}</td><td>{PURPOSE.get(name, '')}</td></tr>"
            )
    return (
        '<table class="compact"><thead><tr><th>Crate</th><th>Version</th><th>Always</th><th>Used for</th></tr></thead><tbody>\n'
        + "\n".join(rows)
        + "\n</tbody></table>"
    )


def rudiment_dsl() -> str:
    lines = []
    for line in (ROOT / "core" / "src" / "rudiments.rs").read_text(encoding="utf-8").splitlines():
        if line.startswith("//!"):
            lines.append(line[3:].removeprefix(" "))
        elif lines:
            break
    return f'<pre class="code"><span class="src">core/src/rudiments.rs (module documentation)</span><code>{html.escape(chr(10).join(lines))}</code></pre>'


CODE = re.compile(r"\{\{code:([^:}]+)::([^}]*?)(?:::(\d+))?\}\}")
LINES = re.compile(r"\{\{lines:([^:}]+)::(\d+)::(\d+)\}\}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--label", default="")
    ap.add_argument("--out", default=str(ROOT / "target" / "devguide" / "roll-model-developer-guide.html"))
    args = ap.parse_args()

    text = TEMPLATE.read_text(encoding="utf-8")
    ver = version()
    stats, modules = stats_and_modules()
    text = CODE.sub(lambda m: extract_item(m.group(1), m.group(2).strip(), int(m.group(3)) if m.group(3) else None), text)
    text = LINES.sub(lambda m: extract_lines(m.group(1), int(m.group(2)), int(m.group(3))), text)
    fixed = {
        "{{VERSION}}": html.escape(ver),
        "{{LABEL}}": html.escape(args.label or f"v{ver}"),
        "{{DATE}}": date.today().isoformat(),
        "{{STATS}}": stats,
        "{{MODULES}}": modules,
        "{{DEPENDENCIES}}": dependencies(),
        "{{RUDIMENT_DSL}}": rudiment_dsl(),
        "{{STYLE}}": style(),
    }
    for key, value in fixed.items():
        if key not in text:
            sys.exit(f"{key} is not in the template")
        text = text.replace(key, value)
    left = re.findall(r"\{\{[A-Za-z_:]+", text)
    if left:
        sys.exit(f"unfilled placeholders: {sorted(set(left))}")
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {out} ({len(text) // 1024} KiB, version {ver})")


if __name__ == "__main__":
    main()
