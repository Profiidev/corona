use std::{env, os::unix::net::UnixStream, path::Path};

use anyhow::{Context, Result, bail};
use freedesktop_desktop_entry::{DesktopEntry, get_languages_from_env};
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response, codec::SyncCodec};

/// A Wayland session from a `wayland-sessions/*.desktop` file
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
  /// The file name without `.desktop`
  pub id: String,
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
        id: id.clone(),
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

/// The accounts people log in with: a uid from 1000 below 60000 and a login
/// shell, in passwd order
pub fn users(passwd: &str) -> Vec<String> {
  passwd
    .lines()
    .filter_map(|line| {
      let fields: Vec<_> = line.split(':').collect();
      let uid: u32 = fields.get(2)?.parse().ok()?;
      let shell = fields.get(6)?;
      let login = !shell.ends_with("nologin") && !shell.ends_with("false");
      ((1000..60000).contains(&uid) && login).then(|| fields[0].to_string())
    })
    .collect()
}

/// One login conversation: `Ok(false)` for a wrong password. On success greetd
/// starts `session` once the greeter exits. Blocks, PAM sleeps on a failure
pub fn login(user: String, password: String, session: Session) -> Result<bool> {
  let socket = env::var("GREETD_SOCK").context("GREETD_SOCK not set, not run by greetd")?;
  converse(UnixStream::connect(socket)?, user, password, session)
}

fn converse(
  mut stream: UnixStream,
  user: String,
  password: String,
  session: Session,
) -> Result<bool> {
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
    assert_eq!(sessions[1].id, "hyprland");
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
      dummy:x:1001:100::/home/dummy:/run/current-system/sw/bin/bash\n\
      nobody:x:65534:65534::/var/empty:/run/current-system/sw/bin/nologin\n";
    assert_eq!(users(passwd), ["test", "dummy"]);
    assert!(users("root:x:0:0::/root:/bin/sh").is_empty());
  }

  /// greetd answering each request with the next of `responses`, the requests
  /// it got as their debug text
  fn greetd(responses: Vec<Response>) -> (UnixStream, std::thread::JoinHandle<Vec<String>>) {
    let (ours, mut theirs) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
      let mut requests = Vec::new();
      for response in responses {
        let Ok(request) = Request::read_from(&mut theirs) else {
          break;
        };
        requests.push(format!("{request:?}"));
        response.write_to(&mut theirs).unwrap();
      }
      requests
    });
    (ours, server)
  }

  fn session() -> Session {
    Session {
      id: "hyprland".into(),
      name: "Hyprland".into(),
      cmd: vec!["start-hyprland".into()],
      env: vec!["XDG_SESSION_TYPE=wayland".into()],
    }
  }

  fn message(auth_message_type: AuthMessageType) -> Response {
    Response::AuthMessage {
      auth_message_type,
      auth_message: "".into(),
    }
  }

  fn error(error_type: ErrorType) -> Response {
    Response::Error {
      error_type,
      description: "nope".into(),
    }
  }

  fn run(responses: Vec<Response>) -> (Result<bool>, Vec<String>) {
    let (stream, server) = greetd(responses);
    let result = converse(stream, "alice".into(), "hunter2".into(), session());
    (result, server.join().unwrap())
  }

  #[test]
  fn logs_in_and_starts_the_session() {
    let (result, requests) = run(vec![
      message(AuthMessageType::Info),
      message(AuthMessageType::Secret),
      Response::Success,
      Response::Success,
    ]);
    assert!(result.unwrap());
    assert!(requests[0].contains("CreateSession") && requests[0].contains("alice"));
    // info gets no answer, the password prompt the password
    assert!(requests[1].contains("response: None"), "{}", requests[1]);
    assert!(requests[2].contains("Some(\"hunter2\")"), "{}", requests[2]);
    assert!(requests[3].contains("StartSession") && requests[3].contains("start-hyprland"));
    assert!(requests[3].contains("XDG_SESSION_TYPE=wayland"));
  }

  #[test]
  fn wrong_password_cancels() {
    let (result, requests) = run(vec![
      message(AuthMessageType::Secret),
      error(ErrorType::AuthError),
      Response::Success,
    ]);
    assert!(!result.unwrap());
    assert_eq!(requests.last().unwrap(), "CancelSession");
  }

  #[test]
  fn other_errors_fail_and_cancel() {
    let (result, requests) = run(vec![error(ErrorType::Error), Response::Success]);
    assert!(result.unwrap_err().to_string().contains("nope"));
    assert_eq!(requests.last().unwrap(), "CancelSession");
  }

  #[test]
  fn greetd_hanging_up_is_an_error() {
    let (result, _) = run(vec![]);
    assert!(result.is_err());
  }
}
