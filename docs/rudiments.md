# Rudiment catalogue for the Rudiments tab

The catalogue behind the **Rudiments** tab: what it offers and how each rudiment is built from strokes. `src/rudiments.rs` holds the same data as patterns, and its tests check them against the sticking, accents, grace notes and lengths listed here. Rows marked ⚑ have note values that still have to be compared with the PAS chart.

## Source and how it was checked

- The list is the **PAS International Drum Rudiments** (Percussive Arts Society, 1984): 40 rudiments in four families. It contains all of the **NARD Standard 26 American Drum Rudiments**, which the PAS chart marks with an asterisk; they are flagged ★ below.
- Names, families and the ★ flags were confirmed against the PAS page, the NARD list and the Wikipedia article (links at the end). Sticking and rhythm were read off the notation of the PAS chart itself (the PDF distributed by Colorado Mesa University), page by page, not from memory. Where the chart was too small to be sure of a note value the row is marked ⚑ and has to be compared with the chart again when it is coded.
- The chart is © PAS. This file records patterns (which hand plays which stroke, accents, ornaments, note values), not the engraving.

## Notation used here

| Symbol | Meaning |
|---|---|
| `R` `L` | A main stroke with that hand |
| `>` | Accent |
| `lR` | **Flam**: a soft grace note with the left hand just before a right-hand main stroke (`rL` the other way round) |
| `llR` | **Drag** (ruff): two soft grace notes before the main stroke |
| `RR` | A **diddle** (double stroke): two equal strokes with one hand |
| `·` | Separates the groups of one cell (the cell is repeated; the left lead is the mirror image and is not written out) |

Every cell is written for the **right-hand lead**; the **left-hand lead** is the same pattern with the hands swapped. For most rudiments the chart prints both leads one after the other; the Rudiments tab will offer *right*, *left* and *alternate* (right lead, then left lead, and so on). The number of repetitions multiplies the cell.

## Timing grid

The Roll Model staff has proportional spacing and no beams, so the timing is kept on a grid it can draw legibly and the tempo is chosen by the user:

- Rolls are written by the PAS in 32nd notes. In the app they are placed on a **16th-note grid**; the speed is set by the tempo, so a 16th at 120 BPM is the stroke rate of a 32nd at 60 BPM. Roll diddles therefore sit on the grid and the closing accent(s) land on the grid position after the last diddle.
- Everything else keeps the chart's note values (16ths, triplets, eighths, quarters). "Beats" is the length of one cell in quarter notes.

## Roll rudiments (15)

| # | Rudiment | Sticking (right lead) | Timing | Notes |
|---|---|---|---|---|
| 1 | Single stroke roll ★ | R L R L R L R L | 16ths, 2 beats | |
| 2 | Single stroke four | R L R L · R L R L | per beat: three 16th-triplet strokes + an eighth, 2 beats | |
| 3 | Single stroke seven | R L R L R L R | sextuplet (6 strokes) + a quarter, 2 beats | |
| 4 | Multiple bounce roll | buzz | one held note | A closed (press) roll: cannot be graded stroke by stroke, see open questions |
| 5 | Triple stroke roll | RRR LLL RRR LLL | 16th triplets, 12 strokes, 2 beats | |
| 6 | Double stroke open roll ★ | RR LL RR LL | 16ths, 2 beats | |
| 7 | Five stroke roll ★ | RR LL R | 2 diddles + accent ⚑ | |
| 8 | Six stroke roll | R> LL RR L> | accent, 2 diddles, accent ⚑ | |
| 9 | Seven stroke roll ★ | RR LL RR L> | 3 diddles + accent ⚑ | |
| 10 | Nine stroke roll ★ | RR LL RR LL R> | 4 diddles + accent ⚑ | |
| 11 | Ten stroke roll ★ | RR LL RR LL R> L> | 4 diddles + two accents ⚑ | |
| 12 | Eleven stroke roll ★ | RR LL RR LL RR L> | 5 diddles + accent ⚑ | |
| 13 | Thirteen stroke roll ★ | RR LL RR LL RR LL R> | 6 diddles + accent ⚑ | |
| 14 | Fifteen stroke roll ★ | RR LL RR LL RR LL RR L> | 7 diddles + accent ⚑ | |
| 15 | Seventeen stroke roll | RR LL RR LL RR LL RR LL R> | 8 diddles + accent ⚑ | |

The closing accent of a roll with an odd number of strokes is on the right hand for 5, 9, 13 and 17 and on the left for 7, 11 and 15 (right lead).

## Diddle rudiments (4)

| # | Rudiment | Sticking (right lead) | Timing | Notes |
|---|---|---|---|---|
| 16 | Single paradiddle ★ | R> L R R · L> R L L | 16ths, 2 beats | Accent on the first stroke of each group |
| 17 | Double paradiddle ★ | R> L R L R R · L> R L R L L | 16ths, 12 strokes, 3 beats | Accents at beat 1 and on the "and" of 2 |
| 18 | Triple paradiddle | R> L R L R L R R · L> R L R L R L L | 16ths, 16 strokes, 4 beats | |
| 19 | Single paradiddle-diddle | R> L R R L L · R> L R R L L | 16ths, 12 strokes, 3 beats | The chart also prints the mirrored left-lead row |

## Flam rudiments (11)

Accents are on the main stroke of the flams unless said otherwise.

| # | Rudiment | Sticking (right lead) | Timing | Notes |
|---|---|---|---|---|
| 20 | Flam ★ | lR> · rL> | two quarters, 2 beats | |
| 21 | Flam accent ★ | lR> L R · rL> R L | eighths in 6/8, 3 beats | |
| 22 | Flam tap ★ | lR> R · rL> L · lR> R · rL> L | 16ths, 2 beats | |
| 23 | Flamacue ★ | lR L> R L · lR | four 16ths + a quarter, 2 beats | The accent is the **second** stroke; the closing flam is on the next beat |
| 24 | Flam paradiddle ★ | lR> L R R · rL> R L L | 16ths, 2 beats | |
| 25 | Single flammed mill | lR> R L R · rL> L R L | 16ths, 2 beats | |
| 26 | Flam paradiddle-diddle ★ | lR> L R R L L · rL> R L L R R | 16ths, 3 beats | |
| 27 | Pataflafla | lR> L R rL> · lR> L R rL> | 16ths, 2 beats | Accents on the first and last stroke of each group |
| 28 | Swiss army triplet | lR> R L · lR> R L | 16th triplets, 1 beat | Mirror: rL> L R · rL> L R |
| 29 | Inverted flam tap | lR> L rL> R · lR> L rL> R | 16ths, 2 beats | |
| 30 | Flam drag | lR> L L R · rL> R R L | eighth, 16th, 16th, eighth per group, 3 beats ⚑ | |

## Drag rudiments (10)

| # | Rudiment | Sticking (right lead) | Timing | Notes |
|---|---|---|---|---|
| 31 | Drag ★ | llR · rrL | two quarters, 2 beats | |
| 32 | Single drag tap ★ | llR L> · rrL R> | eighths, 2 beats | The accent is on the tap |
| 33 | Double drag tap ★ | llR llR L> · rrL rrL R> | eighths in 6/8, 3 beats | |
| 34 | Lesson 25 ★ | llR L R> · llR L R> | per beat: 16th, 16th, eighth, 2 beats | Mirror: rrL R L> · rrL R L> |
| 35 | Single dragadiddle | RR L R R · LL R L L | 16ths, 2 beats | A paradiddle whose first stroke is a diddle (two strokes in one 16th); accent on the first |
| 36 | Drag paradiddle #1 ★ | R> llR L R R · L> rrL R L L | eighth, then four 16ths, 3 beats ⚑ | The paradiddle's first stroke carries the drag |
| 37 | Drag paradiddle #2 ★ | R> llR llR L R R · L> rrL rrL R L L | eighth, eighth, four 16ths, 4 beats ⚑ | |
| 38 | Single ratamacue ★ | llR L R L> · rrL R L R> | 16th triplet + an eighth, 2 beats | The accent is the last stroke |
| 39 | Double ratamacue ★ | llR llR L R L> · rrL rrL R L R> | eighth + 16th triplet + eighth, 3 beats | |
| 40 | Triple ratamacue ★ | llR llR llR L R L> · rrL rrL rrL R L R> | two eighths + 16th triplet + eighth, 4 beats | |

## Counts

| Family | Rudiments | ★ (NARD 26) |
|---|---|---|
| Roll | 15 | 9 |
| Diddle | 4 | 2 |
| Flam | 11 | 6 |
| Drag | 10 | 9 |
| **Total** | **40** | **26** |

20 of the 40 carry grace notes: all 11 flam rudiments and 9 of the 10 drag rudiments (the single dragadiddle has none).

## What the grader does with them

- A flam's and a drag's grace notes are the existing optional grace notes: not required, never counted as extra. They are expected 35 ms before the main note, a drag's earlier one 70 ms. Requiring them, with a check of the spacing, is not done.
- The hit window shrinks to 45 % of the distance to the neighbouring strokes (not below the "perfect" tolerance), because the usual ±100 ms would reach several strokes of a fast rudiment.
- Accents are graded by the existing dynamics score.
- Sticking cannot be heard by a drum module, so it is shown, not graded. It is used to report how even the two hands are: the expected stroke is known for every hit, so the Results tab averages timing and (unaccented) velocity per hand and the feedback names a hand that is late or softer.
- The multiple bounce (buzz) roll is one held note and can only be listened to: Record is switched off for it.
- A clean take is a whole take scoring 85 or better; its tempo is kept as the rudiment's best clean tempo.

## Links

- [PAS International Drum Rudiments](https://www.pas.org/resources/rudiments) (page) and the chart as a [PDF](https://www.coloradomesa.edu/marching-band/documents/rudiments.pdf)
- [NARD, Standard 26 American Drum Rudiments](https://en.wikipedia.org/wiki/National_Association_of_Rudimental_Drummers)
- [Drum rudiment (Wikipedia)](https://en.wikipedia.org/wiki/Drum_rudiment)
