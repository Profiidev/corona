fn main() {
  tracing_subscriber::fmt::init();

  let app = gpui_kit::application().with_assets(corona_components::assets::Assets);

  app.run(move |cx| {
    gpui_component_shell::init(cx);
    corona_shell::init(cx);
  });
}
