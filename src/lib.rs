use crate::bar::Bar;

mod bar;
mod panel;
mod theme;

pub fn run() {
  let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);
    theme::load_themes(cx).expect("Failed to load themes");

    Bar::create(cx).expect("Failed to create bar window");
  });
}
