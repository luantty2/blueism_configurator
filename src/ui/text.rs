use eframe::egui::{self, Color32, FontId, TextStyle};

pub const INFO_LABEL_WIDTH: f32 = 128.0;
pub const BODY_ROW_HEIGHT: f32 = 20.0;

pub fn body_font() -> FontId {
    FontId::proportional(13.0)
}

pub fn section_font(ui: &egui::Ui) -> FontId {
    TextStyle::Small.resolve(ui.style())
}

pub fn body_label(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(body_font())).selectable(false));
}

pub fn body_colored_label(ui: &mut egui::Ui, color: Color32, text: &str) {
    ui.add(
        egui::Label::new(egui::RichText::new(text).font(body_font()).color(color))
            .selectable(false),
    );
}

pub fn small_label(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(section_font(ui))).selectable(false));
}
