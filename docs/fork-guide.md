# SoundCraft fork guide

## Recording inputs

1. Open Setup → Playback Engine. Select the audio system and the intended input/output devices, sample rate and buffer; apply the changes.
2. Open Setup → I/O. The Audio inputs section shows the selected input device and its reported channel count. Map all device inputs creates a mono path per hardware channel plus adjacent stereo pairs. This replaces the path list; custom names should be set afterwards.
3. Rename paths to meaningful names such as Vocal, Guitar or Synth. A mono path selects one channel. A stereo path has independent L and R selectors, so nonadjacent hardware ports work too. Unassigned ports and disabled inputs stay disconnected.
4. Choose a matching mono/stereo track and open its input selector. Select the named path. Audio input setup… takes you directly back to setup. Arm only the intended tracks; enable monitoring when appropriate for your speakers/headphones.

The device picker reports the driver's maximum supported channels; actual channels depend on the opened stream's rate and format. Recording checks the actual stream and refuses any unavailable mapping before creating take files or starting transport. Monitoring leaves missing channels silent. An explicitly selected missing device or driver never silently records the default microphone. Physical port labels and supported rates come from the operating system driver, so channel numbers follow that driver's ordering.

Input paths are session data and participate in save/reopen and undo. Device names, audio-system selection and buffers stay in this machine's editor preferences. Moving a session to a different interface may require mapping its named paths again. The same resolver controls recording and monitoring.

### Session compatibility

This fork writes session format 2. It reads version 0/1 sessions and explicitly migrates their old implicit first-channel inputs. In format 2, No input really disconnects a track. Older upstream readers reject version 2 instead of silently misreading named paths and recording a different source. Keep an original copy if you need to return to upstream. Existing explicit `In N` / `In N-M` labels are supported, with strict bounds and no channel wrapping.

`setup.inputs` is the undoable engine API for list/create/update/delete/defaults. `channels` uses **zero-based** physical ports (`null` means disconnected), unlike the one-based labels in the UI. `track.input` accepts a configured input name, canonical legacy hardware label, existing bus or `none`. Creating a new bus through this command on an audio track requires explicit `create_bus: true`; mistyped audio input names no longer silently create buses. Non-recording tracks retain their existing automatic bus workflow.

## AI Chat

Ask “How do I…?” for numbered highlights on the relevant editor controls. Input mapping,
audio-device settings, menus, panels, mixer strips and transport controls expose discoverable targets. The numbers match
the instructions in the chat, and the outlined controls remain clickable. Highlighting
does not perform the edit. Use **Clear highlights** or **Esc** to dismiss the guide; a new
question, **Stop**, or opening another session clears it.

Open Window → AI Chat. It is a dockable panel in Edit and Mix workspaces. Use its connection button to enter an OpenAI API key. Preferences → AI Chat stores the model, context attachment and secure-key preference; changes on that page apply immediately. The key uses the OS credential store for this preference profile, with no plaintext fallback. Disabling secure remembering deletes the profile's stored key. Chat, draft text and keys are not saved in session files or UI preferences. Portable settings do not make an OS-vault credential portable to another machine.

The shared plugin renders Markdown and presents tool activity as readable status cards. The assistant can discover and invoke SoundCraft's existing editor commands and UI tools, including the input mapping API. Session edits use normal undo. Session replacement cancels stale work. Automatic context includes session metadata and input/device configuration, including paths and track names where the editor reports them. Sending a prompt sends this selected context to OpenAI. The assistant does not receive live audio and must not claim to hear or measure it. AI controls and credentials remain user-owned.

## Distribution and validation

The launcher and SoundCraft update checks use `lakam99/soundcraft`, never upstream's release feed. The Windows portable package includes `portable.txt`; preserve `SoundCraftData` for preferences, recovery and presets. Shared-plugin and dependency license notices accompany the binary. No FFmpeg or ffprobe executable is shipped. `packaging/fork-windows.ps1` packages verified release binaries and the pinned shared plugin license notices.

Automated regressions exercise independent channels, explicit stereo ordering, more than eight ports, unavailable inputs, migration, save/reopen, undo, mocked AI tool calls and credential privacy. UI images and native device enumeration are checked locally. These checks do not certify a physical multi-input recording setup, cross-OS recording, or ASIO support. Native audio backends remain those enabled by SoundCraft's CPAL configuration.
