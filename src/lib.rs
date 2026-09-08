mod config;
mod error;
mod integration;
mod ui;

pub const APP_NAME: &str = "corona";

pub fn run() {
  let app = gpui_kit::application().with_assets(ui::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    integration::init(cx).expect("Failed to init compositor");
    ui::init(cx);
  });
}
