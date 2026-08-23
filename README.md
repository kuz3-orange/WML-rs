# Worst Minecraft Launcher

A Minecraft launcher, written in Rust. Named honestly.

This is an early-stage scaffold: most of the launcher backend is still
`todo!()` stubs (auth, downloads, launching). The window shell (menu bar,
toolbar, instance tree, properties panel, console, status bar) is built and
running on [`egui`](https://github.com/emilk/egui) via `eframe`, styled after
a classic Eclipse/SWT-style desktop tool — it's a shell over static sample
data, not wired to the backend yet.

One backend piece that *is* real: mod installation (`src/mods.rs`,
`src/modrinth.rs`, `src/curseforge.rs`) resolves a mod and its required
dependencies from [Modrinth](https://modrinth.com) and/or
[CurseForge](https://www.curseforge.com), then downloads them into an
instance's `mods/` directory, verifying file hashes where the provider
supplies one. Modrinth's API is public and needs no key. **CurseForge
requires your own API key** from https://console.curseforge.com — set
`WML_CURSEFORGE_API_KEY` in your environment, or CurseForge-backed installs
fail with a clear `MissingApiKey` error.

## Debug vs. release builds

`cargo build` (debug) keeps a console window with timestamped, chatty logs
(`RUST_LOG` still overrides the default level). `cargo build --release`
suppresses the console window on Windows and only logs warnings/errors — the
standard `windows_subsystem` toggle in `src/main.rs`.

## Design

Menu mockups live here: https://claude.ai/code/artifact/22464eda-7738-4f79-a145-469316cceb7b

The "Eclipse IDE" direction (menu bar + toolbar + instance tree + console)
was picked as the visual direction and is what `src/ui.rs` implements.

## Branch workflow

- **`alpha`** — active development. All work lands here first.
- **`beta`** — synced from `alpha` once changes look stable; this is where
  fixes get shaken out.
- **`release`** — promoted from `beta` after roughly a week of testing on
  `beta` with no new issues. This is the stable branch.

## Building

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```
