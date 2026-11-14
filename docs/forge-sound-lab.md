# FORGE Hybrid sound lab

FORGE is SoundCraft's polyphonic modular instrument. Add it from **Track > Add FORGE Instrument**, or choose **FORGE Hybrid** in the Instruments browser. The original worn metal, brass, wood and bakelite skin is image-backed; every knob, readout and routing control is functional.

Start with the playable **Copper Dawn** patch. Its four macros control filter brightness, modulation depth, sub-oscillator body and drive. The instrument is a continuous hardware-style rack: every module has its own knobs and patch sockets, with no tabs or selected-module pages. Drag knobs vertically, hold Shift for fine movement, use arrows when focused, or edit a numeric readout directly. Wave/mode selectors have detents. Double-click or Home resets a knob. Ordinary scrolling does not change it.

Press **ASSIST**, then describe a sound in the existing docked AI chat using your saved provider/model preferences. The assistant can discover the module catalog, inspect your patch and build a validated candidate. It cannot hear your speakers: offline render measurements and your feedback guide refinement. Opening chat does not send a message or replace an unsent draft.

**AUDITION** renders a finite note on a cancellable worker and plays it at a separate preview gain, initially -6 dB. Note, velocity, gate, tail and audition level have physical dials. **STOP** cancels rendering and stops the preview. Audition/Stop stay reachable while the rack scrolls. The recessed CRT-style display shows the last offline preview's measured waveform, with automatic vertical scaling and Peak/RMS in dBFS; it is not a live oscilloscope. Stale traces use the amber warning color; inspection reports staleness explicitly.

Candidate edits are separate from the saved document. While its FORGE editor is open, your monitored MIDI keyboard can play the candidate. **STORE** commits the whole design as one undoable edit; **REVERT** restores the committed sound. Store or revert before recording. If the instrument changed since an AI candidate was built, committing is refused until it is rebuilt against the current state. One FORGE editor owns live preview at a time; drafts for other tracks remain available during the session. Uncommitted drafts are temporary; store a patch to save it with the session.

Click **AUDIO OUT**, then **AUDIO IN** or an oscillator's **FM IN**, to connect a lead. Click **CV OUT** on a module, macro, velocity or key source, then a parameter's **CV** socket, to add modulation. Each lead uses an original rubber texture wrapped around a sagging curved mesh, with a brass connector and molded strain relief on occupied sockets. A lead in hand remains visible; Escape or clicking elsewhere cancels it. The patch bay's level/depth dials adjust each lead, and **UNPLUG** removes exactly that connection. Hover a cable to highlight its endpoints; right-click it to unplug. Expansion uses a detented module selector and **INSERT MODULE** key. Graphs are limited to 16 modules, 48 audio connections and 48 modulations. Missing modules, duplicate connections, invalid values and feedback cycles are rejected before changing the candidate. Unplugged audio paths are valid and silent.

Sources include bandlimited basic oscillators, procedural wavetable morphing, additive partials, white/pink noise and a loaded sample. Filters, drive, tuned resonators, amplifiers, ADSR envelopes and LFOs form the rest of the graph. This is a bounded acyclic instrument, rather than an unrestricted plugin/code execution environment.

**PRINT TO SAMPLER** renders the committed patch at the session rate into a new instrument track, rooted at the selected MIDI note with looping off. It copies the original MIDI input route, keeps the source synth and patch, and creates one undoable derived track. **PRINT TO AUDIO** similarly places a rendered clip at the session start. Both retain the gate/velocity/tail settings. Automation can choose a separate preview/export sample rate. Renders are limited to two million stereo frames; lower duration or rate if necessary.

**Load source sample** attaches audio to FORGE without replacing its graph; add a Sample module and connect it. Session saves include the patch and source/derived audio. **Save patch** writes a portable synthesized JSON preset; patches using external sample modules travel with their session instead. **Export WAV** writes a completed matching preview. These exports protect existing files; choose a new name.

Automation discovers `synth.catalog`, `synth.patch`, `synth.sample_load` and `soundlab.*` through `engine.commands`. `soundlab.inspect` returns the revision required by `soundlab.preview`. Long renders return immediately; poll inspection for completion and real measurements. `soundlab.to_sampler` and `soundlab.to_audio` require a committed source.

Drag a cable body in a clear area to pull its drape aside. Both plugs stay seated and the sound,
patch and Undo history remain unchanged. Knob and socket hit areas retain priority where they
overlap a lead. Focused cables support arrow movement and Home to restore their default drape.
The `soundlab.cables` command offers the same arrangement control to automation. Drape positions
are temporary editor layout, rather than synthesis parameters.

Audio device preferences are machine-local. **Apply** saves requested system, output/input, rate and buffer immediately before replacing the active configuration. A failed preferences write leaves the working choice intact. Missing outputs may use the system fallback, but the saved request stays visible with a warning so reconnect can retry it. Named recording inputs remain strict. Opening a project does not replace these preferences.
