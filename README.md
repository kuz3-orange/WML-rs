# Worst Minecraft Launcher

A Minecraft launcher, written in Rust. Named honestly.

This is an early-stage scaffold: the module layout for the launcher's backend
exists, but nothing is wired up yet (no auth, no downloads, no launching).
The window shell (menu bar, toolbar, instance tree, properties panel,
console, status bar) is built and running on [`egui`](https://github.com/emilk/egui)
via `eframe`, styled after a classic Eclipse/SWT-style desktop tool — but it's
still just a shell over static sample data, not wired to the backend modules.

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
