use std::sync::Arc;

fn main() {
  tracing_subscriber::fmt::init();

  let cli = corona_cli::Cli::parse();

  match cli.command {
    corona_cli::Commands::Ipc { command } => {
      command.execute();
    }
    corona_cli::Commands::Shell => {
      run_shell();
    }
    corona_cli::Commands::Config { command } => command.execute(),
    corona_cli::Commands::Schema { command } => command.execute(),
  }
}

fn run_shell() {
  // cpal names its pipewire streams `cpal-playback-<pid>` with no way to change it,
  // so the notification sound shows as corona this way. Set before any thread starts.
  unsafe {
    std::env::set_var(
      "PIPEWIRE_PROPS",
      r#"{ node.description = "corona" application.icon-name = "corona-settings" }"#,
    )
  };
  let http = corona_reqwest::client().expect("Failed to create the HTTP client");
  let app = gpui_kit::application()
    .with_assets(corona_components::assets::Assets)
    .with_http_client(Arc::new(http));

  app.run(move |cx| {
    corona_shell::i18n::extend_components();
    gpui_component_shell::init(cx);
    corona_shell::init(cx);
  });
}
