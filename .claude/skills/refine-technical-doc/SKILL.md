---
name: refine-technical-doc
description: Audit a chapter of a technical document (a developer guide, an architecture chapter) for terms it uses without explaining, terms used in two senses, and claims that no longer match the code or the measurements; fix them, add a glossary whose entries are linked from the first use of each term, and check the result in the built page. Use when asked to verify that a document explains its notions, to add a glossary, or to refine a manual after the system it describes changed.
---

# Refine a technical document

Made from the audit of the audio chapter of the Roll Model developer guide (2026-10-30). The method is general; the paths below are that project's. The point is that a reader who does not know the field can follow the chapter, and that every sentence is still true.

## 1. Fix what is wrong before adding anything

Read the chapter against its sources, not against itself.

- **Claims against the code.** For every number, name and behaviour the chapter states, open the code or the measurement that backs it (constants, function names, which thread runs what). Wrong or stale claims are fixed first; an unverifiable claim is softened ("probably", "assumed", "not measured") rather than kept.
- **Claims against what you were told.** Do not write more than the evidence says. A report of "no late sounds" is not "no late sounds and nothing dropped"; a measured spread on one machine is not a property of all.
- **Words used loosely.** List every unit or object the chapter names with more than one word (here: *buffer*, *block*, *period*, *endpoint buffer*) or one word for two things (*sample* the number, *sample* the whole sound). Pick one word per thing, say what it is at the first use, and make figures, tables and prose agree.
- **Tables and figures** carry claims too: check their labels against the text, and look at them rendered (overlapping labels and mis-aligned boxes are common in hand-written SVG).

## 2. Find the terms that need explaining

Go through the chapter's prose, paragraph by paragraph, and write down every notion a newcomer could not look up in a sentence: domain terms (*frame*, *jitter*), platform and library names (*WASAPI*, *cpal*), programming notions the argument leans on (*atomic*, *wait-free*, *Arc*), and project types (*Sink*, *Trigger*). For each ask: is it defined **before or at** its first use, **in the chapter**? If not, it is a glossary term. Also include a term that is defined in the text but later than its first use.

Leave out words that every reader of the guide knows (thread, function) unless the chapter gives them a special meaning.

## 3. Write the glossary

One entry per term, alphabetical, as `<dt id="g-ID">Term</dt><dd>…</dd>` inside `<dl class="gloss">`, under an `<h3 id="…-glossary">` at the end of the chapter.

- Define it in terms of things already defined or obvious; give a number where the chapter has one (48 kHz, 480 frames = 10 ms).
- Say **why the chapter needs it** ("the callback must not wait, so it only does wait-free things"), not only what it is.
- Where a word has two senses, give both and say how context tells them apart.
- Point to the code item or the section where it matters (`<code>Estimator</code> in clock.rs`).
- Keep it accurate to the project: if an entry states a fact (a size, a default), it is checked like any other claim.

## 4. Link the first use of each term

`scripts/link_glossary.py` does this mechanically and reports every link it made, so you can read each in context:

```
python -I .claude/skills/refine-technical-doc/scripts/link_glossary.py link \
    --file docs/developer/developer.template.html --section audio \
    --glossary glossary.html --rules rules.json            # dry run, prints each match
… add --write to apply
python -I …/link_glossary.py check --file target/devguide/roll-model-developer-guide.html
```

A worked example from the audio chapter is in `examples/` (`rules.audio-chapter.json`, `glossary.audio-chapter.html`: 47 terms, written as a style reference, not to be reused as is). Write the rules in JSON, so a regex `` is `"\b"`. `rules.json` maps the glossary id (without `g-`) to a regular expression for the term's occurrences, for example `{"frame": "\\bframes?\\b", "wait-free": "\\bwait-free\\b"}`. Matching is case-insensitive and finds the first occurrence in **prose**: it skips figures (`svg`), code listings (`pre`), headings, existing links and `dt`. Read the printed contexts and tighten a rule when it lands on the wrong sense (a verb "runs" instead of the noun "a run"). It stops with an error if a rule finds nothing in the prose, or a glossary entry has no rule, or the glossary is already inserted.

The link is `<a class="gl" href="#g-ID">`: add a dotted-underline style for `a.gl` and a style for `dl.gloss` to the shared stylesheet, and one sentence at the chapter's start saying what the dotted words are.

Scope: "first use" is the first use **within the chapter**. Say so; terms used in earlier chapters are not linked there.

## 5. Check it

- Build the document with its own build script (here `tools/build_devguide.py`; it fails if a code snippet the guide pulls in has moved). Also build every other document that shares the stylesheet.
- `link_glossary.py check` on the built page: no broken internal link; as many first-use links as glossary entries, each term linked once.
- Open the page and look at: a paragraph with dotted links, the glossary, and each figure you touched. A narrow pane hides nothing important if tables scroll, but overlapping text is a defect.
- Re-read the changed claims once more against the code.

## 6. Report honestly

Say what was wrong (with the corrections), how many terms the glossary has, and what the check proved. State the limits: which terms are only linked within the chapter, links that land earlier or later than ideal, and entries whose wording is the author's own and was not checked line by line against the code.

## Project notes (Roll Model)

- Working files are CRLF (`autocrlf`): edit with the Edit tool or a script that normalises `\r\n`, and check the edit applied. `perl -0pi` with `\n` silently matches nothing.
- Python may not be on `PATH`; call the interpreter by its full path (`C:\Python312\python.exe`) and use `-I` for scripts that read files.
- The developer guide is `docs/developer/developer.template.html`; `{{code:PATH::TEXT}}` placeholders pull code from the source when the guide is built, and renumbering chapters means editing the `<h2>N.` lines and every "chapter N" reference.
- A change to the guide is user-visible documentation: commit it with `Refs #N` for the task it belongs to and do not push without being asked.
