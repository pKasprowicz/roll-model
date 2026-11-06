# Checks only the owner can do

Everything that is still waiting for a person with a drum module, a controller, ears, a mouse, a
second computer or a decision. Made on **2026-11-05** from `HANDOFF.md`, the open issues and the
state of `develop` (after release 0.10.5). Nothing in this list can be done by the program's tests
or by the AI session. Every check says **what you need, how to set up, exactly what to do, what you
should see, what to write down if you see something else, and what to report**.

- [0. Read this first](#0-read-this-first): builds, backups, where things are, how to report
- [At a glance](#at-a-glance)
- [A. The current develop build](#a-the-current-develop-build)
- [B. The Roland V31 and your ears](#b-the-roland-v31-and-your-ears)
- [C. Controllers](#c-controllers)
- [D. A real mouse](#d-a-real-mouse)
- [E. Reading and decisions](#e-reading-and-decisions)
- [F. Other machines](#f-other-machines)
- [G. What waits for these checks](#g-what-waits-for-these-checks)

---

## 0. Read this first

### 0.1 The two builds you will need

Some checks need the **released 0.10.5**, some the **current develop build**, some compare the two.
Both print `Version 0.10.5` on the start-up splash (develop has not been given a new number yet), so
keep them apart by **folder**, not by the version text.

1. Make a folder `C:\RollModelChecks` with two sub-folders, `released` and `develop`.
2. **Released:** on <https://github.com/pKasprowicz/roll-model/releases> download
   `roll-model-v0.10.5-windows-x86_64.zip`, unzip it into `C:\RollModelChecks\released`. It contains
   `roll-model.exe`, the manual and an `examples` folder (with `basic_rock_beat.musicxml`).
3. **Develop:** in the repository folder (`D:\projects\DrumTutor`), in PowerShell:

   ```powershell
   git checkout develop
   git pull
   cargo build --release
   Copy-Item target\release\roll-model.exe C:\RollModelChecks\develop\
   Copy-Item examples C:\RollModelChecks\develop\ -Recurse -Force
   ```

   The build takes a few minutes the first time. A release build matters: a debug build is slower
   and would make every timing check meaningless.
4. Run `C:\RollModelChecks\released\roll-model.exe` or `C:\RollModelChecks\develop\roll-model.exe`
   (double-click). **Only one copy at a time**: on Windows a MIDI port can be opened by one program
   only, so a second copy says "could not open".
5. Both builds read the same settings (`%APPDATA%\RollModel`). That is fine: the older build ignores
   settings it does not know.

### 0.2 Before the first session: back up and close other programs

- **Back up:** copy the whole settings folder. In the Explorer address bar paste
  `%APPDATA%\RollModel` to look at it; to copy it:

  ```powershell
  Copy-Item $env:APPDATA\RollModel C:\RollModelChecks\settings-backup -Recurse
  ```

  To go back to it after a session (close the program first):

  ```powershell
  Remove-Item $env:APPDATA\RollModel -Recurse -Force
  Copy-Item C:\RollModelChecks\settings-backup $env:APPDATA\RollModel -Recurse
  ```

- **Close** every other program that uses MIDI or the sound output in an exclusive way: a DAW, a
  MIDI monitor, the controller's vendor software, a browser tab with WebMIDI.
- **Do not "fix" settings to make a check pass.** If something does not work, write down what you
  saw first (0.5), then change things.

### 0.3 Where things are in the program

- **Tabs** (top row): *Home*, *Score*, *Rudiments*, *Metronome*, *Results*, *Settings*.
- **Settings** has pages along its top: *MIDI input*, *Controller*, *Timing*, *Sound*, *General*.
- **Keys:** `R` record, `L` listen (press again to stop), `M` metronome, `K` click on/off,
  `Esc` stop everything. There is no key for loop or passes.
- **Opening a score:** *Home* → **📂 Open score…**, or drop a file onto the window. The sample score
  is `examples\basic_rock_beat.musicxml` (in the repository and in the release zip).
- **The three rows at the top of the Score and Rudiments tabs.** Row 1, the transport: **⏺ Record
  (R)**, **⏵ Listen (L)**, the position text, a zoom slider, *follow*, the **Layout** menu. Row 2:
  *Measures* (from, to; on the Rudiments tab: the rudiment, *repeats* and *lead*). Row 3: *tempo*,
  then *click (K)*, *notes*, **passes**, *count-in bars*, *in Listen*, *ramp +* (and *every* when
  ramp is on). Part of row 3 is dimmed while a take records.
- **The MIDI input monitor** is on *Settings → MIDI input*, "Monitor (latest first)": one line per
  hit, `ch 10 | note 38 (D2) | vel 96 | pedal 64 | Snare`; the last column is the instrument the
  note is mapped to (or *unmapped*).
- **The controller monitor** is on *Settings → Controller*.
- **Version:** the start-up splash.

### 0.4 What you can read when something goes wrong

- `%APPDATA%\RollModel\roll-model.log`: problems the sound and MIDI threads reported. Attach it to
  a failed check.
- `%APPDATA%\RollModel\config.txt`: the settings, readable text.
- A **screenshot** (Win + Shift + S) of the page, the monitor or the status line is worth a
  paragraph of description.

### 0.5 How to report a check

Comment on the check's issue (or just paste this to the AI session). A fail is as useful as a
pass.

```
Check:      (for example A1, passes)
Build:      released 0.10.5 | develop (date of your build)
Machine:    Windows 11 / which computer, which sound output, which devices
Result:     PASS | FAIL | PARTIAL
Steps:      which numbers passed, which did not
What I saw: (one or two sentences, and numbers: milliseconds, BPM, note numbers)
Attached:   screenshot / roll-model.log / nothing
```

If a check has numbers to write down (calibration results, the BPM where extra hits begin),
they go in "What I saw".

---

## At a glance

| # | Check | Issue | Needs | Time | Unblocks |
|---|---|---|---|---|---|
| A1 | Passes instead of loop | #111, #113 | develop build | 15 min | closing the Listen part; recording N passes |
| A2 | Metronome tab without the rolling notes | – | develop build | 3 min | the next release |
| A3 | The screen stays awake | #115 | develop build, Windows | 10 min | closing PLAT-1 |
| A4 | The diary site | – | a browser | 10 min | telling people about it |
| B0b | Takes through the new scheduler | #88 | V31, audio output | 30 min | RV-30 done; FEAT-7 and recording N passes |
| B1 | MIDI input and ports on the real V31 | #108 | V31 | 20 min | closing HW-12 and RV-32 |
| B3 | Rudiments on the V31 | #15 | V31, headphones | 45 min | rudiment tuning, RUD-1 |
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

## A. The current develop build

Use `C:\RollModelChecks\develop\roll-model.exe` (0.1). None of A1 to A3 needs a device, except where
A1 step 8 says so.

### A1. Passes instead of loop (#111, #113)

**What changed.** The *loop* checkbox is gone. In its place is a number box labelled **passes** on
the Score and Rudiments tabs: how many times Listen goes through the selected measures (or the
rudiment) before it stops by itself. 1 is what loop-off was; the top of the box is *unlimited*,
which is what loop-on was.

**Setup.** Start the program. *Home* → **📂 Open score…** → the sample score
`basic_rock_beat.musicxml`. In the third row of the Score tab (next to the tempo) find **passes**; it
shows **1**.
(The box is a number you can **drag sideways** or **double-click and type**.)

**Steps**

1. Press **⏵ Listen**. Expect: the count-in (if *in Listen* is ticked), the music once, it stops by
   itself, and the status line says **Listen finished**.
2. Double-click the passes box, type `3`, Enter. Press Listen. Expect: the range plays **exactly
   three times**; the count-in only before the first; the status line says **Listen finished: 3
   passes**. Count them by ear (or watch the playhead jump back twice).
3. With 3 passes, press Listen, and **during the second pass** change the box to `1`. Expect: the
   pass that is running plays to its end, then it stops.
4. Drag the box up to the top, or type `unlimited`. Expect the box to show the word *unlimited*;
   Listen goes on until you press **Stop listening (L)**.
5. **Restart behaviour.** With *unlimited* set, close the program and start it again: the box is back
   to **1**. Then set it to `3`, close, start: it is still **3**.
6. **Rudiments tab:** the box is there and shows the same value. Set *ramp +* to e.g. 10 % with
   *every* 1 pass and passes to 4: Listen gets faster on each pass, 4 passes, then stops.
7. **Controller (skip without one).** Settings → Controller → **Manage**; in the table find the
   row **Passes** under "Values (knobs, encoders, faders)"; click **Learn** on that row and turn an
   encoder (or move a fader): it is bound. Back on the Score tab, turning the encoder steps the box
   one at a time; with Shift held, five at a time. A fader: bottom is 1, top is *unlimited*.
   The controller's **loop** button toggles the box between 1 and *unlimited* (if you bound one).
8. **Recording (known gap).** Set passes to 3, press **⏺ Record (R)** (V31 needed to hit pads, but you
   can also just let it run): the take is **one pass**; the box cannot be changed while it records.
   This is expected: recording N passes is not built yet.

**Expected:** 1 to 6 as written. **If different:** note which step, what the box showed, and
whether it played one pass too many or too few (a count that is off by one is the likely bug).
Also tell me if dragging the box is too fast or too slow to use.

### A2. The Metronome tab without the rolling notes

**What changed.** The strip of rolling notes under the beat lights was removed (with its *Show the
notes* tick box and the *Match score tempo and bar* button).

**Steps**

1. Open the **Metronome** tab. Expect: the heading, the tempo / beats per bar / subdivision / volume /
   ramp controls, the **row of beat circles**, and nothing below them.
2. Press **⏵ Start (M)**: the circles light in turn, the first in red; change the tempo while it
   runs: it follows after a moment. Press **⏹ Stop (M)**.
3. Make the window narrow and short (drag its corner): everything still reachable.
4. Switch to another tab and back while it runs: the lights keep going.

**Expected:** the tab looks finished, not like something was cut out of it. **If different:**
screenshot, and say what looks missing or leftover.

### A3. The screen stays awake (#115)

**Needs:** the develop build, **Windows**, about ten minutes (you wait out a screen timeout).

**Setup.** Windows **Settings → System → Power & battery → Screen, sleep & hibernate timeouts**. Write
down the current values. Set *Turn off my screen after* and *Put my device to sleep after* to
**1 minute** (both the "on battery" and "plugged in" rows, for whichever you are on).

**Steps**

1. Start the develop build. *Settings → General → Screen*: **Keep the screen awake while Roll Model is
   open** is ticked (it is by default).
2. Do not touch the keyboard or mouse for **3 minutes**. Expect: the screen **stays on**.
3. Optional proof: an **administrator PowerShell**, `powercfg /requests`. Expect an entry under
   `DISPLAY:` (and `SYSTEM:`) that mentions `roll-model.exe` (the exact wording differs by Windows
   version).
4. Untick the box. Do not touch anything for 2 minutes. Expect: **the screen turns off** as usual.
   `powercfg /requests` no longer lists the program.
5. Tick it again, then **close the program**. Do not touch anything for 2 minutes. Expect: the screen
   turns off (the program let go on exit).
6. **Put the Windows power settings back** to the values you wrote down.

**If different:** which step, the Windows version, and whether the machine is a laptop on battery. A
screen that locks although the box is ticked (step 2) is the failure that matters. Note that a
**lock screen set by company policy** can ignore this request.

### A4. The diary site

<https://pkasprowicz.github.io/roll-model/diary/>. In a browser; no build.

1. The list shows **four entries**, the newest first; each opens; the links in them work (issues, the
   manual, the roadmap).
2. `/diary/roadmap/`: three lists, **Now**, **Next**, **Later**. Do they hold what you expect? Move an
   issue by changing its label on GitHub (`roadmap:now`, `roadmap:next`, `roadmap:later`); the page
   follows within a day (or after the next entry is pushed).
3. `/diary/feed.xml`: open it in a feed reader (or just the browser): four entries with summaries.
4. A phone or a narrow browser window: the text is readable, nothing scrolls sideways.
5. Read it as a drummer who has never seen the project. Is any sentence unclear, or does any claim go
   further than what is true?

**Known:** the site's start page does not link to the diary until the next release. **Report:**
wording to change.

---

## B. The Roland V31 and your ears

For every B check: V31 on USB, no other MIDI program running, and a **release build**. B0b needs the
develop build and the released one.

### B0. Calibration through the new scheduler (#88, RV-30 step 1): **done 2026-11-06**

Verified by the owner: the calibration through the `Player` gives the same result as on 0.10.5 and
sounds right. Nothing to do. RV-30 step 2 (takes) has a check of its own: **B0b**, below.

### B0b. Takes through the new scheduler (#88, RV-30 step 2)

**Why.** A take's count-in and clicks now come from the `Player`, and the old metronome thread is gone.
The take's *grading* depends on this timing, so it has to be checked with real hits.

**Needs:** the V31 on USB and the computer's sound output as you normally use it (wired), both builds
(see 0.1), the sample score, a quiet room. Calibrate on **both** builds first (as in the old B0) if you
have not lately: the latency compensation is part of the result.

**Setup.** One build at a time. *Settings → MIDI input*: the V31 connected; *Settings → Sound*:
*Click through the computer's sound output* ticked, *Send the click to the MIDI output* unticked, for
steps 1 to 4. *Home* → open `basic_rock_beat.musicxml`; set *count-in bars* to 2 and *click (K)* ticked
on the Score tab.

**Steps**

1. **Released 0.10.5.** Press **⏺ Record (R)**. You hear a two-bar count-in and then the click as the
   music starts; play the score's snare and kick along with the click as well as you can for the whole
   range. When the take ends (or press Esc after the last bar), open **Results**. Write down the
   **Accuracy**, the **Timing** (mean offset in ms, and its spread) and the **Extra / Missed** numbers.
   Do it **three times**.
2. **Develop build.** The same three takes, with the same care. Write the same numbers.
3. **Compare.** The *mean offset* of the two builds should agree within a few ms (take-to-take variation
   of your playing is bigger than any difference between builds); accuracy similar. A **constant shift**
   of the develop build (every take 10 ms earlier or later) is the failure to look for.
4. **By ear.** During the take the clicks are even, none missing, none doubled, none late; the count-in
   is the right length (two bars).
5. **Switches while recording (develop).** During a take press **K**: the click stops (the count-in
   already passed) and comes back with the next press. Change *Settings → Sound* click volume while a
   take records (open Settings in the same window): the volume follows.
6. **Stopping (develop).** Start a take and press **Esc** during the count-in, and again in the middle:
   it stops at once, nothing sounds afterwards, and the next take starts normally.
7. **The click as a MIDI note (optional).** *Settings → Sound*: untick the computer's click, tick *Send the
   click to the MIDI output*, note 76, channel 10, the V31 as MIDI output. Record three takes on each
   build and compare as in 3. With **both** click paths ticked, the V31's click and the computer's are
   heard together (the output delay moves the MIDI click): note whether they are together or one is
   early.

**Pass:** 3 to 6 as written (7 if you do it). **If different:** both builds' numbers for each take
(accuracy, mean offset, spread, extra, missed), which build is shifted and by how much, and anything
audible. Attach `roll-model.log`.

### B1. MIDI input and ports on the real V31 (#108, HW-12)

**Why.** Commit `39f3f25` changed how every MIDI port is opened and how the drum module's messages are
read; the unit tests use byte sequences, not a port. Your MIDI-clock check already exercised opening an
**output**. This covers the **input**, the hi-hat pedal and the controller.

**Needs:** the V31 on USB; for step 5 an X-Touch Mini; the develop build.

**Steps**

1. *Settings → MIDI input*. **Input port** → choose the V31. Hit a **snare** pad. Expect a monitor line
   such as `ch 10 | note 38 (D2) | vel 90 | Snare`: the **channel is 10**, the note and the velocity
   are what you hit. Hit the kick, a tom, a crash.
2. **Hi-hat.** With the pedal **up**, hit the hi-hat: the last column says **open** hi-hat. Press the
   pedal **down** and hit: **closed** hi-hat. The monitor's `pedal N` column shows the pedal
   position that came with the hit. Move the pedal without hitting: **no line appears** (the pedal is
   not a hit). The slider "Hi-hat pedal (CC#4) value when closed" on the same page is the setting
   that decides open or closed.
3. **A part of the name.** Close the program. Open `%APPDATA%\RollModel\config.txt` in Notepad, set
   `midi_in_port = V31` (only the part of the port name; keep the rest of the file), save, start the
   program. Expect: it connects to the V31 by itself (the Input port box shows the full name).
4. **A name that is not there.** Set `midi_in_port = NOPE`, start: the program says the port was not
   found (on the start-up splash, or on *Settings → MIDI input*), does not crash, and you can still choose a port by
   hand. Put the right value back (or restore your backup).
5. **Controller lights.** Connect the X-Touch Mini (*Settings → Controller*, preset *Behringer X-Touch
   Mini*): pressing a button shows a line in the controller monitor; press **record** on the Mini: its
   record LED lights (lights go out through the output port).
6. **The click as a MIDI note.** *Settings → Sound*: tick *Send the click to the MIDI output*, note 76,
   channel 10, choose the V31 as the MIDI output; *Metronome* tab → Start: the V31 sounds a click on
   each beat (note 76 is a wood block on the V31).

**Pass:** every step as written. **If different:** the step number and what the monitor showed
(a photo of the monitor is fine).

### B2. Start-up timing (#16, HW-11): **done 2026-11-06, positive**

Checked by the owner with the devices plugged in: nothing to fix. The times themselves were not
written down; if CD-13 / RV-61 (MIDI listing and opening off the window's thread) is to be decided on
numbers, run [Session 1](hardware-test-plan.md#session-1-start-up-timing-hw-11) once more and
record them.

### B3. Rudiments on the V31 (#15, HW-10)

Needs the V31, headphones, a quiet room, 45 minutes. The full steps (with where to click) are in
[Session 7](hardware-test-plan.md#session-7-rudiments-on-the-v31-hw-10). It has three parts:

- **A. Double triggers.** *Rudiments* tab, choose rudiment **#1 Single stroke roll**, tempo 60 BPM,
  **Record** a take, play cleanly for 8 bars, look at *Results*: the number in the **Extra** column
  should be 0. Raise the tempo to 120, 160, 200 and repeat; write down **the first tempo at which
  extra hits appear**. Repeat for **#16 Single paradiddle** and **#33 Double drag tap**. If extras
  appear at tempos that are not too fast for you, try one step higher *retrigger* or sensitivity on
  the V31 pad (a V31 setting, not the program's) and write down the setting and the result.
- **B. By ear (Listen).** #1 even spacing; flams **#20 to #24**: the grace note comes before the main
  note, quieter, about 35 ms: write *too early / right / too late*; drags **#31 to #33, #36, #37**
  (two grace notes); the closing notes of **rolls 7 to 15** against the PAS chart (see E1).
- **C. The controller on the rudiments** while the V31 also sends hits (also HW-9).

**Report:** the first extra-hit tempo for each rudiment, the V31 setting you needed, flam and drag
timing verdicts, and the rolls that end wrongly.

### B4. Listen to the kit (#27, SND-1): **done 2026-11-06, positive**

Listened to by the owner: the kit and the cymbals are good as they are, so the `Cymbal` parameters in
`synth.rs` stay. (The hi-hats were known to be very bright; if they still bother you, say so and
they become a task of their own.)

### B5. WAV override and the hi-hat choke (#30, SND-3)

**Needs:** headphones, one short drum WAV of yours (mono or stereo, 16 or 24 bit), any text file
renamed to `Snare.wav`.

1. *Settings → Sound* shows the path of the **sounds folder** (next to the settings file). Put your
   WAV there named exactly `Kick.wav`. Put the fake `Snare.wav` (junk text) there too.
2. Restart the program. *Settings → Sound* → ⏵ on **Kick**: you hear **your sample**, at a level like
   the built-in sounds. ⏵ on **Snare**: you hear the **built-in** snare, and the program reports that
   the file could not be read (status line or `roll-model.log`).
3. **Hi-hat choke.** Listen to a score with an **open hi-hat followed by a closed one** (or hit the
   open hat on the V31 and press the pedal down): the open hat is **cut off** at the moment the hat
   closes, cleanly (no click or burst).
4. Remove both files from the sounds folder.

**Report:** whether the override sounds right and is levelled, whether the damaged file is reported,
whether the choke is clean or clicks.

---

## C. Controllers

All steps are in [`hardware-test-plan.md`](hardware-test-plan.md): every session says what to press and
what you should see, and has a row in the **results sheet** (Appendix A). Do not start a session
without reading "Before you start" in that document once: it says how to put each controller in the
right mode.

**To start a session:** (1) copy Appendix A of the plan into a text file; (2) put the controller in
its mode (the two X-Touch devices in **MC / Mackie Control** mode, chosen while plugging in the USB
cable; the Korg nanoKONTROL2 in **CC mode**; the Novation on a **factory template**); (3) start the
**release** build; (4) *Settings → Controller*, choose the preset and keep the **monitor** in view
("Monitor (latest first)"): every message the controller sends appears there; (5) fill in the sheet
as you go, and send it back.

| Session | Issues | What it answers | Time |
|---|---|---|---|
| [2. X-Touch Mini](hardware-test-plan.md#session-2-x-touch-mini-hw-1-hw-2-hw-4-hw-9) | #1, #2, #4, #17 | do the numbers in the preset match the device; does the Mini switch layers by itself; does the drawn picture look like the device; the Rudiments tab with the controller | 45 min |
| [3. Manage window and Learn](hardware-test-plan.md#session-3-the-manage-window-and-learn-hw-7) | #7 | creating, copying, renaming, deleting presets; Learn a button and an endless encoder; Detect, layers, lights; the channel option. The Learn logic was rebuilt since the plan was written, so this is also the check of that change | 30 min |
| [4. Unplugging and plugging in](hardware-test-plan.md#session-4-unplugging-and-plugging-in-hw-8) | #13 | the program notices a controller pulled out or plugged in | 10 min |
| [5. X-Touch Compact](hardware-test-plan.md#session-5-x-touch-compact-hw-3-hw-5-hw-4) | #3, #5, #4 | the Compact preset, what its side encoders and LAYER buttons send, its picture | 45 min |
| [6. Generic controllers](hardware-test-plan.md#session-6-generic-midi-controllers-hw-6) | #6 | the four generic presets (Launch Control XL, nanoKONTROL2, MPK Mini, BeatStep) | 15 min each |

**Suggested order:** 2, 4, 3, 5, 6. If you can do only one, do **Session 2**: the Mini is the default
controller, and its preset is the one most people will run.

**A number that is wrong.** When a control sends a number different from the one the preset expects,
write the **monitor's line** (a photo is fine). Do not edit the preset: the AI session corrects the
preset from your sheet.

**New since the plan was written** (use a develop build for these): the **Passes** action (A1,
step 7) and the keep-awake setting. In Session 3 the table of actions has a row **Passes**; check that
it can be learned like the others.

---

## D. A real mouse

### D1. Mouse and wheel (#29, UI-1)

Only keyboard and headless tests have exercised these. Use a real mouse with a wheel (a touchpad
scroll is not the same thing).

1. **Layout popup.** Score tab with the sample score → **Layout** (in the transport row). Change *bars
   per row* (drag, and the wheel over it), *start a row at bar*, *new row after a pause*. Click
   outside the popup: it closes. Open it again: your values are kept.
2. **Mixer buttons.** *Settings → Sound → Instruments*. While **Listen** plays the sample score, click
   **Mute** on the kick (it goes silent), un-mute it; click **Solo** on the snare (only the snare plays),
   un-solo; click **⏵** on several instruments (each sounds on its own).
3. **The wheel on every slider.** Hover a slider and scroll one notch: it changes by **one step**, and
   **the page does not scroll** at the same time. Try tempo, volume, count-in, ramp, zoom, and the
   **passes** box (develop build).
4. **Clicking around.** Tabs, the Home tab's recent scores and the × to remove one, the Settings page
   tabs: each responds to **one click**.

**Report:** anything that needs two clicks, jumps, scrolls the page under the wheel, or is too small to
hit.

---

## E. Reading and decisions

### E1. The flagged rudiment rows (#14, RUD-1)

**What.** `docs/rudiments.md` lists the 40 PAS rudiments. The rows marked **⚑** were read from a small
engraving and the **note values** may be wrong (the sticking was read carefully and is tested).

**Which rows:** rudiments **#7 to #15** (the closing notes of the rolls, rows 43 to 51 of the file),
**#30 Flam drag**, **#36 Drag paradiddle #1**, **#37 Drag paradiddle #2**.

**Steps**

1. Open the PAS chart: <https://www.coloradomesa.edu/marching-band/documents/rudiments.pdf> (PAS, 1984).
2. Open `docs/rudiments.md` next to it (in the repository, or on GitHub).
3. For each flagged row, find the rudiment in the chart and compare **only the note values** (what each
   note's length is: eighth, sixteenth, and so on, and the closing note of a roll). The text in the
   last-but-one column is what the catalogue says.
4. Write for each: **right**, or **the correct values** in the chart's own words ("eighth, then four
   sixteenths").

**Report:** a list in the issue. The rudiments are then corrected and the manual's table
(`docs/manual/rudiments.json`) is regenerated.

### E2. The bar numbering (#31, UI-2)

**The problem.** Bars drawn in the score show their **printed number** (repeated after unrolling
repeats). The Measures sliders and *start a row at bar* count by **position**. Open a score with
repeats to see it: bar numbers in the picture and the slider values differ after the repeat.

**Decide one:**
- **(a)** show **both** numbers on the bars;
- **(b)** make the sliders use the **printed** numbers;
- **(c)** leave it, and explain it in the manual.

### E3. The name (#36, NAME-1)

**Known.** THE ROLL MODEL is a registered US trademark (Tune Up Fitness Worldwide, Inc.; Jill Miller's
book and method), a different field; class 41 (education) may overlap. Nothing was checked in the EU or
elsewhere.

**Steps**

1. **US:** <https://tmsearch.uspto.gov/> search "roll model", look at live entries in classes **9**
   (software), **28** and **41**; note the owner and the goods.
2. **EU:** <https://www.tmdn.org/tmview/> search "roll model" and also "rollmodel". **UK:**
   <https://www.gov.uk/search-for-trademark>.
3. **Web:** "Roll Model" with *drum*, *rudiments*, *metronome*, *practice app*; GitHub and app-store
   search; note any product in music or education.
4. **Domains and handles** you would want (`rollmodel.app` and the like).
5. **Decide:** *keep the name*, or *rename*. A rename touches the repository, the crates, the manual,
   the developer guide and the diary; say so and it becomes its own task.

**Report:** the findings (links) and the decision. This is not legal advice; if the overlap matters to
you, ask a trademark lawyer.

### E4. The local folder (#37, NAME-2)

Yes or no: rename `D:\projects\DrumTutor` to match the program. Best done between sessions, with no
editor or AI session open in the folder; old transcripts will no longer find their paths.

### E5. Your own exports (#56, IMP-5)

**Steps (for each program you have)**

1. Export a drum part as **MusicXML** (`.musicxml` or `.mxl`):
   **MuseScore 4:** File → Export → *MusicXML* (choose the drum part only);
   **Guitar Pro 8:** File → Export → *MusicXML*;
   **Sibelius:** File → Export → *MusicXML*;
   **Dorico:** File → Export → *MusicXML*.
2. In Roll Model: *Home* → **📂 Open score…** and choose the file.
3. Check by eye, bar by bar, against the original: every **instrument on the right line**, the
   **repeats and endings**, the **tempo** and the **time signature**, **grace notes (flams, drags)**,
   nothing **missing or doubled**.
4. Do not commit scores that are copyrighted songs; keep them out of the repository.

**Report:** per program: *opens* or the error message, and what is wrong (bar number and what). The
manual's list of tested programs follows. If the export route fails for your files, IMP-7 (a direct
reader) wakes up; otherwise it stays parked.

---

## F. Other machines

### F1. Linux and macOS (#51, AUD-14)

On each machine:

1. **Build.** Linux: `sudo apt install libasound2-dev pkg-config` (or your system's equivalent), then
   `cargo build --release`. macOS: `cargo build --release`.
2. **Run** `target/release/roll-model`: it starts and shows the splash. Open the sample score; press
   **Listen**: you hear it; press **M** (metronome): you hear the click; *Settings → Sound* shows which
   output is open and its rate.
3. **Latency** (needs the output wired back to an input: a cable from an interface's output to its
   input, or a device with a loopback). List the devices, then measure:

   ```bash
   cargo run --release --example audio_latency -- list
   cargo run --release --example audio_latency -- run "<part of the output name>" "<part of the input name>" 40
   ```

   Turn the loopback level so the clicks are clear but the peak the tool prints does not clip.
   Write down the result in milliseconds.
4. **A fixed buffer size.** The engine accepts any block size; whether the system gives a smaller buffer
   than Windows' 22 ms floor is the question. Note what the audio host reports (ALSA / PipeWire /
   CoreAudio) and whether `BufferSize::Fixed` is honoured; do not change code, just report.

**Report:** per machine: builds yes/no, plays yes/no, the latency number, what the host accepts. The
screen keep-awake is Windows-only until these machines are tried.

---

## G. What waits for these checks

| Waiting work | Waits for |
|---|---|
| CD-13 / RV-61: MIDI listing and opening off the window's thread | B2 is done and positive: a decision, not a check (is the work still wanted?) |
| RV-30 / CD-5: one scheduler instead of two (also needed for recording N passes, FEAT-7 and the clock in takes) | **B0b** (takes; step 1, the calibration, is verified) |
| RV-62: allocation in the audio callback | the same hardware session |
| Closing the Listen part of FEAT-8 / FEAT-10 | A1 |
| Closing PLAT-1 | A3 |
| The next release (0.10.6): the passes, the keep-awake, the removed rolling notes | A1, A2 and A3 |
| Controller features CTL-5 (motor faders), CTL-6 (scribble strips), CTL-7 (a second unit), RUD-4 (grading the sticking) | not checks: they need devices you may not have; leave them until you want them |

## Already checked (nothing to do)

The MIDI clock in Listen and on the Metronome tab (HW-13, 2026-11-04); the start-up timing (HW-11) and the sound of the kit (SND-1), both positive (2026-11-06); the calibration through the new scheduler (B0, 2026-11-06); the audio engine on unplugging, a
changed default output and a looping Listen with a ramp (AUD-8); the AUD-9, AUD-10 and AUD-13 latency
questions (no-go, negative, same result).
