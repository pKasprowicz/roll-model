# Concept: support for generic MIDI controllers (beyond Mackie Control)

Status: proposal; phases 1 (preset format and Mapper) 2 (learn UI: assignments table, Learn, new and copied presets, saved at once) and 3 (generated help picture) implemented 2026-10-29; layers and the extras are not. Relative encodings are named `mackie`, `twos`, `offset` and `signbit2`.

## Where we are

`controller.rs` is a pure state machine driven by text presets (`presets/*.txt`): `bind` lines map a note, cc or pitch bend to an `Action`; `led` and `ring` lines say what to send back. Presets already have layers, absolute and relative cc, and a fader pickup. The Settings, Controller page chooses a *preset* (a named device), and the help screen draws a *picture* of that device (`controller_layout.rs`).

What is Mackie-specific today:

| Area | Mackie assumption | Generic controllers differ |
|---|---|---|
| Relative encoders | 1..63 clockwise, 65..127 counter-clockwise (`decode_relative`) | two's complement, offset 64, sign-and-magnitude, or absolute-only knobs |
| Rings | cc 48+n with `0x20 \| position` | none, or a cc/note with a colour or value scale |
| Lights | note on, 127 or 0 | velocity or value selects the colour (pad grids), some need a channel or SysEx to enter a mode |
| Channels | notes and cc on channel 1 are enough | pad banks and knob banks use different channels; the same cc on two channels is two controls |
| Layers | a note selects one | "templates" or banks selected on the device, invisible to the program |
| Picture | a hand-made `Layout` per device | unknown device, no picture |

## Idea in one sentence

Keep presets and the Mapper as they are, add a second way to *make* a preset: choose "Generic MIDI (learn)" and teach the program the controls by pressing them, with a small set of per-control options for the encodings above. A learned setup is saved as an ordinary preset file, so everything else (Mapper, lights, tests, user-written presets) keeps working.

## UI changes

1. **Controller type** on the Controller page: the existing list gains an entry "Generic MIDI (learn)" first. Built-in and user presets stay below it. Choosing a named preset behaves exactly as now.
2. **Assignments table** (see the mockup in the conversation): one row per program action in a fixed order (transport, tempo, measures, volumes, mixer strips). Each row shows the learned control (`note 41`, `cc 13 ch 1`), its kind, an edit button, and a *Learn* button.
3. **Learn flow:** click *Learn* on a row (or *Learn next* to walk down the list), the row says "Press a control…", the next message from the chosen port is assigned. A control already used elsewhere asks to move or swap. Escape cancels. The existing monitor shows what arrived.
4. **Per-control details** (side panel, only the fields that make sense for the kind):
   - behaves as: button, absolute knob or fader, relative knob;
   - relative encoding: two's complement, offset 64, signed bit 2 (the opposite sign bit to Mackie), Mackie; the program can *guess* it from the first few turns (a clockwise turn is the first message after a stop) and shows the guess;
   - MIDI channel: any, or one;
   - range, learned from the lowest and highest value seen (for fader pickup and scaling);
   - light: none, note on with velocity, cc value. Optional "on" and "off" values (colour pads).
5. **Layers** are optional: "Add a layer" creates a second set of assignments and asks which control switches layers (learned like any other). Device-side banks need no support: the assignments are whatever arrives.
6. **Save as preset…** writes the learned setup to the `controllers` folder (the page already has the folder and the copy button), so it shows in the list and can be shared or hand-edited.
7. **Help screen** (click on the controller indicator): for a learned controller there is no hand-drawn picture, so draw a *generated* one from the preset: knobs and faders as dials and sliders, buttons as keys, in a grid ordered by message number and grouped by kind, each labelled with its action. Optional later step: let the user drag the generated controls into the real arrangement and save the positions with the preset.
8. **Indicator in the top bar** is unchanged; the splash and settings page just show the preset name.

## Preset format changes (small, backwards compatible)

New optional words on existing lines; absent means today's behaviour.

```
bind  = note 41 -> record                 # as now
bind  = cc 13 ch 1 abs -> tempo           # channel filter (new), absolute
bind  = cc 14 rel twos -> range_from      # rel encodings: mackie (default), twos, offset, signbit2
led   = record -> note 41 on=127 off=0    # light values (new)
ring  = tempo -> cc 13 scale=127          # ring/value feedback scale (new)
init  = sysex F0 00 20 29 ... F7          # sent after connecting (new, optional)
```

Code touchpoints: `Src` gains an optional channel; `decode_relative` takes the encoding; `Light` gains on/off values; `midi::connect_controller` already opens the output for lights and can send `init`. The Mackie presets need no change.

## Phasing

1. **Format and Mapper** (pure, unit-tested): channel filter, relative encodings, light values. No UI.
2. **Learn UI:** assignments table, learn flow, save as preset. Reuses the monitor and `Preset` writer (a new `Preset::to_text`, tested by round trip).
3. **Generated help picture** from a preset.
4. **Extras:** encoding auto-guess, layer editor, draggable layout, SysEx init, a few shipped presets for common controllers once someone has tried them on a device.

## Risks and open questions

- Nothing here was tried on a real non-Mackie controller; phase 1 should be validated with one (a pad or knob controller) before the UI is built.
- Windows MIDI ports are exclusive: a DAW using the controller blocks it (already noted for Mackie).
- Learn can only see what the device sends: a button that sends nothing in the current mode cannot be assigned, so the page needs a hint about device modes.
- How many actions the learn list should offer by default (all of them, or a core set with "More…")?
- Should learned setups be per device name, so the right one loads when the device is plugged in? Proposed: yes, stored as the preset's `port` line, as now.

## Built-in presets for generic controllers (addition)

The preset list on the Controller page is grouped: *Mackie Control*, *Generic MIDI* (built in), *My presets*. A built-in preset is read-only; selecting one shows *Copy to my presets*, *Show controls* (generated picture) and *View file*. The copy lands in the `controllers` folder (the page already has the copy button and the never-overwrite naming), opens in the assignments editor, and can be re-learned, cleared, extended or reverted to the built-in original.

Candidates for the first built-in generic presets: Novation Launch Control XL, Korg nanoKONTROL2, Akai MPK Mini (pads and knobs), Arturia BeatStep. They would be written from the devices' published MIDI tables, shipped as `presets/*.txt` and marked "not yet tried on the device" until someone does (the same rule as the Mackie presets). Which devices are really worth shipping depends on what the user owns; the list is a proposal.
