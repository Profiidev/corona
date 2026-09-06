mod assets;
mod bar;
mod config;
mod panel;

pub const APP_NAME: &str = "corona";

pub fn run() {
  let app = gpui_kit::application().with_assets(assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    assets::load(cx).expect("Failed to load themes");
    panel::PanelState::init(cx);
    bar::BarState::init(cx);
  });
}
