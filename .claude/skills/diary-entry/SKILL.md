---
name: diary-entry
description: Write and add an entry to the Roll Model development diary (docs/diary/), the public, user-facing diary on the Pages site. Use when asked to write a diary entry, a dev diary post, release notes for the diary, to "add to the diary", or after a release when the diary and the roadmap labels are to be brought up to date.
---

# Add an entry to the development diary

The diary is for the **people who use Roll Model** (drummers), not for developers. It lives in `docs/diary/` and is published at `/diary/` of the Pages site with a feed. The roadmap on that site is generated from GitHub issue labels: never write it by hand. Read `docs/diary/README.md` once for the format.

## 1. Collect the facts, do not recall them

```sh
python3 tools/diary.py facts
```

It lists the releases, the commits and the closed issues since the last entry, and what is under `[Unreleased]` in `CHANGELOG.md`. Write only from that, from `CHANGELOG.md`, `HANDOFF.md` and the issues. If a claim is not in them, leave it out or check the code. If the user told you what they want covered, that decides the topic; `facts` is the background.

## 2. Decide what the entry is

One entry, one subject. Usual ones:

- **A release** ("0.10.5: MIDI clock"): the one or two things a user will notice, then "also in this version" in a sentence or two.
- **A decision or a doubt** ("Why takes do not send the clock yet"): what was chosen, why, what is not known.
- **A step of the roadmap** ("What the V31 trial found"): after a hardware check.

Do not write an entry that only lists commits or refactors. Internal work is one sentence ("tidier inside, tested on its own") unless it changes what a user can do or trust.

## 3. Write it for a drummer

- Say what the user can do now, and why it matters while practising. Name settings the way the screen does (**Settings → Sound**).
- **Say what is not tried or not done.** A feature not yet tried on the V31 gets a `>` note saying so. Never turn "tested against a fake output" into "works".
- Plain words, short paragraphs, 150 to 400 words. No jargon without a half-sentence of explanation. No marketing tone, no "excited to announce".
- First person singular is fine ("I read them all"); the project is one person with an AI, and the README says so. Do not claim anything about people or hardware you were not told about.
- Link issues as `#107` (the build makes links); mention the issue IDs in the header's `issues:` too.
- Markdown allowed: paragraphs, `##` headings, `-` and `1.` lists, `> ` notes, `` `code` ``, `**bold**`, `[text](url)`. Relative links to other diary pages work (`../roadmap/`).

## 4. Add it

Write the body to a scratch file (a temporary folder, not the repository), then:

```sh
python3 tools/diary.py new "Title" --summary "One sentence for the list and the feed." --issues 107,109 --body-file <scratch file>
```

It names the file `docs/diary/YYYY-MM-DD-title.md`, refuses to overwrite one, and reads the result back. The title may contain a colon (`0.10.5: MIDI clock`). Use `--date` only to match a release date.

## 5. Check it

```sh
python3 tools/diary.py check
python3 tools/build_site.py --out <scratch folder>
python3 -m unittest discover -s tools -p "test_*.py"
```

Look at `<scratch folder>/diary/` (the entry page, the list, the roadmap, `feed.xml`) and fix what reads badly. Chain the commit after the build with `&&`, so a broken diary is not committed.

## 6. Bring the roadmap in step (after a release)

The roadmap comes from the labels `roadmap:now`, `roadmap:next`, `roadmap:later` and `idea` on GitHub issues. After a release, propose to the user which issues move (done ones are closed; `now` stays at about three), and change labels only with their yes: `gh issue edit N --add-label roadmap:now --remove-label roadmap:next`. Only issues a user cares about get a roadmap label, never the code-design or review (`RV-*`) ones. Keep `HANDOFF.md` and the issue in step as usual.

## 7. Commit

Commit on `develop` with a `docs:` message and the repository's attribution line. Push only if the user asks or has said to; pushing an entry to `develop` publishes it (the Diary workflow rebuilds the site), so it is outward-facing: show the user the text first unless they asked you to write and publish in one go.
