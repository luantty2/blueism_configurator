use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const CONTROL_SIZE: f32 = 24.0;
pub const CONTROL_CORNER_RADIUS: CornerRadius = CornerRadius::same(4);

#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub hover: Color32,
    pub pressed: Color32,
}

pub fn theme_colors(ui: &egui::Ui) -> ThemeColors {
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

pub fn hover_border_stroke(colors: ThemeColors) -> Stroke {
    Stroke::new(1.0, colors.hover)
}

pub fn sync_global_style(ctx: &egui::Context) {
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

pub fn apply_button_style(ui: &mut egui::Ui, colors: ThemeColors) {
    let style = ui.style_mut();
    style.spacing.interact_size.y = CONTROL_SIZE;
    style.visuals.widgets.inactive.corner_radius = CONTROL_CORNER_RADIUS;
    style.visuals.widgets.hovered.corner_radius = CONTROL_CORNER_RADIUS;
    style.visuals.widgets.active.corner_radius = CONTROL_CORNER_RADIUS;
    style.visuals.widgets.open.corner_radius = CONTROL_CORNER_RADIUS;
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
