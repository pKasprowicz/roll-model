---
name: harsh-code-review
description: Run a deliberately harsh, whole-codebase review that x-rays a Rust (egui, device I/O, real-time audio) project against every defect class the first Roll Model review found (RV-1 to RV-91: wrong-slot state, write storms, success after failure, god objects, two command paths, units as f64, public fields, several sources of truth, duplication, silent failure, per-frame work, blocking on the UI thread, wall-clock tests, doc drift), with a mechanical scan, verified-or-by-reading findings, severities, an order of work and a list of what is good. Use when asked for a harsh code review, a full review of the tree, a repeat of the RV review, a code health check before a release, or "x-ray the code".
---

# Harsh code review

A repeat of `docs/code-review-2026-11-01.md` (RV-1 to RV-91) on the code as it is now. The method that made that review useful: **look for defects first, count with a script, verify bugs by running something, say what is only read, rank honestly, and list what is good so nobody "fixes" it.** The classes of defect are in `references/defect-catalogue.md`; read it before you start.

## What you produce

One report, `docs/code-review-YYYY-MM-DD.md`, in the structure of the first one (open it as the model):

1. **Header:** the commit reviewed (`git rev-parse --short HEAD`, and the version), the date, the scope, the focus, the tone.
2. **How to use this report:** one commit per finding with the tests green; severity and effort scales; "Verified" vs "By reading"; do not reopen what the project recorded as decided.
3. **Verification table:** what was run and what it said (tests, clippy, `fmt --check`, the scan's counts, each throwaway test).
4. **Executive summary:** the ten findings that matter most, one paragraph each, worst first.
5. **Sections by class (A to L of the catalogue)** with the findings.
6. **Suggested order of work** (small and high value first; prerequisites for refactors; structure; types; consolidation; boundaries; docs).
7. **What is good and should stay.**

A **finding** has: an ID, a title that states the defect (not the area), severity (S1 to S4), verified or by reading, effort (S/M/L), **where** (`path:line`, several if needed), **why it hurts** (the mechanism and the consequence, a concrete failing scenario for S1/S2), and **fix** (specific: the type, the function, the order of steps).

## Method

### 1. Prepare

- Read `CLAUDE.md`, `STATUS.md` and `HANDOFF.md` (or the project's equivalents) for the architecture, the invariants and **what is recorded as decided**. Do not re-open a decided item; if you disagree, say so explicitly in a finding.
- Read the **previous review** and the status of its findings (done, in part, open). You will report on those too (step 4).
- Note the scope: default is the whole tree (`src/`, the core crate, tests, CI, docs). A narrower scope (a directory, or the diff since the last reviewed commit: `git diff <commit>..HEAD --stat`) is a *delta review*; say so in the header.
- Work on a clean tree at a known commit. Do not change the code during the review; throwaway tests are reverted (`git checkout -- <file>`).

### 2. Measure

```sh
python3 .claude/skills/harsh-code-review/scripts/scan.py --core core --ui-dirs src/app --audio-dirs src/audio --run-tools
```

(`--run-tools` runs `cargo fmt --check`, clippy default and pedantic, and the tests: minutes. Without it only the text scan runs. Adjust `--core`, `--ui-dirs`, `--audio-dirs` to the project.) The scan finds **candidates** for the classes of the catalogue and gives counts for the verification table. It is not a verdict: a count is evidence, never a finding by itself. `clippy::pedantic` is read for *patterns* (many `must_use`, lossy casts, float `==`), not fixed.

### 3. Read, class by class

Go through the catalogue in order, **A first** (defects are what the review is for), and read the code the scan points to, then the code it cannot see:

- every `poll_*`/`install`/`apply` against the state it writes and *when* (wrong-slot, ordering);
- the whole save and load path of every file the program writes (atomicity, debounce, what a failed read does);
- every flow that has a failure branch: does the screen say so, does the flow stop;
- the command paths (keys, controller, menu), match arms and guards;
- every thread boundary: queues (caps), locks (policy), channels, what happens when the UI stops repainting;
- the audio callback and anything it touches;
- the ten largest files, and every function over 80 lines;
- the tests: clocks, sleeps, what builds the whole app object, what failure paths have no test;
- CI and tooling: gates that exist, gates the docs claim exist;
- the docs a newcomer must read: do they match the code (versions, file names, sizes, claims)?

**Verify what you can.** For a suspected bug, write a throwaway test that reproduces it and **run it**; a failing test makes it "Verified", with the reproduction in the report. Revert the test afterwards. What you only traced by hand is "By reading", and the report says so. Never write "this will crash" about something that was not run.

### 4. Check the old findings

For every finding of the previous review that the project marks done, spot-check that the fix still holds (the scan's count for its class should not have grown back: a wildcard arm, a public field, a `fs::write`). A regression is a new finding, ranked by its class, that names the old ID. A finding still open is carried by reference (ID, one line, "worse/better/unchanged"), not re-argued.

### 5. Write

- **Rank honestly.** S1 only for wrong behaviour now or on the next change, with the scenario. Do not inflate to make the report sound serious; do not soften a real defect to be polite. If there are no S1s, say so and say how you looked.
- **Harsh means precise, not rude.** No adjectives about people. Each finding is a mechanism, a location and a fix. No "consider refactoring", no "might be worth looking at": either it is a finding with a fix, or it is not in the report.
- **No padding.** One finding per defect, merged where it is one cause (the first review had 47 for 27,000 lines). Related findings point at each other (RV-1 → RV-10).
- **Numbers from the scan** go in the verification table and in the findings they support; say which check, which command.
- **Say what is not covered** (things that need a device: real-time audio, hardware; platforms you could not run).
- **End with what is good**, concretely, so it is not "fixed".

IDs: findings of a new review use a new prefix so they cannot collide with the old ones: `R2-1`, `R2-2` … for the second review (the section letters of the catalogue stay). Keep the old IDs when you refer to old findings.

### 6. Hand over, do not fix

The review changes nothing but the report. When the report is written:

- Show the user the executive summary and the counts by severity.
- Offer, and do **only with their yes**, to put the findings into the project's tracking the way the first review was (`HANDOFF.md` rows and GitHub issues with the labels `review`, `code-design`, `severity-S*`, one commit per finding afterwards). That is a separate step; the issue creation is outward-facing.
- Commit the report on `develop` with a `docs:` message and the repository's attribution line; push only if asked.

## Do not

- Fix anything during the review, reformat, or "tidy while you are there".
- Report a lint count, a long file or a style preference as a defect without the mechanism that hurts.
- Re-open decided items, or report the same defect twice under two classes.
- Present a hand-traced bug as verified, or a scan candidate as a finding.
- Skip class A because the scan has nothing to say about it: the S1s of the first review (RV-1 to RV-4) were found by reading and by a throwaway test, not by a count.
