#!/usr/bin/env python3
"""Mechanical x-ray of a Rust workspace for the defect classes of the harsh code review.

    python3 scan.py [--root DIR] [--core CRATE_DIR] [--ui-dirs src/app,src] [--audio-dirs src/audio]
                    [--run-tools] [--examples N] [--json]

Each check is a heuristic that finds *candidates*: a person (the review) reads every one before it
becomes a finding. The counts are evidence for the report's verification table, not verdicts.
Standard library only. With --run-tools it also runs `cargo fmt --check` and `cargo clippy`.
"""
import argparse
import collections
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

SKIP_DIRS = {"target", ".git", "node_modules", "site", "dist"}


def rust_files(root: Path):
    for p in sorted(root.rglob("*.rs")):
        if not SKIP_DIRS.intersection(p.relative_to(root).parts):
            yield p


def is_test_file(rel: Path) -> bool:
    return "tests" in rel.parts or rel.name in ("tests.rs", "test.rs") or rel.name.endswith("_tests.rs")


def load(root: Path):
    """[(rel path, lines, first test line or None)] where lines after `#[cfg(test)]` are test code."""
    files = []
    for p in rust_files(root):
        rel = p.relative_to(root)
        lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
        cut = None
        for i, line in enumerate(lines):
            if line.strip().startswith("#[cfg(test)]"):
                cut = i
                break
        if is_test_file(rel):
            cut = 0
        files.append((rel, lines, cut))
    return files


def code_lines(files):
    """(rel, number, text) for every non-test line that is not a comment."""
    for rel, lines, cut in files:
        for i, line in enumerate(lines):
            if cut is not None and i >= cut:
                break
            s = line.strip()
            if s.startswith("//") or not s:
                continue
            yield rel, i + 1, line


def test_lines(files):
    for rel, lines, cut in files:
        if cut is None:
            continue
        for i in range(cut, len(lines)):
            yield rel, i + 1, lines[i]


class Result:
    def __init__(self, rv, name, count, where, note=""):
        self.rv, self.name, self.count, self.where, self.note = rv, name, count, where, note


def grep(files, pattern, only_code=True, flags=0):
    rx = re.compile(pattern, flags)
    source = code_lines(files) if only_code else (
        (rel, i + 1, l) for rel, lines, _ in files for i, l in enumerate(lines))
    return [(rel, n, line.strip()) for rel, n, line in source if rx.search(line)]


def fmt(hits, limit):
    return [f"{r}:{n}  {t[:110]}" for r, n, t in hits[:limit]]


def per_file(hits, limit):
    c = collections.Counter(str(r) for r, _, _ in hits)
    return [f"{p}: {n}" for p, n in c.most_common(limit)]


def fn_spans(files):
    """(rel, name, start line, length, signature text) of every fn, by brace matching (code only)."""
    spans = []
    for rel, lines, cut in files:
        end_code = cut if cut is not None else len(lines)
        i = 0
        while i < end_code:
            m = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)", lines[i])
            if not m:
                i += 1
                continue
            j, depth, started, sig = i, 0, False, []
            while j < len(lines):
                sig.append(lines[j])
                depth += lines[j].count("{") - lines[j].count("}")
                if "{" in lines[j]:
                    started = True
                if (started and depth <= 0) or (not started and lines[j].rstrip().endswith(";")):
                    break
                j += 1
            spans.append((rel, m.group(1), i + 1, j - i + 1, "\n".join(sig)))
            i += 1
    return spans


def params_of(sig: str):
    head = sig.split("{", 1)[0]
    m = re.search(r"fn\s+\w+\s*(?:<.*?>)?\s*\((.*)\)\s*(?:->|where|$)", head, re.S)
    if not m:
        return []
    out, depth, cur = [], 0, ""
    for ch in m.group(1):
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return [p for p in out if p and not re.match(r"&?(?:mut\s+)?self", p)]


def check_all(root: Path, args):
    files = load(root)
    n = args.examples
    res = []
    all_code = list(code_lines(files))
    total = sum(len(l) for _, l, _ in files)

    # Size and shape.
    sizes = sorted(((len(l), str(r)) for r, l, _ in files), reverse=True)
    res.append(Result("RV-13/RV-70", "Largest files (lines)", sizes[0][0] if sizes else 0, [f"{p}: {c}" for c, p in sizes[:n]], f"{total} lines in {len(files)} files"))
    long140 = [(r, i + 1, l.strip()) for r, lines, _ in files for i, l in enumerate(lines) if len(l) > 140]
    long200 = [h for h in long140 if len(h[2]) > 190]
    res.append(Result("RV-91", "Lines over 140 / over ~200 characters", len(long140), per_file(long140, n), f"{len(long200)} over ~200"))
    spans = fn_spans(files)
    big = sorted(spans, key=lambda s: -s[3])
    res.append(Result("RV-70/RV-13", "Functions over 80 lines", sum(1 for s in spans if s[3] > 80), [f"{s[0]}:{s[2]} {s[1]} ({s[3]} lines)" for s in big[:n]]))
    many = [(s, params_of(s[4])) for s in spans]
    many = [(s, p) for s, p in many if len(p) >= 6]
    res.append(Result("RV-22", "Functions with 6 or more parameters", len(many), [f"{s[0]}:{s[2]} {s[1]} ({len(p)})" for s, p in many[:n]]))
    bools = [(s, [p for p in ps if re.search(r":\s*bool\b", p)]) for s, ps in [(s, params_of(s[4])) for s in spans]]
    bools = [(s, b) for s, b in bools if b]
    res.append(Result("RV-22", "Functions with a bool parameter", len(bools), [f"{s[0]}:{s[2]} {s[1]}({', '.join(b)})" for s, b in bools[:n]]))

    # State machines, command paths, exhaustiveness.
    wild = grep(files, r"^\s*_\s*=>\s*(\{\s*\}|\(\))")
    res.append(Result("RV-12", "Wildcard match arms that do nothing (`_ => {}`)", len(wild), fmt(wild, n), "each turns off exhaustiveness checking"))
    guards = grep(files, r"=>|\bif\b.*=>", True)
    side_guard = [h for h in grep(files, r"\bif\s+(?:self\.)?\w+\([^)]*\)\s*=>") if "&mut" not in h[2]]
    res.append(Result("RV-12", "Match arms with a call in the guard (check each for side effects)", len(side_guard), fmt(side_guard, n)))

    # Error handling and robustness.
    unwraps = grep(files, r"\.unwrap\(\)|\.expect\(")
    res.append(Result("RV-5/RV-51", "unwrap()/expect() outside test code", len(unwraps), per_file(unwraps, n)))
    unsafe_n = grep(files, r"\bunsafe\b")
    res.append(Result("RV-15", "unsafe", len(unsafe_n), fmt(unsafe_n, n)))
    locks = grep(files, r"\.lock\(\)")
    styles = collections.Counter()
    for r, i, t in locks:
        styles["unwrap" if ".unwrap()" in t else "into_inner" if "into_inner" in t else "map_err/other" if "map_err" in t or "?" in t else "other"] += 1
    res.append(Result("RV-8", "Mutex lock() sites by poison policy (more than one policy is a finding)", len(locks), [f"{k}: {v}" for k, v in styles.most_common()]))
    writes = [h for h in grep(files, r"\bfs::write\(") if "atomic" not in h[2]]
    res.append(Result("RV-2", "fs::write straight onto a target (non-atomic?)", len(writes), fmt(writes, n), "check: temp file + rename, and what a failed read does"))
    ignored = grep(files, r"let\s+_\s*=\s*[^;]*(?:fs::|write|save|send|connect|open)")
    res.append(Result("RV-51", "Results thrown away with `let _ =` on I/O-like calls", len(ignored), fmt(ignored, n)))
    stringly = [(s, 1) for s in spans if re.search(r"->\s*Result<[^>]*,\s*String\s*>", s[4].split("{", 1)[0])]
    res.append(Result("RV-50", "Functions returning Result<_, String>", len(stringly), [f"{s[0]}:{s[2]} {s[1]}" for s, _ in stringly[:n]]))
    debug_only = grep(files, r"debug_assert|cfg!\(debug_assertions\)|#\[cfg\(debug_assertions\)\]")
    res.append(Result("RV-5", "Checks that exist only in debug builds", len(debug_only), fmt(debug_only, n)))
    unbounded = grep(files, r"Arc<Mutex<Vec<")
    res.append(Result("RV-7", "Shared Vec queues (is each one capped?)", len(unbounded), fmt(unbounded, n)))

    # Types and units.
    named_units = grep(files, r"\b\w+_(?:ms|secs?|pct|bpm|q|hz|db)\s*:\s*(?:f32|f64|u\d+|i\d+|usize)")
    res.append(Result("RV-20", "Quantities typed as primitives with the unit in the name", len(named_units), per_file(named_units, n)))
    tuples = grep(files, r"(?:Vec<|->\s*|:\s*)\(\s*[\w&:<>\[\]']+\s*,\s*[\w&:<>\[\]']+\s*,\s*[\w&:<>\[\]']+")
    res.append(Result("RV-21", "Tuples of three or more as types", len(tuples), fmt(tuples, n)))
    pubfields = grep(files, r"^\s*pub\s+(?:\([^)]*\)\s+)?\w+\s*:\s*[A-Za-z&\[(]")
    res.append(Result("RV-24", "Public (or pub(crate)) struct fields", len(pubfields), per_file(pubfields, n)))
    sentinel = grep(files, r"\b(?:usize::MAX|u8::MAX|u32::MAX|i32::MIN|-1\.0|255|100\s*\+)\b.*(?:=|==|!=|=>)")
    res.append(Result("RV-23", "Possible sentinel values (check by reading)", len(sentinel), fmt(sentinel, n)))
    casts = grep(files, r"\bas\s+(?:u8|u16|i8|i16|usize|u32)\b")
    res.append(Result("RV-41/RV-5", "Narrowing/index casts with `as` (enum indices persisted?)", len(casts), per_file(casts, n)))
    enum_index = grep(files, r"\bALL\[|from_index|\.position\(\|.*==\s*\*?self\)")
    res.append(Result("RV-41", "Enums looked up by position in a list", len(enum_index), fmt(enum_index, n)))

    # Magic numbers and several sources of truth.
    clamps = grep(files, r"\.clamp\(\s*[-\d.]")
    res.append(Result("RV-40", "clamp() with literal bounds (one range per setting?)", len(clamps), per_file(clamps, n)))
    ranges = grep(files, r"\.range\(\s*[-\d.]+\s*\.\.=|Slider::new\([^)]*[-\d.]+\.\.=[-\d.]+")
    res.append(Result("RV-40", "Slider/range literals beside the clamps (compare with the above)", len(ranges), per_file(ranges, n)))
    literals = grep(files, r"(?<![\w.\"'#])\d+\.\d+(?:f32|f64)?\b|(?<![\w.\"'#])[1-9]\d{2,}\b")
    literals = [h for h in literals if not re.match(r"\s*(?:const|static|pub const|//)", h[2])]
    res.append(Result("RV-43/RV-42", "Numeric literals in code (not const) by file", len(literals), per_file(literals, n)))

    # Coupling and the UI.
    drawing = [s for s in spans if re.match(r"(?:draw|paint|render|show|ui)\w*", s[1]) and re.search(r"&mut self", s[4].split("{", 1)[0])]
    res.append(Result("RV-18", "Drawing functions that take &mut self (do they change domain state?)", len(drawing), [f"{s[0]}:{s[2]} {s[1]}" for s in drawing[:n]]))
    status = grep(files, r"\b(?:status|error|notice|message)\s*=\s*(?:Some\()?(?:format!|\".|String::)")
    ui_dirs = [d for d in args.ui_dirs.split(",") if d]
    res.append(Result("RV-17", "Writes to a status/error/notice field (a return channel?)", len(status), per_file(status, n)))
    blocking = grep(files, r"\bfs::(?:read|write|read_dir|metadata|create)\w*\(|\.join\(\)|thread::sleep|list_inputs\(|list_outputs\(|Device|default_host\(")
    in_ui = [h for h in blocking if any(str(h[0]).replace("\\", "/").startswith(d.rstrip("/") + "/") for d in ui_dirs)]
    res.append(Result("RV-61", "Blocking-looking calls in UI directories", len(in_ui), per_file(in_ui, n)))
    frame_fns = [s for s in spans if re.match(r"(?:update|draw|show|paint|ui|render)\w*", s[1])]
    alloc = []
    for rel, name, start, length, sig in frame_fns:
        body = sig
        k = len(re.findall(r"\.clone\(\)|\bVec::new\(|\bvec!\[|format!\(|\.collect::<|\.to_string\(\)|\.to_vec\(\)|String::from", body))
        if k >= 6:
            alloc.append((k, rel, start, name))
    alloc.sort(reverse=True)
    res.append(Result("RV-60", "Per-frame functions with 6+ allocations/clones (candidates)", len(alloc), [f"{r}:{s} {nm} ({k})" for k, r, s, nm in alloc[:n]]))
    audio_dirs = [d for d in args.audio_dirs.split(",") if d]
    cb = [h for h in grep(files, r"\bVec::|\bBox::|Arc::new|format!\(|\.push\(|\.lock\(\)|\.clone\(\)") if any(str(h[0]).replace("\\", "/").startswith(d.rstrip("/") + "/") for d in audio_dirs)]
    res.append(Result("RV-62", "Allocation/locking in the audio directories (is it inside the callback?)", len(cb), per_file(cb, n)))

    # Duplication.
    window, seen, dup = 8, collections.defaultdict(list), []
    for rel, lines, cut in files:
        end = cut if cut is not None else len(lines)
        norm = [re.sub(r"\s+", " ", l.strip()) for l in lines[:end]]
        for i in range(0, max(0, end - window)):
            block = norm[i:i + window]
            if sum(1 for b in block if len(b) > 12 and not b.startswith("//")) < window - 1:
                continue
            seen[hashlib.md5("\n".join(block).encode()).hexdigest()].append(f"{rel}:{i + 1}")
    for sites in seen.values():
        if len({s.split(":")[0] + str(int(s.split(":")[1]) // window) for s in sites}) > 1:
            dup.append(sites)
    dup.sort(key=len, reverse=True)
    res.append(Result("RV-30/31/32", f"Repeated blocks of {window}+ lines", len(dup), [" = ".join(s[:3]) for s in dup[:n]]))

    # Purity of the core.
    if args.core:
        core = Path(args.core)
        impure = [h for h in grep(files, r"\buse\s+(?:eframe|egui|midir|cpal|rodio|rfd|winit)\b|Instant::now\(\)|std::thread|std::process") if str(h[0]).replace("\\", "/").startswith(core.as_posix().rstrip("/") + "/")]
        res.append(Result("RV-15", f"Impure things in {core} (GUI/device crates, clocks, threads)", len(impure), fmt(impure, n)))

    # Tests.
    tl = list(test_lines(files))
    wall = [(r, i, l.strip()) for r, i, l in tl if re.search(r"Instant::now\(\)|thread::sleep|SystemTime::now", l)]
    res.append(Result("RV-80", "Wall clock or sleep in test code", len(wall), per_file(wall, n)))
    n_tests = sum(1 for _, _, l in tl if re.match(r"\s*#\[test\]", l))
    heavy = [(r, i, l.strip()) for r, i, l in tl if re.search(r"::with_config\(|App::new\(|DrumApp::", l)]
    res.append(Result("RV-81", "Test lines that build the whole application object", len(heavy), per_file(heavy, n), f"{n_tests} tests in all"))

    # Comments.
    restate = []
    for rel, lines, cut in files:
        for i in range(len(lines) - 1):
            c = re.match(r"\s*///\s*(.+)", lines[i])
            d = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:fn\s+)?(\w+)\s*[:(<]", lines[i + 1])
            if c and d:
                cw = set(re.findall(r"[a-z]+", c.group(1).lower())) - {"the", "a", "an", "of", "is", "to", "in", "for"}
                dw = set(re.findall(r"[a-z]+", d.group(1).lower().replace("_", " ")))
                if cw and len(cw) <= 6 and cw <= dw | {"state", "value", "number"}:
                    restate.append((rel, i + 1, lines[i].strip()))
    res.append(Result("RV-91", "Doc comments that only restate the name below them", len(restate), fmt(restate, n)))
    todo = grep(files, r"\b(?:TODO|FIXME|XXX|HACK)\b", only_code=False)
    res.append(Result("RV-74", "TODO/FIXME/XXX/HACK markers", len(todo), fmt(todo, n)))
    scaff = grep(files, r"#\[allow\((?:dead_code|unused\w*)\)\]|#!\[allow\(")
    res.append(Result("RV-74", "allow(dead_code/unused) scaffolding", len(scaff), fmt(scaff, n)))

    # Docs and process.
    docs = []
    for p in sorted(root.glob("*.md")) + sorted((root / "docs").glob("*.md")) if (root / "docs").exists() else sorted(root.glob("*.md")):
        size = p.stat().st_size
        if size > 40_000:
            docs.append(f"{p.relative_to(root)}: {size // 1024} KB, {len(p.read_text(encoding='utf-8', errors='replace').splitlines())} lines")
    res.append(Result("RV-90", "Markdown files over 40 KB (mandatory-reading size)", len(docs), docs))
    ver = None
    cargo = root / "Cargo.toml"
    if cargo.exists():
        m = re.search(r'^version\s*=\s*"([^"]+)"', cargo.read_text(encoding="utf-8"), re.M)
        ver = m.group(1) if m else None
    stale = []
    for p in list(root.glob("*.md")):
        for i, line in enumerate(p.read_text(encoding="utf-8", errors="replace").splitlines()):
            for m in re.finditer(r"\b(?:now|currently|version|is)\s+v?(\d+\.\d+\.\d+)\b", line):
                if ver and m.group(1) != ver:
                    stale.append(f"{p.name}:{i + 1}  {line.strip()[:100]}")
    res.append(Result("RV-90", f"Docs naming a version other than the current {ver} as 'now'", len(stale), stale[:n]))
    return res, total


def run_tools(root: Path):
    out = []

    def run(cmd, what):
        try:
            p = subprocess.run(cmd, cwd=root, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=900)
            return p.returncode, p.stdout + p.stderr
        except (OSError, subprocess.TimeoutExpired) as e:
            return None, f"{what} could not run: {e}"

    code, text = run(["cargo", "fmt", "--all", "--check"], "cargo fmt")
    out.append(Result("RV-85", "`cargo fmt --check` diff hunks", len(re.findall(r"^Diff in ", text, re.M)), [f"exit code {code}"]))
    code, text = run(["cargo", "clippy", "--workspace", "--all-targets", "--", "-W", "clippy::all"], "clippy")
    warns = re.findall(r"^warning: (.+)$", text, re.M)
    out.append(Result("RV-85", "`cargo clippy --all-targets` warnings", len([w for w in warns if "generated" not in w]), [f"{k}: {v}" for k, v in collections.Counter(w[:60] for w in warns if "generated" not in w).most_common(6)]))
    code, text = run(["cargo", "clippy", "--workspace", "--all-targets", "--", "-W", "clippy::pedantic"], "pedantic")
    pw = [w for w in re.findall(r"^warning: (.+)$", text, re.M) if "generated" not in w]
    out.append(Result("RV-85", "`clippy::pedantic` warnings (for the pattern, not to fix all)", len(pw), [f"{k}: {v}" for k, v in collections.Counter(w[:60] for w in pw).most_common(8)]))
    code, text = run(["cargo", "test", "--workspace", "--release"], "cargo test")
    passed = sum(int(x) for x in re.findall(r"(\d+) passed", text))
    failed = sum(int(x) for x in re.findall(r"(\d+) failed", text))
    out.append(Result("verification", "`cargo test --workspace --release`", passed, [f"{failed} failed", f"exit code {code}"]))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=".")
    ap.add_argument("--core", default="", help="the crate folder that must stay free of GUI/device code, e.g. core")
    ap.add_argument("--ui-dirs", default="src", help="comma separated folders where blocking calls are suspect")
    ap.add_argument("--audio-dirs", default="src/audio", help="comma separated folders with real-time code")
    ap.add_argument("--run-tools", action="store_true", help="also run cargo fmt --check, clippy (default and pedantic) and the tests")
    ap.add_argument("--examples", type=int, default=6)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    root = Path(args.root).resolve()
    results, total = check_all(root, args)
    if args.run_tools:
        results = run_tools(root) + results
    if args.json:
        print(json.dumps([r.__dict__ for r in results], indent=1))
        return
    print(f"# Scan of {root.name}: {total} lines of Rust\n")
    print("| Check | Class | Count |\n|---|---|---|")
    for r in results:
        print(f"| {r.name} | {r.rv} | {r.count} |")
    print()
    for r in results:
        if r.count or r.where:
            print(f"### {r.name} ({r.rv}): {r.count}" + (f"  \n{r.note}" if r.note else ""))
            for w in r.where:
                print(f"- {w}")
            print()


if __name__ == "__main__":
    sys.exit(main())
