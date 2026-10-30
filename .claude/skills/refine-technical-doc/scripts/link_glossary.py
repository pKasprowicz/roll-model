"""Links the first use of each glossary term in a chapter of an HTML document, and checks the result.

  link   --file F --section ID --glossary G --rules R [--write]
         Finds, in the <section id="ID"> of F, the first occurrence in prose of every term of R (a JSON
         object: glossary id without "g-" -> regular expression), wraps it in
         <a class="gl" href="#g-ID">, and appends the glossary fragment G at the end of the section.
         Without --write nothing is changed: the context of every link is printed for review.
  check  --file BUILT.html
         Reports internal links that point nowhere, and compares the number of glossary links with the
         number of glossary entries.

Standard library only. Keeps the line endings of the file (CRLF stays CRLF).
"""
import argparse
import json
import pathlib
import re
import sys

SKIP = {"svg", "pre", "h2", "h3", "h4", "a", "dt", "script", "style"}


def read(path):
    raw = pathlib.Path(path).read_bytes().decode("utf-8")
    return raw.replace("\r\n", "\n"), "\r\n" if "\r\n" in raw else "\n"


def link(args):
    text, eol = read(args.file)
    gloss, _ = read(args.glossary)
    rules = json.loads(pathlib.Path(args.rules).read_text(encoding="utf-8"))

    start = text.find(f'<section id="{args.section}"')
    if start < 0:
        sys.exit(f'no <section id="{args.section}"> in {args.file}')
    end = text.index("</section>", start)
    if 'class="gl"' in text[start:end] or 'class="gloss"' in text[start:end]:
        sys.exit("this section already has glossary links or a glossary")

    entries = set(re.findall(r'<dt id="g-([\w-]+)"', gloss))
    problems = []
    for term in sorted(entries - rules.keys()):
        problems.append(f"glossary entry without a rule: {term}")
    for term in sorted(rules.keys() - entries):
        problems.append(f"rule without a glossary entry: {term}")

    done, out, stack = {}, [], []
    for piece in re.split(r"(<[^>]+>)", text[start:end]):
        if piece.startswith("<") and piece.endswith(">"):
            m = re.match(r"<(/?)([a-zA-Z0-9]+)", piece)
            if m and m.group(2).lower() in SKIP:
                name, closing = m.group(2).lower(), m.group(1) == "/"
                if closing:
                    if name in stack:
                        stack.remove(name)
                elif not piece.endswith("/>"):
                    stack.append(name)
            out.append(piece)
            continue
        if stack or not piece.strip():
            out.append(piece)
            continue
        hits = []
        for term, rx in rules.items():
            if term in done:
                continue
            m = re.search(rx, piece, re.I)
            if m:
                hits.append((m.start(), m.end(), term))
        hits.sort()
        chosen, last = [], -1
        for s, e, term in hits:
            if s >= last:
                chosen.append((s, e, term))
                last = e
        for s, e, term in reversed(chosen):
            done[term] = piece[max(0, s - 40):e + 30].replace("\n", " ")
            piece = piece[:s] + f'<a class="gl" href="#g-{term}">' + piece[s:e] + "</a>" + piece[e:]
        out.append(piece)

    for term in sorted(rules.keys() - done.keys()):
        problems.append(f"rule never matches prose: {term}")
    for term, ctx in done.items():
        print(f"{term:14} ...{ctx}...")
    if problems:
        print("\nPROBLEMS:")
        for p in problems:
            print("  " + p)
        sys.exit(1)

    if not args.write:
        print(f"\n{len(done)} links; dry run, nothing written (add --write)")
        return
    new_text = text[:start] + "".join(out) + "\n" + gloss + text[end:]
    pathlib.Path(args.file).write_bytes(new_text.replace("\n", eol).encode("utf-8"))
    print(f"\n{len(done)} links written to {args.file}")


def check(args):
    text, _ = read(args.file)
    ids = set(re.findall(r'id="([^"]+)"', text))
    broken = sorted({h for h in re.findall(r'href="#([^"]+)"', text) if h not in ids})
    glinks = re.findall(r'class="gl" href="#(g-[\w-]+)"', text)
    entries = re.findall(r'<dt id="(g-[\w-]+)"', text)
    print("broken internal links:", broken or "none")
    print(f"glossary entries: {len(entries)}; first-use links: {len(glinks)} ({len(set(glinks))} distinct)")
    unlinked = sorted(set(entries) - set(glinks))
    repeated = sorted({g for g in glinks if glinks.count(g) > 1})
    print("entries never linked:", unlinked or "none")
    print("terms linked more than once:", repeated or "none")
    if broken or unlinked or repeated:
        sys.exit(1)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("link")
    a.add_argument("--file", required=True)
    a.add_argument("--section", required=True)
    a.add_argument("--glossary", required=True)
    a.add_argument("--rules", required=True)
    a.add_argument("--write", action="store_true")
    a.set_defaults(fn=link)
    b = sub.add_parser("check")
    b.add_argument("--file", required=True)
    b.set_defaults(fn=check)
    args = ap.parse_args()
    args.fn(args)
