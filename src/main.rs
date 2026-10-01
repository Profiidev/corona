use std::sync::Arc;

fn main() {
  tracing_subscriber::fmt::init();

  let http = corona_reqwest::client().expect("Failed to create the HTTP client");
  let app = gpui_kit::application()
    .with_assets(corona_components::assets::Assets)
    .with_http_client(Arc::new(http));

  app.run(move |cx| {
    gpui_component_shell::init(cx);
    corona_shell::init(cx);
  });
}
