#![allow(dead_code, unused_variables)]

mod auth;
mod config;
mod instance;
mod java;
mod launch;
mod mojang;
mod ui;

fn main() -> eframe::Result<()> {
    ui::run()
}
