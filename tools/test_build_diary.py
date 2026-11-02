"""Tests of the diary builder: python3 -m unittest discover -s tools -p "test_*.py" """
import json
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_diary as bd  # noqa: E402
import diary  # noqa: E402

REPO = "https://github.com/o/r"


def issue(number, title, labels=(), state="OPEN", closed=None):
    return {"number": number, "title": title, "labels": [{"name": n} for n in labels], "state": state, "closedAt": closed, "url": ""}


class EntryFiles(unittest.TestCase):
    def write(self, folder, name, text):
        path = Path(folder) / name
        path.write_text(text, encoding="utf-8")
        return path

    def test_a_good_entry_is_read(self):
        with tempfile.TemporaryDirectory() as d:
            p = self.write(d, "2026-01-02-a.md", "---\ntitle: A: b\ndate: 2026-01-02\nsummary: S\nissues: 1, 22\n---\nText.\n")
            e = bd.parse_entry(p)
            self.assertEqual((e.title, e.day, e.summary, e.issues, e.body), ("A: b", date(2026, 1, 2), "S", [1, 22], "Text."))

    def test_what_is_wrong_is_said_with_the_file(self):
        bad = {
            "no header": "Text only.\n",
            "no summary": "---\ntitle: T\ndate: 2026-01-02\n---\nText.\n",
            "bad date": "---\ntitle: T\ndate: 2026-13-45\nsummary: S\n---\nText.\n",
            "no text": "---\ntitle: T\ndate: 2026-01-02\nsummary: S\n---\n\n",
        }
        with tempfile.TemporaryDirectory() as d:
            for name, text in bad.items():
                with self.subTest(name), self.assertRaises(ValueError) as cm:
                    bd.parse_entry(self.write(d, "2026-01-02-x.md", text))
                self.assertIn("2026-01-02-x.md", str(cm.exception))
            with self.assertRaises(ValueError) as cm:
                bd.parse_entry(self.write(d, "2026-01-03-wrong-day.md", "---\ntitle: T\ndate: 2026-01-02\nsummary: S\n---\nText.\n"))
            self.assertIn("begin with its date", str(cm.exception))

    def test_entries_come_newest_first_and_the_readme_is_not_one(self):
        with tempfile.TemporaryDirectory() as d:
            for day in ("2026-01-02", "2026-03-04"):
                self.write(d, f"{day}-x.md", f"---\ntitle: T{day}\ndate: {day}\nsummary: S\n---\nText.\n")
            self.write(d, "README.md", "not an entry")
            self.assertEqual([e.day.isoformat() for e in bd.load_entries(Path(d))], ["2026-03-04", "2026-01-02"])

    def test_the_real_entries_are_all_readable(self):
        self.assertTrue(bd.load_entries())

    def test_a_new_entry_is_named_by_its_date_and_title_and_never_replaces_one(self):
        with tempfile.TemporaryDirectory() as d:
            path = diary.new_entry("Hello, World!", "S", "Body.", date(2026, 5, 6), [7], Path(d))
            self.assertEqual(path.name, "2026-05-06-hello-world.md")
            self.assertEqual(bd.parse_entry(path).issues, [7])
            with self.assertRaises(ValueError):
                diary.new_entry("Hello, World!", "S", "Body.", date(2026, 5, 6), [], Path(d))
            with self.assertRaises(ValueError):
                diary.new_entry("Empty", "S", "  ", date(2026, 5, 6), [], Path(d))
            self.assertFalse((Path(d) / "2026-05-06-empty.md").exists(), "a file that does not read is not left behind")


class Text(unittest.TestCase):
    def test_inline_marks(self):
        html = bd.inline("**bold** and *em* and `a < b` and [link](https://x.y/?a=1&b=2) and #107", REPO)
        self.assertIn("<strong>bold</strong>", html)
        self.assertIn("<em>em</em>", html)
        self.assertIn("<code>a &lt; b</code>", html)
        self.assertIn('<a href="https://x.y/?a=1&amp;b=2">link</a>', html)
        self.assertIn(f'<a href="{REPO}/issues/107">#107</a>', html)

    def test_text_is_escaped_and_code_is_left_alone(self):
        self.assertEqual(bd.inline("<b>x</b>", REPO), "&lt;b&gt;x&lt;/b&gt;")
        self.assertEqual(bd.inline("`#1 **no**`", REPO), "<code>#1 **no**</code>")

    def test_blocks(self):
        html = bd.markdown("Para one\ncontinues.\n\n## Head\n\n- a\n- b\n  more\n\n1. x\n2. y\n\n> note\n\n```\ncode <x>\n```", REPO)
        self.assertIn("<p>Para one continues.</p>", html)
        self.assertIn("<h2>Head</h2>", html)
        self.assertIn("<ul><li>a</li><li>b more</li></ul>", html)
        self.assertIn("<ol><li>x</li><li>y</li></ol>", html)
        self.assertIn('<div class="callout">note</div>', html)
        self.assertIn("<pre><code>code &lt;x&gt;</code></pre>", html)


class Roadmap(unittest.TestCase):
    ISSUES = [
        issue(1, "FEAT-1: One", ["roadmap:now"]),
        issue(2, "Two", ["roadmap:next", "needs-device"]),
        issue(3, "Three", ["roadmap:later"]),
        issue(4, "Four", []),
        issue(5, "Five", ["idea"]),
        issue(6, "Six", ["roadmap:now"], "CLOSED", "2026-02-01T00:00:00Z"),
        issue(7, "Seven", [], "CLOSED", "2026-03-01T00:00:00Z"),
        issue(8, "Eight", ["idea"], "CLOSED", "2026-04-01T00:00:00Z"),
    ]

    def test_issues_are_sorted_by_their_labels(self):
        lists = bd.plan(self.ISSUES)
        self.assertEqual([i["number"] for i in lists["roadmap:now"]], [1])
        self.assertEqual([i["number"] for i in lists["roadmap:next"]], [2])
        self.assertEqual([i["number"] for i in lists["roadmap:later"]], [3])
        self.assertEqual([i["number"] for i in lists["ideas"]], [5])
        self.assertEqual([i["number"] for i in lists["done"]], [8, 6], "closed ones with a label, the latest first; an unlabelled issue is not on the roadmap")

    def test_the_page_names_the_horizons_and_what_waits_for_a_device(self):
        html = bd.roadmap_body(self.ISSUES, REPO)
        for word in ("<h2>Now</h2>", "<h2>Next</h2>", "<h2>Later</h2>", "<h2>Ideas</h2>", "<h2>Recently done</h2>", "waits for a device"):
            self.assertIn(word, html)
        self.assertIn("#1</a> One</li>", html)
        self.assertNotIn("FEAT-1:", html, "the ID prefix of the title is dropped")
        self.assertNotIn("Four", html)

    def test_an_empty_horizon_says_so_and_unreadable_issues_do_not_stop_the_build(self):
        self.assertIn("<em>nothing</em>", bd.roadmap_body([], REPO))
        self.assertIn("could not be read", bd.roadmap_body(None, REPO))


class Site(unittest.TestCase):
    def test_the_diary_is_written_with_a_feed_that_is_xml(self):
        with tempfile.TemporaryDirectory() as d:
            written = bd.build(Path(d), "vTest", Roadmap.ISSUES)
            names = {str(p.relative_to(Path(d) / "diary")).replace("\\", "/") for p in written}
            entries = bd.load_entries()
            self.assertIn("index.html", names)
            self.assertIn("roadmap/index.html", names)
            self.assertIn("feed.xml", names)
            for e in entries:
                self.assertIn(f"{e.slug}/index.html", names)
            tree = ET.parse(Path(d) / "diary" / "feed.xml")
            ns = {"a": "http://www.w3.org/2005/Atom"}
            self.assertEqual(len(tree.findall("a:entry", ns)), len(entries))
            first = tree.find("a:entry", ns)
            self.assertTrue(first.find("a:link", ns).get("href").startswith("https://"))
            index = (Path(d) / "diary" / "index.html").read_text(encoding="utf-8")
            self.assertNotIn("{{", index)
            self.assertIn(entries[0].title.split(":")[0], index)

    def test_the_site_address_follows_the_repository(self):
        self.assertEqual(bd.site_url("https://github.com/Owner/name"), "https://owner.github.io/name/")


if __name__ == "__main__":
    unittest.main()
