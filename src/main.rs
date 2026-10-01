use std::sync::Arc;

use reqwest_client::ReqwestClient;

fn main() {
  tracing_subscriber::fmt::init();

  let http = ReqwestClient::user_agent(concat!("corona/", env!("CARGO_PKG_VERSION")))
    .expect("Failed to initialize HTTP client");
  let app = gpui_kit::application()
    .with_assets(corona_components::assets::Assets)
    .with_http_client(Arc::new(http));

  app.run(move |cx| {
    gpui_component_shell::init(cx);
    corona_shell::init(cx);
  });
}
