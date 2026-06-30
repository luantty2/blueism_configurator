#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod hid_backend;
mod ui;

fn main() -> eframe::Result<()> {
    application::app::run()
}
