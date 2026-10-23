use std::io::{BufRead, IsTerminal};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::launcher::commands::{Dmenu, DmenuRequest, Open, OpenRequest};

#[derive(Subcommand)]
pub enum LauncherCommands {
  /// Choose one of the lines on stdin; prints it, exits 1 when cancelled. Like `rofi -dmenu`
  Dmenu {
    /// Shown as the breadcrumb
    #[arg(short, long)]
    prompt: Option<String>,
    /// Shown in the empty search field
    #[arg(long)]
    placeholder: Option<String>,
    /// Ctrl+Enter marks lines, Enter prints every marked one
    #[arg(long)]
    multi: bool,
  },
  /// Open a `corona://launcher/<namespace>/<command>?arguments=…` link
  OpenUrl { url: String },
}

impl LauncherCommands {
  pub fn execute(self) {
    match self {
      LauncherCommands::Dmenu {
        prompt,
        placeholder,
        multi,
      } => {
        let lines = read_lines();
        let request = DmenuRequest {
          lines,
          prompt,
          placeholder,
          multi,
        };
        match Dmenu::send(request) {
          Ok(chosen) => {
            for line in chosen {
              println!("{line}");
            }
          }
          Err(_) => std::process::exit(1),
        }
      }
      LauncherCommands::OpenUrl { url } => {
        let sent = parse_url(&url).and_then(Open::send);
        if let Err(e) = sent {
          tracing::error!("Failed to open {url}: {e:#}");
          std::process::exit(1);
        }
      }
    }
  }
}

fn read_lines() -> Vec<String> {
  let stdin = std::io::stdin();
  if stdin.is_terminal() {
    return Vec::new();
  }
  stdin
    .lock()
    .lines()
    .map_while(Result::ok)
    .filter(|l| !l.is_empty())
    .collect()
}

/// `corona://launcher/<ns>/<command>?arguments=…` as the command `<ns>:<command>`
fn parse_url(url: &str) -> Result<OpenRequest> {
  let rest = url
    .strip_prefix("corona://launcher")
    .context("not a corona://launcher link")?;
  let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
  let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
  let command = match parts.as_slice() {
    [] => None,
    [ns, command] => Some(format!("{}:{}", decode(ns)?, decode(command)?)),
    _ => bail!("expected corona://launcher/<namespace>/<command>"),
  };
  let mut arguments = None;
  for pair in query.split('&').filter(|p| !p.is_empty()) {
    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
    if matches!(key, "arguments" | "query") {
      arguments = Some(decode(value)?);
    }
  }
  Ok(OpenRequest {
    command,
    query: arguments,
  })
}

/// Percent-decoding, `+` as a space
fn decode(s: &str) -> Result<String> {
  let mut bytes = Vec::with_capacity(s.len());
  let mut iter = s.bytes();
  while let Some(b) = iter.next() {
    match b {
      b'%' => {
        let hex = [iter.next(), iter.next()];
        let [Some(h), Some(l)] = hex else {
          bail!("bad escape in {s}");
        };
        let byte = u8::from_str_radix(std::str::from_utf8(&[h, l])?, 16)
          .with_context(|| format!("bad escape in {s}"))?;
        bytes.push(byte);
      }
      b'+' => bytes.push(b' '),
      b => bytes.push(b),
    }
  }
  Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn urls() {
    let r = parse_url("corona://launcher/corona/volume?arguments=40%25").unwrap();
    assert_eq!(r.command.as_deref(), Some("corona:volume"));
    assert_eq!(r.query.as_deref(), Some("40%"));
    let r = parse_url("corona://launcher").unwrap();
    assert_eq!(r, OpenRequest::default());
    let r = parse_url("corona://launcher/?query=hello+world").unwrap();
    assert_eq!(r.query.as_deref(), Some("hello world"));
    assert!(parse_url("https://example.com").is_err());
    assert!(parse_url("corona://launcher/a/b/c").is_err());
    assert!(parse_url("corona://launcher/a/b?arguments=%zz").is_err());
  }
}
