use crate::application::preferences::{self, UserPreferences};
use crate::application::runtime::ConfiguratorRuntime;
use crate::hid_backend::device_discovery::BlueismHid;
use crate::hid_backend::firmware_repository;
use crate::ui::i18n::{Language, LanguageId};
use crate::ui::text;
use crate::ui::theme::{apply_button_style, theme_colors};
use crate::ui::{font, theme, widgets};
use eframe::egui;

pub fn run() -> eframe::Result<()> {
    let hid = BlueismHid::new().ok();
    let preferences = preferences::load_user_preferences();
    let selected_language = LanguageId::from_storage_key(&preferences.language);
    let language = Language::load(selected_language);
    let title = language.text("app.title").to_owned();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([360.0, 720.0])
            .with_min_inner_size([360.0, 720.0])
            .with_max_inner_size([600.0, 720.0])
            .with_icon(load_app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| Ok(Box::new(ConfiguratorApp::new(cc, hid, preferences)))),
    )
}

fn load_app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icons/app-icon.png"))
        .unwrap_or_else(|_| egui::IconData::default())
}

pub struct ConfiguratorApp {
    runtime: ConfiguratorRuntime,
    language: Language,
    settings_open: bool,
    selected_language: LanguageId,
    selected_appearance: SettingsAppearance,
    applied_theme: Option<egui::Theme>,
    cache_clear_message: Option<CacheClearMessage>,
}

impl ConfiguratorApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        hid: Option<BlueismHid>,
        preferences: UserPreferences,
    ) -> Self {
        font::install_fonts(&cc.egui_ctx);
        theme::sync_global_style(&cc.egui_ctx);
        let selected_language = LanguageId::from_storage_key(&preferences.language);
        let selected_appearance = SettingsAppearance::from_storage_key(&preferences.appearance);

        Self {
            runtime: ConfiguratorRuntime::new(hid),
            language: Language::load(selected_language),
            settings_open: false,
            selected_language,
            selected_appearance,
            applied_theme: None,
            cache_clear_message: None,
        }
    }
}

impl eframe::App for ConfiguratorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_appearance(ctx);

        self.runtime.poll_background_tasks();

        egui::TopBottomPanel::top("header")
            .exact_height(56.0)
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(self.language.text("app.title"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let gear = egui::RichText::new("\u{f013}").font(egui::FontId::new(
                            14.0,
                            egui::FontFamily::Name("fontawesome".into()),
                        ));
                        if ui
                            .add_sized([24.0, 24.0], egui::Button::new(gear))
                            .clicked()
                        {
                            self.cache_clear_message = None;
                            self.settings_open = true;
                        }
                        ui.add_space(6.0);
                        text::small_label(ui, &format!("v{}", env!("CARGO_PKG_VERSION")));
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            widgets::show_device_connection(ui, ctx, &mut self.runtime, &self.language);
            widgets::show_device_tabs(ui, ctx, &mut self.runtime, &self.language);
        });

        let mut settings_open = self.settings_open;
        egui::Window::new(self.language.text("settings.title"))
            .default_size([260.0, 180.0])
            .resizable(false)
            .open(&mut settings_open)
            .show(ctx, |ui| {
                let language_label = self.language.text("settings.language").to_owned();
                let appearance_label = self.language.text("settings.appearance").to_owned();
                let appearance_value = self.selected_appearance.label(&self.language).to_owned();
                let cache_label = self.language.text("settings.cache").to_owned();
                let cache_clear = self.language.text("settings.cache.clear").to_owned();
                let cache_cleared = self.language.text("settings.cache.cleared").to_owned();
                let cache_clear_failed =
                    self.language.text("settings.cache.clear.failed").to_owned();

                show_settings_dropdown_row(
                    ui,
                    &language_label,
                    "settings_language_selector",
                    self.selected_language.native_name(),
                    |ui| {
                        for language_id in LanguageId::ALL {
                            if ui
                                .selectable_label(
                                    self.selected_language == language_id,
                                    language_id.native_name(),
                                )
                                .clicked()
                            {
                                self.selected_language = language_id;
                                self.language = Language::load(language_id);
                                self.save_preferences();
                            }
                        }
                    },
                );
                ui.add_space(8.0);
                show_settings_dropdown_row(
                    ui,
                    &appearance_label,
                    "settings_appearance_selector",
                    &appearance_value,
                    |ui| {
                        for appearance in SettingsAppearance::ALL {
                            if ui
                                .selectable_label(
                                    self.selected_appearance == appearance,
                                    appearance.label(&self.language),
                                )
                                .clicked()
                            {
                                self.selected_appearance = appearance;
                                self.save_preferences();
                            }
                        }
                    },
                );
                ui.add_space(8.0);
                let cache_button_label =
                    if matches!(self.cache_clear_message, Some(CacheClearMessage::Succeeded)) {
                        cache_cleared.as_str()
                    } else {
                        cache_clear.as_str()
                    };
                let can_clear_cache =
                    !matches!(self.cache_clear_message, Some(CacheClearMessage::Succeeded));
                if show_settings_button_row(ui, &cache_label, cache_button_label, can_clear_cache) {
                    self.cache_clear_message =
                        Some(match firmware_repository::clear_firmware_cache() {
                            Ok(()) => CacheClearMessage::Succeeded,
                            Err(error) => CacheClearMessage::Failed(error),
                        });
                }
                if let Some(message) = &self.cache_clear_message {
                    if let CacheClearMessage::Failed(error) = message {
                        ui.add_space(6.0);
                        ui.colored_label(
                            ui.style().visuals.error_fg_color,
                            format!("{cache_clear_failed}: {error}"),
                        );
                    }
                }
            });
        self.settings_open = settings_open;
    }
}

impl ConfiguratorApp {
    fn apply_appearance(&mut self, ctx: &egui::Context) {
        let theme = self.selected_appearance.resolve(ctx);
        if self.applied_theme == Some(theme) {
            return;
        }

        match theme {
            egui::Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            egui::Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        }
        theme::sync_global_style(ctx);
        self.applied_theme = Some(theme);
    }

    fn save_preferences(&self) {
        let preferences = UserPreferences {
            language: self.selected_language.storage_key().to_owned(),
            appearance: self.selected_appearance.storage_key().to_owned(),
        };
        let _ = preferences::save_user_preferences(&preferences);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsAppearance {
    System,
    Light,
    Dark,
}

impl SettingsAppearance {
    const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    fn label(self, language: &Language) -> &str {
        match self {
            Self::System => language.text("settings.appearance.current"),
            Self::Light => language.text("settings.appearance.light"),
            Self::Dark => language.text("settings.appearance.dark"),
        }
    }

    fn storage_key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn from_storage_key(key: &str) -> Self {
        match key {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    fn resolve(self, ctx: &egui::Context) -> egui::Theme {
        match self {
            Self::System => ctx.system_theme().unwrap_or(egui::Theme::Light),
            Self::Light => egui::Theme::Light,
            Self::Dark => egui::Theme::Dark,
        }
    }
}

enum CacheClearMessage {
    Succeeded,
    Failed(String),
}

fn show_settings_dropdown_row(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: &'static str,
    selected_text: &str,
    add_options: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        text::body_label(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.scope(|ui| {
                let colors = theme_colors(ui);
                apply_button_style(ui, colors);
                egui::ComboBox::from_id_salt(id_salt)
                    .selected_text(selected_text)
                    .width(112.0)
                    .show_ui(ui, add_options);
            });
        });
    });
}

fn show_settings_button_row(
    ui: &mut egui::Ui,
    label: &str,
    button_label: &str,
    enabled: bool,
) -> bool {
    ui.horizontal(|ui| {
        text::body_label(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.scope(|ui| {
                let colors = theme_colors(ui);
                apply_button_style(ui, colors);
                ui.add_enabled(enabled, egui::Button::new(button_label))
                    .clicked()
            })
            .inner
        })
        .inner
    })
    .inner
}
