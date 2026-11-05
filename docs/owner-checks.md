# Checks only the owner can do

Everything that is still waiting for a person with a drum module, a controller, ears, a mouse, a
second computer or a decision. Made on **2026-11-05** from `HANDOFF.md`, the open issues and the
state of `develop` (`e8b4b24`, after release 0.10.5). Nothing in this list can be done by the
program's tests or by the AI session; each item says what you need, what to do, what counts as a
pass, and what to report.

**How to use it**

- Do what you have the means for; the groups are independent. The order within each group is the
  suggested one.
- **Report in the issue** (a comment is enough: "pass", or what you saw). The AI session then
  updates `HANDOFF.md`, the manual and the code. A failed check is not a problem: it is the
  result that was wanted.
- Checks marked **develop build** need a build of the current `develop` (`cargo build --release`,
  then `target\release\roll-model.exe`); the released 0.10.5 does not have them yet.
- The hardware sessions (group C) have their own step-by-step plan with a results sheet:
  [`hardware-test-plan.md`](hardware-test-plan.md). This list does not repeat it; it says which
  session answers which question.
- Backup first: copy `%APPDATA%\RollModel` somewhere before the first session.

## At a glance

| # | Check | Issue | Needs | Time | Unblocks |
|---|---|---|---|---|---|
| A1 | Passes instead of loop | #111, #113 | develop build | 15 min | closing the Listen part; then recording N passes |
| A2 | Metronome tab without the rolling notes | – | develop build | 3 min | the next release |
| A3 | The screen stays awake | #115 | develop build, Windows | 10 min | closing PLAT-1 |
| A4 | The diary site | – | a browser | 10 min | telling people about it |
| B1 | MIDI input and ports on the real V31 | #108 | V31 | 20 min | closing HW-12 and RV-32 |
| B0 | Calibration through the new scheduler (RV-30, step 1) | #88 | V31, audio output | 15 min | RV-30 step 2 (takes) |
| B2 | Start-up timing | #16 | V31 and a controller | 10 min | CD-13 / RV-61 (MIDI off the UI thread) |
| B3 | Rudiments on the V31 | #15 | V31, headphones | 45 min | rudiment tuning, RUD-1 |
| B4 | Listen to the kit | #27 | headphones | 20 min | tuning the cymbals |
| B5 | WAV override and the hi-hat choke | #30 | headphones | 15 min | closing SND-3 |
| C1 | X-Touch Mini | #1, #2, #4, #17 | Mini in MC mode | 45 min | the default preset |
| C2 | Manage window and Learn | #7 | Mini (and a generic one) | 30 min | RV-70 |
| C3 | Unplugging and plugging in | #13 | any controller | 10 min | closing HW-8 |
| C4 | X-Touch Compact | #3, #5, #4 | Compact in MC mode | 45 min | the Compact preset |
| C5 | Generic controllers | #6 | each one you own | 15 min each | the four presets |
| D1 | Mouse and wheel | #29 | a mouse | 15 min | closing UI-1 |
| E1 | The flagged rudiment rows | #14 | the PAS chart | 30 min | `docs/rudiments.md` |
| E2 | Bar numbering | #31 | a decision | 5 min | UI-2 |
| E3 | The name | #36 | a web search | 30 min | wide release |
| E4 | The local folder | #37 | a decision | 1 min | NAME-2 |
| E5 | Your own exports | #56 | MuseScore, Guitar Pro, Sibelius, Dorico | 30 min | the manual's list of programs; IMP-7 |
| F1 | Linux and macOS | #51 | those machines | 1 h each | AUD-14 |

---

## A. The current `develop` build

### A1. Passes instead of loop (develop build; #111, #113)

The *loop* checkbox is a *passes* box (1 to 99, or *unlimited* at the top) on the Score and
Rudiments tabs.

1. Score tab, open a score. The box next to *notes* shows **1**. Press Listen: it plays once,
   stops, and the status line says **Listen finished**.
2. Set the box to **3** (drag or type). Listen plays the range exactly three times, the count-in
   (if on) only once, and the status line says **Listen finished: 3 passes**.
3. Set it to 3, start Listen, and lower it to 1 during the second pass: the pass that is running
   finishes, then it stops.
4. Type `unlimited` (or drag to the top): Listen goes on until Stop. Close the program and start
   it again: **unlimited is back to 1; a count such as 3 stays 3.**
5. Rudiments tab: the same box, the same value. With *ramp +* on and 4 passes, the tempo rises
   every few passes as before.
6. The *loop* key (check the key in the Score tab's tooltip) and the controller's loop button go
   between 1 and unlimited. *Loop worst bar* after a take still loops.
7. With a controller connected: Settings → Controller → Manage, press **Learn** on the action
   **Passes** and turn an encoder: it steps the number (Shift: five at a time). A fader sets it:
   bottom is 1, top is unlimited.
8. Start a recording with passes set to 3. **Expected (known gap):** the take is one pass; the
   count does not apply to recording yet. While recording, the box is locked.

**Pass:** 1 to 4 behave as written. **Report:** anything that stops too early or late, a pass
count that is wrong by one, anything that feels wrong in the box (dragging is too fast or too
slow?).

### A2. The Metronome tab (develop build)

The rolling notes under the beat lights were removed.

1. Metronome tab, press Start: the beat lights, tempo, beats per bar, subdivision, volume and
   ramp are all there and work.
2. Nothing is drawn under the lights, and nothing looks like it is missing (a gap, a leftover
   label). Make the window narrow and short: still usable.
3. Switch to another tab and back while it runs: the lights go on.

**Pass:** the tab looks finished. **Report:** a screenshot if it looks odd.

### A3. The screen stays awake (develop build, Windows; #115)

You need to wait out a real screen timeout, so make it short for the test.

1. Windows Settings → System → Power: set *Turn my screen off after* and *Put my device to sleep
   after* to **1 minute** (note the old values).
2. Roll Model: Settings → General → Screen: **Keep the screen awake** ticked (it is by default).
3. Do not touch the keyboard or mouse for 3 minutes (the program can just sit on the Home tab).
   **Pass:** the screen stays on.
4. Optional, objective: an administrator PowerShell, `powercfg /requests`. While the program is
   open you should see an entry under DISPLAY (and SYSTEM) from `roll-model.exe`.
5. Untick the switch. **Pass:** within about a minute the screen turns off as it normally does
   (and `powercfg /requests` no longer lists the program).
6. Tick it again, close the program: **pass** if the screen turns off after the timeout again.
7. Put the Windows power settings back.

**Report:** which step failed, and the Windows version. A screen that locks although the switch
is on is the interesting failure.

### A4. The diary site (a browser; no build)

<https://pkasprowicz.github.io/roll-model/diary/>

1. The list has four entries; each opens; the links inside them work (issues, the manual, the
   roadmap).
2. `/diary/roadmap/`: Now, Next and Later each list what you expect (labels `roadmap:now`,
   `roadmap:next`, `roadmap:later` on the issues; change them with `gh issue edit` or on
   GitHub). Is anything in the wrong column?
3. `/diary/feed.xml` in a feed reader (or the browser): four entries with their summaries.
4. A phone, or a narrow window: readable, no sideways scroll.
5. Reading as a drummer who has never seen the project: is any sentence unclear, or does any
   claim go further than what is true?

**Known:** the start page does not link to the diary until the next release.
**Report:** wording you want changed.

---

## B. The Roland V31 (and ears)

### B0. Calibration through the new scheduler (develop build; #88, RV-30 step 1)

The calibration's 16 clicks now come from the `Player` (the scheduler of Listen) and no longer
from the old metronome thread. RV-30 is **paused on this check**: takes move next, and the plan
is one hardware check after each step.

1. With the V31 on USB and the computer's audio output as usual, run **Settings → Timing →
   Calibrate** on the **released 0.10.5** and note the result in milliseconds; play along with
   the 16 clicks as you always do (any pad, on the click).
2. The same on the **develop build**.
3. Optional: both again with the click as a MIDI note back into the V31 (Settings → Sound,
   *Send the click to the MIDI output*), with the computer's audio click off.
4. Start the calibration and press Listen or the Metronome while it runs: the clicks stop and the
   calibration is cancelled (it used to fail a moment later).

**Pass:** the two results agree to within a few milliseconds (your loopback measurement was about
45 ms), the clicks are on time and none is missing, and nothing sounds twice. **Report:** both
numbers, and anything audible that was not there before (a late or doubled click, a gap).

### B1. MIDI input and ports on the real V31 (#108; HW-12)

Commit `39f3f25` changed how every MIDI port is opened and how the drum module's messages are
read; the unit tests use byte sequences, not a port. Your MIDI-clock check already exercised
opening an **output**; this covers the **input** and the controller. No extra program is needed:
the app's monitor shows what arrives.

1. Close other programs that use MIDI. Settings → MIDI input: choose the V31 and connect. Hit a
   snare pad: the monitor shows the hit with the instrument, the **note, the velocity and
   channel 10**. Hit a few other pads and the hi-hat.
2. The hi-hat: with the pedal up, hit the hat (open), then press the pedal down and hit it
   (closed): the two hits are told apart (open and closed hi-hat). Move the pedal alone: it
   does not appear as a hit.
3. A name that is only part of the port's name: close the program, edit
   `%APPDATA%\RollModel\config.txt`, set `midi_in_port = V31` (just the part), start the program:
   it connects by itself.
4. A name that does not exist: set `midi_in_port = NOPE`: the program says it was not found and
   does not crash.
5. Controller: connect the X-Touch Mini (Settings → Controller): its messages arrive in the
   monitor, and pressing **record** lights the record LED on the Mini (lights go through the
   output port).
6. The click as a MIDI note: Settings → Sound: tick *Send the click to the MIDI output*, note 76,
   channel 10, choose the V31 as output, start the Metronome tab: the V31 sounds the click (an
   unmapped note 76 is a wood block on the V31).

**Pass:** every step does what it says. **Report:** the step number and what the monitor showed.

### B2. Start-up timing (#16; HW-11)

[Session 1 of the test plan](hardware-test-plan.md#session-1-start-up-timing-hw-11). With the V31
and a controller plugged in, note the step times that the splash shows (release build).
**Why it matters:** if listing and opening the MIDI ports takes noticeable time, the work is
moved off the window's thread (CD-13, RV-61); if it does not, that work is dropped.

### B3. Rudiments on the V31 (#15; HW-10)

[Session 7](hardware-test-plan.md#session-7-rudiments-on-the-v31-hw-10), three parts:

- **A. Double triggers:** the first tempo at which *extra* hits appear on #1 single stroke
  roll, #16 single paradiddle and #33 double drag tap; and the V31 retrigger setting you needed.
- **B. By ear:** flams (#20 to #24) and drags (#31 to #33, #36, #37) in Listen: grace note too
  early, right, or too late; which roll endings (7 to 15) sound wrong against the chart.
- **C. Controller on the rudiments** while the V31 also sends hits (also HW-9).

### B4. Listen to the kit (#27; SND-1)

Headphones, Listen with the built-in sounds, a score with the whole kit (the sample score, then
a busy rudiment or two).

1. Play each instrument from the mixer (Settings → Sound, the play buttons).
2. Cymbals: crash 1 and 2, china, splash, ride, ride bell: do they sound like cymbals, decay
   believably, and sit well against each other? Is any too bright, too dull, too long?
3. The hi-hats (known to be very bright): closed, open, pedal.
4. Toms, kick, snare, cross-stick: loudness against each other (use the instrument volumes if
   needed, they are saved).

**Report:** for each instrument that is wrong, "too bright / too dull / too long / too short / too
loud / too quiet" in a line. The cymbal parameters in `synth.rs` are then tuned from it.

### B5. WAV override and the hi-hat choke (#30; SND-3)

1. **WAV override:** put a short WAV named `Kick.wav` (any kick sample, mono or stereo, 16 or 24
   bit) in the `sounds` folder next to the settings file (Settings → Sound shows the path).
   Restart the program, play the kick from the mixer: it is your file, levelled like the built-in
   one. Put a damaged file named `Snare.wav` there too: the snare stays the built-in sound and the
   program says why. Remove the files afterwards.
2. **Hi-hat choke:** Listen to a score with an open hi-hat followed by a closed one (or play an open
   hat and press the pedal on the V31): the open hat is **cut off** at once when the hat closes.

**Report:** whether the override sounds right, and whether the choke is clean or clicks.

---

## C. Controllers

All in [`hardware-test-plan.md`](hardware-test-plan.md); each session says what to press and what
you should see, and has a line in the results sheet (Appendix A).

| Session | Issues | What it answers |
|---|---|---|
| [2. X-Touch Mini](hardware-test-plan.md#session-2-x-touch-mini-hw-1-hw-2-hw-4-hw-9) | #1, #2, #4, #17 | do the numbers in the preset match the device; does the Mini switch layers by itself; does the drawn picture look like the device; the Rudiments tab with the controller |
| [3. Manage window and Learn](hardware-test-plan.md#session-3-the-manage-window-and-learn-hw-7) | #7 | creating, copying, renaming, deleting presets; Learn a button and an endless encoder; Detect, layers, lights; the channel option. (Since the plan was written: the Learn logic was rebuilt, so this session is also the check of that change.) |
| [4. Unplugging and plugging in](hardware-test-plan.md#session-4-unplugging-and-plugging-in-hw-8) | #13 | the program notices a controller pulled out or plugged in |
| [5. X-Touch Compact](hardware-test-plan.md#session-5-x-touch-compact-hw-3-hw-5-hw-4) | #3, #5, #4 | the Compact preset, what its side encoders and LAYER buttons send, its picture |
| [6. Generic controllers](hardware-test-plan.md#session-6-generic-midi-controllers-hw-6) | #6 | the four generic presets (Launch Control XL, nanoKONTROL2, MPK Mini, BeatStep), 15 minutes each |

If you can do only one thing from this group, do **Session 2**: the Mini is the default
controller. New since the plan was written: the **Passes** action (A1, step 7) and the
**keep awake** setting do not need a controller session, but a controller with an encoder is
useful for A1.

---

## D. A real mouse

### D1. Mouse and wheel (#29; UI-1)

Only keyboard and headless tests have exercised these.

1. Score tab → **Layout** popup: open it, change bars per row, the alignment, the break-at-pause
   option; close it by clicking outside.
2. Settings → Sound, the mixer: the **mute**, **solo** and **play** buttons of several
   instruments, while Listen plays.
3. The **wheel on every slider** (tempo, volume, count-in, ramp, zoom...): hover and scroll, one
   step per notch; the page must not scroll at the same time. Also over the new *passes* box.
4. Tabs, the splash, the Home tab's recent scores: clicking works everywhere, nothing needs a
   second click.

**Report:** anything that needs two clicks, jumps, or scrolls the page under the wheel.

---

## E. Reading and decisions

### E1. The flagged rudiment rows (#14; RUD-1)

The chart: <https://www.coloradomesa.edu/marching-band/documents/rudiments.pdf> (PAS, 1984).

1. Open `docs/rudiments.md` and find the rows marked **⚑**: the closing note values of rolls
   7 to 15, drag paradiddles #1 and #2, and the flam drag.
2. Put the chart next to them. Only the **note values** are in doubt (the sticking was read
   carefully and is tested).
3. For each row: "right" or the corrected value.

**Report:** the list of corrections in the issue; the rudiments are then fixed and
`docs/manual/rudiments.json` regenerated.

### E2. The bar numbering (#31; UI-2)

The score shows each bar's *printed* number (repeated after unrolling repeats); the Measures
sliders and *start a row at bar* use the *position*. Decide:

- **(a)** show both numbers on the bars, **(b)** make the sliders use the printed numbers, or
- **(c)** leave it and explain it in the manual.

### E3. The name (#36; NAME-1)

Before the program is shipped widely. Known: THE ROLL MODEL is a registered US trademark (Tune Up
Fitness Worldwide), a different field with possible overlap in education (class 41); nothing was
checked in the EU or elsewhere.

1. US: the USPTO trademark search for "Roll Model", classes 9 (software), 28, 41.
2. EU/UK: TMview or EUIPO's search, UK IPO.
3. A web search for "Roll Model" with drum, rudiment, metronome, music; GitHub and app-store
   search.
4. Domain names you would want.
5. Decide: **keep the name**, or **rename** (this touches the repository, the crates, the
   documents and the diary: say so before it is done).

### E4. The local folder (#37; NAME-2)

Rename `D:\projects\DrumTutor` to match the program, yes or no. (Renaming a folder an open
session is working in is best done between sessions; paths in old transcripts stop working.)

### E5. Your own exports (#56; IMP-5)

Drum scores exported from the programs you have, to see how the importer copes:

1. Export a drum part as MusicXML (`.musicxml` or `.mxl`) from **MuseScore 4, Guitar Pro 8,
   Sibelius, Dorico** (whichever you have).
2. Open each in the program. Check by eye: every instrument on the right line, repeats and
   endings, the tempo and the time signature, grace notes, nothing missing or doubled.
3. Do not commit scores that are copyrighted songs.

**Report:** per program: opens, and what is wrong. The manual's list of tested programs follows.
If the export route fails for your files, IMP-7 (a direct reader) wakes up; otherwise it stays
parked.

---

## F. Other machines

### F1. Linux and macOS (#51; AUD-14)

On each machine: build (`cargo build --release`, Linux needs `libasound2-dev pkg-config`), check
that the program starts and plays Listen and the metronome; run `examples/audio_latency.rs` with
a loopback cable (the instructions are in the example's header and in
[`audio-latency-study.md`](audio-latency-study.md)); then try a fixed buffer size and measure
again. **Report what the host accepts** before any code is written. The screen keep-awake is
Windows only until these machines are tried.

---

## G. What waits for these checks

| Waiting work | Waits for |
|---|---|
| CD-13 / RV-61: MIDI listing and opening off the window's thread | B2 (the start-up times) |
| RV-30 / CD-5: one scheduler instead of two (also needed for recording N passes, FEAT-7 and the clock in takes) | B0 for step 1 (calibration); then a real take on a release build for step 2: the AI session will say which interface to bridge (V31 and the audio output) and wait for you |
| RV-62: allocation in the audio callback | the same hardware session |
| Closing the Listen part of FEAT-8 / FEAT-10 | A1 |
| Closing PLAT-1 | A3 |
| The next release (0.10.6): the passes, the keep-awake, the removed rolling notes | A1, A2 and A3 |
| Controller features CTL-5 (motor faders), CTL-6 (scribble strips), CTL-7 (a second unit), RUD-4 (grading the sticking) | not checks: they need devices you may not have; leave them until you want them |

## Already checked (nothing to do)

The MIDI clock in Listen and on the Metronome tab (HW-13, 2026-11-04); the audio engine on
unplugging, a changed default output and a looping Listen with a ramp (AUD-8); the AUD-9, AUD-10
and AUD-13 latency questions (no-go, negative, same result).
