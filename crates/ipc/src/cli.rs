//! What the integrations' `corona ipc` subcommands share

use clap_complete::CompletionCandidate;
use corona_utils::level::parse_level;

use crate::Switch;

/// The default volume and brightness step, 5%
pub const STEP: &str = "5";

pub fn level(s: &str) -> Result<f32, String> {
  parse_level(s).map_err(|e| e.to_string())
}

/// Completions from a list the running shell sent, none when it is not running
pub fn candidates<E>(names: Result<Vec<String>, E>) -> Vec<CompletionCandidate> {
  names.map(plain).unwrap_or_default()
}

/// Values with a description shown beside them
pub fn described(items: impl IntoIterator<Item = (String, String)>) -> Vec<CompletionCandidate> {
  items
    .into_iter()
    .map(|(value, help)| CompletionCandidate::new(value).help(Some(help.into())))
    .collect()
}

pub fn plain(items: impl IntoIterator<Item = String>) -> Vec<CompletionCandidate> {
  items.into_iter().map(CompletionCandidate::new).collect()
}

/// Logs what failed, if anything did
pub fn done<T>(result: Result<T, impl std::fmt::Display>, what: &str) {
  if let Err(e) = result {
    tracing::error!("Failed to {what}: {}", e);
  }
}

pub fn print_lines(lines: Vec<String>) {
  for line in lines {
    println!("{line}");
  }
}

/// Prints the state for `status`, the error for anything
pub fn print_switch(result: Result<bool, impl std::fmt::Display>, switch: Switch, what: &str) {
  match result {
    Ok(on) if matches!(switch, Switch::Status) => println!("{}", if on { "on" } else { "off" }),
    Ok(_) => {}
    Err(e) => tracing::error!("Failed to switch {what}: {}", e),
  }
}

#[cfg(any(test, feature = "test-support"))]
pub mod testing {
  use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
  };

  use serde_json::{Value, json};

  /// Runs `run` against a fake shell answering `reply`, returns the request it got
  pub fn request(reply: Value, run: impl FnOnce()) -> Value {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::remove_var("WAYLAND_DISPLAY");
      std::env::remove_var("CORONA_SOCKET");
    }
    let listener = UnixListener::bind(dir.path().join("corona.sock")).unwrap();
    let shell = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let mut line = String::new();
      BufReader::new(&stream).read_line(&mut line).unwrap();
      writeln!(stream, "{}", json!({ "Ok": reply })).unwrap();
      serde_json::from_str::<Value>(&line).unwrap()
    });
    run();
    shell.join().unwrap()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn levels() {
    for ok in ["65", "65%", "0.65", " 65 % "] {
      assert!((level(ok).unwrap() - 0.65).abs() < 1e-6, "{ok}");
    }
    for bad in ["-1", "abc", "inf", "NaN", ""] {
      assert!(level(bad).is_err(), "{bad}");
    }
  }

  #[test]
  fn candidates_from_the_shell() {
    let names = candidates::<()>(Ok(vec!["a".into(), "b".into()]));
    let names: Vec<_> = names.iter().map(|c| c.get_value().to_owned()).collect();
    assert_eq!(names, ["a", "b"]);
    // shell not running
    assert!(candidates(Err("down")).is_empty());
  }
}
