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
  }
}

fn run_shell() {
  let http = corona_reqwest::client().expect("Failed to create the HTTP client");
  let app = gpui_kit::application()
    .with_assets(corona_components::assets::Assets)
    .with_http_client(Arc::new(http));

  app.run(move |cx| {
    gpui_component_shell::init(cx);
    corona_shell::init(cx);
  });
}
