# The development diary

A public diary for the people who use Roll Model: what changed, what is being worked on, what may
come. It is published with the documentation, at `/diary/` of the Pages site, with a feed.

**Two kinds of page, kept apart**

- **Entries** are written by hand, one file each: `docs/diary/YYYY-MM-DD-title.md`. They are
  Markdown with a short header (below). Write them for a drummer, not a programmer: what is new,
  why it matters, what is not tried yet. Plain words, short, honest about what is untested.
- **The roadmap** is never written by hand. `tools/build_diary.py` makes it from the GitHub issues
  by their labels, so it cannot drift from the issue list.

**The header of an entry**

```
---
title: 0.10.5: MIDI clock
date: 2026-11-02
summary: One sentence for the list and the feed.
issues: 107, 109
---
```

The body follows: paragraphs, `##` headings, `-` lists, `> ` for a note, `` `code` ``, `**bold**`,
`[links](https://…)`; `#107` becomes a link to the issue.

**Roadmap labels** (set on the issue, one of the three)

| Label | Meaning |
| --- | --- |
| `roadmap:now` | being worked on; keep it to about three |
| `roadmap:next` | follows the current work |
| `roadmap:later` | wanted, no order yet |
| `idea` | suggested, not decided (the *Idea* issue template sets it) |

Only issues a user would care about get a roadmap label; the code-design and review issues
(`RV-*`) do not. A closed issue with one of these labels shows under *Recently done*.

**Writing an entry**

```sh
python3 tools/diary.py facts        # releases, commits and closed issues since the last entry
python3 tools/diary.py new "Title" --summary "One sentence." --issues 107 --body-file text.md
python3 tools/diary.py check        # every entry is readable
python3 tools/build_site.py --out site   # look at site/diary/
```

The project skill `diary-entry` (`.claude/skills/diary-entry/`) does these steps. After each
release: write an entry, then move issues between the roadmap labels.

**When it goes live.** The site follows `main`, so an entry written on `develop` appears with the
next release. The Pages workflow also takes `docs/diary/` from `develop` every day and when it
changes there, so a diary entry does not have to wait for a release.
