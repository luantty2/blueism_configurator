use crate::application::data::{
    ConnectionTarget, DeviceConnectionStatus, DeviceInformation, DeviceRefreshStatus, DeviceTab,
    DeviceType, DiscoveredHidDevice, FirmwareSummaryInformation, HidConnectionBus,
    TabContentLoadingStatus,
};
use crate::application::runtime::{format_discovered_device_label, ConfiguratorRuntime};
use crate::hid_backend::config_channel;
use crate::ui::theme::{
    apply_button_style, hover_border_stroke, theme_colors, CONTROL_CORNER_RADIUS, CONTROL_SIZE,
};
use eframe::egui::{
    self, AboveOrBelow, Color32, FontFamily, FontId, Rect, Response, Sense, Shape, Vec2, Widget,
};
use eframe::epaint::TextShape;
use std::time::{Duration, Instant};

pub fn show_device_connection(ui: &mut egui::Ui, ctx: &egui::Context, runtime: &mut ConfiguratorRuntime) {
    ui.add_space(0.0);
    ui.horizontal(|ui| {
        ui.label("Device connection");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let angle = match runtime.refresh_status() {
                DeviceRefreshStatus::Refreshing { started_at, .. } => {
                    let elapsed = Instant::now().duration_since(*started_at).as_secs_f32();
                    elapsed * std::f32::consts::TAU
                }
                DeviceRefreshStatus::Idle => 0.0,
            };

            if ui
                .add_enabled(runtime.can_refresh_devices(), RefreshIconButton { angle })
                .on_hover_text("Refresh devices")
                .clicked()
            {
                runtime.start_device_refresh();
            }

            if runtime.is_refreshing() {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        });
    });

    ui.add_space(6.0);
    ui.scope(|ui| {
        let colors = theme_colors(ui);
        apply_button_style(ui, colors);
        let device_options = runtime
            .devices()
            .iter()
            .enumerate()
            .map(|(index, device)| (index, format_discovered_device_label(device)))
            .collect::<Vec<_>>();

        ui.add_enabled_ui(runtime.can_change_selected_device(), |ui| {
            egui::ComboBox::from_id_salt("device_connection_selector")
                .selected_text(runtime.selected_device_label())
                .width(ui.available_width())
                .icon(paint_caret_down_icon)
                .show_ui(ui, |ui| {
                    ui.scope(|ui| {
                        apply_button_style(ui, colors);
                        if runtime.devices().is_empty() {
                            ui.label("No devices found");
                        }

                        for (index, label) in &device_options {
                            if ui
                                .selectable_label(
                                    runtime.selected_device_index() == Some(*index),
                                    label,
                                )
                                .clicked()
                            {
                                runtime.select_device(*index);
                            }
                        }
                    });
                });
        });
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let is_connecting = runtime.is_connecting();
        let reserved_spinner_width = if is_connecting { CONTROL_SIZE + 8.0 } else { 0.0 };
        let button_width = (ui.available_width() - reserved_spinner_width).max(0.0);
        let label = if runtime.is_connected() {
            "Disconnect"
        } else {
            "Connect"
        };

        let response = ui.add_enabled(
            runtime.can_press_connection_button(),
            ConnectActionButton {
                label,
                width: button_width,
            },
        );

        if response.clicked() {
            match runtime.connection_status() {
                DeviceConnectionStatus::Disconnected => runtime.start_connection(),
                DeviceConnectionStatus::Connected => runtime.disconnect_current_device(),
                DeviceConnectionStatus::Connecting { .. } => {}
            }
        }

        if is_connecting {
            ui.add(egui::Spinner::new());
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    });
}

pub fn show_device_tabs(ui: &mut egui::Ui, ctx: &egui::Context, runtime: &mut ConfiguratorRuntime) {
    if !runtime.is_connected() {
        return;
    }

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        for tab in DeviceTab::ALL {
            let response = ui.add(DeviceTabButton {
                tab,
                selected: runtime.active_tab() == tab,
            });
            if response.clicked() {
                runtime.select_tab(tab);
            }
        }
    });

    ui.add_space(12.0);
    show_active_tab_content(ui, runtime);
    if matches!(
        runtime.tab_loading_status(),
        TabContentLoadingStatus::Loading { .. }
    ) {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub fn show_active_tab_content(ui: &mut egui::Ui, runtime: &ConfiguratorRuntime) {
    if matches!(
        runtime.tab_loading_status(),
        TabContentLoadingStatus::Loading { tab, .. } if tab == runtime.active_tab()
    ) {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
        });
        return;
    }

    match runtime.active_tab() {
        DeviceTab::Info => show_info_tab(ui, runtime.loaded_device_information()),
        DeviceTab::Firmware => show_placeholder_tab(ui, "Firmware content placeholder"),
        DeviceTab::Operation => show_placeholder_tab(ui, "Operations content placeholder"),
    }
}

fn show_info_tab(ui: &mut egui::Ui, info: Option<&DeviceInformation>) {
    let Some(info) = info else {
        ui.label("Unavailable");
        return;
    };

    let detailed = info.details.as_ref();
    let firmware = detailed.and_then(|detailed| detailed.firmware.as_ref());
    let identity = detailed.and_then(|detailed| detailed.identity.as_ref());
    let bootloader_variant = detailed
        .and_then(|detailed| detailed.bootloader_variant.as_deref())
        .unwrap_or("Unavailable");
    let supported_features = detailed
        .map(supported_features)
        .unwrap_or_else(|| "Unavailable".to_owned());

    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.set_width(ui.available_width());

        info_section_header(ui, "Device");
        info_row(ui, "Board name", &info.device.board_name);
        info_row(ui, "Device type", device_type(&info.device).label());
        info_row(ui, "Connection target", connection_target(&info.device).label());

        ui.add_space(10.0);
        info_section_header(ui, "Identity");
        info_row(ui, "HW ID", &info.device.hwid);
        info_row(ui, "Vendor ID", &hex_u16(identity.map(|identity| identity.vendor_id)));
        info_row(ui, "Product ID", &hex_u16(identity.map(|identity| identity.product_id)));
        info_row(
            ui,
            "Generation",
            identity
                .map(|identity| identity.generation.as_str())
                .unwrap_or("Unavailable"),
        );

        ui.add_space(10.0);
        info_section_header(ui, "Firmware Summary");
        info_row(
            ui,
            "Version",
            firmware
                .map(|firmware| firmware.version.as_str())
                .unwrap_or("Unavailable"),
        );
        info_row(ui, "Image length", &image_length(firmware));
        info_row(ui, "Flash area ID", &flash_area_id(firmware));
        info_row(ui, "Bootloader variant", bootloader_variant);

        ui.add_space(10.0);
        info_section_header(ui, "Capabilities");
        info_row(ui, "Supported features", &supported_features);
    });
}

fn show_placeholder_tab(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.label(text);
    });
}

fn device_type(device: &DiscoveredHidDevice) -> DeviceType {
    match device.board_name.as_str() {
        "nrf52840dongle" => DeviceType::Dongle,
        _ => DeviceType::Unknown,
    }
}

fn connection_target(device: &DiscoveredHidDevice) -> ConnectionTarget {
    if device.recipient != config_channel::LOCAL_RECIPIENT {
        return ConnectionTarget::PeerThroughDongle;
    }

    match device.connection_bus {
        HidConnectionBus::Usb => ConnectionTarget::DirectUsbDevice,
        HidConnectionBus::Bluetooth => ConnectionTarget::DirectBluetoothDevice,
        HidConnectionBus::Unknown => ConnectionTarget::DirectDevice,
    }
}

fn supported_features(detailed: &crate::application::data::DeviceInformationDetails) -> String {
    let mut features = Vec::new();

    if detailed.modules.iter().any(|module| module.starts_with("dfu")) {
        features.push("DFU");
    }
    if detailed.modules.iter().any(|module| module == "ble_bond") {
        features.push("BLE bond");
    }
    if detailed.modules.iter().any(|module| module.starts_with("led_stream")) {
        features.push("LED stream");
    }

    if features.is_empty() {
        "Unavailable".to_owned()
    } else {
        features.join(", ")
    }
}

fn hex_u16(value: Option<u16>) -> String {
    value
        .map(|value| format!("0x{value:04x}"))
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn image_length(firmware: Option<&FirmwareSummaryInformation>) -> String {
    firmware
        .map(|firmware| format!("{} bytes", firmware.image_len))
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn flash_area_id(firmware: Option<&FirmwareSummaryInformation>) -> String {
    firmware
        .map(|firmware| firmware.flash_area_id.to_string())
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn info_section_header(ui: &mut egui::Ui, text: &str) {
    ui.small(text);
    ui.add_space(2.0);
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let row_size = Vec2::new(ui.available_width(), 20.0);
    let (rect, _) = ui.allocate_exact_size(row_size, Sense::hover());
    let text_color = ui.visuals().text_color();
    let font_id = FontId::default();
    let y = rect.center().y;

    ui.painter().text(
        egui::pos2(rect.left(), y),
        egui::Align2::LEFT_CENTER,
        label,
        font_id.clone(),
        text_color,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 150.0, y),
        egui::Align2::LEFT_CENTER,
        value,
        font_id,
        text_color,
    );
}

struct RefreshIconButton {
    angle: f32,
}

struct DeviceTabButton {
    tab: DeviceTab,
    selected: bool,
}

struct ConnectActionButton {
    label: &'static str,
    width: f32,
}

fn paint_caret_down_icon(
    ui: &egui::Ui,
    rect: Rect,
    visuals: &egui::style::WidgetVisuals,
    _is_open: bool,
    _above_or_below: AboveOrBelow,
) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{f0d7}",
        FontId::new(12.0, FontFamily::Name("fontawesome".into())),
        visuals.fg_stroke.color,
    );
}

fn paint_rotated_fontawesome(
    ui: &egui::Ui,
    center: egui::Pos2,
    glyph: &str,
    size: f32,
    angle: f32,
    color: Color32,
) {
    let galley = ui.painter().layout_no_wrap(
        glyph.to_owned(),
        FontId::new(size, FontFamily::Name("fontawesome".into())),
        color,
    );
    let half_size = galley.size() / 2.0;
    let (sin, cos) = angle.sin_cos();
    let rotated_half_size = Vec2::new(
        cos * half_size.x - sin * half_size.y,
        sin * half_size.x + cos * half_size.y,
    );
    let pos = center - rotated_half_size;

    ui.painter().add(Shape::Text(
        TextShape::new(pos, galley, color).with_angle(angle),
    ));
}

impl Widget for RefreshIconButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let size = Vec2::splat(CONTROL_SIZE);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let inactive_visuals = &ui.style().visuals.widgets.inactive;
            let fill = if response.is_pointer_button_down_on() {
                colors.pressed
            } else {
                inactive_visuals.weak_bg_fill
            };

            ui.painter().rect_filled(rect, CONTROL_CORNER_RADIUS, fill);

            if response.hovered() && !response.is_pointer_button_down_on() {
                ui.painter().rect_stroke(
                    rect,
                    CONTROL_CORNER_RADIUS,
                    hover_border_stroke(colors),
                    egui::StrokeKind::Inside,
                );
            }

            paint_rotated_fontawesome(
                ui,
                rect.center(),
                "\u{f021}",
                14.0,
                self.angle,
                inactive_visuals.fg_stroke.color,
            );
        }

        response
    }
}

impl Widget for DeviceTabButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let label = self.tab.label();
        let font_id = FontId::default();
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            font_id.clone(),
            ui.visuals().text_color(),
        );
        let desired_size = Vec2::new(galley.size().x + 22.0, CONTROL_SIZE);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let visuals = ui.style().interact(&response);
            let fill = if response.is_pointer_button_down_on() || self.selected {
                colors.pressed
            } else {
                Color32::TRANSPARENT
            };

            if fill != Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, CONTROL_CORNER_RADIUS, fill);
            }

            if response.hovered() && !response.is_pointer_button_down_on() && !self.selected {
                ui.painter().rect_stroke(
                    rect,
                    CONTROL_CORNER_RADIUS,
                    hover_border_stroke(colors),
                    egui::StrokeKind::Inside,
                );
            }

            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font_id,
                visuals.fg_stroke.color,
            );
        }

        response
    }
}

impl Widget for ConnectActionButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let desired_size = Vec2::new(self.width, CONTROL_SIZE);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let visuals = ui.style().interact(&response);
            let fill = if response.is_pointer_button_down_on() {
                colors.pressed
            } else {
                visuals.weak_bg_fill
            };

            ui.painter().rect_filled(rect, CONTROL_CORNER_RADIUS, fill);

            if response.hovered() && !response.is_pointer_button_down_on() {
                ui.painter().rect_stroke(
                    rect,
                    CONTROL_CORNER_RADIUS,
                    hover_border_stroke(colors),
                    egui::StrokeKind::Inside,
                );
            }

            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                self.label,
                FontId::default(),
                visuals.fg_stroke.color,
            );
        }

        response
    }
}
