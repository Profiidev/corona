use std::io;

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::Cli;

pub fn generate_completions(shell: Shell) {
  let mut cmd = Cli::command();
  generate(shell, &mut cmd, "corona", &mut io::stdout());
}
