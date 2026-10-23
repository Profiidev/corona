//! Open windows and workspaces: switch, close, go to

use std::rc::Rc;

use corona_compositor::{Compositor, CompositorExt, types};
use corona_launcher::{
  Accessory, Action, ArgSpec, Command, Icon, ListView, Meta, Outcome, Query,
  kit::{ListSource, opener, watch},
  registry, simple,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

fn window_row(window: &types::Window, close: bool) -> Rc<dyn Command> {
  let w = window.clone();
  let address = window.address.clone();
  simple(
    format!("window:{}", window.address),
    move |_| Meta {
      title: w.title.clone().into(),
      subtitle: Some(w.class.clone().into()),
      icon: Icon::App(vec![w.class.clone().into()]),
      keywords: vec![w.class.clone().into(), w.workspace.clone().into()],
      accessories: vec![Accessory::Text(
        t!("app.launcher.windows.workspace", id = w.workspace.clone()).into(),
      )],
      ..Default::default()
    },
    move |_| {
      let (focus, closed) = (address.clone(), address.clone());
      let focus_action = Action::sync(
        "focus",
        t!("app.launcher.windows.focus"),
        move |_, _, cx| {
          cx.compositor().focus_window(&focus)?;
          Ok(Outcome::close())
        },
      );
      let close_action = Action::sync(
        "close",
        t!("app.launcher.windows.close"),
        move |_, _, cx| {
          cx.compositor().close_window(&closed)?;
          Ok(Outcome::KeepOpen)
        },
      )
      .destructive()
      .shortcut("ctrl-w")
      .keep_open();
      match close {
        true => vec![close_action, focus_action],
        false => vec![focus_action, close_action],
      }
    },
  )
}

/// Windows to switch to (or close), most recently focused first
fn windows_view(close: bool) -> corona_launcher::View {
  let source = ListSource::new(
    match close {
      true => "windows:close",
      false => "windows",
    },
    move |_: &Query, cx: &App| {
      let mut windows: Vec<&types::Window> = cx
        .compositor()
        .list_windows(cx)
        .iter()
        .filter(|w| !w.hidden)
        .collect();
      windows.sort_by_key(|w| w.focus_history_id);
      windows.into_iter().map(|w| window_row(w, close)).collect()
    },
  )
  .on_open(|cx| {
    let _ = Compositor::refresh_windows(cx).log_err();
  })
  .observe(|notify, cx| {
    let windows = cx.compositor().windows.clone();
    vec![watch(&windows, &notify, cx)]
  });
  let title = match close {
    true => t!("app.launcher.windows.close_window"),
    false => t!("app.launcher.windows.switch"),
  };
  ListView::new(title, t!("app.launcher.windows.search"), Rc::new(source)).into_view()
}

fn workspaces_view() -> corona_launcher::View {
  let source = ListSource::new("workspaces", |_: &Query, cx: &App| {
    let compositor = cx.compositor();
    let active = compositor.active_workspace(cx).id.clone();
    compositor
      .list_workspaces(cx)
      .iter()
      .map(|ws| {
        let titles: Vec<String> = compositor
          .list_windows(cx)
          .iter()
          .filter(|w| w.workspace == ws.id)
          .map(|w| w.class.clone())
          .collect();
        let (id, name, monitor) = (ws.id.clone(), ws.name.clone(), ws.monitor.clone());
        let is_active = ws.id == active;
        simple(
          format!("workspace:{}", ws.id),
          move |_| Meta {
            title: t!("app.launcher.windows.workspace", id = name.clone()).into(),
            subtitle: (!titles.is_empty()).then(|| SharedString::from(titles.join(", "))),
            icon: Icon::Named(IconName::LayoutGrid),
            accessories: match is_active {
              true => vec![Accessory::Icon(IconName::Check)],
              false => vec![Accessory::Text(monitor.clone().into())],
            },
            ..Default::default()
          },
          move |_| {
            let id = id.clone();
            vec![Action::sync(
              "go",
              t!("app.launcher.windows.go"),
              move |_, _, cx| {
                cx.compositor().focus_workspace(&id)?;
                Ok(Outcome::close())
              },
            )]
          },
        )
      })
      .collect()
  })
  .observe(|notify, cx| {
    let (workspaces, windows) = (
      cx.compositor().workspaces.clone(),
      cx.compositor().windows.clone(),
    );
    vec![
      watch(&workspaces, &notify, cx),
      watch(&windows, &notify, cx),
    ]
  });
  ListView::new(
    t!("app.launcher.windows.switch_workspace"),
    t!("app.launcher.windows.search_workspaces"),
    Rc::new(source),
  )
  .into_view()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      opener(
        "windows:switch",
        t!("app.launcher.windows.switch"),
        Icon::Tile(IconName::AppWindow, 200.),
        &["window", "focus", "alt tab"],
        |_| windows_view(false),
      ),
      simple(
        "windows:close",
        |_| Meta {
          title: t!("app.launcher.windows.close_window").into(),
          icon: Icon::Tile(IconName::X, 0.),
          keywords: vec!["window".into(), "kill".into()],
          ..Default::default()
        },
        |_| {
          vec![
            Action::sync(
              "close",
              t!("app.launcher.windows.close_focused"),
              |_, _, cx| {
                let compositor = cx.compositor();
                let window = compositor
                  .active_window(cx)
                  .map(|w| w.address.clone())
                  .ok_or_else(|| anyhow::anyhow!("{}", t!("app.launcher.windows.none_focused")))?;
                compositor.close_window(&window)?;
                Ok(Outcome::close())
              },
            ),
            Action::sync("pick", t!("app.launcher.windows.close_pick"), |_, _, _| {
              Ok(Outcome::Push(windows_view(true)))
            }),
          ]
        },
      ),
      simple(
        "workspace:go",
        |_| Meta {
          title: t!("app.launcher.windows.go_to_workspace").into(),
          icon: Icon::Tile(IconName::LayoutGrid, 200.),
          keywords: vec!["workspace".into(), "desktop".into()],
          args: vec![ArgSpec::number(t!("app.launcher.windows.number"))],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "go",
            t!("app.launcher.windows.go"),
            |args, _, cx| {
              let Some(corona_launcher::ArgValue::Number(n)) = args.get(0) else {
                anyhow::bail!("{}", t!("app.launcher.windows.number"));
              };
              cx.compositor().focus_workspace(&(*n as i64).to_string())?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
      opener(
        "workspace:switch",
        t!("app.launcher.windows.switch_workspace"),
        Icon::Tile(IconName::LayoutGrid, 200.),
        &["workspace", "desktop"],
        |_| workspaces_view(),
      ),
    ],
    cx,
  );
}
