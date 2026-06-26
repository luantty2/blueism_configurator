use crate::{
    config_channel::{self, DetailedDeviceInfo},
    hid::{self, BlueismHid, DeviceSummary},
};
use eframe::egui::{
    self, AboveOrBelow, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect,
    Response, Sense, Shape, Stroke, Vec2, Widget,
};
use eframe::epaint::TextShape;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

pub struct BlueismApp {
    hid: Option<BlueismHid>,
    devices: Vec<DeviceSummary>,
    selected_device: Option<usize>,
    connection_state: ConnectionState,
    active_tab: DeviceTab,
    tab_load_state: TabLoadState,
    selected_info: Option<InfoData>,
    refresh_state: RefreshState,
    last_dark_mode: Option<bool>,
}

impl BlueismApp {
    pub fn new(cc: &eframe::CreationContext<'_>, hid: Option<BlueismHid>) -> Self {
        install_cjk_font(&cc.egui_ctx);
        sync_global_style(&cc.egui_ctx);

        let mut app = Self {
            hid,
            devices: Vec::new(),
            selected_device: None,
            connection_state: ConnectionState::Disconnected,
            active_tab: DeviceTab::Info,
            tab_load_state: TabLoadState::Idle,
            selected_info: None,
            refresh_state: RefreshState::Idle,
            last_dark_mode: Some(cc.egui_ctx.style().visuals.dark_mode),
        };

        app.start_refresh();
        app
    }

    fn apply_scanned_devices(&mut self, devices: Vec<DeviceSummary>) {
        self.devices = devices;
        if self
            .selected_device
            .is_some_and(|index| index >= self.devices.len())
        {
            self.selected_device = None;
        }
    }

    fn selected_device_label(&self) -> String {
        self.selected_device
            .and_then(|index| self.devices.get(index))
            .map(device_label)
            .unwrap_or_else(|| "No device selected".to_owned())
    }

    fn start_refresh(&mut self) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let devices = hid::scan_devices();
            let _ = sender.send(devices);
        });

        let now = Instant::now();
        self.refresh_state = RefreshState::Refreshing {
            started_at: now,
            minimum_until: now + Duration::from_secs(1),
            receiver,
        };
    }

    fn fetch_selected_info(&mut self) {
        let Some(index) = self.selected_device else {
            self.selected_info = None;
            return;
        };
        let Some(device) = self.devices.get(index) else {
            self.selected_info = None;
            return;
        };

        let detailed = self
            .hid
            .as_ref()
            .and_then(|hid| hid.open(&device.path).ok())
            .and_then(|opened| config_channel::read_detailed_info(&opened, device.recipient).ok());

        self.selected_info = Some(InfoData {
            device: device.clone(),
            detailed,
        });
    }
}

impl eframe::App for BlueismApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark_mode = ctx.style().visuals.dark_mode;
        if self.last_dark_mode != Some(dark_mode) {
            sync_global_style(ctx);
            self.last_dark_mode = Some(dark_mode);
        }

        if matches!(
            self.connection_state,
            ConnectionState::Connecting { complete_at } if Instant::now() >= complete_at
        ) {
            self.connection_state = ConnectionState::Connected;
            self.active_tab = DeviceTab::Info;
            self.tab_load_state = TabLoadState::Loading {
                tab: DeviceTab::Info,
                complete_at: Instant::now() + Duration::from_secs(1),
            };
        }

        if matches!(
            self.tab_load_state,
            TabLoadState::Loading { complete_at, .. } if Instant::now() >= complete_at
        ) {
            if matches!(self.tab_load_state, TabLoadState::Loading { tab: DeviceTab::Info, .. }) {
                self.fetch_selected_info();
            }
            self.tab_load_state = TabLoadState::Loaded;
        }

        let refresh_result = match &self.refresh_state {
            RefreshState::Refreshing {
                minimum_until,
                receiver,
                ..
            } if Instant::now() >= *minimum_until => match receiver.try_recv() {
                Ok(devices) => Some(devices),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Vec::new()),
            },
            RefreshState::Idle | RefreshState::Refreshing { .. } => None,
        };
        if let Some(devices) = refresh_result {
            self.refresh_state = RefreshState::Idle;
            self.apply_scanned_devices(devices);
        }

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
            ui.add_space(0.0);
            ui.horizontal(|ui| {
                ui.label("Device connection");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_refreshing = matches!(self.refresh_state, RefreshState::Refreshing { .. });
                    let is_connected = matches!(self.connection_state, ConnectionState::Connected);
                    let is_connecting = matches!(self.connection_state, ConnectionState::Connecting { .. });
                    let can_refresh = !is_refreshing && !is_connected && !is_connecting;
                    let angle = match &self.refresh_state {
                        RefreshState::Refreshing { started_at, .. } => {
                            let elapsed = Instant::now().duration_since(*started_at).as_secs_f32();
                            elapsed * std::f32::consts::TAU
                        }
                        RefreshState::Idle => 0.0,
                    };
                    if ui
                        .add_enabled(can_refresh, RefreshButton { angle })
                        .on_hover_text("Refresh devices")
                        .clicked()
                    {
                        self.start_refresh();
                    }

                    if is_refreshing {
                        ctx.request_repaint_after(Duration::from_millis(16));
                    }
                });
            });

            ui.add_space(6.0);
            let is_refreshing = matches!(self.refresh_state, RefreshState::Refreshing { .. });
            let is_connected = matches!(self.connection_state, ConnectionState::Connected);
            let is_connecting = matches!(self.connection_state, ConnectionState::Connecting { .. });
            let can_change_selection = !is_refreshing && !is_connected && !is_connecting;
            ui.scope(|ui| {
                let colors = theme_colors(ui);
                apply_button_style(ui, colors);
                let previous_selection = self.selected_device;
                let device_options: Vec<_> = self
                    .devices
                    .iter()
                    .enumerate()
                    .map(|(index, device)| (index, device_label(device)))
                    .collect();

                ui.add_enabled_ui(can_change_selection, |ui| {
                    egui::ComboBox::from_id_salt("device_connection_selector")
                        .selected_text(self.selected_device_label())
                        .width(ui.available_width())
                        .icon(paint_caret_down_icon)
                        .show_ui(ui, |ui| {
                            ui.scope(|ui| {
                                apply_button_style(ui, colors);
                                if self.devices.is_empty() {
                                    ui.label("No devices found");
                                }

                                for (index, label) in &device_options {
                                    ui.selectable_value(
                                        &mut self.selected_device,
                                        Some(*index),
                                        label,
                                    );
                                }
                            });
                        });
                });

                if can_change_selection && self.selected_device != previous_selection {
                    self.connection_state = ConnectionState::Disconnected;
                    self.tab_load_state = TabLoadState::Idle;
                    self.selected_info = None;
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can_press = self.selected_device.is_some() && !is_connecting && !is_refreshing;
                let reserved_spinner_width = if is_connecting { REFRESH_BUTTON_SIZE + 8.0 } else { 0.0 };
                let button_width = (ui.available_width() - reserved_spinner_width).max(0.0);
                let label = if matches!(self.connection_state, ConnectionState::Connected) {
                    "Disconnect"
                } else {
                    "Connect"
                };

                let response = ui.add_enabled(can_press, ConnectButton {
                    label,
                    width: button_width,
                });

                if response.clicked() {
                    self.connection_state = match self.connection_state {
                        ConnectionState::Disconnected => ConnectionState::Connecting {
                            complete_at: Instant::now() + Duration::from_secs(2),
                        },
                        ConnectionState::Connected => {
                            self.tab_load_state = TabLoadState::Idle;
                            self.selected_info = None;
                            ConnectionState::Disconnected
                        }
                        ConnectionState::Connecting { complete_at } => {
                            ConnectionState::Connecting { complete_at }
                        }
                    };
                }

                if is_connecting {
                    ui.add(egui::Spinner::new());
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            });

            if matches!(self.connection_state, ConnectionState::Connected) {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    for tab in DeviceTab::ALL {
                        let response = ui.add(TabButton {
                            tab,
                            selected: self.active_tab == tab,
                        });
                        if response.clicked() {
                            self.active_tab = tab;
                            if tab == DeviceTab::Info {
                                self.selected_info = None;
                            }
                            self.tab_load_state = TabLoadState::Loading {
                                tab,
                                complete_at: Instant::now() + Duration::from_secs(1),
                            };
                        }
                    }
                });

                ui.add_space(12.0);
                show_active_tab(ui, self.active_tab, self.tab_load_state, self.selected_info.as_ref());
                if matches!(self.tab_load_state, TabLoadState::Loading { .. }) {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
        });
    }
}

fn device_label(device: &DeviceSummary) -> String {
    format!("{} (HW ID: {})", device.board_name, device.hwid)
}

fn show_active_tab(
    ui: &mut egui::Ui,
    active_tab: DeviceTab,
    load_state: TabLoadState,
    selected_info: Option<&InfoData>,
) {
    if matches!(load_state, TabLoadState::Loading { tab, .. } if tab == active_tab) {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
        });
        return;
    }

    match active_tab {
        DeviceTab::Info => show_info_tab(ui, selected_info),
        DeviceTab::Firmware => show_placeholder_tab(ui, "Firmware content placeholder"),
        DeviceTab::Operation => show_placeholder_tab(ui, "Operations content placeholder"),
    }
}

fn show_info_tab(ui: &mut egui::Ui, info: Option<&InfoData>) {
    let Some(info) = info else {
        ui.label("Unavailable");
        return;
    };

    let detailed = info.detailed.as_ref();
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
        info_row(ui, "Device type", device_type(&info.device));
        info_row(ui, "Connection target", connection_target(&info.device));

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

fn device_type(device: &DeviceSummary) -> &'static str {
    match device.board_name.as_str() {
        "nrf52840dongle" => "Dongle",
        "blueism_nrf54l15" => "Unknown",
        _ => "Unknown",
    }
}

fn connection_target(device: &DeviceSummary) -> &'static str {
    if device.recipient == config_channel::LOCAL_RECIPIENT {
        "Direct device"
    } else {
        "Peer through dongle"
    }
}

fn supported_features(detailed: &DetailedDeviceInfo) -> String {
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

fn image_length(firmware: Option<&config_channel::FwInfo>) -> String {
    firmware
        .map(|firmware| format!("{} bytes", firmware.image_len))
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn flash_area_id(firmware: Option<&config_channel::FwInfo>) -> String {
    firmware
        .map(|firmware| firmware.flash_area_id.to_string())
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn show_placeholder_tab(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.label(text);
    });
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

struct RefreshButton {
    angle: f32,
}
struct TabButton {
    tab: DeviceTab,
    selected: bool,
}

struct ConnectButton {
    label: &'static str,
    width: f32,
}

struct InfoData {
    device: DeviceSummary,
    detailed: Option<DetailedDeviceInfo>,
}

enum ConnectionState {
    Disconnected,
    Connecting { complete_at: Instant },
    Connected,
}

enum RefreshState {
    Idle,
    Refreshing {
        started_at: Instant,
        minimum_until: Instant,
        receiver: Receiver<Vec<DeviceSummary>>,
    },
}

#[derive(Clone, Copy)]
enum TabLoadState {
    Idle,
    Loading { tab: DeviceTab, complete_at: Instant },
    Loaded,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeviceTab {
    Info,
    Firmware,
    Operation,
}

impl DeviceTab {
    const ALL: [Self; 3] = [Self::Info, Self::Firmware, Self::Operation];

    fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Firmware => "Firmware",
            Self::Operation => "Operation",
        }
    }
}

const REFRESH_BUTTON_SIZE: f32 = 24.0;
const BUTTON_CORNER_RADIUS: CornerRadius = CornerRadius::same(4);

#[derive(Clone, Copy)]
struct ThemeColors {
    hover: Color32,
    pressed: Color32,
}

fn theme_colors(ui: &egui::Ui) -> ThemeColors {
    if ui.visuals().dark_mode {
        ThemeColors {
            hover: Color32::from_rgb(0x67, 0x8b, 0xcf),
            pressed: Color32::from_rgb(0x28, 0x28, 0x28),
        }
    } else {
        ThemeColors {
            hover: Color32::from_rgb(0x7d, 0xaa, 0xff),
            pressed: Color32::from_rgb(0xd6, 0xd6, 0xd6),
        }
    }
}

fn hover_border_stroke(colors: ThemeColors) -> Stroke {
    Stroke::new(1.0, colors.hover)
}

fn sync_global_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let selection_color = if style.visuals.dark_mode {
        Color32::from_rgb(0x28, 0x28, 0x28)
    } else {
        Color32::from_rgb(0xd6, 0xd6, 0xd6)
    };
    let text_color = style.visuals.text_color();

    style.visuals.selection.bg_fill = selection_color;
    style.visuals.selection.stroke = Stroke::new(1.0, text_color);
    style.visuals.widgets.active.weak_bg_fill = selection_color;
    style.visuals.widgets.active.bg_fill = selection_color;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_color);
    style.visuals.widgets.open.weak_bg_fill = selection_color;
    style.visuals.widgets.open.bg_fill = selection_color;
    style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, text_color);

    ctx.set_style(style);
}

fn apply_button_style(ui: &mut egui::Ui, colors: ThemeColors) {
    let style = ui.style_mut();
    style.spacing.interact_size.y = REFRESH_BUTTON_SIZE;
    style.visuals.widgets.inactive.corner_radius = BUTTON_CORNER_RADIUS;
    style.visuals.widgets.hovered.corner_radius = BUTTON_CORNER_RADIUS;
    style.visuals.widgets.active.corner_radius = BUTTON_CORNER_RADIUS;
    style.visuals.widgets.open.corner_radius = BUTTON_CORNER_RADIUS;
    style.visuals.widgets.hovered.weak_bg_fill = style.visuals.widgets.inactive.weak_bg_fill;
    style.visuals.widgets.hovered.fg_stroke = style.visuals.widgets.inactive.fg_stroke;
    style.visuals.widgets.hovered.bg_stroke = hover_border_stroke(colors);
    style.visuals.widgets.active.weak_bg_fill = colors.pressed;
    style.visuals.widgets.active.bg_fill = colors.pressed;
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
    style.visuals.widgets.open.weak_bg_fill = colors.pressed;
    style.visuals.widgets.open.bg_fill = colors.pressed;
    style.visuals.widgets.open.bg_stroke = Stroke::NONE;
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

impl Widget for RefreshButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let size = Vec2::splat(REFRESH_BUTTON_SIZE);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let inactive_visuals = &ui.style().visuals.widgets.inactive;
            let fill = if response.is_pointer_button_down_on() {
                colors.pressed
            } else {
                inactive_visuals.weak_bg_fill
            };

            ui.painter().rect_filled(rect, BUTTON_CORNER_RADIUS, fill);

            if response.hovered() && !response.is_pointer_button_down_on() {
                ui.painter().rect_stroke(
                    rect,
                    BUTTON_CORNER_RADIUS,
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

impl Widget for TabButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let label = self.tab.label();
        let font_id = FontId::default();
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            font_id.clone(),
            ui.visuals().text_color(),
        );
        let desired_size = Vec2::new(galley.size().x + 22.0, REFRESH_BUTTON_SIZE);
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
                ui.painter().rect_filled(rect, BUTTON_CORNER_RADIUS, fill);
            }

            if response.hovered() && !response.is_pointer_button_down_on() && !self.selected {
                ui.painter().rect_stroke(
                    rect,
                    BUTTON_CORNER_RADIUS,
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

impl Widget for ConnectButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let desired_size = Vec2::new(self.width, REFRESH_BUTTON_SIZE);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let visuals = ui.style().interact(&response);
            let fill = if response.is_pointer_button_down_on() {
                colors.pressed
            } else {
                visuals.weak_bg_fill
            };

            ui.painter().rect_filled(rect, BUTTON_CORNER_RADIUS, fill);

            if response.hovered() && !response.is_pointer_button_down_on() {
                ui.painter().rect_stroke(
                    rect,
                    BUTTON_CORNER_RADIUS,
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

fn install_cjk_font(ctx: &egui::Context) {
    const FONT_CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    let Some(font_bytes) = FONT_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_owned(), FontData::from_owned(font_bytes).into());
    fonts.font_data.insert(
        "fontawesome".to_owned(),
        FontData::from_static(include_bytes!("../assets/fa-solid-900.ttf")).into(),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("cjk".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("cjk".to_owned());
    fonts
        .families
        .insert(FontFamily::Name("fontawesome".into()), vec!["fontawesome".to_owned()]);

    ctx.set_fonts(fonts);
}
