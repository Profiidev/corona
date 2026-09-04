use crate::bar::Bar;

mod bar;

pub fn run() {
  let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

  app.run(move |cx| {
    gpui_kit::init(cx);

    Bar::create(cx).expect("Failed to create bar window");
  });
}
