use corona::Corona;

fn main() {
  tracing_subscriber::fmt::init();

  Corona::init()
    .expect("Failed to initialize Corona state")
    .run()
    .expect("Failed to run Corona event loop");
}
