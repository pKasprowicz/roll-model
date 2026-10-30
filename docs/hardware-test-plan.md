# Hardware test plan

What to try with the real devices, in which order, and what to write down. It covers the open
`HW-1` to `HW-11` tasks of `HANDOFF.md`. Everything else in the program has been tried only
without devices, so these sessions are where the guesses in the presets get checked.

You do not need to read any code. Every step says what to press, what you should see, and what
to do when you see something else. A results sheet is at the end (Appendix A): fill it in as you
go and give it back, and the presets and pictures are corrected from it.

- [Before you start](#before-you-start)
- [Plan at a glance](#plan-at-a-glance)
- [Session 1: start-up timing (HW-11)](#session-1-start-up-timing-hw-11)
- [Session 2: X-Touch Mini (HW-1, HW-2, HW-4, HW-9)](#session-2-x-touch-mini-hw-1-hw-2-hw-4-hw-9)
- [Session 3: the Manage window and Learn (HW-7)](#session-3-the-manage-window-and-learn-hw-7)
- [Session 4: unplugging and plugging in (HW-8)](#session-4-unplugging-and-plugging-in-hw-8)
- [Session 5: X-Touch Compact (HW-3, HW-5, HW-4)](#session-5-x-touch-compact-hw-3-hw-5-hw-4)
- [Session 6: generic MIDI controllers (HW-6)](#session-6-generic-midi-controllers-hw-6)
- [Session 7: rudiments on the V31 (HW-10)](#session-7-rudiments-on-the-v31-hw-10)
- [What to do with a number that is wrong](#what-to-do-with-a-number-that-is-wrong)
- [Appendix A: results sheet](#appendix-a-results-sheet)
- [Appendix B: reading the monitor](#appendix-b-reading-the-monitor)

## Before you start

**What you need.** Only the devices of the sessions you do: the Roland V31 (USB), a Behringer
X-Touch Mini, an X-Touch Compact, and any of the four generic controllers (Novation Launch
Control XL, Korg nanoKONTROL2, Akai MPK Mini, Arturia BeatStep). A session does not need the
others. Headphones for the sessions that listen. A phone or a watch with seconds is not needed:
the splash shows the times.

**The program.** Use a release build, because the debug build is slower and would make the
start-up times meaningless:

```bash
cargo build --release
```

Run `target\release\roll-model.exe`. (An installed release is fine for every session except
Session 1: the step times on the splash exist only in builds made after 0.7.0.)

**Close other programs that use MIDI** (a DAW, a MIDI monitor, the controller's vendor
software, a browser tab with WebMIDI). On Windows a MIDI port can be opened by one program at a
time; if the program says "could not open", something else has it.

**Where the files are.** Settings: `%APPDATA%\RollModel\config.txt`. Your own controller
presets: `%APPDATA%\RollModel\controllers\`. Paste `%APPDATA%\RollModel` into the Explorer
address bar to open the folder. Before the first session, copy the whole `RollModel` folder
somewhere as a backup; nothing here should damage it, but the tests change settings.

**Controller modes.** The two X-Touch devices must be in **MC (Mackie Control) mode**. How to
choose the mode is in each device's manual (on the Mini it is chosen while plugging in the USB
cable). If the numbers the monitor shows are nowhere near the tables below, the device is
probably in another mode: write down what it sends and which mode you had. The Korg nanoKONTROL2 must be in **CC mode** (hold SET MARKER and CYCLE while plugging in
the USB cable). The Novation must be set to a **factory template** (the template number lights
up when you press the TEMPLATE buttons).

**How to take notes.** Copy Appendix A into a text file (or print it). For every control that
sends something different from what the preset expects, write the number the monitor shows.
Photos of the monitor and of the help screen are welcome and often faster than typing.

**The monitor** is on Settings → Controller, the section "Monitor (latest first)". Every message
the controller sends appears there as one line, newest first, for example
`ch 1 · note 95 on, velocity 127  →  record`. Appendix B explains how to read the lines.

**Safe to try.** Nothing in these sessions deletes scores or history. The program saves its
settings by itself; tests that change them (preset, port) are listed with how to undo them.

## Plan at a glance

| Session | Tasks | Needs | Time | Changes anything? |
|---|---|---|---|---|
| 1. Start-up timing | HW-11 | V31 and any controller plugged in | 10 min | no; the numbers decide task CD-13 |
| 2. X-Touch Mini | HW-1, HW-2, HW-4 (Mini), HW-9 | Mini in MC mode | 45 min | findings fix `presets/x-touch-mini.txt` and the picture |
| 3. Manage window and Learn | HW-7 | Mini, and one generic controller if you have it | 30 min | creates and deletes test presets |
| 4. Unplug and plug in | HW-8 | any controller | 10 min | no |
| 5. X-Touch Compact | HW-3, HW-5, HW-4 (Compact) | Compact in MC mode | 45 min | findings fix `presets/x-touch-compact.txt` and the picture |
| 6. Generic controllers | HW-6 | each controller you own, 15 min each | 15 min per device | findings fix the four presets |
| 7. Rudiments on the V31 | HW-10 | V31, headphones | 45 min | listening and tuning notes |

Suggested order: 1, then 2, 4, 3 (the Mini is the default controller, so what you learn in 2
makes 3 and 4 easier), then 5, 6, 7. Sessions are independent; do what you have time for. If you
can do only one, do Session 2: the Mini is the default controller, and its preset is the one
most users will run.

---

## Session 1: start-up timing (HW-11)

**Why.** The program lists and opens the MIDI ports on the window thread during start-up. If a
driver takes a second to answer, the window stalls for a second. Task CD-13 (move that work to a
background thread) is only worth doing if the times are large; the first measurement had no
devices attached.

**What you need.** The V31 and your usual controller plugged in and switched on; the program
closed.

1. Close the program. Plug in the V31 and the controller. Wait ten seconds (Windows enumerates
   the devices when they appear; measure after that, not during).
2. Start `target\release\roll-model.exe`.
3. The splash window lists the steps as they finish. Each line ends with the time that step
   took, for example `Found 3 MIDI inputs and 2 MIDI outputs · 41 ms`. The splash stays open
   after loading; **do not click or press a key yet**.
4. Take a photo or a screenshot of the splash (Windows: `Win+Shift+S`), or write the lines down.
5. Close the splash by pressing any key. Close the program.
6. Repeat steps 2 to 5 **two more times** (three runs in all). The first run after plugging in a
   device is often slower than later ones; both numbers are interesting.
7. Repeat once with the devices **unplugged** (the program running without any device is the
   normal case for most users).

**Write down** (Appendix A, HW-11): the time of every line for each run, and which devices were
plugged in. Note a line that is far larger than the others (more than about 300 ms).

**What the numbers mean.**

| Largest step | Decision |
|---|---|
| under 100 ms | nothing to do; CD-13 stays closed except for the 2-second port watch |
| 100 to 500 ms | worth doing, low priority |
| over 500 ms | worth doing soon: the window is frozen that long on every start |

**Also try** (2 minutes): with the program running and the controller plugged in, watch whether
the window judders every two seconds (the port watch lists the MIDI ports on the window thread).
Scroll the score or drag a slider for ten seconds. Write down "smooth" or "judders about every
2 s".

---

## Session 2: X-Touch Mini (HW-1, HW-2, HW-4, HW-9)

**Before.** The Mini is in MC mode (see "Controller modes" above) and connected. Start the
program, go to Settings → Controller, choose the preset **Behringer X-Touch Mini**. The port
should be found by itself; the top bar shows the controller's name with a green dot. If the dot
is red or the port is missing, press **⟳ Refresh and reconnect**; if the Mini does not appear
in the "Port" list, it is not in a mode that exposes a MIDI port.

The Mini has two **layers** (A and B, chosen with the two buttons under the fader, "LAYER A"
and "LAYER B"). The preset assumes the **program** switches layers: it watches for the LAYER
buttons' messages and changes what every other control means. The status line on the page shows
the current layer.

### HW-1: do the numbers in the preset match the device?

Press or turn each control **once** and read the monitor. For each line compare the number with
the table. The table is what the preset expects; the "Layer A" meaning is what should happen in
the program, so you can also check that the right thing happened.

*Top row of buttons, left to right (layer A):*

| # | Expect | Layer A does | Layer B does | Result |
|---|---|---|---|---|
| 1 | note 89 | Shift (hold) | mute 1 | |
| 2 | note 90 | Tempo −5 | mute 2 | |
| 3 | note 40 | Tempo +5 | mute 3 | |
| 4 | note 41 | Click on/off | mute 4 | |
| 5 | note 42 | Tap tempo | mute 5 | |
| 6 | note 43 | Notes on/off (Listen) | mute 6 | |
| 7 | note 44 | Metronome on/off | mute 7 | |
| 8 | note 45 | Next tab | mute 8 | |

*Bottom row of buttons, left to right:*

| # | Printed on the device | Expect | Layer A does | Layer B does | Result |
|---|---|---|---|---|---|
| 1 | MC | note 87 | Loop worst bar | solo 1 | |
| 2 | (blank) | note 88 | Discard take | solo 2 | |
| 3 | rewind | note 91 | Range back | solo 3 | |
| 4 | fast forward | note 92 | Range forward | solo 4 | |
| 5 | LOOP | note 86 | Loop | solo 5 | |
| 6 | stop | note 93 | Stop | solo 6 | |
| 7 | play | note 94 | Listen | solo 7 | |
| 8 | record | note 95 | Record | solo 8 | |

*Encoders:* turn each one **slowly one click clockwise**, then one click counter-clockwise, then
**push** it.

| Encoder | Turn expect | Push expect | Layer A turn | Layer A push | Layer B turn |
|---|---|---|---|---|---|
| 1 | cc 16 | note 32 | Tempo | 100 % tempo | Kick volume |
| 2 | cc 17 | note 33 | First bar | – | Snare volume |
| 3 | cc 18 | note 34 | Last bar | – | Hi-hat volume |
| 4 | cc 19 | note 35 | Click volume | – | Tom 1 volume |
| 5 | cc 20 | note 36 | Music volume | – | Tom 2 volume |
| 6 | cc 21 | note 37 | Count-in | – | Tom 3/4 volume |
| 7 | cc 22 | note 38 | Bars per row | – | Crash volume |
| 8 | cc 23 | note 39 | Recent scores | Open score | Ride volume |

Check also: **clockwise = value 1** (or small numbers), **counter-clockwise = 65** (or a number
in the 65 to 127 range). If clockwise shows 65 and counter-clockwise shows 1, the encoder uses
the opposite sign: write "signbit2". If clockwise shows 1 and counter-clockwise 127, write
"twos". If it hovers around 64 (65 clockwise, 63 counter-clockwise), write "offset". The program
assumes the Mackie one; this matters because the wrong one makes encoders run backwards.

*Fader:* move it slowly from bottom to top and watch the monitor. Expect `ch 1 · fader N` with
N from 0 to 16383. Write down N at the bottom and at the top (they are normally 0 and 16383, or
close).

*Lights.* The preset sends light messages to the device; the device must obey them.

| Test | How | Expect |
|---|---|---|
| Button light | on layer A press **Click** (top row, button 4) | its light switches on, and off again on the next press |
| Layer light | press **Record** while a score is loaded | the record light is on during the recording; off when it ends |
| Ring | turn encoder 1 (tempo) | the ring around it shows a position that moves with the value |
| Ring range | turn it to its highest and lowest values | the ring fills and empties; no flicker, no dark ring at an extreme |
| Layer B lights | press the layer B button, then **mute 1** | its light follows the mute state of the Kick group |

Write down "OK" or what happened. The most likely problem is a ring that shows the wrong
position (a different value range) or lights that never come on (the device wants another note
number).

### HW-2: does the Mini switch layers by itself?

The Mini remembers its own layer (A or B) and may or may not change the numbers it sends when
you press LAYER A/B. The program changes layers itself; if the device **also** changes its
numbers, the two layers' numbers in the preset would be wrong.

1. Press **LAYER A**, then press the first top-row button. Note the monitor line (expect note 89).
2. Press **LAYER B**, then press the same button. Note the monitor line.
3. Do the same with the first encoder (turn one click) and with the fader.

| Result | Meaning |
|---|---|
| the same numbers in A and in B | as expected; the program does the layer work |
| different numbers in B | the device remaps: write down the numbers of layer B for **every** control. The preset must have different numbers for layer B |
| the monitor shows `layer switch` when you press a LAYER button | correct: the program saw the button |
| nothing at all appears when you press a LAYER button | the device keeps it to itself; write that down: the program cannot see layers then and needs another way to switch |

### HW-4 (Mini): does the drawn picture look like the device?

Click the controller's name in the top bar. The help screen opens with a picture of the Mini.
Hold the real Mini next to the screen, with the LAYER buttons nearest to you.

Check, and write down any difference:

1. The two rows of 8 buttons: the order, and whether the top row of the picture is the top row
   of the device.
2. The printed symbols on the bottom row (MC, a blank one, rewind, fast forward, LOOP, stop,
   play, record) in the right order.
3. The eight encoders in one row above the buttons, and the fader on the right.
4. The two LAYER buttons: position and labels.
5. The picture does not react to presses; it follows the layer the device is on. Compare the
   labels in the picture with what each control did in HW-1: if a control did "Tempo −5" in the
   program but the picture labels the same spot "Tempo +5", the picture's positions are swapped.
   Write down which two.
6. Press LAYER B on the device: the picture's labels change to mute/solo/volume (when the
   controller is not connected the help screen has a layer selector instead).

A photo of the real device next to the help screen helps more than any description.

### HW-9: the controller on the Rudiments tab

1. Click the **Rudiments** tab. Pick any rudiment (for example #1, Single stroke roll).
2. Layer A. Turn encoders and read what happens on the screen:

| Control | Expect |
|---|---|
| encoder 1 | tempo in BPM (the Rudiments tab counts BPM, not per cent) |
| encoder 2 | how often the rudiment repeats |
| encoder 3 | which hand leads: right, left or alternating |
| encoder 8 | the rudiment: turning moves through the list |
| rewind / fast forward (bottom row 3 and 4) | previous / next rudiment |
| push of encoder 1 | back to the rudiment's starting tempo |
| tempo −5 / tempo +5 (top row 2 and 3) | tempo −5 / +5 BPM |

3. Start a recording with the **record** button. While it records, turn encoders 2, 3 and 8:
   nothing should change (the controls are locked while a take records). Stop with **stop**.
4. Press **listen**: the rudiment plays. Press **stop**.

Write down anything that does the wrong thing, runs backwards, or is too coarse or too fine a
step (for example "encoder 2 skips 3 repeats per click").

---

## Session 3: the Manage window and Learn (HW-7)

**Why.** The learn flow was only tested with simulated messages. A real endless encoder, a real
mouse and a real device may behave differently (double messages, repeated notes, encoders that
send in another encoding).

**What you need.** The Mini (or any controller, Mackie or generic). Back up
`%APPDATA%\RollModel\controllers\` first if it holds presets you care about; this session adds
and deletes presets with the names "Test…".

### A. The window

1. Settings → Controller → **Manage**. A window opens with the preset list on the left, grouped
   as "Mackie Control" and "Generic MIDI", and **Add**, **Copy**, **Delete…** under the list.
2. Click different presets in the list. The right side shows that preset's assignments; the
   preset in the selector on the page changes too (clicking a preset chooses it, because learning
   needs its connection).
3. Check the window can be moved, resized and closed without losing the main window's focus, and
   that closing the main window closes it.

### B. Copy, rename, delete

1. Click **Behringer X-Touch Mini** in the list, then **Copy**. A copy named like "Behringer
   X-Touch Mini 2" appears in the list and is chosen. It is a file in the controllers folder.
2. Rename it to **Test mini**: edit the name field on the right and confirm. Check the file in
   the folder is renamed too.
3. Try to rename it to an existing name, and to a name with a slash (`a/b`): expect a message
   and no change.
4. **Delete…** → the button asks again → confirm. The preset disappears, and the selector falls
   back to another preset.
5. Try to rename or delete **Behringer X-Touch Mini** (the built-in one): the buttons are
   disabled or refuse.

### C. Learning a button

1. Copy the Mini again (as in B), so you edit a copy.
2. In the list of assignments find **Stop**. Click **Learn** on that row.
3. Press a **different** button of the Mini (for example the first top-row button). The row now
   shows that button's note number. Press the button's real function in the program; it should
   now do "stop".
4. Click **Clear** on the row: the binding goes. Press the button: the monitor says "not bound".
5. Tick **After learning a control, go on to the next action** (then the next action in the
   list waits for a control by itself). Learn three actions in a row, pressing three buttons.
6. Click **Cancel** while an action waits: it stops waiting.

Write down: did the row take the right number? Did a *button release* (note off or velocity 0)
get learned by mistake? Did one press learn twice?

### D. Learning an endless encoder

1. In the copy, find **Tempo**. The row has a kind selector (absolute, relative mackie, relative
   twos, relative offset, relative signbit2).
2. Choose the kind that matches the encoder (see HW-1: Mackie-style by default for the X-Touch).
3. **Learn**, then turn the encoder one click clockwise. The row shows `cc N` and the kind.
4. Turn the encoder: the tempo goes **up** when you turn clockwise. If it goes down, the kind
   is wrong: choose another and learn again.
5. Learn the same encoder with the wrong kind on purpose (relative offset for a Mackie
   encoder) and describe what the tempo does; it is a quick way to see the failure mode.

If you have a **generic controller** (any of the four), do D with one of its knobs using
**absolute**, and with one of its endless encoders if it has any.

### E. Remember the channel

Tick **Limit a learned control to the MIDI channel it sends on** before learning and learn
a button. The row should show `ch N` next to the number. Press the same button: it works.
This is for controllers that share notes between channels.

### F. After

Delete all the "Test" presets. Choose **Behringer X-Touch Mini** again in the selector.

**Write down** problems, in particular any case where Learn picked up the wrong message, took
two presses, or left the program waiting forever.

---

## Session 4: unplugging and plugging in (HW-8)

The program looks for the controller's port every two seconds.

1. Start with the controller **unplugged** and the Mini preset chosen. The top bar shows the
   controller as missing (red).
2. Plug it in. Within a few seconds the dot turns green and the status says it is connected.
   Press a button: the monitor shows it. Write down how long it took ("under 2 s", "about 5 s").
3. Pull the cable out **while the program runs**. Within a few seconds the dot turns red. The
   program must keep running (no freeze, no crash). Write down what the status line says.
4. Plug it in again. It connects again by itself. Repeat 3 and 4 three times.
5. Pull the cable **while a take is recording** (or Listen is playing). The take must continue
   or stop cleanly; nothing should hang.
6. Pull it out while the **Manage** window is open and learning is waiting. The window should
   leave learning mode or say the controller is gone.
7. Plug the controller into **another USB port**. Windows names ports differently (for example
   "X-TOUCH MINI" vs "2- X-TOUCH MINI"): check the program still finds it, and if it does not,
   write down the port name shown in the Port list.

Write down: connection time, any message, and anything that froze.

---

## Session 5: X-Touch Compact (HW-3, HW-5, HW-4)

**Before.** The Compact is in MC mode (see "Controller modes"; if the strip buttons show
`note 0` to `note 23` you have the right one). Choose the preset **Behringer X-Touch Compact**. The Compact has nine faders (eight
channel strips and the main), eight encoders on the top left, eight rotary encoders on the right
side, three rows of strip buttons (REC, SOLO, MUTE) and a SELECT row, six transport buttons on
the right, and two LAYER buttons.

### HW-3: the preset against the device

Press everything once and compare with the monitor. The preset is based on the Mackie spec
only; none of it was ever tried on the device.

*Faders:* move each of the 9 faders from bottom to top.

| Fader | Expect | In the program |
|---|---|---|
| 1 to 8 | `ch 1 · fader N` … `ch 8 · fader N` (channel = fader number) | volume of Kick, Snare, Hi-hat, Tom 1, Tom 2, Tom 3/4, Crash, Ride in Listen |
| 9 (main) | `ch 9 · fader N` | music volume |

*Strip buttons* (for strip 1 to 8):

| Row | Expect | In the program |
|---|---|---|
| REC (top) | notes 0 to 7 | plays a hit of the group (to hear its level) |
| SOLO | notes 8 to 15 | solo the group (the light follows) |
| MUTE | notes 16 to 23 | mute the group (the light follows) |

*SELECT row* (under the faders), left to right: expect notes 24 to 31: Shift, Click, Notes,
Metronome, Tap tempo, Loop worst bar, Discard take, Next tab.

*Top encoders*, turn one click clockwise and counter-clockwise, then push: expect cc 16 to 23
(turn) and notes 32 to 39 (push). In the program: tempo, first bar, last bar, click volume,
count-in, bars per row, (unused), recent scores. Push of 1 = reset tempo, push of 8 = open the
score.

*Transport buttons on the right:* press each and note it. Expect: rewind note 91 (range back),
fast forward 92 (range forward), stop 93, play 94 (listen), record 95, cycle 86 (loop). Write
down the order in which they are printed on the device from left to right, top to bottom (this
is also what HW-4 needs).

*Lights:* the same checks as in HW-1: a button light that follows Click, record, mute and solo;
a ring around encoder 1 that follows the tempo.

*Faders and motors.* The program never moves a fader. Load a score, change the tempo with the
mouse, then touch fader 9 (music volume): the fader does **not** jump to the program's value;
it takes over only when you move it past the value shown (the "pickup"). Check this feels
natural: write down "OK" or "the value jumps when I touch the fader".

### HW-5: the controls the preset leaves unbound

The Compact has controls the preset does not use. Find what they send:

1. The **eight rotary encoders on the right** (a column or a block next to the transport).
   Turn each: note `cc` numbers and the values (Appendix B tells you how to read them), and push
   each: note the notes.
2. The two **LAYER A / LAYER B** buttons: press each; write the messages. If a LAYER button
   changes what **all other** controls send afterwards, write down the new numbers for at least
   one fader, one encoder and one button.
3. The **button under the ninth fader** (the main fader's button).
4. Any other control not listed above (display buttons, a jog wheel if there is one).

For each, write: name or position on the device, message, value range. These become new
actions.

### HW-4 (Compact): does the picture look like the device?

Click the controller name in the top bar and compare the picture with the Compact:

1. The order of the **six transport buttons** on the right (the picture's order against the
   device's order).
2. The **SELECT** row and the strip numbers (1 to 8 left to right, with the main fader on the
   right).
3. The eight encoders on the top, the eight encoders on the side, the LAYER buttons.
4. Compare each spot's label in the picture with what the control did in Session 5's checks.

Photos help again.

---

## Session 6: generic MIDI controllers (HW-6)

The four generic presets were written from memory and from a web search. Only these numbers
were confirmed: Novation Launch Control XL knob 1 = cc 13; Arturia BeatStep knobs 1 and 2 =
cc 10 and cc 74; Korg nanoKONTROL2 record = cc 64. Everything else is a guess, and the Akai MPK
Mini binds **two** ranges of knobs because sources disagree.

For each controller you own, do the following (15 minutes):

1. Put it in the right mode (see "Controller modes").
2. Settings → Controller → choose its preset. If the Port list does not show the device, pick
   it by hand and note the exact name (the preset matches part of the name; if it does not
   match, tell me the full name).
3. Press / turn / move **every control** one at a time and read the monitor. For each, the
   monitor line says either `→ action` (the number was right) or `(not bound)` (the preset has
   a different number). Use the table in the preset file's comment as a checklist:
   open the preset in the Manage window and look at the list of assignments.
4. Write down every `(not bound)` line with the control's name on the device.
5. For knobs and faders: check that the value runs 0 to 127 over the whole travel (some
   controllers only reach 126) and that it does not jump (see "pickup" in HW-3).
6. For endless encoders: write the kind (Appendix B).
7. For pads: pads send notes; check that velocity 0 or note-off does not trigger the action twice.

### Controller-specific notes

**Novation Launch Control XL.** Factory template selected (the template buttons on the right
of the device). Check: top row of knobs = cc 13 to 20; faders = cc 77 to 84; the track
focus/control buttons (notes 41 to 44, 57 to 60 and 73 to 76, 89 to 92); the eight buttons on
the right (cc 104 to 111). Which template is lit (1 to 8) and whether the numbers change
between templates is worth a note.

**Korg nanoKONTROL2.** CC mode (hold SET MARKER and CYCLE while plugging in). Check: faders
cc 0 to 7, knobs cc 16 to 23, S buttons cc 32 to 39, M cc 48 to 55, R cc 64 to 71, the
transport buttons, track and marker buttons. The buttons send value 127 on press and 0 on
release: confirm.

**Akai MPK Mini.** Program 1 selected. Which set of knob numbers moves (cc 17 to 24 or cc 70 to
77)? **Write down which; the other can be deleted from the preset.** Pad notes: bank A, pads 1
to 8; write the notes. Does PROG CHANGE or the pad banks change what pads send?

**Arturia BeatStep** (the first one, not the Pro). Control mode (the BeatStep's mode, not the
software's). Check the 16 knobs (cc numbers), the big knob (cc 7), the 16 pads (notes 44 to 59;
which pad numbers are in the top row and which in the bottom row), and the other buttons.

**Write down** (Appendix A, HW-6) the corrected number for every control.

---

## Session 7: rudiments on the V31 (HW-10)

**Why.** Two things can only be judged by ear and by hand: fast rudiments on a mesh pad
(double triggers) and the grace notes of flams and drags (spacing and loudness).

**What you need.** The V31 connected, headphones, a quiet room.

### A. Double triggers

1. Settings → MIDI input: connect the V31. Hit a pad: the monitor on that page shows the hit and
   the instrument.
2. Rudiments tab, rudiment **#1 Single stroke roll**. Set a slow tempo (60 BPM), record a
   take. Look at the Results: expect no extra hits.
3. Raise the tempo to 120 BPM, 160 BPM, 200 BPM. Each time play as cleanly as you can for 8
   bars. Write down the tempo where **Extra** hits start to appear (the results page lists
   "extra" hits).
4. Do the same for **#16 Single paradiddle** and **#33 Double drag tap**. Note the first tempo
   with extras.
5. If extras appear at tempos you do not think are too fast, the pad's **retrigger** setting
   (in the V31's trigger settings) or the sensitivity
   is the cause; try one setting higher, repeat, and write down the setting and the result. This is a V31 setting, not a
   program setting; the program's adaptive window only decides how a hit is *matched*.

### B. The ear test of the rudiments

Use Listen (the **listen** button or `L`):

1. #1 single stroke roll: even, no accents. Expect even spacing.
2. #20 to #24 (flam, flam accent, flam tap, flamacue, flam paradiddle): the grace note before the main note should be audible but quieter, about
   35 ms before. Does it sound like a flam? Too early (you hear two notes)? Too late (it
   sounds like a single thick note)? Write "too early / right / too late".
3. Drags (#31 to #33, #36 and #37): two grace notes before the main note.
4. Rolls: the closing notes of rolls 7 to 15 are marked ⚑ in `docs/rudiments.md` because they
   were read from a small engraving. Listen to the roll's ending and compare with the written
   chart (PAS 1984, link in `docs/rudiments.md`). Write down which rolls end wrongly.

### C. Controller on the rudiments

This is HW-9 (Session 2); repeat it here if you are using the V31 and the controller together.
Check the mapping stays correct when the V31 is also connected and receives hits.

**Write down**: first tempo with extras for each rudiment, the V31 setting, the flam/drag
timing opinion, rolls that end wrongly.

---

## What to do with a number that is wrong

You never need to edit program code. Presets are plain text files.

1. Settings → Controller → **Manage**. Click the preset, press **Copy**. The copy is a file in
   `%APPDATA%\RollModel\controllers\` and is chosen automatically.
2. Fix the control either **in the window** (the control's row: **Learn**, then press the
   control on the device) or by hand: open the `.txt` file in any text editor. A line looks like
   `bind = A note 89 -> shift` (layer A, note 89 does Shift). Change the number.
   For a fader the line is `bind = bend 1 -> gain 1` (pitch bend of channel 1); the number
   after `bend` is the **channel**.
3. Save, go back to the program. The program reads the file when you choose the preset again;
   press **⟳ Refresh and reconnect** if it does not seem to change.
4. When your copy works, send me the file (the whole `.txt`). The built-in preset is corrected
   from it, and your copy can be deleted.

Do not edit the built-in presets in the program's folder; they are inside the program.

## Appendix A: results sheet

Copy this into a text file, fill in what you tried, and give it back with any photos.

```text
Date:
Program version (shown on the splash):
Computer / Windows version:
Devices: V31 (firmware?), X-Touch Mini (firmware?), X-Touch Compact, others:

HW-11 start-up times (ms per line; 3 runs with devices, 1 without)
  run 1:
  run 2:
  run 3:
  no devices:
  largest step and which one:
  window judders every 2 s while running?  yes / no

HW-1 Mini numbers (write only what differs from the table in the plan)
  top row:
  bottom row:
  encoder turn / push:
  encoder kind (mackie / twos / offset / signbit2):
  fader range (bottom / top):
  button lights:           OK / problem:
  rings:                   OK / problem:
  layer B lights:          OK / problem:

HW-2 Mini layers
  same numbers in A and B?  yes / no  (if no: list layer B numbers)
  LAYER buttons show "layer switch"?  yes / no

HW-4 pictures
  Mini differences (with photo):
  Compact differences (with photo):

HW-9 Rudiments tab with the Mini
  anything wrong:

HW-7 Manage window and Learn
  copy / rename / delete:
  learn a button:
  learn an encoder (kind that worked):
  channel limit:
  other problems:

HW-8 plug / unplug
  connection time:
  unplug while recording:
  other port name:

HW-3 Compact numbers (differences only)
  faders:
  strip buttons REC / SOLO / MUTE:
  SELECT row:
  encoders turn / push:
  transport buttons (order and numbers):
  lights and rings:
  fader pickup feels:   OK / problem:

HW-5 Compact extras
  side encoders (cc numbers, values):
  LAYER A / B buttons:
  button under fader 9:
  anything else:

HW-6 generic controllers (one block per device)
  device and mode:
  port name shown:
  (not bound) controls and their real numbers:
  encoders kind:
  notes:

HW-10 V31 and rudiments
  first tempo with extra hits: #1:    #16:    #33:
  V31 retrigger setting and effect:
  flams (too early / right / too late):
  drags:
  rolls that end wrongly:
```

## Appendix B: reading the monitor

A monitor line has three parts:

```text
ch 1 · note 95 on, velocity 127  →  record
^^^^   ^^^^^^^^^^^^^^^^^^^^^^^^     ^^^^^^^^
channel   what the device sent       what the preset does with it
```

| Line | Meaning |
|---|---|
| `ch 1 · note 95 on, velocity 127` | a button pressed (note on). Buttons send `on` with velocity 127 when pressed and again with velocity 0 (or `off`) when released; the program acts on the press only |
| `ch 1 · note 95 off` | button released |
| `ch 1 · cc 16 = 1` | a knob or encoder sent controller number 16 with value 1 |
| `ch 1 · fader 8192` | a fader (pitch bend); the value runs 0 to 16383, the number after `ch` is the fader's channel |
| `→ record` | bound: the action it triggers |
| `(not bound)` | the device sent it, the preset has no line for it |
| `→ layer switch` | it is a LAYER button of the preset |

**Reading an endless encoder.** Turn it one click clockwise, then one click counter-clockwise,
and compare the two values with this table:

| Clockwise | Counter-clockwise | The kind is |
|---|---|---|
| 1 | 65 | `mackie` (the X-Touch family) |
| 1 | 127 | `twos` |
| 65 | 63 | `offset` |
| 65 | 1 | `signbit2` |
| 0 to 127 stepping by many | | an **absolute** knob (a position), not an endless encoder |

A fast turn sends bigger numbers (for example 3 or 5 instead of 1); the table is for single
clicks. Write the numbers of the slowest click you can make.

**Buttons that send a cc.** Some controllers (the Korg nanoKONTROL2) send buttons as
`cc N = 127` and `cc N = 0`. The preset treats a cc with value greater than 0 as a press.

**Not on the list?** If a control shows nothing at all, it is not sending MIDI to this port
(some controllers have a second port for such controls, or the control only changes the
device's own state, like a LAYER button on the Mini). Write "silent".
