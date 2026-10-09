//! The `git` command line, run without a terminal and without ever asking for
//! credentials. Every call blocks, so callers run off the main thread.

use std::{
  ffi::OsString,
  fs,
  io::Read,
  path::Path,
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::plugin::paths::remove_tree_under;

const NETWORK: Duration = Duration::from_secs(60);
const LOCAL: Duration = Duration::from_secs(20);
/// The largest file [`show_file`] reads
pub const SHOW_LIMIT: usize = 4 * 1024 * 1024;

/// Whether `git` can be run at all
pub fn available() -> bool {
  Command::new("git")
    .arg("--version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|s| s.success())
}

/// A clone that has trees but fetches file contents only when they are read,
/// and no working tree. Not shallow: a shallow clone grafts later fetches on as
/// unrelated history.
pub fn clone_blobless(url: &str, dest: &Path) -> Result<()> {
  let dest_arg = dest.as_os_str().to_owned();
  run(
    None,
    [
      "clone".into(),
      "--origin".into(),
      "origin".into(),
      "--filter=blob:none".into(),
      "--no-checkout".into(),
      "--quiet".into(),
      url.into(),
      dest_arg,
    ],
    false,
    NETWORK,
    usize::MAX,
  )?;
  if !has_origin(dest) {
    bail!("cloned {url} but it has no `origin` remote");
  }
  Ok(())
}

/// Clones `location` into `repo`, or points an existing clone at it, so a
/// changed source never fetches from where it used to be. A broken clone is
/// cloned again; only ever below `parent`.
pub fn ensure_repo(parent: &Path, repo: &Path, location: &str) -> Result<()> {
  if repo.join(".git").exists() || repo.join("HEAD").exists() {
    if has_origin(repo) {
      return git(repo, &["remote", "set-url", "origin", location], LOCAL).map(drop);
    }
    remove_tree_under(parent, repo)?;
  }
  if let Some(dir) = repo.parent() {
    fs::create_dir_all(dir)?;
  }
  clone_blobless(location, repo)
}

fn has_origin(repo: &Path) -> bool {
  git(repo, &["remote", "get-url", "origin"], LOCAL).is_ok()
}

/// Updates the remote branches only; what is applied stays
pub fn fetch(repo: &Path) -> Result<()> {
  git(repo, &["fetch", "--quiet", "origin"], NETWORK).map(drop)
}

/// The newest fetched revision of the default branch
pub fn remote_head(repo: &Path) -> Result<String> {
  rev_parse(repo, "refs/remotes/origin/HEAD")
}

/// The applied revision
pub fn head(repo: &Path) -> Result<String> {
  rev_parse(repo, "HEAD")
}

fn rev_parse(repo: &Path, rev: &str) -> Result<String> {
  let out = git(repo, &["rev-parse", "--verify", "--quiet", rev], LOCAL)?;
  Ok(String::from_utf8(out)?.trim().to_string())
}

/// Moves the applied revision, without a working tree to update
pub fn set_head(repo: &Path, rev: &str) -> Result<()> {
  git(repo, &["update-ref", "HEAD", rev], LOCAL).map(drop)
}

/// The contents of `path` at `rev`, at most [`SHOW_LIMIT`] bytes. With
/// `local_only` a file not fetched yet is an error rather than a download.
pub fn show_file(repo: &Path, rev: &str, path: &str, local_only: bool) -> Result<Vec<u8>> {
  let spec = format!("{rev}:{path}");
  run(
    Some(repo),
    ["show".into(), spec.into()],
    local_only,
    LOCAL,
    SHOW_LIMIT,
  )
}

/// Whether `path` exists at `rev`; reads trees only, never file contents
pub fn has_path(repo: &Path, rev: &str, path: &str) -> bool {
  let spec = format!("{rev}:{path}");
  run(
    Some(repo),
    [
      "rev-parse".into(),
      "--verify".into(),
      "--quiet".into(),
      spec.into(),
    ],
    true,
    LOCAL,
    usize::MAX,
  )
  .is_ok()
}

/// Writes `subdir` at `rev` to `dest/<subdir>`, fetching only its files. The
/// clone itself keeps no checkout.
pub fn export_subdir(repo: &Path, rev: &str, subdir: &str, dest: &Path) -> Result<()> {
  fs::create_dir_all(dest)?;
  let index = dest.join(".git-index");
  let result = run_with(
    Some(repo),
    [
      "--work-tree".into(),
      dest.as_os_str().to_owned(),
      "checkout".into(),
      rev.into(),
      "--".into(),
      subdir.into(),
    ],
    &[("GIT_INDEX_FILE", index.as_os_str().to_owned())],
    false,
    NETWORK,
    usize::MAX,
  );
  fs::remove_file(&index).ok();
  result.map(drop)
}

fn git(repo: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
  run(
    Some(repo),
    args.iter().map(|a| OsString::from(*a)),
    false,
    timeout,
    usize::MAX,
  )
}

fn run(
  repo: Option<&Path>,
  args: impl IntoIterator<Item = OsString>,
  local_only: bool,
  timeout: Duration,
  limit: usize,
) -> Result<Vec<u8>> {
  run_with(repo, args, &[], local_only, timeout, limit)
}

/// Runs git with every way it could ask for credentials turned off, killed
/// after `timeout`. Reads at most `limit` bytes of output.
fn run_with(
  repo: Option<&Path>,
  args: impl IntoIterator<Item = OsString>,
  env: &[(&str, OsString)],
  local_only: bool,
  timeout: Duration,
  limit: usize,
) -> Result<Vec<u8>> {
  let args: Vec<OsString> = args.into_iter().collect();
  let label = args
    .iter()
    .map(|a| a.to_string_lossy())
    .collect::<Vec<_>>()
    .join(" ");
  let ssh = std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh".to_string());

  let mut command = Command::new("git");
  if let Some(repo) = repo {
    command.arg("-C").arg(repo);
  }
  command
    .args([
      "-c",
      "credential.interactive=false",
      "-c",
      "core.askPass=/bin/false",
      "-c",
      "http.lowSpeedLimit=1000",
      "-c",
      "http.lowSpeedTime=20",
    ])
    .args(&args)
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_ASKPASS", "/bin/false")
    .env("SSH_ASKPASS", "/bin/false")
    .env("SSH_ASKPASS_REQUIRE", "never")
    .env("GIT_SSH_COMMAND", format!("{ssh} -oBatchMode=yes"))
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  if local_only {
    command.env("GIT_NO_LAZY_FETCH", "1");
  }
  for (key, value) in env {
    command.env(key, value);
  }

  let started = Instant::now();
  let mut child = command
    .spawn()
    .context("git is not installed or cannot run")?;
  // read on threads: a full pipe would otherwise stall git until the timeout
  let mut stdout = child.stdout.take().context("git stdout")?;
  let mut stderr = child.stderr.take().context("git stderr")?;
  let out = thread::spawn(move || {
    let mut buf = Vec::new();
    let read = (&mut stdout)
      .take(limit.saturating_add(1) as u64)
      .read_to_end(&mut buf);
    // drain the rest so git can exit
    std::io::copy(&mut stdout, &mut std::io::sink()).ok();
    read.map(|_| buf)
  });
  let err = thread::spawn(move || {
    let mut buf = String::new();
    stderr.read_to_string(&mut buf).ok();
    buf
  });

  let deadline = started + timeout;
  let status = loop {
    if let Some(status) = child.try_wait()? {
      break status;
    }
    if Instant::now() > deadline {
      child.kill().ok();
      child.wait().ok();
      bail!("git {label} timed out after {}s", timeout.as_secs());
    }
    thread::sleep(Duration::from_millis(10));
  };
  let out = out.join().ok().context("git output")??;
  let err = err.join().unwrap_or_default();

  let elapsed = started.elapsed();
  if elapsed >= Duration::from_secs(1) {
    tracing::info!("git {label} took {elapsed:?}");
  } else {
    tracing::debug!("git {label} took {elapsed:?}");
  }
  if !status.success() {
    bail!("git {label} failed: {}", err.trim());
  }
  if out.len() > limit {
    bail!("git {label}: output larger than {limit} bytes");
  }
  Ok(out)
}

/// A git repository to fetch plugins from, for tests
#[cfg(test)]
pub mod fixture {
  use std::{fs, path::PathBuf, process::Command};

  pub struct Remote {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
  }

  impl Default for Remote {
    fn default() -> Self {
      Self::new()
    }
  }

  impl Remote {
    pub fn new() -> Self {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("remote");
      fs::create_dir_all(&path).unwrap();
      let remote = Self { _dir: dir, path };
      remote.git(&["init", "--quiet", "--initial-branch=main"]);
      // lets blobless clones filter, like a hosted repository does
      remote.git(&["config", "uploadpack.allowFilter", "true"]);
      remote.git(&["config", "uploadpack.allowAnySHA1InWant", "true"]);
      remote
    }

    pub fn url(&self) -> String {
      format!("file://{}", self.path.display())
    }

    pub fn write(&self, path: &str, contents: &str) -> &Self {
      let path = self.path.join(path);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, contents).unwrap();
      self
    }

    pub fn remove(&self, path: &str) -> &Self {
      let path = self.path.join(path);
      if path.is_dir() {
        fs::remove_dir_all(path).unwrap();
      } else {
        fs::remove_file(path).unwrap();
      }
      self
    }

    /// Commits everything, returns the revision
    pub fn commit(&self) -> String {
      self.git(&["add", "-A"]);
      self.git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        "c",
      ]);
      self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    pub fn git(&self, args: &[&str]) -> String {
      let out = Command::new("git")
        .arg("-C")
        .arg(&self.path)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
      assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
      );
      String::from_utf8(out.stdout).unwrap()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{fixture::Remote, *};

  fn cloned(remote: &Remote) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("src/repo");
    ensure_repo(tmp.path(), &repo, &remote.url()).unwrap();
    (tmp, repo)
  }

  #[test]
  fn clones_without_a_checkout() {
    let remote = Remote::new();
    remote.write("a/plugin.toml", "id = \"a\"\n");
    let rev = remote.commit();
    let (_tmp, repo) = cloned(&remote);
    assert_eq!(head(&repo).unwrap(), rev);
    assert_eq!(remote_head(&repo).unwrap(), rev);
    assert!(!repo.join("a").exists());
    assert!(has_path(&repo, &rev, "a/plugin.toml"));
    assert!(!has_path(&repo, &rev, "b/plugin.toml"));
    assert_eq!(
      show_file(&repo, "HEAD", "a/plugin.toml", false).unwrap(),
      b"id = \"a\"\n"
    );
    assert!(show_file(&repo, "HEAD", "missing", false).is_err());
  }

  #[test]
  fn fetch_moves_the_remote_head_only() {
    let remote = Remote::new();
    remote.write("a/plugin.toml", "1");
    let first = remote.commit();
    let (_tmp, repo) = cloned(&remote);
    remote.write("a/plugin.toml", "2");
    let second = remote.commit();

    fetch(&repo).unwrap();
    assert_eq!(head(&repo).unwrap(), first);
    assert_eq!(remote_head(&repo).unwrap(), second);
    assert_eq!(
      show_file(&repo, &second, "a/plugin.toml", false).unwrap(),
      b"2"
    );

    set_head(&repo, &second).unwrap();
    assert_eq!(head(&repo).unwrap(), second);
  }

  #[test]
  fn ensure_repo_rebinds_and_heals() {
    let (one, two) = (Remote::new(), Remote::new());
    one.write("x", "one");
    one.commit();
    two.write("x", "two");
    let two_rev = two.commit();
    let (tmp, repo) = cloned(&one);

    ensure_repo(tmp.path(), &repo, &two.url()).unwrap();
    fetch(&repo).unwrap();
    assert_eq!(remote_head(&repo).unwrap(), two_rev);

    // a directory that is no clone is replaced by one
    fs::remove_dir_all(&repo).unwrap();
    fs::create_dir_all(repo.join(".git")).unwrap();
    ensure_repo(tmp.path(), &repo, &two.url()).unwrap();
    assert_eq!(head(&repo).unwrap(), two_rev);
  }

  #[test]
  fn exports_one_subdir() {
    let remote = Remote::new();
    remote
      .write("a/plugin.toml", "a")
      .write("a/assets/x.js", "x")
      .write("b/plugin.toml", "b");
    let rev = remote.commit();
    let (tmp, repo) = cloned(&remote);
    let dest = tmp.path().join("export");
    export_subdir(&repo, &rev, "a", &dest).unwrap();
    assert_eq!(fs::read_to_string(dest.join("a/plugin.toml")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dest.join("a/assets/x.js")).unwrap(), "x");
    assert!(!dest.join("b").exists());
    assert!(!dest.join(".git-index").exists());
    assert!(export_subdir(&repo, &rev, "missing", &tmp.path().join("e2")).is_err());
  }

  #[test]
  fn show_is_capped() {
    let remote = Remote::new();
    remote.write("big", &"x".repeat(SHOW_LIMIT + 1));
    remote.commit();
    let (_tmp, repo) = cloned(&remote);
    let e = show_file(&repo, "HEAD", "big", false).unwrap_err();
    assert!(e.to_string().contains("larger"), "{e}");
  }

  #[test]
  fn unreachable_remote_fails_without_asking() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let e = ensure_repo(tmp.path(), &repo, "file:///nonexistent/repo").unwrap_err();
    assert!(e.to_string().contains("failed"), "{e}");
    assert!(available());
  }
}
