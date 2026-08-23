#![allow(dead_code, unused_variables)]

mod auth;
mod config;
mod instance;
mod java;
mod launch;
mod mojang;

fn main() {
    println!("Worst Minecraft Launcher v{}", env!("CARGO_PKG_VERSION"));
    println!("(backend scaffold — nothing is wired up yet)");
}
