---
title: The MIDI clock works on a real device
date: 2026-11-04
summary: The MIDI clock of 0.10.5 is checked on a device and works in Listen and on the Metronome tab; recording is still to come.
issues: 107, 109, 110
---
The MIDI clock that came in 0.10.5 has now been tried on a real device, and the whole feature works as intended. When I published 0.10.5 it had only been tested against a fake output, so this is the check that note was waiting for.

## What was checked

The clock from **Listen** and from the **Metronome** tab, with the choices you can make under **Settings → Sound**: where *Start* goes out, the optional position for a later bar, and looping. The device follows the tempo and starts and stops with the music.

If you want to use it with your own gear, the steps are in the [manual](../../manual/#sound) under *MIDI clock*. Any device that follows an external MIDI clock should work the same way, but I have only tried the one.

## What is still missing

**Recording** does not send the clock yet (#110). Takes run on an older part of the program that is being replaced, and I would rather add the clock once, in the new part, than twice. It is on the [roadmap](../roadmap/) under *Later*.

The "not tried on a device" warning has been removed from the manual. The 0.10.5 entry above still says what was true on the day it was written.
