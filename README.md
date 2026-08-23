# Worst Minecraft Launcher

A Minecraft launcher, written in Rust. Named honestly.

This is an early-stage scaffold: the module layout for the launcher's backend
exists, but nothing is wired up yet (no auth, no downloads, no launching).
The GUI framework hasn't been picked yet either.

## Design

Menu mockups (Main Menu, Sign-in, Instances, Settings) live here:
https://claude.ai/code/artifact/22464eda-7738-4f79-a145-469316cceb7b

These are visual references, not tied to any specific GUI toolkit.

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
