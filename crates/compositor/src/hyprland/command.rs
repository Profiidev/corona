use std::{
  fmt::Display,
  io::{Read, Write},
  os::unix::net::UnixStream,
  path::PathBuf,
  time::Duration,
};

use anyhow::Result;

use crate::hyprland::encoding::decode_ipc_response;

/// A hung Hyprland must not hang the UI thread that asked
#[cfg(not(test))]
const TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Ipc {
  pub cmd_socket: PathBuf,
}

impl Ipc {
  pub(super) fn send_cmd(&self, cmd: &Command) -> Result<String> {
    let mut socket = UnixStream::connect(&self.cmd_socket)?;
    socket.set_read_timeout(Some(TIMEOUT))?;
    socket.set_write_timeout(Some(TIMEOUT))?;
    socket.write_all(cmd.to_string().as_bytes())?;
    let mut res = Vec::new();
    socket.read_to_end(&mut res)?;
    Ok(decode_ipc_response(&res))
  }

  pub(super) fn eval(&self, lua: impl AsRef<str>) -> Result<String> {
    self.send_cmd(&Command {
      command: format!("eval {}", lua.as_ref()),
      flags: CommandFlags::empty(),
    })
  }

  pub(super) fn dsp(&self, call: impl AsRef<str>) -> Result<()> {
    let res = self.eval(format!("hl.dispatch(hl.dsp.{})", call.as_ref()))?;
    if res != "ok" {
      anyhow::bail!("Hyprland dsp call failed: {}", res);
    }
    Ok(())
  }
}

/// `value` as a Lua literal: whole numbers as they are, anything else as a quoted string, so
/// a workspace or window from a plugin can never end the expression it is put in
pub(crate) fn lua_value(value: &str) -> String {
  let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
  let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
  let number =
    digits(value.strip_prefix('-').unwrap_or(value)) || value.strip_prefix("0x").is_some_and(hex);
  if number {
    return value.to_string();
  }
  lua_string(value)
}

/// `value` as a double quoted Lua string
pub(crate) fn lua_string(value: &str) -> String {
  let mut out = String::with_capacity(value.len() + 2);
  out.push('"');
  for c in value.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      c if c.is_control() => {
        for b in c.to_string().bytes() {
          out.push_str(&format!("\\{b:03}"));
        }
      }
      c => out.push(c),
    }
  }
  out.push('"');
  out
}

/// https://github.com/hyprland-community/hyprland-rs/blob/master/src/data/regular.rs
pub struct Command {
  pub command: String,
  pub flags: CommandFlags,
}

bitflags::bitflags! {
  pub struct CommandFlags: u8 {
    const JSON = 1;
    const REFRESH = 2;
  }
}

impl Display for Command {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.flags.contains(CommandFlags::JSON) {
      f.write_str("j")?;
    }
    if self.flags.contains(CommandFlags::REFRESH) {
      f.write_str("r")?;
    }
    write!(f, "/{}", self.command)
  }
}

#[macro_export]
macro_rules! hypr_data_cmd {
  ($cmd:ident, $arg:literal, $output:ty, $parsed:ty, $convert:expr) => {
    impl $crate::hyprland::command::Ipc {
      pub fn $cmd(&self) -> anyhow::Result<$parsed> {
        let cmd = $crate::hyprland::command::Command {
          command: $arg.to_string(),
          flags: $crate::hyprland::command::CommandFlags::JSON,
        };
        let res = self.send_cmd(&cmd)?;
        let output: $output = serde_json::from_str(&res)?;
        Ok($convert(output))
      }
    }
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::hyprland::fake::FakeHyprland;

  fn command(flags: CommandFlags) -> String {
    Command {
      command: "clients".into(),
      flags,
    }
    .to_string()
  }

  #[test]
  fn flags_prefix_the_command() {
    assert_eq!(command(CommandFlags::empty()), "/clients");
    assert_eq!(command(CommandFlags::JSON), "j/clients");
    assert_eq!(command(CommandFlags::REFRESH), "r/clients");
    assert_eq!(
      command(CommandFlags::JSON | CommandFlags::REFRESH),
      "jr/clients"
    );
  }

  #[test]
  fn sends_one_command_per_connection() {
    let hypr = FakeHyprland::start();
    let ipc = hypr.ipc();
    let answer = ipc
      .send_cmd(&Command {
        command: "cursorpos".into(),
        flags: CommandFlags::JSON,
      })
      .unwrap();
    assert_eq!(answer, r#"{"x": -5, "y": 1200}"#);
    assert_eq!(ipc.eval("hl.version()").unwrap(), "ok");
    assert_eq!(hypr.commands(), ["j/cursorpos", "/eval hl.version()"]);
  }

  #[test]
  fn dispatch_needs_ok() {
    let hypr = FakeHyprland::start();
    let ipc = hypr.ipc();
    ipc.dsp("exec_cmd(\"true\")").unwrap();
    assert_eq!(
      hypr.commands(),
      [r#"/eval hl.dispatch(hl.dsp.exec_cmd("true"))"#]
    );
    hypr.answer(r#"/eval hl.dispatch(hl.dsp.nope())"#, "no such dispatcher");
    assert_eq!(
      ipc.dsp("nope()").unwrap_err().to_string(),
      "Hyprland dsp call failed: no such dispatcher"
    );
    hypr.answer(r#"/eval hl.dispatch(hl.dsp.trailing())"#, "ok\n");
    assert_eq!(
      ipc.dsp("trailing()").unwrap_err().to_string(),
      "Hyprland dsp call failed: ok\n"
    );
  }

  #[test]
  fn lua_literals() {
    assert_eq!(lua_value("12"), "12");
    assert_eq!(lua_value("-1"), "-1");
    assert_eq!(lua_value("0xAbC"), "0xAbC");
    for text in ["", "-", "0x", "1.5", "1e3", " 1", "special:magic", "0xg"] {
      assert!(lua_value(text).starts_with('"'), "{text}");
    }
    assert_eq!(lua_string("a\"b\\c\n\r\t\0ü"), r#""a\"b\\c\n\r\009\000ü""#);
  }

  #[test]
  fn a_hung_hyprland_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".socket.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    // accepts, then never answers
    let held = std::thread::spawn(move || listener.accept().map(|(stream, _)| stream));
    let started = std::time::Instant::now();
    assert!(Ipc { cmd_socket: path }.eval("x").is_err());
    assert!(started.elapsed() < TIMEOUT * 10);
    drop(held.join());
  }

  #[test]
  fn missing_socket_is_an_error() {
    let ipc = Ipc {
      cmd_socket: "/nonexistent/.socket.sock".into(),
    };
    assert!(ipc.eval("x").is_err());
    assert!(ipc.dsp("x").is_err());
  }

  #[test]
  fn abrupt_socket_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".socket.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let held = std::thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let mut buf = [0u8; 128];
      let _ = stream.read(&mut buf);
    });
    let ipc = Ipc { cmd_socket: path };
    let res = ipc.send_cmd(&Command {
      command: "version".into(),
      flags: CommandFlags::empty(),
    });
    assert_eq!(res.unwrap(), "");
    drop(held.join());
  }
}
