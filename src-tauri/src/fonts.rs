//! Redistributable CJK fallback embedded into the executable; no font installation.
use eframe::egui;
pub const LICENSE:&str=include_str!("../../LICENSES/NotoSansCJK-OFL.txt");

pub fn install(ctx:&egui::Context) {
    let mut definitions=egui::FontDefinitions::default();
    definitions.font_data.insert("homepanel-cjk".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/NotoSansCJKsc-Regular.otf")).into());
    for family in [egui::FontFamily::Proportional,egui::FontFamily::Monospace] {
        definitions.families.get_mut(&family).unwrap().push("homepanel-cjk".into());
    }
    ctx.set_fonts(definitions);
}

/// Runs before account directories, networking, tray or single-instance activation.
pub fn verify()->bool {
    let ctx=egui::Context::default();install(&ctx);
    let mut valid=false;
    let _=ctx.run(egui::RawInput::default(),|ctx| {
        valid=ctx.fonts_mut(|fonts|fonts.has_glyphs(&egui::FontId::proportional(16.0),"米家设备温度湿度设置开启关闭℃"));
    });
    valid
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_font_contains_chinese_without_system_font_files() {
        assert!(super::verify());
        assert!(super::LICENSE.contains("SIL OPEN FONT LICENSE"));
    }
}
