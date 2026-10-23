//! Installed apps, their desktop actions, browsing and rescanning

use std::{path::PathBuf, rc::Rc, sync::Arc, time::Duration};

use anyhow::Result;
use corona_compositor::CompositorExt;
use corona_config::ConfigProvider;
use corona_desktop::apps::{self, App as DesktopApp};
use gpui_kit::{App, AppContext, Entity, Global, SharedString, Subscription, assets::IconName};
use notify::{RecursiveMode, Watcher};
use rust_i18n::t;

use crate::{
  command::{
    Accessory, Action, Clip, Command, CommandSource, Hud, Icon, Meta, Outcome, Section, Tone,
    simple,
  },
  matching::Query,
  registry,
  view::ListView,
};

const DEBOUNCE: Duration = Duration::from_millis(500);

/// The scanned apps; bumped when the application directories change
struct Apps {
  list: Entity<Arc<Vec<DesktopApp>>>,
}

impl Global for Apps {}

pub fn init(cx: &mut App) {
  let list = cx.new(|_| apps::apps());
  cx.set_global(Apps { list: list.clone() });
  watch(list, cx);
  registry::register_source(Rc::new(AppsSource { browse: false }), cx);
  registry::register_all([browse(), refresh()], cx);
}

fn watch(list: Entity<Arc<Vec<DesktopApp>>>, cx: &mut App) {
  let (tx, rx) = flume::unbounded();
  let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
    if event.is_ok_and(|e| !e.kind.is_access()) {
      let _ = tx.send(());
    }
  });
  let mut watcher = match watcher {
    Ok(watcher) => watcher,
    Err(e) => return tracing::warn!("not watching desktop entries: {e}"),
  };
  for dir in apps::directories() {
    let _ = watcher.watch(&dir, RecursiveMode::Recursive);
  }
  cx.spawn(async move |cx| {
    let _watcher = watcher;
    while rx.recv_async().await.is_ok() {
      cx.background_executor().timer(DEBOUNCE).await;
      rx.drain();
      let scanned = cx
        .background_executor()
        .spawn(async { apps::refresh() })
        .await;
      list.update(cx, |list, cx| {
        *list = scanned;
        cx.notify();
      });
    }
  })
  .detach();
}

fn list(cx: &App) -> Arc<Vec<DesktopApp>> {
  cx.global::<Apps>().list.read(cx).clone()
}

/// The first open window of `app`, by class
fn running_window(app: &DesktopApp, cx: &App) -> Option<String> {
  let classes = app.window_classes();
  cx.compositor()
    .list_windows(cx)
    .iter()
    .find(|w| classes.contains(&w.class.to_lowercase()))
    .map(|w| w.address.clone())
}

fn launch(app: &DesktopApp, argv: &[String], cx: &App) -> Result<Outcome> {
  let terminal = &cx.config().launcher.terminal;
  let argv = apps::command_for(app, argv, terminal);
  apps::spawn(&app.id, &argv, app.working_dir.as_deref())?;
  Ok(Outcome::close())
}

struct AppCommand {
  app: DesktopApp,
}

impl AppCommand {
  fn icon(&self) -> Icon {
    Icon::App(self.app.icon_names().into_iter().map(Into::into).collect())
  }
}

impl Command for AppCommand {
  fn id(&self) -> SharedString {
    format!("app:{}", self.app.id).into()
  }

  fn meta(&self, cx: &App) -> Meta {
    let app = &self.app;
    let mut keywords: Vec<SharedString> = app
      .keywords
      .iter()
      .chain(app.generic_name.iter())
      .cloned()
      .map(Into::into)
      .collect();
    keywords.extend(app.window_classes().into_iter().map(Into::into));
    let running = running_window(app, cx).is_some();
    Meta {
      title: app.name.clone().into(),
      subtitle: app
        .generic_name
        .clone()
        .or(app.comment.clone())
        .map(Into::into),
      icon: self.icon(),
      keywords,
      section: Section::Applications,
      accessories: match running {
        true => vec![Accessory::Tag(
          t!("app.launcher.apps.running").into(),
          Tone::Off,
        )],
        false => Vec::new(),
      },
      ..Default::default()
    }
  }

  fn actions(&self, cx: &App) -> Vec<Action> {
    let app = self.app.clone();
    let mut actions = Vec::new();
    if let Some(address) = running_window(&app, cx) {
      actions.push(
        Action::sync("focus", t!("app.launcher.apps.focus"), move |_, _, cx| {
          cx.compositor().focus_window(&address)?;
          Ok(Outcome::close())
        })
        .icon(IconName::AppWindow),
      );
    }
    let launched = app.clone();
    actions.push(
      Action::sync(
        "launch",
        match actions.is_empty() {
          true => t!("app.launcher.apps.launch"),
          false => t!("app.launcher.apps.new_instance"),
        },
        move |_, _, cx| launch(&launched, &launched.exec, cx),
      )
      .icon(IconName::Rocket),
    );
    for action in &app.actions {
      let (app, argv) = (app.clone(), action.exec.clone());
      actions.push(Action::sync(
        format!("action:{}", action.id),
        action.name.clone(),
        move |_, _, cx| launch(&app, &argv, cx),
      ));
    }
    let exec = app.exec.join(" ");
    actions.push(
      Action::sync(
        "copy_exec",
        t!("app.launcher.apps.copy_exec"),
        move |_, _, _| Ok(Outcome::Copy(Clip::Text(exec.clone()))),
      )
      .icon(IconName::Copy),
    );
    let path: PathBuf = app.path.clone();
    actions.push(
      Action::sync("reveal", t!("app.launcher.apps.reveal"), move |_, _, cx| {
        cx.reveal_path(&path);
        Ok(Outcome::close())
      })
      .icon(IconName::FolderOpen),
    );
    actions
  }
}

/// `App › action` as its own result, like "Firefox › New private window"
struct DesktopActionCommand {
  app: DesktopApp,
  index: usize,
}

impl Command for DesktopActionCommand {
  fn id(&self) -> SharedString {
    let action = &self.app.actions[self.index];
    format!("app:{}:{}", self.app.id, action.id).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let action = &self.app.actions[self.index];
    Meta {
      title: format!("{} › {}", self.app.name, action.name).into(),
      icon: Icon::App(self.app.icon_names().into_iter().map(Into::into).collect()),
      keywords: vec![action.name.clone().into()],
      section: Section::Applications,
      ..Default::default()
    }
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (app, argv) = (self.app.clone(), self.app.actions[self.index].exec.clone());
    vec![Action::sync(
      "launch",
      t!("app.launcher.apps.launch"),
      move |_, _, cx| launch(&app, &argv, cx),
    )]
  }
}

/// Apps and their desktop actions; `browse` also lists `NoDisplay` apps
struct AppsSource {
  browse: bool,
}

impl CommandSource for AppsSource {
  fn id(&self) -> SharedString {
    match self.browse {
      true => "apps:browse".into(),
      false => "apps".into(),
    }
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let mut commands: Vec<Rc<dyn Command>> = Vec::new();
    for app in list(cx).iter().filter(|a| self.browse || !a.no_display) {
      commands.push(Rc::new(AppCommand { app: app.clone() }));
      // desktop actions only once typing, they would crowd the empty list
      if !self.browse && !query.is_empty() {
        for index in 0..app.actions.len() {
          commands.push(Rc::new(DesktopActionCommand {
            app: app.clone(),
            index,
          }));
        }
      }
    }
    commands
  }

  fn observe(&self, notify: Rc<dyn Fn(&mut App)>, cx: &mut App) -> Vec<Subscription> {
    let list = cx.global::<Apps>().list.clone();
    let windows = cx.compositor().windows.clone();
    let again = notify.clone();
    vec![
      cx.observe(&list, move |_, cx| notify(cx)),
      cx.observe(&windows, move |_, cx| again(cx)),
    ]
  }
}

fn browse() -> Rc<dyn Command> {
  simple(
    "apps:browse",
    |_| Meta {
      title: t!("app.launcher.apps.browse").into(),
      icon: Icon::Tile(IconName::LayoutGrid, 220.),
      keywords: vec!["applications".into(), "programs".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::sync("open", t!("app.launcher.open"), |_, _, _| {
        Ok(Outcome::Push(
          ListView::new(
            t!("app.launcher.apps.browse"),
            t!("app.launcher.apps.search"),
            Rc::new(AppsSource { browse: true }),
          )
          .grid(6, 92., true)
          .into_view(),
        ))
      })]
    },
  )
}

fn refresh() -> Rc<dyn Command> {
  simple(
    "apps:refresh",
    |_| Meta {
      title: t!("app.launcher.apps.refresh").into(),
      icon: Icon::Tile(IconName::RefreshCw, 220.),
      keywords: vec!["rescan".into(), "reload".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::spawn(
        "refresh",
        t!("app.launcher.apps.refresh"),
        |_, cx| {
          let list = cx.global::<Apps>().list.clone();
          let scan = cx.background_executor().spawn(async { apps::refresh() });
          let cx = cx.to_async();
          async move {
            let scanned = scan.await;
            let count = scanned.len();
            list.update(&mut cx.clone(), |list, cx| {
              *list = scanned;
              cx.notify();
            });
            Ok(Outcome::Close(Some(Hud::new(t!(
              "app.launcher.apps.refreshed",
              count = count
            )))))
          }
        },
      )]
    },
  )
}
