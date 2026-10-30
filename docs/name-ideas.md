> **Decided:** the application is called **Roll Model** (it was "Drum Tutor"). This document is kept as the record of the brainstorm.

# Name ideas

> **Made with AI.** This project was created with extensive use of AI (Anthropic's Claude), directed and reviewed by the author. Check anything that matters.

Status: **undecided.** The app is still called "Drum Tutor" (crate `drum-tutor`), a working title. These are proposals from 2026-10-24, kept for later. The brief was a more badass name in three flavours: silly, Japanese, word-play. They need not be mixed.

The "crates.io" column says whether a crate of that name (or its `_` spelling) was in the public index on 2026-10-24. The project is not published, so this only matters if it ever is, but it is a cheap sign of how crowded a name is. Nothing here has been checked for trademarks, domains, GitHub projects or app stores.

## Silly

| Name | Why | crates.io |
|---|---|---|
| **Ba-Dum-Tss** | The rimshot after a joke; the app grades you on the punchline | free |
| **Oni-Sensei** (鬼先生) | "Demon teacher": silly and Japanese at once, judges your flams without mercy | free |
| **Thwack Attack** | Sounds like a cheap 90s arcade game | free |
| **Ghost Note Ghost** | A mascot for the quiet notes you keep forgetting to play | free |
| **Sir Hits-a-Lot** | The tutor as a knight with sticks | free |

## Japanese

Readings are rough guides: Raijin ≈ RYE-jin, Uchikomi ≈ oo-chee-ko-mee, Keiko ≈ KAY-ko, Oni ≈ OH-nee, Hyōshi ≈ HYOH-shee.

| Name | Meaning and fit | crates.io |
|---|---|---|
| **Raijin** (雷神) | God of thunder. Traditionally drawn ringed with taiko drums, so it is the most literal "badass drummer". Short, reads like a product. Many other things use the name | taken |
| **Uchikomi** (打ち込み) | In judo, repetitive entry drills; in Japanese music production, programmed drums. Both fit a tool that makes you repeat a bar until it is clean | taken |
| **Dojo** (道場) | Training hall. Blunt and instantly understood, but a very common name for software | taken (`drum-dojo` free) |
| **Keiko** (稽古) | Practice, training. Also a common first name, so it can be a friendly personal-trainer persona | free |
| **Hyōshi** (拍子) | Beat, time signature. Quiet and elegant, a real musical term | free (`hyoshi`) |
| **Renda** (連打) | Rapid hits, a drum roll. Short and punchy; gamer slang for button mashing | free |

## Word-play

| Name | Why | crates.io |
|---|---|---|
| **Ride or Die** | Ride cymbal plus the loyalty phrase. The most badass pun here | free |
| **Stickler** | Drumstick plus someone strict about details, which is what the timing grader is | free |
| **Hi-Hat Trick** | Hat trick plus hi-hat; implies three in a row | free |
| **Roll Model** | Role model plus drum roll; friendlier in tone | free |
| **Crash Course** | Crash cymbal plus a quick introduction. Collides with a well-known YouTube channel | taken |
| **Flambé** | Fire pun on the "flam" rudiment; fiery and a bit fancy | free |

## Shortlist (from the brainstorm)

1. **Raijin**, with the tagline "drum training that hits back".
2. **Oni-Sensei**, if the app should be funny as well.
3. **Ride or Die**, if an English name is preferred.

Not decided by the user yet. Add notes and the verdict below.

## Before choosing

- Search the name on GitHub, in a web search, and in app stores for existing drum, music or game products. Raijin, Dojo and Taiko-related names will be crowded.
- Look up the trademark in the regions that matter (EUIPO/USPTO classes for software and music education), and see whether a domain or a GitHub organisation name is free.
- Say the name aloud; check that a Japanese name is spelled and glossed correctly (macrons are optional in the app name: "Hyoshi", "Raijin").
- The crate and package name need not match the product name (for example product "Raijin", crate `drum-raijin`), which avoids a clash on crates.io.

## What a rename touches

26 occurrences of the current name in 9 files at 0.3.0:

- `Cargo.toml` (`name`, `description`) and `Cargo.lock` (regenerated). The binary file name follows the package name.
- `src/main.rs`: the window title and the app name given to `eframe::run_native`.
- `src/app.rs`: the heading on the welcome panel.
- `src/midi.rs`: the MIDI client names (`drum-tutor`, `drum-tutor-in`, `drum-tutor-out`, `drum-tutor-list`), which show up in other MIDI software.
- `core/src/config.rs`: the settings folder (`%APPDATA%\DrumTutor`, `~/Library/Application Support/DrumTutor`, `~/.config/drum-tutor`), its doc comment and the first line of the file.
  - **Migration:** on first start, if the new folder does not exist but the old one does, copy the old `config.txt` across, or users lose their settings and recent-scores list. Add a test.
- `README.md`, `HANDOFF.md`, `CLAUDE.md`, `CHANGELOG.md`: titles and mentions.
- Outside the repo: the folder name (`D:\projects\DrumTutor`) and, if there is one, the remote repository.

SemVer: a rename that moves the settings folder is a visible change, so release it as a minor version (while below 1.0) with a changelog entry that mentions the migration.
