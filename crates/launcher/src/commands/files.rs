//! File search over an index of the configured folders, `~/` and `/` path browsing,
//! recent files and the file actions

use std::{
  path::{Path, PathBuf},
  rc::Rc,
  sync::{Arc, RwLock},
  time::{Duration, Instant, SystemTime},
};

use corona_config::{ConfigProvider, expand_home, launcher::FilesConfig};
use corona_desktop::apps;
use gpui_kit::{App, AppContext, Entity, Global, SharedString, assets::IconName};
use nucleo_matcher::{
  Config, Matcher, Utf32Str,
  pattern::{CaseMatching, Normalization, Pattern},
};
use rust_i18n::t;

use crate::{
  command::{
    Action, Clip, Command, CommandSource, Detail, DetailBody, Icon, Meta, Outcome, Scope, Section,
    SourceKind,
  },
  kit::{ListSource, opener, watch},
  matching::Query,
  registry, simple,
  view::ListView,
};

const RESULTS: usize = 100;
const MAX_FILES: usize = 500_000;
/// ponytail: the index is rebuilt when a search opens and it is older than this, not watched;
/// inotify on a whole home runs into watch limits
const STALE: Duration = Duration::from_secs(120);

struct FileIndex {
  files: Arc<RwLock<Vec<PathBuf>>>,
  built: Option<Instant>,
  /// True while a walk runs, for the loading bar
  building: Entity<bool>,
}

impl Global for FileIndex {}

fn roots(config: &FilesConfig) -> Vec<PathBuf> {
  config
    .roots
    .iter()
    .map(|r| expand_home(Path::new(r)))
    .filter(|r| r.is_dir())
    .collect()
}

fn walk(config: &FilesConfig) -> Vec<PathBuf> {
  let roots = roots(config);
  let Some((first, rest)) = roots.split_first() else {
    return Vec::new();
  };
  let mut builder = ignore::WalkBuilder::new(first);
  for root in rest {
    builder.add(root);
  }
  let excludes = config.excludes.clone();
  builder
    .hidden(!config.hidden)
    .git_ignore(config.respect_ignore)
    .ignore(config.respect_ignore)
    .git_global(config.respect_ignore)
    .parents(false)
    .filter_entry(move |e| {
      let name = e.file_name().to_string_lossy();
      !excludes.iter().any(|x| *x == name)
    });
  builder
    .build()
    .filter_map(Result::ok)
    .filter(|e| e.depth() > 0)
    .map(|e| e.into_path())
    .take(MAX_FILES)
    .collect()
}

/// Rebuilds the index in the background when it is missing or stale
fn refresh_index(cx: &mut App) {
  let index = cx.global::<FileIndex>();
  if *index.building.read(cx) || index.built.is_some_and(|b| b.elapsed() < STALE) {
    return;
  }
  let (files, building) = (index.files.clone(), index.building.clone());
  building.update(cx, |b, cx| {
    *b = true;
    cx.notify();
  });
  let config = cx.config().launcher.files.clone();
  let walked = cx.background_spawn(async move { walk(&config) });
  cx.spawn(async move |cx| {
    let walked = walked.await;
    if let Ok(mut files) = files.write() {
      *files = walked;
    }
    cx.update(|cx| {
      cx.global_mut::<FileIndex>().built = Some(Instant::now());
      building.update(cx, |b, cx| {
        *b = false;
        cx.notify();
      });
    });
  })
  .detach();
}

/// The best `limit` paths for `query` by their file name, then the whole path
fn search(files: &[PathBuf], query: &str, limit: usize) -> Vec<PathBuf> {
  let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
  let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
  let mut buf = Vec::new();
  let mut scored: Vec<(u32, &PathBuf)> = files
    .iter()
    .filter_map(|path| {
      let name = path.file_name()?.to_string_lossy();
      let by_name = pattern.score(Utf32Str::new(&name, &mut buf), &mut matcher);
      let score = match by_name {
        Some(s) => s + 1000,
        None => pattern.score(
          Utf32Str::new(&path.to_string_lossy(), &mut buf),
          &mut matcher,
        )?,
      };
      Some((score, path))
    })
    .collect();
  scored.sort_by(|a, b| {
    b.0
      .cmp(&a.0)
      .then_with(|| a.1.as_os_str().len().cmp(&b.1.as_os_str().len()))
  });
  scored
    .into_iter()
    .take(limit)
    .map(|(_, p)| p.clone())
    .collect()
}

/// `/home/me/x` as `~/x`
pub fn tilde(path: &Path) -> String {
  match dirs::home_dir().and_then(|h| path.strip_prefix(h).ok().map(Path::to_path_buf)) {
    Some(rest) if rest.as_os_str().is_empty() => "~".into(),
    Some(rest) => format!("~/{}", rest.display()),
    None => path.display().to_string(),
  }
}

fn icon_for(path: &Path) -> IconName {
  if path.is_dir() {
    return IconName::Folder;
  }
  match path
    .extension()
    .map(|e| e.to_string_lossy().to_lowercase())
    .as_deref()
  {
    Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp") => IconName::FileImage,
    Some("mp4" | "mkv" | "webm" | "mov") => IconName::FilePlay,
    Some("mp3" | "flac" | "ogg" | "wav" | "opus") => IconName::FileMusic,
    Some("zip" | "tar" | "gz" | "xz" | "zst" | "7z") => IconName::FileArchive,
    Some("rs" | "py" | "js" | "ts" | "c" | "h" | "cpp" | "go" | "nix" | "sh" | "toml" | "json") => {
      IconName::FileCode
    }
    Some("txt" | "md" | "org" | "log") => IconName::FileText,
    _ => IconName::File,
  }
}

fn is_image(path: &Path) -> bool {
  matches!(icon_for(path), IconName::FileImage)
}

fn size(bytes: u64) -> String {
  const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
  let mut value = bytes as f64;
  let mut unit = 0;
  while value >= 1024. && unit < UNITS.len() - 1 {
    value /= 1024.;
    unit += 1;
  }
  format!("{value:.1} {}", UNITS[unit])
}

fn modified(time: SystemTime) -> String {
  jiff::Timestamp::try_from(time)
    .map(|ts| {
      ts.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
    })
    .unwrap_or_default()
}

/// The actions every file has
pub fn file_actions(path: &Path) -> Vec<Action> {
  let p = path.to_path_buf();
  let (open, reveal, copy, uri, trash, with) =
    (p.clone(), p.clone(), p.clone(), p.clone(), p.clone(), p);
  vec![
    Action::sync("open", t!("app.launcher.files.open"), move |_, _, cx| {
      cx.open_with_system(&open);
      Ok(Outcome::close())
    }),
    Action::sync(
      "open_with",
      t!("app.launcher.files.open_with"),
      move |_, _, _| Ok(Outcome::Push(open_with(with.clone()))),
    )
    .icon(IconName::AppWindow),
    Action::sync(
      "reveal",
      t!("app.launcher.files.reveal"),
      move |_, _, cx| {
        cx.reveal_path(&reveal);
        Ok(Outcome::close())
      },
    )
    .icon(IconName::FolderOpen)
    .shortcut("ctrl-enter"),
    Action::sync(
      "copy_path",
      t!("app.launcher.files.copy_path"),
      move |_, _, _| Ok(Outcome::Copy(Clip::Text(copy.display().to_string()))),
    )
    .icon(IconName::Copy)
    .shortcut("ctrl-shift-c"),
    Action::sync(
      "copy_file",
      t!("app.launcher.files.copy_file"),
      move |_, _, _| {
        Ok(Outcome::Copy(Clip::Text(format!(
          "file://{}",
          uri.display()
        ))))
      },
    )
    .icon(IconName::Files),
    Action::sync("trash", t!("app.launcher.files.trash"), move |_, _, _| {
      trash::delete(&trash)?;
      Ok(Outcome::hud(t!("app.launcher.files.trashed")))
    })
    .icon(IconName::Trash)
    .destructive(),
  ]
}

/// Apps to open the file with; the app gets the path after its command
fn open_with(path: PathBuf) -> crate::View {
  ListView::new(
    t!("app.launcher.files.open_with"),
    t!("app.launcher.apps.search"),
    Rc::new(ListSource::new("open_with", move |_, _| {
      apps::apps()
        .iter()
        .filter(|a| !a.no_display)
        .map(|app| {
          let (app, path) = (app.clone(), path.clone());
          let shown = app.clone();
          simple(
            format!("open_with:{}", app.id),
            move |_| Meta {
              title: shown.name.clone().into(),
              icon: Icon::App(shown.icon_names().into_iter().map(Into::into).collect()),
              ..Default::default()
            },
            move |_| {
              let (app, path) = (app.clone(), path.clone());
              vec![Action::sync(
                "open",
                t!("app.launcher.files.open"),
                move |_, _, cx| {
                  let mut argv = app.exec.clone();
                  argv.push(path.display().to_string());
                  let argv = apps::command_for(&app, &argv, &cx.config().launcher.terminal);
                  apps::spawn(&app.id, &argv, None)?;
                  Ok(Outcome::close())
                },
              )]
            },
          )
        })
        .collect()
    })),
  )
  .into_view()
}

pub struct FileRow {
  pub path: PathBuf,
  /// Tab puts this in the search field, for directories while browsing
  pub completion: Option<String>,
}

impl Command for FileRow {
  fn id(&self) -> SharedString {
    format!("file:{}", self.path.display()).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let name = self
      .path
      .file_name()
      .map(|n| n.to_string_lossy().to_string())
      .unwrap_or_else(|| self.path.display().to_string());
    Meta {
      title: name.into(),
      subtitle: self.path.parent().map(|p| tilde(p).into()),
      icon: match is_image(&self.path) {
        true => Icon::Image(self.path.as_path().into()),
        false => Icon::Named(icon_for(&self.path)),
      },
      section: Section::Results,
      ..Default::default()
    }
  }

  fn detail(&self, _: &App) -> Option<Detail> {
    let meta = std::fs::metadata(&self.path).ok()?;
    let body = if is_image(&self.path) {
      DetailBody::Image(self.path.as_path().into())
    } else if meta.is_file() && meta.len() < 1024 * 1024 {
      let mut text = std::fs::read(&self.path).ok()?;
      text.truncate(4096);
      match String::from_utf8(text) {
        Ok(text) => DetailBody::Code(text.into()),
        Err(_) => DetailBody::None,
      }
    } else {
      DetailBody::None
    };
    let mut metadata = vec![(
      t!("app.launcher.files.location").into(),
      tilde(self.path.parent().unwrap_or(&self.path)).into(),
    )];
    if meta.is_file() {
      metadata.push((
        t!("app.launcher.files.size").into(),
        size(meta.len()).into(),
      ));
    }
    if let Ok(time) = meta.modified() {
      metadata.push((
        t!("app.launcher.files.modified").into(),
        modified(time).into(),
      ));
    }
    Some(Detail {
      body,
      metadata,
      tags: Vec::new(),
    })
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    file_actions(&self.path)
  }

  fn complete(&self, _: &App) -> Option<String> {
    self.completion.clone()
  }
}

/// The index searched with the query
fn files_view(query: String) -> crate::View {
  ListView::new(
    t!("app.launcher.files.search_title"),
    t!("app.launcher.files.search"),
    Rc::new(
      ListSource::new("files", |query, cx| {
        if query.is_empty() {
          return Vec::new();
        }
        let index = cx.global::<FileIndex>();
        let Ok(files) = index.files.read() else {
          return Vec::new();
        };
        search(&files, &query.raw, RESULTS)
          .into_iter()
          .map(|path| {
            Rc::new(FileRow {
              path,
              completion: None,
            }) as Rc<dyn Command>
          })
          .collect()
      })
      .filters()
      .on_open(refresh_index)
      .loading(|cx| *cx.global::<FileIndex>().building.read(cx))
      .observe(|notify, cx| {
        let building = cx.global::<FileIndex>().building.clone();
        vec![watch(&building, &notify, cx)]
      }),
    ),
  )
  .preview()
  .query(query)
  .empty(t!("app.launcher.files.type"))
  .into_view()
}

/// Entries of the typed directory under `base`, matching what follows its last `/`
pub struct PathSource {
  base: PathBuf,
}

impl PathSource {
  /// The directory and the name filter for `typed`
  fn split(&self, typed: &str) -> (PathBuf, String) {
    match typed.rsplit_once('/') {
      Some((dir, rest)) => (self.base.join(dir), rest.to_string()),
      None => (self.base.clone(), typed.to_string()),
    }
  }
}

impl CommandSource for PathSource {
  fn id(&self) -> SharedString {
    format!("path:{}", self.base.display()).into()
  }

  fn filters(&self) -> bool {
    true
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let typed = query.raw.as_str();
    let (dir, filter) = self.split(typed);
    let filter = filter.to_lowercase();
    let Ok(read) = std::fs::read_dir(&dir) else {
      return Vec::new();
    };
    let mut entries: Vec<(bool, String, PathBuf)> = read
      .filter_map(Result::ok)
      .filter_map(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        let hidden = name.starts_with('.') && !filter.starts_with('.');
        (!hidden && lower.starts_with(&filter)).then(|| (!e.path().is_dir(), lower, e.path()))
      })
      .collect();
    entries.sort();
    let prefix = typed.rsplit_once('/').map_or("", |(dir, _)| dir);
    entries
      .into_iter()
      .take(RESULTS)
      .map(|(file, _, path)| {
        let name = path
          .file_name()
          .map(|n| n.to_string_lossy().to_string())
          .unwrap_or_default();
        let completion = (!file).then(|| match prefix.is_empty() {
          true => format!("{name}/"),
          false => format!("{prefix}/{name}/"),
        });
        Rc::new(FileRow { path, completion }) as Rc<dyn Command>
      })
      .collect()
  }
}

fn path_scope(id: &'static str, prefix: &'static str, base: PathBuf) -> Rc<dyn Command> {
  struct Browse {
    id: &'static str,
    prefix: &'static str,
    base: PathBuf,
  }
  impl Command for Browse {
    fn id(&self) -> SharedString {
      self.id.into()
    }
    fn meta(&self, _: &App) -> Meta {
      Meta {
        title: t!("app.launcher.files.browse", path = self.prefix).into(),
        icon: Icon::Tile(IconName::FolderTree, 50.),
        keywords: vec!["path".into(), "folder".into()],
        ..Default::default()
      }
    }
    fn actions(&self, _: &App) -> Vec<Action> {
      let base = self.base.clone();
      vec![Action::sync(
        "open",
        t!("app.launcher.files.open"),
        move |_, _, cx| {
          cx.open_with_system(&base);
          Ok(Outcome::close())
        },
      )]
    }
    fn scope(&self, _: &App) -> Option<Scope> {
      Some(Scope {
        prefix: self.prefix.into(),
        label: self.prefix.into(),
        placeholder: t!("app.launcher.files.path_placeholder").into(),
        source: Rc::new(PathSource {
          base: self.base.clone(),
        }),
      })
    }
  }
  Rc::new(Browse { id, prefix, base })
}

/// `href` and `modified` of each bookmark in `recently-used.xbel`, newest first
pub fn recent(xbel: &str) -> Vec<PathBuf> {
  let attr = |tag: &str, name: &str| -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
  };
  let mut found: Vec<(String, PathBuf)> = xbel
    .split("<bookmark ")
    .skip(1)
    .filter_map(|tag| {
      let tag = format!(" {}", &tag[..tag.find('>')?]);
      let href = attr(&tag, "href")?;
      let path = crate::commands::web::decode(href.strip_prefix("file://")?);
      let modified = attr(&tag, "modified").unwrap_or_default();
      Some((modified, PathBuf::from(path)))
    })
    .collect();
  found.sort_by(|a, b| b.0.cmp(&a.0));
  found.into_iter().map(|(_, p)| p).collect()
}

fn recent_view() -> crate::View {
  ListView::new(
    t!("app.launcher.files.recent"),
    t!("app.launcher.files.search"),
    Rc::new(ListSource::new("files:recent", |_, _| {
      let Some(path) = dirs::data_dir().map(|d| d.join("recently-used.xbel")) else {
        return Vec::new();
      };
      let text = std::fs::read_to_string(path).unwrap_or_default();
      recent(&text)
        .into_iter()
        .filter(|p| p.exists())
        .take(RESULTS)
        .map(|path| {
          Rc::new(FileRow {
            path,
            completion: None,
          }) as Rc<dyn Command>
        })
        .collect()
    })),
  )
  .preview()
  .empty(t!("app.launcher.files.no_recent"))
  .into_view()
}

/// "Search files for …" when nothing matched
struct FilesFallback;

impl CommandSource for FilesFallback {
  fn id(&self) -> SharedString {
    "fallback:files".into()
  }

  fn kind(&self) -> SourceKind {
    SourceKind::Fallback
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let text = query.raw.trim().to_string();
    let title = t!("app.launcher.files.search_for", query = text.clone());
    vec![simple(
      "fallback:files",
      move |_| Meta {
        title: title.clone().into(),
        icon: Icon::Tile(IconName::FileSearch, 50.),
        ..Default::default()
      },
      move |_| {
        let text = text.clone();
        vec![Action::sync(
          "search",
          t!("app.launcher.files.search_title"),
          move |_, _, _| Ok(Outcome::Push(files_view(text.clone()))),
        )]
      },
    )]
  }
}

pub fn init(cx: &mut App) {
  let building = cx.new(|_| false);
  cx.set_global(FileIndex {
    files: Arc::default(),
    built: None,
    building,
  });
  let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
  registry::register_all(
    [
      opener(
        "files:search",
        t!("app.launcher.files.search_title"),
        Icon::Tile(IconName::FileSearch, 50.),
        &["files", "find", "documents"],
        |_| files_view(String::new()),
      ),
      opener(
        "files:recent",
        t!("app.launcher.files.recent"),
        Icon::Tile(IconName::FileClock, 50.),
        &["files", "recent", "documents"],
        |_| recent_view(),
      ),
      path_scope("files:home", "~/", home),
      path_scope("files:root", "/", PathBuf::from("/")),
    ],
    cx,
  );
  registry::register_source(Rc::new(FilesFallback), cx);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn searches_names_first() {
    let files: Vec<PathBuf> = ["/a/notes/x.txt", "/b/report.pdf", "/c/rep/other.md"]
      .iter()
      .map(PathBuf::from)
      .collect();
    let found = search(&files, "report", 10);
    assert_eq!(found[0], PathBuf::from("/b/report.pdf"));
    assert!(search(&files, "zzzz", 10).is_empty());
  }

  #[test]
  fn reads_recent_files() {
    let xbel = r#"<xbel>
      <bookmark href="file:///home/a/old%20file.txt" added="x" modified="2026-01-01T00:00:00Z" visited="x">
      </bookmark>
      <bookmark href="file:///home/a/new.txt" modified="2026-05-01T00:00:00Z">
      </bookmark>
      <bookmark href="https://example.com" modified="2026-06-01T00:00:00Z"></bookmark>
    </xbel>"#;
    assert_eq!(
      recent(xbel),
      [
        PathBuf::from("/home/a/new.txt"),
        PathBuf::from("/home/a/old file.txt")
      ]
    );
  }

  #[test]
  fn browses_paths() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Docs")).unwrap();
    std::fs::write(dir.path().join("Docs/a.txt"), "x").unwrap();
    std::fs::write(dir.path().join("doc.md"), "x").unwrap();
    std::fs::write(dir.path().join(".hidden"), "x").unwrap();
    let source = PathSource {
      base: dir.path().to_path_buf(),
    };
    assert_eq!(
      source.split("Docs/a"),
      (dir.path().join("Docs"), "a".into())
    );
    assert_eq!(source.split("do"), (dir.path().to_path_buf(), "do".into()));
  }
}
