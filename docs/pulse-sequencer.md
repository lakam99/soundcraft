# PULSE step sequencer

PULSE is a continuous, instrument-connected step rack. Open it from the workspace's **PULSE** key
or **Track → Add PULSE Sequencer**. Select a PULSE part to reopen its controls. New parts preserve
existing clips and instruments. Without a selected MIDI/instrument track, PULSE creates FORGE.

## Play and keep a pattern

1. Choose **MELODY**, **ARP**, **CHORD** or **DRUM** on the MODE dial.
2. Turn each step's PITCH knob, and press its ON/REST key. The numbered key selects that step's
   velocity, gate, chance, ratchets, tie and accent controls below.
3. Press **RUN** to loop the pattern through the track's instrument. Other clips are temporarily
   silenced in the audition; the document is unchanged. Cyan lamps follow the clock. Amber keys
   show enabled steps, and a brass outline marks the selected edit step.
4. Editing stops the audition safely. Press RUN again to hear the change. **STOP** releases notes.
5. **STORE** writes the candidate as one undoable edit. **REVERT** restores the stored pattern.
   Closing the panel stops playback and retains the candidate for this editor session. Store before
   closing the project to preserve it across restarts.

Drag knobs; hold Shift for finer movement. Arrow keys adjust a focused knob. Numeric readouts
also accept exact typed values. The chassis and knob caps reuse FORGE's original worn materials.

## Banks, clock and voices

There are up to eight pattern banks, eight lanes per bank and 64 steps per lane. Sixteen physical
step controls are visible at once. The **EDIT PAGE** keys change the visible controls, never the
pattern's length or playback. **LENGTH** sets the actual clock length. Steps beyond the length
are retained when shortened, so extending a pattern brings them back.

**PLAY BANK** selects the edited bank and clears the chain. **CHAIN +** appends that bank to the
play order (up to 32 entries). **CLEAR CHAIN** returns to the selected playback bank. Bank buttons
select the bank being edited; the playback-bank/chain display separately shows what will play.

Clock divisions include quarter through sixty-fourth notes and triplets. SWING delays odd clock
steps by 0–75% of an interval. Each lane has forward, reverse, pendulum, seeded random or walk
order, mute/solo, root and transposition. All lanes share the part's instrument.
If any lane uses pendulum direction, that bank completes the full outward and return trip
(2 × LENGTH − 2 clock intervals, or one for a single step) before advancing. Other lanes wrap
their own steps during that shared bank clock.

CHANCE applies once to a step, including all its ratchets. RATCHETS divide that step's swung
interval into repeated attacks. ACCENT adds 20 velocity units, capped at 127. A TIE continues
the previous attack without retriggering; rests and bank boundaries break ties. A tie at the
start is silent. Ties require one ratchet and 100% chance; enabling Tie sets those controls.

The variation seed and PASSES generate repeatable probability, random direction and arp choices.
After PASSES × CHAIN, the same generated phrase loops. Print produces exactly that MIDI phrase;
random choices are not regenerated differently during printing.

## Chord-memory arpeggios

ARP reads the visible chord memory, selected order and octave span. ROOT is an absolute MIDI
note; chord values and step PITCH are semitone offsets from it. MAJOR, MINOR and SUS load quick
chord shapes and engage ARP. The memory's interval controls allow other voicings. CHORD plays
the complete memory at every enabled step. Ratchets repeat the chosen arp pitch.

To capture a keyboard chord, stop PULSE, hold 1–8 MIDI notes and press **LATCH KEYS**. The notes become the
lane's root and chord memory and stay after release. This is explicit chord capture; continuous
held-key following and external hardware clock/CV output are not implemented.

## Drum loops

Use the built-in **Drum Synth**, a drum sampler or another dedicated drum instrument. Drum mode uses a
fixed DRUM KEY; stored/exported drum notes use MIDI channel 10. The current instrument host plays
one timbre per track, so choose a dedicated kit. Drum mode does not turn a melodic patch into a drum kit. Common keys
are kick 36, snare 38, closed hat 42, open hat 46 and clap 39. Add a VOICE for each drum, set its
key and enable its rhythm. When the first lane is a drum, added lanes use useful kit notes.
The assistant can create a dedicated drum part with `pulse.new {kind:"drum"}`; explicitly name
an existing target track to reuse it. It never replaces an existing instrument implicitly.

## FORGE, AI and MIDI

Ask the assistant to design a FORGE sound, build a PULSE melody or arpeggio, and audition it.
FORGE's sound candidate and PULSE's pattern candidate combine during audition. Both remain
separate from the saved document until accepted and stored. The assistant can set a complete
program or edit up to 64 steps in one tool call.

**PRINT MIDI** copies the accepted notes immediately after the source part on the same track.
The source program remains editable. The assistant can instead print to an explicit sampler or
instrument track and position. Store both candidates before deriving a sampler with FORGE, then
print to the new sampler track. Avoid doubling the same phrase by muting the source when placing
the printed copy at the same time.

Saved programs contain ordinary MIDI notes plus their editable PULSE controls. If those notes are
edited separately in the piano roll, PULSE refuses to overwrite them. Press **EDIT MIDI**, or
right-click the part and choose **Edit PULSE as MIDI**, to detach its controls and
keep the independent MIDI, or create a separate PULSE part. Tempo changes reflow the source
part's sample extent in the same undo transaction. The program's step clock remains in MIDI ticks.
If slowing the clock would extend a PULSE part over existing music, the whole tempo change is
rejected with an explanation; move the following parts to leave space first. Creating/printing
also rejects occupied explicit destinations. A new part without an explicit position uses the
track end when the insertion is occupied.

AI/control command discovery: `sequencer.catalog`, `sequencer.inspect`, `pulse.*`.
Inspect returns the current `base_revision`; include it with preview/bulk edits. Independently
changed source parts invalidate candidates, so stale AI requests cannot overwrite newer work.
