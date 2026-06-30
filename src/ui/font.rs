use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "cjk".to_owned(),
        FontData::from_static(include_bytes!("../../assets/SourceHanSansSC-Light-2.otf")).into(),
    );
    fonts.font_data.insert(
        "fontawesome".to_owned(),
        FontData::from_static(include_bytes!("../../assets/fa-solid-900.ttf")).into(),
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
    fonts.families.insert(
        FontFamily::Name("fontawesome".into()),
        vec!["fontawesome".to_owned()],
    );

    ctx.set_fonts(fonts);
}
