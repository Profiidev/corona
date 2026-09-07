mod assets;
mod bar;
mod compositor;
mod config;
mod desktop_entry;
mod error;
mod panel;

pub const APP_NAME: &str = "corona";

pub fn run() {
  let app = gpui_kit::application().with_assets(assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    assets::load(cx).expect("Failed to load themes");
    compositor::init(cx).expect("Failed to init compositor");
    panel::PanelState::init(cx);
    bar::BarState::init(cx);
  });
}
