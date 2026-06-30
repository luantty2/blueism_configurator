use crate::application::data::{
    ConnectionTarget, DeviceConnectionStatus, DeviceInformation, DeviceRefreshStatus, DeviceTab,
    DiscoveredHidDevice, FirmwareFlashStatus, FirmwareSource, FirmwareSummaryInformation,
    HidConnectionBus, LocalFirmwarePackageStatus, NetworkFirmwarePackageStatus,
    NetworkFirmwareRepositoryStatus, TabContentLoadingStatus,
};
use crate::application::runtime::{format_discovered_device_label, ConfiguratorRuntime};
use crate::hid_backend::config_channel;
use crate::ui::i18n::Language;
use crate::ui::text;
use crate::ui::theme::{
    apply_button_style, hover_border_stroke, theme_colors, CONTROL_CORNER_RADIUS, CONTROL_SIZE,
};
use eframe::egui::{self, Color32, FontFamily, FontId, Rect, Response, Sense, Shape, Vec2, Widget};
use eframe::epaint::TextShape;
use egui_extras::{Column, TableBuilder};
use std::time::{Duration, Instant};

const FA_KEYBOARD: &str = "\u{f11c}";
const FA_ID_CARD_CLIP: &str = "\u{f47f}";
const FA_FILE_CODE: &str = "\u{f1c9}";
const FA_ROCKET: &str = "\u{f135}";
const FA_FILE_ZIPPER: &str = "\u{f1c6}";
const FA_FILE_IMPORT: &str = "\u{f56f}";
const FA_CHART_COLUMN: &str = "\u{e0e3}";

pub fn show_device_connection(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    ui.add_space(0.0);
    ui.horizontal(|ui| {
        text::body_label(ui, language.text("device.connection"));

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
                .on_hover_text(language.text("device.refresh.tooltip"))
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
                .selected_text(
                    runtime
                        .selected_device_label()
                        .unwrap_or_else(|| language.text("device.no_device_selected").to_owned()),
                )
                .width(ui.available_width())
                .icon(paint_caret_down_icon)
                .show_ui(ui, |ui| {
                    ui.scope(|ui| {
                        apply_button_style(ui, colors);
                        if runtime.devices().is_empty() {
                            text::body_label(ui, language.text("device.no_devices_found"));
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
        let reserved_spinner_width = if is_connecting {
            CONTROL_SIZE + 8.0
        } else {
            0.0
        };
        let button_width = (ui.available_width() - reserved_spinner_width).max(0.0);
        let label = if runtime.is_connected() {
            language.text("device.disconnect")
        } else {
            language.text("device.connect")
        };

        let response = ui.add_enabled(
            runtime.can_press_connection_button(),
            ConnectActionButton {
                label: label.to_owned(),
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

pub fn show_device_tabs(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    if !runtime.is_connected() {
        return;
    }

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        for tab in DeviceTab::ALL {
            let response = ui.add_enabled(
                !runtime.is_firmware_flashing(),
                DeviceTabButton {
                    label: device_tab_label(tab, language).to_owned(),
                    selected: runtime.active_tab() == tab,
                },
            );
            if response.clicked() {
                runtime.select_tab(tab);
            }
        }
    });

    ui.add_space(12.0);
    show_active_tab_content(ui, runtime, language);
    if matches!(
        runtime.tab_loading_status(),
        TabContentLoadingStatus::Loading { .. }
    ) || runtime.is_firmware_flashing()
        || runtime.is_network_firmware_repository_loading()
        || runtime.is_network_firmware_package_downloading()
    {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub fn show_active_tab_content(
    ui: &mut egui::Ui,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
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
        DeviceTab::Info => show_info_tab(ui, runtime.loaded_device_information(), language),
        DeviceTab::Firmware => show_firmware_tab(ui, runtime, language),
        DeviceTab::Operation => show_operation_tab(ui, runtime, language),
    }
}

fn show_info_tab(ui: &mut egui::Ui, info: Option<&DeviceInformation>, language: &Language) {
    let Some(info) = info else {
        text::body_label(ui, language.text("common.unavailable"));
        return;
    };

    let detailed = info.details.as_ref();
    let firmware = detailed.and_then(|detailed| detailed.firmware.as_ref());
    let identity = detailed.and_then(|detailed| detailed.identity.as_ref());
    let bootloader_variant = detailed
        .and_then(|detailed| detailed.bootloader_variant.as_deref())
        .unwrap_or(language.text("common.unavailable"));
    let supported_features = detailed
        .map(|detailed| supported_features(detailed, language))
        .unwrap_or_else(|| language.text("common.unavailable").to_owned());

    let device_rows = vec![
        InfoTableRow::Icon(FA_KEYBOARD),
        InfoTableRow::Value(
            language.text("info.board_name").to_owned(),
            info.device.board_name.clone(),
        ),
        InfoTableRow::Value(
            language.text("info.device_type").to_owned(),
            language.text("common.unavailable").to_owned(),
        ),
        InfoTableRow::Value(
            language.text("info.connection_target").to_owned(),
            connection_target_label(connection_target(&info.device), language).to_owned(),
        ),
    ];
    let identity_rows = vec![
        InfoTableRow::Icon(FA_ID_CARD_CLIP),
        InfoTableRow::Value(
            language.text("info.hw_id").to_owned(),
            info.device.hwid.clone(),
        ),
        InfoTableRow::Value(
            language.text("info.vendor_id").to_owned(),
            hex_u16(identity.map(|identity| identity.vendor_id), language),
        ),
        InfoTableRow::Value(
            language.text("info.product_id").to_owned(),
            hex_u16(identity.map(|identity| identity.product_id), language),
        ),
        InfoTableRow::Value(
            language.text("info.generation").to_owned(),
            identity
                .map(|identity| identity.generation.clone())
                .unwrap_or_else(|| language.text("common.unavailable").to_owned()),
        ),
    ];
    let firmware_rows = vec![
        InfoTableRow::Icon(FA_FILE_CODE),
        InfoTableRow::Value(
            language.text("info.version").to_owned(),
            firmware
                .map(|firmware| firmware.version.clone())
                .unwrap_or_else(|| language.text("common.unavailable").to_owned()),
        ),
        InfoTableRow::Value(
            language.text("info.image_length").to_owned(),
            image_length(firmware, language),
        ),
        InfoTableRow::Value(
            language.text("info.flash_area_id").to_owned(),
            flash_area_id(firmware, language),
        ),
        InfoTableRow::Value(
            language.text("info.bootloader_variant").to_owned(),
            bootloader_variant.to_owned(),
        ),
    ];
    let capabilities_rows = vec![
        InfoTableRow::Icon(FA_ROCKET),
        InfoTableRow::Value(
            language.text("info.supported_features").to_owned(),
            supported_features,
        ),
    ];

    show_info_table(
        ui,
        &[
            (&device_rows[..], false),
            (&identity_rows[..], true),
            (&firmware_rows[..], true),
            (&capabilities_rows[..], true),
        ],
        language,
    );
}

enum InfoTableRow {
    Icon(&'static str),
    Value(String, String),
}

fn show_info_table(ui: &mut egui::Ui, groups: &[(&[InfoTableRow], bool)], language: &Language) {
    let row_count = groups.iter().map(|(rows, _)| rows.len()).sum::<usize>();
    show_bounded_table(ui, row_count, language, |mut body| {
        for (rows, add_separator) in groups {
            for (index, table_row) in rows.iter().enumerate() {
                show_info_table_row(&mut body, table_row, *add_separator && index == 0);
            }
        }
    });
}

fn show_info_table_row(
    body: &mut egui_extras::TableBody<'_>,
    table_row: &InfoTableRow,
    overline: bool,
) {
    body.row(text::BODY_ROW_HEIGHT, |mut row| {
        row.set_overline(overline);
        match table_row {
            InfoTableRow::Icon(icon) => {
                row.col(|ui| {
                    fontawesome_label(ui, icon, 13.0);
                });
                row.col(|_| {});
            }
            InfoTableRow::Value(label, value) => {
                row.col(|ui| {
                    text::body_label(ui, label);
                });
                row.col(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(value).font(text::body_font()))
                            .selectable(true),
                    );
                });
            }
        }
    });
}

fn fontawesome_label(ui: &mut egui::Ui, glyph: &str, size: f32) {
    let colors = theme_colors(ui);

    ui.add(
        egui::Label::new(
            egui::RichText::new(glyph)
                .font(FontId::new(size, FontFamily::Name("fontawesome".into())))
                .color(colors.hover),
        )
        .selectable(false),
    );
}

fn show_operation_tab(ui: &mut egui::Ui, runtime: &mut ConfiguratorRuntime, language: &Language) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            text::body_label(ui, language.text("operation.reset_device"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !runtime.is_firmware_flashing(),
                        FirmwareUploadButton {
                            label: language.text("operation.reset").to_owned(),
                        },
                    )
                    .clicked()
                {
                    runtime.reset_current_device();
                }
            });
        });
    });
}

fn show_firmware_tab(ui: &mut egui::Ui, runtime: &mut ConfiguratorRuntime, language: &Language) {
    let width = ui.available_width();
    let height = ui.available_height();

    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            let footer_height = firmware_footer_height(runtime);
            let footer_gap = if footer_height > 0.0 { 8.0 } else { 0.0 };
            let content_height = (ui.available_height() - footer_height - footer_gap).max(0.0);

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), content_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("firmware_tab_scroll_area")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            show_firmware_tab_content(ui, runtime, language);
                        });
                },
            );

            if footer_height > 0.0 {
                ui.add_space(footer_gap);
                show_firmware_footer(ui, runtime, language);
            }
        },
    );
}

fn show_firmware_tab_content(
    ui: &mut egui::Ui,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    ui.set_width(ui.available_width());
    text::body_label(ui, language.text("firmware.source"));
    ui.add_space(6.0);

    ui.add_enabled_ui(!runtime.is_firmware_flashing(), |ui| {
        let colors = theme_colors(ui);
        apply_button_style(ui, colors);
        egui::ComboBox::from_id_salt("firmware_source_selector")
            .selected_text(firmware_source_label(runtime.firmware_source(), language))
            .width(ui.available_width())
            .icon(paint_caret_down_icon)
            .show_ui(ui, |ui| {
                ui.scope(|ui| {
                    apply_button_style(ui, colors);
                    for source in FirmwareSource::ALL {
                        if ui
                            .selectable_label(
                                runtime.firmware_source() == source,
                                firmware_source_label(source, language),
                            )
                            .clicked()
                        {
                            runtime.select_firmware_source(source);
                        }
                    }
                });
            });
    });

    match runtime.firmware_source() {
        FirmwareSource::Network => {
            ui.add_space(10.0);
            show_network_firmware_repository_status(ui, runtime, language);
        }
        FirmwareSource::Local => {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                text::body_label(ui, language.text("firmware.select_firmware"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !runtime.is_firmware_flashing(),
                            FirmwareUploadButton {
                                label: language.text("firmware.browse").to_owned(),
                            },
                        )
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(language.text("firmware.dfu_package_filter"), &["zip"])
                            .pick_file()
                        {
                            runtime.select_local_firmware_package(path);
                        }
                    }
                });
            });

            ui.add_space(8.0);
            show_local_firmware_package_status(ui, runtime, language);
        }
    }
}

fn show_network_firmware_repository_status(
    ui: &mut egui::Ui,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    match runtime.network_firmware_repository_status() {
        NetworkFirmwareRepositoryStatus::Idle => {
            text::body_label(ui, language.text("firmware.repository_not_loaded"));
        }
        NetworkFirmwareRepositoryStatus::Loading { .. } => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                text::body_label(ui, language.text("firmware.repository_loading"));
            });
        }
        NetworkFirmwareRepositoryStatus::Loaded(_) => {
            ui.add_space(8.0);
            text::body_label(ui, language.text("firmware.select_firmware"));
            ui.add_space(6.0);
            let firmware_options = runtime.network_firmware_options();
            let selected_text = runtime
                .selected_network_firmware()
                .and_then(|index| firmware_options.get(index))
                .map(network_firmware_label)
                .unwrap_or_else(|| language.text("firmware.no_firmware_selected").to_owned());

            ui.add_enabled_ui(!runtime.is_firmware_flashing(), |ui| {
                let colors = theme_colors(ui);
                apply_button_style(ui, colors);
                egui::ComboBox::from_id_salt("network_firmware_selector")
                    .selected_text(selected_text)
                    .width(ui.available_width())
                    .icon(paint_caret_down_icon)
                    .show_ui(ui, |ui| {
                        ui.scope(|ui| {
                            apply_button_style(ui, colors);
                            if firmware_options.is_empty() {
                                text::body_label(
                                    ui,
                                    language.text("firmware.no_compatible_firmware"),
                                );
                            }

                            for (index, firmware) in firmware_options.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        runtime.selected_network_firmware() == Some(index),
                                        network_firmware_label(firmware),
                                    )
                                    .clicked()
                                {
                                    runtime.select_network_firmware(index, firmware.clone());
                                }
                            }
                        });
                    });
            });

            match runtime.network_firmware_package_status() {
                NetworkFirmwarePackageStatus::Idle => {}
                NetworkFirmwarePackageStatus::Downloading {
                    progress,
                    started_at,
                    ..
                } => {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        text::body_label(ui, language.text("firmware.downloading"));
                    });
                    ui.add_space(8.0);
                    ui.add(FirmwareProgressBar {
                        progress: *progress,
                        width: ui.available_width(),
                    });
                    ui.add_space(4.0);
                    text::small_label(ui, &remaining_time_text(*progress, *started_at, language));
                }
                NetworkFirmwarePackageStatus::Failed(error) => {
                    ui.add_space(8.0);
                    text::body_colored_label(ui, Color32::from_rgb(0xd0, 0x5c, 0x5c), error);
                }
            }

            ui.add_space(8.0);
            show_local_firmware_package_status(ui, runtime, language);
        }
        NetworkFirmwareRepositoryStatus::Failed(error) => {
            text::body_colored_label(ui, Color32::from_rgb(0xd0, 0x5c, 0x5c), error);
        }
    }
}

fn network_firmware_label(
    firmware: &crate::hid_backend::firmware_repository::FirmwareRepositoryFirmware,
) -> String {
    firmware.version.clone()
}

fn device_tab_label(tab: DeviceTab, language: &Language) -> &str {
    match tab {
        DeviceTab::Info => language.text("tab.info"),
        DeviceTab::Firmware => language.text("tab.firmware"),
        DeviceTab::Operation => language.text("tab.operation"),
    }
}

fn firmware_source_label(source: FirmwareSource, language: &Language) -> &str {
    match source {
        FirmwareSource::Network => language.text("firmware.source.network"),
        FirmwareSource::Local => language.text("firmware.source.local"),
    }
}

fn connection_target_label(target: ConnectionTarget, language: &Language) -> &str {
    match target {
        ConnectionTarget::DirectUsbDevice => language.text("connection.direct_usb_device"),
        ConnectionTarget::DirectBluetoothDevice => {
            language.text("connection.direct_bluetooth_device")
        }
        ConnectionTarget::DirectDevice => language.text("connection.direct_device"),
        ConnectionTarget::PeerThroughDongle => language.text("connection.peer_through_dongle"),
    }
}

fn firmware_check_label<'a>(name: &'a str, language: &'a Language) -> &'a str {
    match name {
        "Selected file" => language.text("firmware.check.selected_file"),
        "Open package" => language.text("firmware.check.open_package"),
        "Board name" => language.text("firmware.check.board_name"),
        "Device FW version" => language.text("firmware.check.device_fw_version"),
        "Device bootloader" => language.text("firmware.check.device_bootloader"),
        "Device flash area ID" => language.text("firmware.check.device_flash_area_id"),
        "Target slot" => language.text("firmware.check.target_slot"),
        "Selected image" => language.text("firmware.check.selected_image"),
        "Image bootloader" => language.text("firmware.check.image_bootloader"),
        "Image size" => language.text("firmware.check.image_size"),
        "Manifest image size" => language.text("firmware.check.manifest_image_size"),
        "Image validation" => language.text("firmware.check.image_validation"),
        "Manifest format" => language.text("firmware.check.manifest_format"),
        "FW version from file" => language.text("firmware.check.fw_version_from_file"),
        _ => name,
    }
}

fn show_local_firmware_package_status(
    ui: &mut egui::Ui,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    match runtime.local_firmware_package_status() {
        LocalFirmwarePackageStatus::NotSelected => {}
        LocalFirmwarePackageStatus::Valid(package) => {
            show_firmware_package_checks(ui, &package.checks, language);
        }
        LocalFirmwarePackageStatus::Invalid(checks) => {
            show_firmware_package_checks(ui, checks, language);
        }
    }
}

fn show_firmware_footer(ui: &mut egui::Ui, runtime: &mut ConfiguratorRuntime, language: &Language) {
    if matches!(
        runtime.local_firmware_package_status(),
        LocalFirmwarePackageStatus::Valid(_)
    ) || runtime.is_firmware_flashing()
        || runtime.firmware_source() == FirmwareSource::Network
    {
        show_firmware_flash_controls(ui, runtime, language);
    }

    match runtime.firmware_flash_status() {
        FirmwareFlashStatus::Succeeded => {
            ui.add_space(8.0);
            text::body_label(ui, language.text("firmware.flash_completed"));
        }
        FirmwareFlashStatus::Failed(message) => {
            ui.add_space(8.0);
            text::body_colored_label(ui, Color32::from_rgb(0xd0, 0x5c, 0x5c), message);
        }
        FirmwareFlashStatus::Idle | FirmwareFlashStatus::Flashing { .. } => {}
    }
}

fn firmware_footer_height(runtime: &ConfiguratorRuntime) -> f32 {
    let has_flash_controls = matches!(
        runtime.local_firmware_package_status(),
        LocalFirmwarePackageStatus::Valid(_)
    ) || runtime.is_firmware_flashing()
        || runtime.firmware_source() == FirmwareSource::Network;
    let has_message = matches!(
        runtime.firmware_flash_status(),
        FirmwareFlashStatus::Succeeded | FirmwareFlashStatus::Failed(_)
    );

    let mut height = 0.0;
    if has_flash_controls {
        height += CONTROL_SIZE;
    }
    if runtime.is_firmware_flashing() {
        height += 8.0 + 18.0 + 4.0 + text::BODY_ROW_HEIGHT;
    }
    if has_message {
        height += 8.0 + text::BODY_ROW_HEIGHT;
    }

    height
}

fn show_firmware_package_checks(
    ui: &mut egui::Ui,
    checks: &[crate::application::data::FirmwarePackageCheck],
    language: &Language,
) {
    let error_color = Color32::from_rgb(0xd0, 0x5c, 0x5c);
    let text_color = ui.visuals().text_color();
    let row_count = firmware_package_check_row_count(checks);
    let package_names = ["Selected file", "Open package"];
    let device_context_names = [
        "Board name",
        "Device FW version",
        "Device bootloader",
        "Device flash area ID",
    ];
    let target_names = ["Target slot", "Selected image", "Image bootloader"];
    let validation_names = [
        "Image size",
        "Manifest image size",
        "Image validation",
        "Manifest format",
        "FW version from file",
    ];

    show_bounded_table(ui, row_count, language, |mut body| {
        show_firmware_section_icon(&mut body, checks, &package_names, FA_FILE_ZIPPER, false);
        show_firmware_check_group(
            &mut body,
            checks,
            &package_names,
            false,
            text_color,
            error_color,
            language,
        );
        show_firmware_section_icon(
            &mut body,
            checks,
            &device_context_names,
            FA_FILE_IMPORT,
            true,
        );
        show_firmware_check_group(
            &mut body,
            checks,
            &device_context_names,
            false,
            text_color,
            error_color,
            language,
        );
        show_firmware_section_icon(&mut body, checks, &target_names, FA_CHART_COLUMN, true);
        show_firmware_check_group(
            &mut body,
            checks,
            &target_names,
            false,
            text_color,
            error_color,
            language,
        );
        show_firmware_check_group(
            &mut body,
            checks,
            &validation_names,
            false,
            text_color,
            error_color,
            language,
        );

        for check in checks
            .iter()
            .filter(|check| !check.passed && !is_firmware_check_displayed(&check.name))
        {
            show_firmware_check_row(&mut body, check, text_color, error_color, language);
        }
    });
}

fn show_bounded_table(
    ui: &mut egui::Ui,
    row_count: usize,
    language: &Language,
    add_body: impl FnOnce(egui_extras::TableBody<'_>),
) {
    let body_height = table_body_height(ui, row_count);
    let table_height = table_total_height(ui, body_height);

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), table_height),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .vscroll(false)
                .min_scrolled_height(0.0)
                .max_scroll_height(body_height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(
                    Column::initial(text::INFO_LABEL_WIDTH)
                        .at_least(text::INFO_LABEL_WIDTH)
                        .clip(true)
                        .resizable(true),
                )
                .column(Column::remainder().at_least(120.0))
                .header(text::BODY_ROW_HEIGHT, |mut header| {
                    header.col(|ui| {
                        text::body_label(ui, language.text("table.item"));
                    });
                    header.col(|ui| {
                        text::body_label(ui, language.text("table.value"));
                    });
                })
                .body(add_body);
        },
    );
}

fn table_body_height(ui: &egui::Ui, row_count: usize) -> f32 {
    let row_spacing = ui.spacing().item_spacing.y;
    row_count as f32 * (text::BODY_ROW_HEIGHT + row_spacing)
}

fn table_total_height(ui: &egui::Ui, body_height: f32) -> f32 {
    text::BODY_ROW_HEIGHT + ui.spacing().item_spacing.y + body_height
}

fn firmware_package_check_row_count(
    checks: &[crate::application::data::FirmwarePackageCheck],
) -> usize {
    let check_rows = checks
        .iter()
        .filter(|check| is_firmware_check_displayed(&check.name) || !check.passed)
        .count();
    let package_names = ["Selected file", "Open package"];
    let device_context_names = [
        "Board name",
        "Device FW version",
        "Device bootloader",
        "Device flash area ID",
    ];
    let target_names = ["Target slot", "Selected image", "Image bootloader"];
    let section_icons = [
        &package_names[..],
        &device_context_names[..],
        &target_names[..],
    ]
    .iter()
    .filter(|names| firmware_check_group_has_rows(checks, names))
    .count();

    check_rows + section_icons
}

fn show_firmware_check_group(
    body: &mut egui_extras::TableBody<'_>,
    checks: &[crate::application::data::FirmwarePackageCheck],
    names: &[&str],
    add_separator: bool,
    text_color: Color32,
    error_color: Color32,
    language: &Language,
) {
    if !firmware_check_group_has_rows(checks, names) {
        return;
    }

    if add_separator {
        let mut first_row = true;
        for name in names {
            if let Some(check) = checks.iter().find(|check| check.name == *name) {
                show_firmware_check_row_with_overline(
                    body,
                    check,
                    first_row,
                    text_color,
                    error_color,
                    language,
                );
                first_row = false;
            }
        }
        return;
    }

    for name in names {
        if let Some(check) = checks.iter().find(|check| check.name == *name) {
            show_firmware_check_row(body, check, text_color, error_color, language);
        }
    }
}

fn firmware_check_group_has_rows(
    checks: &[crate::application::data::FirmwarePackageCheck],
    names: &[&str],
) -> bool {
    names
        .iter()
        .any(|name| checks.iter().any(|check| check.name == *name))
}

fn show_firmware_section_icon(
    body: &mut egui_extras::TableBody<'_>,
    checks: &[crate::application::data::FirmwarePackageCheck],
    names: &[&str],
    icon: &'static str,
    overline: bool,
) {
    if !firmware_check_group_has_rows(checks, names) {
        return;
    }

    body.row(text::BODY_ROW_HEIGHT, |mut row| {
        row.set_overline(overline);
        row.col(|ui| {
            fontawesome_label(ui, icon, 13.0);
        });
        row.col(|_| {});
    });
}

fn show_firmware_check_row_with_overline(
    body: &mut egui_extras::TableBody<'_>,
    check: &crate::application::data::FirmwarePackageCheck,
    overline: bool,
    text_color: Color32,
    error_color: Color32,
    language: &Language,
) {
    body.row(text::BODY_ROW_HEIGHT, |mut row| {
        row.set_overline(overline);
        row.col(|ui| {
            text::body_label(ui, firmware_check_label(&check.name, language));
        });
        row.col(|ui| {
            let value_color = if check.passed {
                text_color
            } else {
                error_color
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&check.value)
                        .font(text::body_font())
                        .color(value_color),
                )
                .selectable(true),
            );
        });
    });
}

fn show_firmware_check_row(
    body: &mut egui_extras::TableBody<'_>,
    check: &crate::application::data::FirmwarePackageCheck,
    text_color: Color32,
    error_color: Color32,
    language: &Language,
) {
    body.row(text::BODY_ROW_HEIGHT, |mut row| {
        row.col(|ui| {
            text::body_label(ui, firmware_check_label(&check.name, language));
        });
        row.col(|ui| {
            let value_color = if check.passed {
                text_color
            } else {
                error_color
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&check.value)
                        .font(text::body_font())
                        .color(value_color),
                )
                .selectable(true),
            );
        });
    });
}

fn is_firmware_check_displayed(name: &str) -> bool {
    matches!(
        name,
        "Selected file"
            | "Open package"
            | "Board name"
            | "Device FW version"
            | "Device bootloader"
            | "Device flash area ID"
            | "Target slot"
            | "Selected image"
            | "Image bootloader"
            | "Image size"
            | "Manifest image size"
            | "Image validation"
            | "Manifest format"
            | "FW version from file"
    )
}

fn show_firmware_flash_controls(
    ui: &mut egui::Ui,
    runtime: &mut ConfiguratorRuntime,
    language: &Language,
) {
    let is_flashing = runtime.is_firmware_flashing();
    let reserved_spinner_width = if is_flashing { CONTROL_SIZE + 8.0 } else { 0.0 };
    let button_width = (ui.available_width() - reserved_spinner_width).max(0.0);

    ui.horizontal(|ui| {
        let response = ui.add_enabled(
            runtime.can_flash_firmware(),
            FirmwareFlashButton {
                label: language.text("firmware.flash").to_owned(),
                width: button_width,
            },
        );
        if response.clicked() {
            runtime.start_firmware_flash();
        }

        if is_flashing {
            ui.add(egui::Spinner::new());
        }
    });

    if let FirmwareFlashStatus::Flashing {
        progress,
        started_at,
        ..
    } = runtime.firmware_flash_status()
    {
        ui.add_space(8.0);
        ui.add(FirmwareProgressBar {
            progress: *progress,
            width: ui.available_width(),
        });
        ui.add_space(4.0);
        text::small_label(ui, &remaining_time_text(*progress, *started_at, language));
    }
}

fn remaining_time_text(progress: f32, started_at: Instant, language: &Language) -> String {
    if progress <= 0.0 {
        return language.text("firmware.remaining_calculating").to_owned();
    }

    let elapsed = Instant::now().saturating_duration_since(started_at);
    let remaining_secs = (elapsed.as_secs_f32() * (1.0 - progress) / progress).max(0.0) as u64;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;

    language
        .text("firmware.remaining")
        .replace("{hours}", &hours.to_string())
        .replace("{minutes}", &minutes.to_string())
        .replace("{seconds}", &seconds.to_string())
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

fn supported_features(
    detailed: &crate::application::data::DeviceInformationDetails,
    language: &Language,
) -> String {
    let mut features = Vec::new();

    if detailed
        .modules
        .iter()
        .any(|module| module.starts_with("dfu"))
    {
        features.push(language.text("capability.dfu"));
    }
    if detailed.modules.iter().any(|module| module == "ble_bond") {
        features.push(language.text("capability.ble_bond"));
    }
    if detailed
        .modules
        .iter()
        .any(|module| module.starts_with("led_stream"))
    {
        features.push(language.text("capability.led_stream"));
    }

    if features.is_empty() {
        language.text("common.unavailable").to_owned()
    } else {
        features.join(", ")
    }
}

fn hex_u16(value: Option<u16>, language: &Language) -> String {
    value
        .map(|value| format!("0x{value:04x}"))
        .unwrap_or_else(|| language.text("common.unavailable").to_owned())
}

fn image_length(firmware: Option<&FirmwareSummaryInformation>, language: &Language) -> String {
    firmware
        .map(|firmware| format!("{} bytes", firmware.image_len))
        .unwrap_or_else(|| language.text("common.unavailable").to_owned())
}

fn flash_area_id(firmware: Option<&FirmwareSummaryInformation>, language: &Language) -> String {
    firmware
        .map(|firmware| firmware.flash_area_id.to_string())
        .unwrap_or_else(|| language.text("common.unavailable").to_owned())
}

struct RefreshIconButton {
    angle: f32,
}

struct DeviceTabButton {
    label: String,
    selected: bool,
}

struct ConnectActionButton {
    label: String,
    width: f32,
}

struct FirmwareUploadButton {
    label: String,
}

struct FirmwareFlashButton {
    label: String,
    width: f32,
}

struct FirmwareProgressBar {
    progress: f32,
    width: f32,
}

fn paint_caret_down_icon(
    ui: &egui::Ui,
    rect: Rect,
    visuals: &egui::style::WidgetVisuals,
    _is_open: bool,
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
        let label = self.label;
        let font_id = text::body_font();
        let galley =
            ui.painter()
                .layout_no_wrap(label.clone(), font_id.clone(), ui.visuals().text_color());
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
                text::body_font(),
                visuals.fg_stroke.color,
            );
        }

        response
    }
}

impl Widget for FirmwareUploadButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let font_id = text::body_font();
        let galley = ui.painter().layout_no_wrap(
            self.label.to_owned(),
            font_id.clone(),
            ui.visuals().text_color(),
        );
        let desired_size = Vec2::new(galley.size().x + 26.0, CONTROL_SIZE);
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
                font_id,
                visuals.fg_stroke.color,
            );
        }

        response
    }
}

impl Widget for FirmwareFlashButton {
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
                text::body_font(),
                visuals.fg_stroke.color,
            );
        }

        response
    }
}

impl Widget for FirmwareProgressBar {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let desired_size = Vec2::new(self.width, CONTROL_SIZE);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let colors = theme_colors(ui);
            let visuals = ui.visuals().widgets.inactive;
            ui.painter()
                .rect_filled(rect, CONTROL_CORNER_RADIUS, visuals.weak_bg_fill);

            let progress = self.progress.clamp(0.0, 1.0);
            if progress > 0.0 {
                let filled_rect = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + rect.width() * progress, rect.bottom()),
                );
                ui.painter()
                    .rect_filled(filled_rect, CONTROL_CORNER_RADIUS, colors.pressed);
                ui.painter().rect_stroke(
                    filled_rect,
                    CONTROL_CORNER_RADIUS,
                    egui::Stroke::new(1.0, colors.hover),
                    egui::StrokeKind::Inside,
                );
            }

            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}%", progress * 100.0),
                text::body_font(),
                visuals.fg_stroke.color,
            );
        }

        response
    }
}
