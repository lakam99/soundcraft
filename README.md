# SoundCraft Fork

Independent fork of [SoundCraft](https://github.com/storytold/soundcraft), an open-source Rust digital audio workstation. Original copyrights and the MIT OR Apache-2.0 licenses are retained. Restricted ArtCraft brand artwork is excluded from this fork.

This fork joins the FilmCraft and EffectCraft ecosystem:

- Named mono and stereo hardware inputs, explicit left/right channel mapping, and direct input setup from track input menus.
- All device channels are available, up to 256; unavailable channels never wrap onto a different microphone.
- A docked AI Chat panel powered by the [shared Craft AI Chat plugin](https://github.com/lakam99/craft-ai-chat), with Markdown, readable tool activity, session context, normal editor command access and secure OS credential storage.
- Install and update from the [Craft Fork Launcher](https://github.com/lakam99/craft-launcher/releases/latest). SoundCraft itself also checks this fork's releases.

Download the Windows x64 portable ZIP from [the latest release](https://github.com/lakam99/soundcraft/releases/latest), extract it and run `soundcraft.exe`. Keep `SoundCraftData` when upgrading. Windows packages are unsigned; no macOS or Linux package is claimed for this fork release.

See [the fork guide](docs/fork-guide.md) for input setup, session compatibility, AI privacy and validation limits.

## Build and test

Install Rust 1.95 or newer and the platform prerequisites described in [CONTRIBUTING.md](CONTRIBUTING.md).

```sh
git clone https://github.com/lakam99/soundcraft
cd soundcraft
cargo run --release -p soundcraft -- --demo
cargo xtask ci
```

The existing native and web editors remain supported. AI transport and credential storage are native-desktop features. See [architecture](docs/architecture.md), [agent control](docs/agents.md), [testing](docs/testing.md), [ROADMAP.md](ROADMAP.md), and [ATTRIBUTION.md](ATTRIBUTION.md) for the upstream foundation and licensed assets.
