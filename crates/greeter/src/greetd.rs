use std::{env, os::unix::net::UnixStream, path::Path};

use anyhow::{Context, Result, bail};
use freedesktop_desktop_entry::{DesktopEntry, get_languages_from_env};
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response, codec::SyncCodec};

/// A Wayland session from a `wayland-sessions/*.desktop` file
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
  pub name: String,
  pub cmd: Vec<String>,
  pub env: Vec<String>,
}

/// The sessions in `$XDG_DATA_DIRS/wayland-sessions`, by name
pub fn sessions() -> Vec<Session> {
  let dirs = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
  find(
    dirs
      .split(':')
      .map(|dir| Path::new(dir).join("wayland-sessions")),
  )
}

/// An earlier dir's file hides a later one of the same name, as XDG says
fn find(dirs: impl Iterator<Item = impl AsRef<Path>>) -> Vec<Session> {
  let locales = get_languages_from_env();
  let mut seen = std::collections::HashSet::new();
  let mut sessions: Vec<_> = dirs
    .filter_map(|dir| std::fs::read_dir(dir).ok())
    .flat_map(|files| files.flatten().map(|file| file.path()))
    .filter(|path| path.extension().is_some_and(|e| e == "desktop"))
    .filter(|path| seen.insert(path.file_name().map(ToOwned::to_owned)))
    .filter_map(|path| {
      let entry = DesktopEntry::from_path(&path, Some(&locales)).ok()?;
      if entry.hidden() || entry.no_display() {
        return None;
      }
      let id = path.file_stem()?.to_string_lossy().into_owned();
      let desktops = entry.desktop_entry("DesktopNames").unwrap_or(&id);
      Some(Session {
        name: entry.name(&locales)?.into_owned(),
        cmd: ["systemd-cat", "-t", &id]
          .into_iter()
          .map(String::from)
          .chain(entry.parse_exec().ok()?)
          .collect(),
        env: vec![
          "XDG_SESSION_TYPE=wayland".into(),
          format!("XDG_SESSION_DESKTOP={id}"),
          format!(
            "XDG_CURRENT_DESKTOP={}",
            desktops.trim_end_matches(';').replace(';', ":")
          ),
        ],
      })
    })
    .collect();
  sessions.sort_by(|a, b| a.name.cmp(&b.name));
  sessions
}

/// The first account people log in with: a uid from 1000 below 60000 and a login shell.
/// ponytail: one user only, a user picker when a machine has more
pub fn first_user(passwd: &str) -> Option<String> {
  passwd.lines().find_map(|line| {
    let fields: Vec<_> = line.split(':').collect();
    let uid: u32 = fields.get(2)?.parse().ok()?;
    let shell = fields.get(6)?;
    let login = !shell.ends_with("nologin") && !shell.ends_with("false");
    ((1000..60000).contains(&uid) && login).then(|| fields[0].to_string())
  })
}

/// One login conversation: `Ok(false)` for a wrong password. On success greetd
/// starts `session` once the greeter exits. Blocks, PAM sleeps on a failure
pub fn login(user: String, password: String, session: Session) -> Result<bool> {
  let socket = env::var("GREETD_SOCK").context("GREETD_SOCK not set, not run by greetd")?;
  let mut stream = UnixStream::connect(socket)?;
  let mut send = |request: Request| -> Result<Response> {
    request.write_to(&mut stream)?;
    Ok(Response::read_from(&mut stream)?)
  };

  let mut response = send(Request::CreateSession { username: user })?;
  let mut started = false;
  loop {
    response = match response {
      Response::AuthMessage {
        auth_message_type, ..
      } => {
        let answer = match auth_message_type {
          AuthMessageType::Secret | AuthMessageType::Visible => Some(password.clone()),
          AuthMessageType::Info | AuthMessageType::Error => None,
        };
        send(Request::PostAuthMessageResponse { response: answer })?
      }
      Response::Success if started => return Ok(true),
      Response::Success => {
        started = true;
        send(Request::StartSession {
          cmd: session.cmd.clone(),
          env: session.env.clone(),
        })?
      }
      Response::Error {
        error_type,
        description,
      } => {
        // or the next CreateSession is refused
        let _ = send(Request::CancelSession);
        match error_type {
          ErrorType::AuthError => return Ok(false),
          ErrorType::Error => bail!("greetd: {description}"),
        }
      }
    };
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_sessions() {
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let write = |dir: &tempfile::TempDir, file: &str, text: &str| {
      std::fs::write(dir.path().join(file), text).unwrap()
    };
    write(
      &first,
      "hyprland.desktop",
      "[Desktop Entry]\nName=Hyprland\nExec=start-hyprland --flag %U\nDesktopNames=Hyprland;wlroots;\n",
    );
    write(&first, "x.txt", "not a session");
    write(
      &first,
      "hidden.desktop",
      "[Desktop Entry]\nName=Hidden\nExec=x\nHidden=true\n",
    );
    // hidden by the first dir's file
    write(
      &second,
      "hyprland.desktop",
      "[Desktop Entry]\nName=Other\nExec=other\n",
    );
    write(
      &second,
      "cage.desktop",
      "[Desktop Entry]\nName=Cage\nExec=cage foot\n",
    );

    let sessions = find([first.path(), second.path(), Path::new("/nonexistent")].into_iter());
    let names: Vec<_> = sessions.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Cage", "Hyprland"]);
    assert_eq!(
      sessions[1].cmd,
      ["systemd-cat", "-t", "hyprland", "start-hyprland", "--flag"]
    );
    assert_eq!(sessions[1].env[1], "XDG_SESSION_DESKTOP=hyprland");
    assert_eq!(sessions[1].env[2], "XDG_CURRENT_DESKTOP=Hyprland:wlroots");
    assert_eq!(sessions[0].env[2], "XDG_CURRENT_DESKTOP=cage");
  }

  #[test]
  fn picks_a_person() {
    let passwd = "root:x:0:0::/root:/bin/sh\n\
      nixbld1:x:30001:30000::/var/empty:/run/current-system/sw/bin/nologin\n\
      greeter:x:999:999::/var/empty:/bin/false\n\
      test:x:1000:100::/home/test:/run/current-system/sw/bin/bash\n\
      nobody:x:65534:65534::/var/empty:/run/current-system/sw/bin/nologin\n";
    assert_eq!(first_user(passwd).as_deref(), Some("test"));
    assert_eq!(first_user("root:x:0:0::/root:/bin/sh"), None);
  }
}
