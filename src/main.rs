use crate::state::Corona;

mod adapter;
mod state;

fn main() {
  tracing_subscriber::fmt::init();

  let _state = Corona::init().expect("Failed to initialize Corona state");
}
