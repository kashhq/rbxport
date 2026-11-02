<!-- FLX fork document. Source: Mixxx Pioneer-DDJ-FLX4 mapping (GPL-2.0-or-later), extracted 2026-11-02. Evidence level: [OBS mapping] unless marked otherwise; nothing here is from a capture or the physical controller yet. -->

# Pioneer DDJ-FLX4 MIDI Reference

Derived from Mixxx's community mapping (`Pioneer-DDJ-FLX4.midi.xml`, 281 inputs / 112 outputs, and
`Pioneer-DDJ-FLX4-script.js`, based on the DDJ-400 mapping). Everything here is what the mapping
*uses*; controls the mapping ignores are listed as unknown, and inferences are marked **(inferred)**.
All values hex. "Ch" = 1-based MIDI channel; status low nibble = Ch-1.

## 1. Channel layout

| Ch | Note status | CC status | Carries |
|----|-------------|-----------|---------|
| 1  | 0x90 | 0xB0 | Deck 1: transport, jog, tempo, loop, sync, shift, pad-mode buttons, channel CUE (PFL); mixer strip 1 (trim, EQ, channel fader); deck 1 VU (out) |
| 2  | 0x91 | 0xB1 | Deck 2: same layout as Ch 1, identical data1 values |
| 5  | 0x94 | 0xB4 | Beat FX (select, left/right, on/off, level/depth, CH SELECT = CH1) |
| 6  | 0x95 | — | Beat FX when CH SELECT = CH2 (CH2 position note, ON/OFF) |
| 7  | 0x96 | 0xB6 | Global: browse encoder + push, LOAD 1/2, crossfader, headphone MIX, filter knobs (both channels) |
| 8  | 0x97 | — | Deck 1 performance pads, unshifted |
| 9  | 0x98 | — | Deck 1 performance pads, SHIFT held |
| 10 | 0x99 | — | Deck 2 performance pads, unshifted |
| 11 | 0x9A | — | Deck 2 performance pads, SHIFT held |
| 16 | 0x9F | — | Output only: jog-ring "track loaded" animation (note 0x00 deck 1, 0x01 deck 2) |

**SHIFT convention.** There are two SHIFT buttons (0x90/0x91 note 0x3F, 0x7F down / 0x00 up). The
controller applies shift *itself*:
- Deck/mixer buttons: shifted press arrives on the **same channel with a different note number**
  (e.g. PLAY 0x0B → SHIFT+PLAY 0x0E). The unshifted note is not sent.
- Pads: shifted press arrives on the **next channel** with the **same note** (0x97→0x98, 0x99→0x9A).
- Pad-mode buttons: shifted press selects the secondary mode and sends its own note (see §4).
- Knobs/faders: same CC regardless of shift, except browse (0x40→0x64) and jog (see §2.2).
  Beat FX LEVEL/DEPTH sends the same CC; the host must track SHIFT state itself.
- Which deck's SHIFT is held matters only for that deck's controls; the host sees the press on Ch 1 or 2.

Note-on value convention for all buttons/pads: 0x7F press, 0x00 release (note-on with velocity 0,
not note-off status 0x8n — Mixxx never maps 0x8n).

## 2. Inputs

Kind legend: **note** = button (0x7F/0x00); **CC7** = 7-bit absolute; **CC14** = 14-bit, MSB then
LSB, value = (MSB<<7)|LSB, 0x0000–0x3FFF; **rel** = relative, 0x40 = no motion.

### 2.1 Transport (per deck: status 0x90 deck 1, 0x91 deck 2)

| Control | data1 | Kind | Notes |
|---|---|---|---|
| SHIFT | 0x3F | note | Held state; host should track per deck |
| PLAY/PAUSE | 0x0B | note | |
| SHIFT+PLAY/PAUSE | 0x0E | note | Mixxx: reverse-roll (censor) while held |
| CUE | 0x0C | note | Hold semantics: send on press and release |
| SHIFT+CUE | 0x48 | note | Mixxx: jump to track start |
| BEAT SYNC (short press) | 0x58 | note | Press and release both sent |
| BEAT SYNC (long press) | 0x5C | note | **Controller detects the long press itself** and sends a separate note |
| SHIFT+BEAT SYNC | 0x60 | note | Mixxx: cycle tempo range 6/10/16/25 % |
| Channel CUE (PFL) | 0x54 | note | Lives on the deck channel, not Ch 7 |
| SHIFT+Channel CUE | 0x68 | note | Mixxx: toggle quantize |

### 2.2 Jog wheel (per deck: CC 0xB0/0xB1, note 0x90/0x91)

| Control | Status | data1 | Kind | Notes |
|---|---|---|---|---|
| Platter top rotate, vinyl mode ON | 0xBn | 0x22 | rel | Scratch when touched |
| Platter top rotate, vinyl mode OFF | 0xBn | 0x23 | rel | Pitch bend |
| Jog side (outer ring) rotate | 0xBn | 0x21 | rel | Pitch bend |
| SHIFT + platter rotate | 0xBn | 0x29 | rel | Fast search |
| Platter touch | 0x9n | 0x36 | note | 0x7F touch, 0x00 release |
| SHIFT + platter touch | 0x9n | 0x67 | note | Shifted touch note |

Relative encoding: delta = value − 0x40 (below 0x40 = counter-clockwise/rewind, above = forward).
Each message carries a small signed tick count, not a position.
- Resolution: the script calls `scratchEnable(deck, 720, 33⅓, …)`, i.e. it assumes **720 ticks per
  revolution at 33⅓ rpm**. This is the mapping's assumption, not a measured value.
- Vinyl mode is a **controller-side setting**: the same physical platter emits 0x22 or 0x23 depending
  on it. Mixxx treats both the same way and decides scratch vs bend by touch state (touch → scratch
  ticks; no touch → bend at delta × 0.8). SHIFT search uses delta × 150.
- Mixxx's shift-touch note 0x67 runs the same touch handler, so scratch also engages when shifted.
- No SHIFT variant of the side ring (0x21) is mapped. Whether the hardware sends one is unknown.

### 2.3 Tempo (per deck)

| Control | Status | data1 | Kind | Notes |
|---|---|---|---|---|
| TEMPO slider MSB | 0xBn | 0x00 | CC14 | Host latches MSB |
| TEMPO slider LSB | 0xBn | 0x20 | CC14 | Host computes full value on LSB |

Centre = 0x2000. Mixxx: rate = 1 − value/0x2000, so value 0x0000 = +range and 0x3FFF ≈ −range. The
slider is inverted relative to the raw value. No hardware centre-detent message is mapped.
Soft takeover is enabled, so the hardware position is not echoed.

### 2.4 Loop and cue/loop call (per deck: status 0x90/0x91)

| Control | data1 | Shift data1 | Notes |
|---|---|---|---|
| LOOP IN / 4 BEAT | 0x10 | 0x4C | Shift: Mixxx toggles loop-in adjust via jog |
| LOOP OUT | 0x11 | 0x4E | Shift: loop-out adjust via jog |
| RELOOP/EXIT | 0x4D | 0x50 | Shift: Mixxx reloop-and-stop |
| CUE/LOOP CALL ◀ | 0x51 | 0x3E | Mixxx: halve loop / shift: beatjump −32 |
| CUE/LOOP CALL ▶ | 0x53 | 0x3D | Mixxx: double loop / shift: beatjump +32 |

No long-press variant of LOOP IN ("4 BEAT") is mapped. The script header says it was "hacky" and left
unimplemented. It is unknown whether the controller emits a distinct long-press note as it does for SYNC.

### 2.5 Mixer, per channel (deck 1 on 0xB0/0x90, deck 2 on 0xB1/0x91, filter on Ch 7)

| Control | Status | MSB | LSB | Kind |
|---|---|---|---|---|
| TRIM | 0xBn | 0x04 | 0x24 | CC14 |
| EQ HI | 0xBn | 0x07 | 0x27 | CC14 |
| EQ MID | 0xBn | 0x0B | 0x2B | CC14 |
| EQ LOW | 0xBn | 0x0F | 0x2F | CC14 |
| Channel fader | 0xBn | 0x13 | 0x33 | CC14 |
| FILTER CH1 | 0xB6 | 0x17 | 0x37 | CC14 |
| FILTER CH2 | 0xB6 | 0x18 | 0x38 | CC14 |
| Channel CUE (PFL) | 0x9n | 0x54 | — | note (see §2.1) |

General rule: LSB CC = MSB CC + 0x20 (standard MIDI 14-bit pairing). Centre-detent knobs (EQ, trim,
filter) centre at 0x2000.

### 2.6 Master / headphones / crossfader (Ch 7)

| Control | Status | MSB | LSB | Kind |
|---|---|---|---|---|
| Crossfader | 0xB6 | 0x1F | 0x3F | CC14 |
| Headphones MIX (cue/master blend) | 0xB6 | 0x0C | 0x2C | CC14 |

MASTER LEVEL and HEADPHONES LEVEL are not mapped. They are presumably analog-only on the hardware's
own audio interface (inferred).

### 2.7 Browse (Ch 7)

| Control | Status | data1 | Kind | Notes |
|---|---|---|---|---|
| BROWSE rotate | 0xB6 | 0x40 | rel | Mixxx "SelectKnob": two's-complement, 0x01 = +1, 0x7F = −1 |
| SHIFT + BROWSE rotate | 0xB6 | 0x64 | rel | Script treats 0x7F as one direction, anything else as the other |
| BROWSE press | 0x96 | 0x41 | note | |
| SHIFT + BROWSE press | 0x96 | 0x42 | note | |
| LOAD deck 1 | 0x96 | 0x46 | note | |
| LOAD deck 2 | 0x96 | 0x47 | note | |

Note the browse encoder uses a **different relative encoding** (two's complement around 0x00) from the
jog (offset around 0x40).

### 2.8 Beat FX (Ch 5 / Ch 6)

| Control | Status | data1 | Kind | Notes |
|---|---|---|---|---|
| BEAT FX SELECT (▼) | 0x94 | 0x63 | note | |
| SHIFT + BEAT FX SELECT | 0x94 | 0x64 | note | |
| BEAT ◀ | 0x94 | 0x4A | note | |
| BEAT ▶ | 0x94 | 0x4B | note | |
| CH SELECT = CH1 | 0x94 | 0x10 | note | Switch position: 0x7F when entering, 0x00 when leaving |
| CH SELECT = CH2 | 0x95 | 0x11 | note | Same, on Ch 6 |
| BEAT FX ON/OFF | 0x94 / 0x95 | 0x47 | note | Ch 5 when CH SELECT is CH1 or the third position; Ch 6 when CH2 |
| SHIFT + ON/OFF | 0x94 / 0x95 | 0x43 | note | Same channel rule |
| LEVEL/DEPTH | 0xB4 | 0x02 | CC7 (used) | Mapping uses MSB only as 7-bit. LSB probably 0xB4 0x22 (inferred, unmapped) |

The CH SELECT switch has a third position. The XML descriptions call it "1+2", while Pioneer's panel
may label it MASTER; this is ambiguous. No message for it is mapped. In Mixxx, moving off CH1 or CH2
sends 0x00 on that note, which disables that channel's FX routing.

### 2.9 Smart CFX / Smart Fader

Not mapped. The script header lists both as "not implemented yet". Their messages are unknown from this mapping.

## 3. LED outputs

All LEDs in the mapping are **on/off only: 0x7F = on, 0x00 = off**. No velocity-based colour
encoding is used anywhere; the pads are effectively single-colour from this mapping's view.
The LED address is the same status/data1 as the button's input message.

### 3.1 Deck state LEDs (0x90 deck 1, 0x91 deck 2)

| LED | data1 | Lit when |
|---|---|---|
| PLAY/PAUSE | 0x0B | Playing (or blinking per host's play-indicator) |
| PLAY/PAUSE, shift layer | 0x47 | Same as above, so the LED holds while SHIFT is down |
| CUE | 0x0C | Cue indicator |
| CUE, shift layer | 0x48 per script / 0x47 per XML | **Conflict, see §5** |
| BEAT SYNC | 0x58 | Sync enabled |
| Channel CUE (PFL) | 0x54 | Headphone cue on |
| LOOP IN | 0x10 | Loop buttons: 0x7F at init; blink 500 ms while a loop is active |
| LOOP OUT | 0x11 | As LOOP IN |
| LOOP IN shift layer | 0x4C | Mirrors 0x10 |
| LOOP OUT shift layer | 0x4E | Mirrors 0x11 |
| RELOOP/EXIT | 0x4D | Loop active |
| RELOOP/EXIT shift layer | 0x50 | Mirrors 0x4D |
| Pad-mode buttons | 0x1B 0x69 0x1E 0x6B 0x20 0x6D 0x22 0x6F | Host sends 0x7F on the pressed mode's note (see §4) |
| Master tempo / key lock | — | Not mapped; no message known |

During loop-adjust the script blinks only the adjusted side and holds the other side at 0x00.

### 3.2 VU meters

| Meter | Status | data1 | Value |
|---|---|---|---|
| Channel 1 level | 0xB0 | 0x02 | 0x00–0x7F, host-computed (level × 127) |
| Channel 2 level | 0xB1 | 0x02 | Same |

The controller does not meter itself. The host must stream levels; the mapping sends on every change.

### 3.3 Jog ring

| Effect | Status | data1 | Value |
|---|---|---|---|
| "Track loaded" animation, deck 1 | 0x9F | 0x00 | 0x7F trigger, 0x00 when unloaded |
| "Track loaded" animation, deck 2 | 0x9F | 0x01 | Same |

Sent once at init for both decks and on every load. No jog position or needle LEDs are driven.

### 3.4 Beat FX

| LED | Status | data1 | Lit when |
|---|---|---|---|
| BEAT FX ON/OFF | 0x94 | 0x47 | Focused FX slot enabled |
| BEAT FX ON/OFF, shift layer | 0x94 | 0x43 | Mirrors 0x47 |

The script only ever lights on Ch 5 (0x94), even when CH SELECT is CH2 (inputs then arrive on 0x95).
Whether the 0x95 address also drives the LED is unknown.

### 3.5 Pad LEDs (0x97/0x99 unshifted layer, 0x98/0x9A shift layer)

Each pad LED is addressed by the **mode's note number**, so the host must write the LED for the
mode being shown. Both layers are written so the LEDs stay correct while SHIFT is held.

| Mode | Notes | Lit when |
|---|---|---|
| Hot cue | 0x00–0x07 | Hot cue N set |
| Beat loop | 0x60–0x67 | That loop size active |
| Beat jump | 0x26, 0x27 (shift layer only) | Track loaded. These are the shifted pads 7/8 that resize jumps |
| Sampler | 0x30–0x37 | Sample loaded. Blinks 250 ms while playing, back to on when stopped |
| Keyboard (repurposed for stems) | 0x40–0x43 | Stem N exists and is not muted |
| Keyboard (repurposed for stems) | 0x44–0x47 | Stem N's quick-effect is enabled |
| Key shift | 0x70–0x77 | 8-bit bitmap of the semitone offset (see §4) |
| Pad FX1 / FX2 | unknown | Not mapped |

At shutdown the script writes 0x00 to 0x00–0x07 and 0x30–0x37 on all four pad channels.

## 4. Pad modes

There are four mode buttons per deck. Each button selects a primary mode, and SHIFT+button selects a
secondary mode. **The controller switches modes internally.** The mode button sends its own note, and
the pads then send that mode's note range. The host does not need to track mode to decode pads.

| Button | Primary mode (note) | SHIFT → secondary mode (note) |
|---|---|---|
| HOT CUE | Hot cue (0x1B) | Keyboard (0x69) |
| PAD FX1 | Pad FX 1 (0x1E) | Pad FX 2 (0x6B) |
| BEAT JUMP | Beat jump (0x20) | Beat loop (0x6D) |
| SAMPLER | Sampler (0x22) | Key shift (0x6F) |

Mode buttons are on 0x90 (deck 1) and 0x91 (deck 2). The shifted note is not a fixed offset: it is
+0x4E for HOT CUE and +0x4D for the others.

Pad note ranges, pad 1..8 = base+0..base+7. Physical order is presumably top row 1–4, bottom row 5–8 (inferred). Unshifted
on 0x97 (deck 1) / 0x99 (deck 2), shifted on 0x98 / 0x9A.

| Mode | Base | Unshifted meaning (Mixxx) | Shifted meaning (Mixxx) |
|---|---|---|---|
| Hot cue | 0x00 | Set/trigger hot cue 1–8 | Delete hot cue |
| Pad FX 1 | 0x10? | **Not mapped**. Base inferred from the 0x10-spaced pattern | — |
| Beat jump | 0x20 | Jump −1, +1, −2, +2, −4, +4, −8, +8 beats | Pad 7 (0x26): sizes ÷16; pad 8 (0x27): sizes ×16; others unmapped |
| Sampler | 0x30 | Play sample, or load if empty | Stop if playing, else eject |
| Keyboard | 0x40 | Mixxx repurposes: pads 1–4 toggle stem mute, 5–8 toggle stem FX | Pads 1–4 solo stem; 5–8 next FX preset |
| Pad FX 2 | 0x50? | **Not mapped**. Base inferred | — |
| Beat loop | 0x60 | Loop ¼, ½, 1, 2, 4, 8, 16, 32 beats | Not mapped as input (LEDs written) |
| Key shift | 0x70 | Set key: 0, +1, +2, +3, −3, −2, −1, 0 semitones | Relative: +1, +2, +3, +4, −4, −3, −2, −1 |

- **Beat jump sizes** scale together. Mixxx clamps pad 2 between 1/16 and 16 beats (sizes ×16 or ÷16
  per press). This is host logic, not hardware.
- **Sampler** pads map to host slots: left deck pads 1–4 → slots 1–4, pads 5–8 → slots 9–12; right
  deck pads 1–4 → slots 5–8, pads 5–8 → slots 13–16.
- **Key shift LED bitmap**, bit 7 = pad 1 … bit 0 = pad 8:

| Offset | Bits | Offset | Bits |
|---|---|---|---|
| 0 | 1000 0001 | | |
| +1 | 0100 0000 | −1 | 0000 0010 |
| +2 | 0010 0000 | −2 | 0000 0100 |
| +3 | 0001 0000 | −3 | 0000 1000 |
| +4 | 1001 0000 | −4 | 0000 1001 |
| +5 | 0101 0000 | −5 | 0000 1010 |
| +6 | 0011 0000 | −6 | 0000 1100 |
| +7 | 1011 0000 | −7 | 0000 1101 |
| +8 | 0111 0000 | −8 | 0000 1110 |
| > +8 | 1111 0000 | < −8 | 0000 1111 |

  The bitmap is reset on track load.
- **"Beat jump 2"** does not exist in this mapping. Only the eight modes above are addressed.

## 5. Gotchas

1. **Keep-alive SysEx is mandatory.** The host sends this every **200 ms** and once at startup:
   `F0 00 40 05 00 00 04 05 00 50 02 F7`
   The script says it was reverse-engineered with Wireshark. The startup send "seems to double as" a
   query: the controller answers by **dumping current positions of all knobs and faders**. Without
   the keep-alive, the controller presumably drops back to standalone/demo behaviour (inferred, not stated).
2. **No init SysEx beyond the keep-alive.** Startup only sends LED writes: VU off, jog animation on
   both decks, loop LEDs on, and pad-mode state.
3. **Pad-mode LEDs are host-echoed.** On every mode-button press the script sends 0x7F back on that
   button's note. It never sends 0x00 to the previous mode, so the controller evidently handles
   exclusivity itself. Whether the echo is required at all is unverified.
4. **Shift-layer LEDs must be mirrored.** Play (0x0B/0x47), loop (0x10/0x4C, 0x11/0x4E), reloop
   (0x4D/0x50), beat FX (0x47/0x43), and every pad channel pair (0x97/0x98, 0x99/0x9A). If the
   shifted address is left dark, the LED goes out whenever SHIFT is held.
5. **Shift is hardware-applied but host-tracked.** Buttons and pads arrive already shifted. Beat FX
   LEVEL/DEPTH does not change CC, so the host tracks SHIFT (either deck's SHIFT counts) to switch
   between level and depth.
6. **14-bit ordering.** MSB arrives first. Mixxx latches it and computes on LSB for tempo. A host
   that acts on MSB alone gets 7-bit steps; one that ignores MSB arrival order can glitch.
7. **Sync has two notes.** Short-press 0x58 and long-press 0x5C are separate notes, timed by the controller.
8. **Two jog rotate CCs for one platter.** 0x22 vs 0x23 reflects the controller's vinyl-mode setting,
   not touch. The touch note (0x36) is the only reliable "hand on platter" signal.
9. **Different relative encodings.** Jog is offset-binary (0x40 centre). Browse is two's complement (0x01/0x7F).
10. **Mapping bugs worth knowing.** These are not hardware facts.
    - XML maps the shifted CUE LED to **0x47**, the shifted PLAY LED address. The script's light table
      says **0x48**, which matches the SHIFT+CUE input note. 0x48 is almost certainly correct.
    - The script's deck 2 VU light entry uses status **0xB0**, so init and shutdown clear deck 1's
      meter twice and never deck 2's. The live update path correctly uses 0xB1.
    - The loop-adjust guard `engine.getValue(group, "loop_enabled" === 0)` is a precedence bug. It
      never blocks, so SHIFT+LOOP IN/OUT toggles adjust mode even with no loop.
    - Sampler LEDs blink by writing to status+1 for the shift layer. That works only because each
      deck's shift channel is unshifted+1.
11. **Not implemented in the mapping:** native Keyboard mode (repurposed for stems), Pad FX 1/2,
    LOOP IN long-press 4-beat loop, CUE/LOOP CALL memory/delete, Smart CFX, Smart Fader, master tempo
    LED, third position of the FX CH SELECT switch, Beat FX LEVEL LSB, any pad colour encoding.
    Messages for these must come from Pioneer's official MIDI message list or a capture.
