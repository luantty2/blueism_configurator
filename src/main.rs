mod application;
mod hid_backend;
mod ui;

fn main() -> eframe::Result<()> {
    application::app::run()
}
