use crate::bar::Bar;

mod bar;
mod config;
mod lock;
mod panel;
mod theme;

pub fn run() {
  let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    theme::load(cx).expect("Failed to load themes");
    panel::PanelState::init(cx);

    Bar::create(cx).expect("Failed to create bar window");
  });
}
