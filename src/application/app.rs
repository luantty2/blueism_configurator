use crate::application::runtime::ConfiguratorRuntime;
use crate::hid_backend::device_discovery::BlueismHid;
use crate::ui::{font, theme, widgets};
use eframe::egui;

pub fn run() -> eframe::Result<()> {
    let hid = BlueismHid::new().ok();
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
        Box::new(move |cc| Ok(Box::new(ConfiguratorApp::new(cc, hid)))),
    )
}

pub struct ConfiguratorApp {
    runtime: ConfiguratorRuntime,
    last_dark_mode: Option<bool>,
}

impl ConfiguratorApp {
    fn new(cc: &eframe::CreationContext<'_>, hid: Option<BlueismHid>) -> Self {
        font::install_fonts(&cc.egui_ctx);
        theme::sync_global_style(&cc.egui_ctx);

        Self {
            runtime: ConfiguratorRuntime::new(hid),
            last_dark_mode: Some(cc.egui_ctx.style().visuals.dark_mode),
        }
    }
}

impl eframe::App for ConfiguratorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark_mode = ctx.style().visuals.dark_mode;
        if self.last_dark_mode != Some(dark_mode) {
            theme::sync_global_style(ctx);
            self.last_dark_mode = Some(dark_mode);
        }

        self.runtime.poll_background_tasks();

        egui::TopBottomPanel::top("header")
            .exact_height(56.0)
            .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.heading("Blueism Configurator");
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            widgets::show_device_connection(ui, ctx, &mut self.runtime);
            widgets::show_device_tabs(ui, ctx, &mut self.runtime);
        });
    }
}
