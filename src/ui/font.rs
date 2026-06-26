use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &egui::Context) {
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
