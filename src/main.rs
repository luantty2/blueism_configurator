mod app;
mod config_channel;
mod hid;

fn main() -> eframe::Result<()> {
    let hid = hid::BlueismHid::new().ok();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Blueism Configurator")
            .with_inner_size([360.0, 720.0])
            .with_min_inner_size([360.0, 720.0])
            .with_max_inner_size([600.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Blueism Configurator",
        options,
        Box::new(move |cc| Ok(Box::new(app::BlueismApp::new(cc, hid)))),
    )
}
