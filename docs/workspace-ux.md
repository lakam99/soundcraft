# SoundCraft workspace design

The Modern workspace supports recording audio and composing with instruments from
the same arrangement. It is the default; Window → Workspace → Classic retains the
specialist interface and its independent panel arrangement.

## Design principles

- **Recognition:** visible Audio track, Instrument track and Import actions give
  an empty project clear starting points. Timeline tools and transport use original
  icons with named hover help, shortcut hints and accessible labels.
- **Hierarchy:** transport stays at the bottom. Arrangement tools stay at the top;
  less frequent operations move into More tools and the existing menus.
- **Consistency:** Inspector, Browser, Mix and MIDI editor toggle in place. F2–F5
  operate those views in Modern; focused text inputs keep their keys. Classic keeps
  its established shortcuts.
- **Context:** selecting a track shows its audio input or instrument, channel
  controls, output and effects in the Inspector.
- **Progressive disclosure:** channel details and specialist operations remain
  available without occupying the first row of everyday controls.
- **Feedback and recovery:** controls expose accessible names and selected state;
  the transport shows audio connection state and a direct setup action. Document
  edits retain the engine's Undo behavior. Effects append to empty slots.
- **Honesty:** Project media lists imported project content. The plugin browser
  lists built-in and cached hosted plugins without scanning on every frame. The
  MIDI view edits notes; this build does not support live MIDI input.

## Visual system

Modern Dark uses graphite surfaces, teal focus/selection and muted track-color
accents. Modern Light uses pale surfaces and the same hierarchy. Matte controls,
quiet borders, larger lane controls and named mixer cards replace the hardware-style
control banks. Colored clips keep the visual focus. Advanced lane controls live in
the track's ellipsis menu; routing and automation live in mixer Details. Classic
retains its original controls and styling. Small text and selected/semantic labels
are checked for a minimum 4.5:1 contrast in both Modern palettes.

## Built-in effects

All 27 built-in effects have an individual identity, purpose, settings illustration
and curated main controls. EQ adds a computed frequency-response plot. Room and
plate, echo timing, modulation, harmonic texture and utility views use distinct
geometric motifs. Settings illustrations describe controls, not measured audio.
The parameter cards retain the original ranges, units, logarithmic tapers and
choice labels, with exact numeric entry and individual resets. EQ bands and the
Channel Strip modules have labeled sections. Long editors scroll within the window.
Bypass, factory and user presets, undo and command automation use the same engine
paths. Hosted plugin editors and the three instrument editors keep their existing
behavior. Classic retains its original parameter table.

Modern continuous parameters use rotary controls: drag vertically, hold Shift for
fine adjustment, use arrows when focused, or Ctrl/Command + wheel. Plain scrolling
scrolls the editor. Double-click the dial or use Reset to restore the default;
double-click its numeric readout for exact entry. The Effect skin selector offers
Graphite, Brushed alloy, Porcelain and Ember, plus category-specific automatic
materials. The choice is remembered per effect in editor preferences, independently
of projects and audio presets. Decorations show settings, not live audio meters.

## Live MIDI keyboard

Add an Instrument track, choose a sound, then select the keyboard and input channel
in the Inspector. New instrument tracks monitor the selected track automatically.
Monitor auditions while stopped; armed tracks receive input regardless of selection.
Arm the track and press Record to create a growing MIDI take. Stop closes held notes
and sustain, then Play auditions the recorded part. A take, its checkpoints and mixer
changes made during it form one Undo step. MIDI-only recording never opens an audio
input; arming audio tracks alongside instruments also records their audio inputs.

Device and channel routing are saved in the project. A missing saved keyboard stays
selected and is identified as disconnected. Reconnect refreshes devices and retries
busy ports; Silence held notes sends immediate cleanup. Device loss or input overflow
stops the take and retains captured notes. Recorded velocities, release velocities,
channels, sustain, pressure, pitch bend and other channel controls are retained.
Each track plays one instrument part: live notes/controllers and timeline playback
use channel 1 at the instrument, while input-channel filtering occurs before routing.

MIDI recording supports ordinary takes and selection punch with pre/post-roll.
Loop recording and loop playback must be disabled before a MIDI take. Existing MIDI
on an armed track is muted while recording, so checkpointed notes do not play twice.
Stop recording before deleting clips/tracks, changing instruments/routing, seeking
or changing tempo. Notes are checkpointed in the project for normal autosave recovery.
Native input uses the OS MIDI backend; browser builds retain MIDI editing/playback.

## Workspace memory

Modern and Classic store separate layouts and visibility. On upgrading old
preferences, the original arrangement is retained as Classic while other editor
preferences remain in place. Switching workspaces cancels an active panel drag.
Projects and audio/MIDI data are unchanged by a workspace switch.

## Workflows

For audio, add a track, choose its input in the Inspector, map hardware channels
when needed, arm the track, then Record. The adjacent audio-track format menu
offers Mono and Stereo; advanced track creation remains available.

For composition, Write MIDI creates an instrument and an editable part. The
Instrument track action adds a track without a part. The instrument browser
distinguishes New track from Replace selected. The Inspector offers a MIDI-part
action and the existing instrument editor.

The mixer opens below the arrangement. Modern strips put the channel name first,
then pan, monitor/arm, mute/solo, meters and faders. Details exposes routing,
automation and the other channel sections. MIDI and Mix
can share the lower tab group. Layouts remain movable and resizable.

## Acceptance checks

`modern_workspace` exercises the default workspace at small and high-DPI sizes,
audio and instrument creation, MIDI-part editing, effect insertion without
replacement, focused-input shortcuts and rendered Classic layout recovery.
`mix_panel` covers lower-panel placement, fader/mute operation, close/reopen,
MIDI tab switching, saved placement and legacy Mix-window automation.
Existing Classic workflow fixtures continue to exercise specialist behavior.
`effects_makeover` covers all 27 effect windows, short high-DPI displays, parameter
editing/reset, presets/bypass, discrete undo steps and scrolling to the last EQ band.

Navigation uses common DAW conventions. The view-key reference is PreSonus's
[public Studio One 4 key-command specification](https://pae-web.presonusmusic.com/downloads/products/pdf/presonus-studio_one_4-key_commands.pdf).
SoundCraft's implementation and interface artwork are its own.
