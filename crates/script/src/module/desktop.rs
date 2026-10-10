//! `corona/desktop`: the file picker and the user's default apps.

use std::{
  fs,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, ensure};
use corona_macros::named;
use gpui_kit::{App, PathPromptOptions};
use gpui_shell::HostModule;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::host_fn::Module;

#[derive(Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct PickFiles {
  multiple: Option<bool>,
  /// Pick directories instead of files.
  directory: Option<bool>,
  /// The prompt on the dialog.
  accept_label: Option<String>,
}

/// `path` as it is opened: an existing absolute path, neither a launcher nor
/// an executable, which would run instead of open.
fn openable(path: &Path) -> Result<PathBuf> {
  ensure!(path.is_absolute(), "`{}` is not absolute", path.display());
  let path = fs::canonicalize(path).with_context(|| path.display().to_string())?;
  ensure!(
    !path
      .extension()
      .is_some_and(|ext| ext.eq_ignore_ascii_case("desktop")),
    "`{}` is a launcher",
    path.display()
  );
  let meta = fs::metadata(&path)?;
  ensure!(
    !meta.is_file() || meta.permissions().mode() & 0o111 == 0,
    "`{}` is executable",
    path.display()
  );
  Ok(path)
}

/// A URI with a scheme, not a `file:` one, which `openPath` checks.
fn uri_ok(uri: &str) -> Result<()> {
  let scheme = uri
    .split_once(':')
    .map(|(scheme, _)| scheme)
    .filter(|scheme| {
      scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
          .chars()
          .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    })
    .with_context(|| format!("`{uri}` has no scheme"))?;
  ensure!(
    !scheme.eq_ignore_ascii_case("file"),
    "open files with openPath"
  );
  Ok(())
}

pub fn module() -> HostModule {
  Module::new("corona/desktop")
    .func(named!(
      "pickFiles",
      /// Asks the user for files, null when they cancel. Reading them still
      /// needs an `fs` grant.
      |cx: &mut App, options: Option<PickFiles>| {
        let options = options.unwrap_or_default();
        let directory = options.directory.unwrap_or(false);
        let paths = cx.prompt_for_paths(PathPromptOptions {
          files: !directory,
          directories: directory,
          multiple: options.multiple.unwrap_or(false),
          prompt: options.accept_label.map(Into::into),
        });
        async move {
          let paths = paths.await??;
          anyhow::Ok(paths.map(|paths| {
            paths
              .into_iter()
              .map(|path| path.to_string_lossy().into_owned())
              .collect::<Vec<_>>()
          }))
        }
      }
    ))
    .func(named!(
      "openPath",
      /// Opens a file or directory in the user's default app.
      |cx: &mut App, path: String| -> Result<()> {
        cx.open_with_system(&openable(Path::new(&path))?);
        Ok(())
      }
    ))
    .func(named!(
      "openUri",
      /// Opens a URI, e.g. `https:` or `mailto:`, in the user's default app.
      |cx: &mut App, uri: String| -> Result<()> {
        uri_ok(&uri)?;
        cx.open_url(&uri);
        Ok(())
      }
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::module::harness;

  #[gpui::test]
  fn refused_uris_never_reach_the_platform(cx: &mut TestAppContext) {
    let body = r#"report([m.openUri("file:///etc/passwd"), m.openPath("a.txt")]);"#;
    let (view, cx) = harness::view(cx, body, |_, _, _| module());
    let reports = view.last();
    assert!(reports[0]["message"].as_str().unwrap().contains("openPath"));
    assert!(
      reports[1]["message"]
        .as_str()
        .unwrap()
        .contains("not absolute")
    );
    assert_eq!(cx.opened_url(), None);
  }

  #[gpui::test]
  fn opens_uris(cx: &mut TestAppContext) {
    let (view, cx) = harness::view(cx, r#"report(m.openUri("mailto:a@b.c"));"#, |_, _, _| {
      module()
    });
    assert_eq!(view.last(), serde_json::Value::Null);
    assert_eq!(cx.opened_url().as_deref(), Some("mailto:a@b.c"));
  }

  #[test]
  fn openable_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "").unwrap();
    assert!(openable(&file).is_ok());
    assert!(openable(dir.path()).is_ok());

    assert!(openable(Path::new("a.txt")).is_err());
    assert!(openable(&dir.path().join("missing")).is_err());

    let launcher = dir.path().join("a.DESKTOP");
    fs::write(&launcher, "").unwrap();
    assert!(openable(&launcher).is_err());
    // nor through a link
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&launcher, &link).unwrap();
    assert!(openable(&link).is_err());

    let script = dir.path().join("run.sh");
    fs::write(&script, "").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(openable(&script).is_err());
  }

  #[test]
  fn uris() {
    for ok in [
      "https://example.com",
      "mailto:a@b.c",
      "kdeconnect://x",
      "a+b.c-d:x",
    ] {
      assert!(uri_ok(ok).is_ok(), "{ok}");
    }
    for bad in [
      "file:///etc/passwd",
      "FILE:/x",
      "example.com",
      ":x",
      "1a:x",
      "a b:x",
      "",
    ] {
      assert!(uri_ok(bad).is_err(), "{bad}");
    }
  }
}
