mod compositor;
mod config;
mod desktop_entry;
mod error;
mod ui;
mod utils;

pub const APP_NAME: &str = "corona";

pub fn run() {
  let app = gpui_kit::application().with_assets(ui::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    config::load(cx).expect("Failed to load config");
    compositor::init(cx).expect("Failed to init compositor");
    ui::init(cx);
  });
}
