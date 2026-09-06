mod bar;
mod config;
mod panel;
mod theme;

pub const APP_NAME: &str = "corona";

pub fn run() {
  let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    theme::load(cx).expect("Failed to load themes");
    panel::PanelState::init(cx);
    bar::BarState::init(cx);
  });
}
