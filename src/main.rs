use corona::Corona;

fn main() {
  tracing_subscriber::fmt::init();

  let mut state = Corona::init().expect("Failed to initialize Corona state");
  state.run().expect("Failed to run Corona event loop");
}
