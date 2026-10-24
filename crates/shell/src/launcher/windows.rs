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

/// The focused window's toggles, layouts, moves; close all; show desktop; monitors; layouts
pub fn init_actions(cx: &mut App) {
  use crate::commands::window::{
    CloseAll, FocusMonitor, KeyboardLayout, Layout, MoveRequest, MoveToMonitor, MoveToWorkspace,
    Place, ShowDesktop, Toggle, WindowToggle,
  };
  use corona_ipc::IpcCommand;
  use corona_launcher::{Confirm, Hud, kit::Choices};

  let toggle = |id: &'static str, title: SharedString, icon: IconName, what: WindowToggle| {
    simple(
      id,
      move |_| Meta {
        title: title.clone(),
        icon: Icon::Tile(icon, 200.),
        keywords: vec!["window".into()],
        ..Default::default()
      },
      move |_| {
        vec![Action::sync(
          "toggle",
          t!("app.launcher.switch"),
          move |_, _, cx| {
            Toggle::handle((what, None), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    )
  };
  let layout_names: Vec<&'static str> = Layout::ALL
    .iter()
    .map(|l| match l {
      Layout::Left => "left",
      Layout::Right => "right",
      Layout::Top => "top",
      Layout::Bottom => "bottom",
      Layout::LeftThird => "left-third",
      Layout::CenterThird => "center-third",
      Layout::RightThird => "right-third",
      Layout::LeftTwoThirds => "left-two-thirds",
      Layout::RightTwoThirds => "right-two-thirds",
      Layout::TopLeft => "top-left",
      Layout::TopRight => "top-right",
      Layout::BottomLeft => "bottom-left",
      Layout::BottomRight => "bottom-right",
      Layout::Center => "center",
      Layout::Maximize => "maximize",
      Layout::AlmostMaximize => "almost-maximize",
      Layout::Restore => "restore",
    })
    .collect();
  let names = layout_names.clone();
  registry::register_all(
    [
      toggle(
        "window:float",
        t!("app.launcher.windows.float").into(),
        IconName::PictureInPicture2,
        WindowToggle::Floating,
      ),
      toggle(
        "window:fullscreen",
        t!("app.launcher.windows.fullscreen").into(),
        IconName::Maximize,
        WindowToggle::Fullscreen,
      ),
      toggle(
        "window:pin",
        t!("app.launcher.windows.pin").into(),
        IconName::Pin,
        WindowToggle::Pin,
      ),
      simple(
        "window:move_workspace",
        |_| Meta {
          title: t!("app.launcher.windows.move_to_workspace").into(),
          icon: Icon::Tile(IconName::MoveRight, 200.),
          keywords: vec!["window".into(), "send".into()],
          args: vec![ArgSpec::number(t!("app.launcher.windows.number"))],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "move",
            t!("app.launcher.windows.move"),
            |args, _, cx| {
              let workspace = args
                .text(0)
                .map(str::to_string)
                .or_else(|| match args.get(0) {
                  Some(corona_launcher::ArgValue::Number(n)) => Some((*n as i64).to_string()),
                  _ => None,
                });
              let workspace = workspace
                .ok_or_else(|| anyhow::anyhow!("{}", t!("app.launcher.windows.number")))?;
              MoveToWorkspace::handle(
                MoveRequest {
                  address: None,
                  workspace,
                  follow: false,
                },
                cx,
              )?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
      simple(
        "window:move_monitor",
        |cx| {
          let mut choices: Vec<SharedString> = vec!["next".into(), "previous".into()];
          choices.extend(
            cx.compositor()
              .list_monitors(cx)
              .iter()
              .map(|m| SharedString::from(m.name.clone())),
          );
          Meta {
            title: t!("app.launcher.windows.move_to_monitor").into(),
            icon: Icon::Tile(IconName::MonitorUp, 200.),
            keywords: vec!["window".into(), "display".into()],
            args: vec![ArgSpec::choice(t!("app.launcher.windows.monitor"), choices).optional()],
            ..Default::default()
          }
        },
        |_| {
          vec![Action::sync(
            "move",
            t!("app.launcher.windows.move"),
            |args, _, cx| {
              let which = args.text(0).unwrap_or("next").to_string();
              MoveToMonitor::handle((which, None), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
      simple(
        "window:layout",
        move |_| Meta {
          title: t!("app.launcher.windows.layout").into(),
          icon: Icon::Tile(IconName::LayoutPanelLeft, 200.),
          keywords: vec!["window".into(), "snap".into(), "tile".into(), "half".into()],
          args: vec![ArgSpec::choice(
            t!("app.launcher.windows.position"),
            names.clone(),
          )],
          ..Default::default()
        },
        move |_| {
          let names = layout_names.clone();
          vec![Action::sync(
            "place",
            t!("app.launcher.windows.place"),
            move |args, _, cx| {
              let name = args.text(0).unwrap_or_default();
              let layout = names
                .iter()
                .position(|n| *n == name)
                .map(|i| Layout::ALL[i])
                .ok_or_else(|| anyhow::anyhow!("{}", t!("app.launcher.windows.position")))?;
              Place::handle((layout, None), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
      simple(
        "window:close_all",
        |_| Meta {
          title: t!("app.launcher.windows.close_all").into(),
          icon: Icon::Tile(IconName::SquareX, 0.),
          keywords: vec!["windows".into(), "quit".into()],
          ..Default::default()
        },
        |_| {
          let confirm = |keep: bool| Confirm {
            title: match keep {
              true => t!("app.launcher.windows.close_others"),
              false => t!("app.launcher.windows.close_all"),
            }
            .into(),
            message: t!("app.launcher.windows.close_all_message").into(),
            button: t!("app.launcher.windows.close").into(),
            skippable: true,
          };
          [false, true]
            .into_iter()
            .map(|keep| {
              Action::sync(
                if keep { "close_others" } else { "close_all" },
                match keep {
                  true => t!("app.launcher.windows.close_others"),
                  false => t!("app.launcher.windows.close_all"),
                },
                move |_, _, cx| {
                  let n = CloseAll::handle(keep, cx)?;
                  Ok(Outcome::Close(Some(Hud::new(t!(
                    "app.launcher.windows.closed",
                    count = n
                  )))))
                },
              )
              .destructive()
              .confirm(confirm(keep))
            })
            .collect()
        },
      ),
      simple(
        "workspace:show_desktop",
        |_| Meta {
          title: t!("app.launcher.windows.show_desktop").into(),
          icon: Icon::Tile(IconName::Monitor, 200.),
          keywords: vec!["desktop".into(), "hide windows".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "show",
            t!("app.launcher.windows.show_desktop"),
            |_, _, cx| {
              ShowDesktop::handle((), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
      opener(
        "monitor:focus",
        t!("app.launcher.windows.focus_monitor"),
        Icon::Tile(IconName::Monitor, 250.),
        &["display", "screen"],
        |_| {
          Choices {
            id: "monitor:focus",
            items: Rc::new(|cx| {
              cx.compositor()
                .list_monitors(cx)
                .iter()
                .filter(|m| !m.disabled)
                .map(|m| (m.name.clone().into(), m.name.clone().into()))
                .collect()
            }),
            current: Rc::new(|cx| Some(cx.compositor().active_monitor(cx).name.clone().into())),
            apply: Rc::new(|name, cx| FocusMonitor::handle(name.to_string(), cx)),
            preview: None,
            icon: Icon::Named(IconName::Monitor),
          }
          .view(
            t!("app.launcher.windows.focus_monitor"),
            t!("app.launcher.windows.search_monitors"),
          )
        },
      ),
      simple(
        "keyboard:layout",
        |cx| Meta {
          title: t!("app.launcher.windows.keyboard_layout").into(),
          subtitle: cx
            .compositor()
            .keyboard_layout(cx)
            .map(|l| l.to_string().into()),
          icon: Icon::Tile(IconName::Keyboard, 60.),
          keywords: vec![
            "keyboard".into(),
            "layout".into(),
            "xkb".into(),
            "language".into(),
          ],
          ..Default::default()
        },
        |_| {
          vec![
            Action::sync("pick", t!("app.launcher.open"), |_, _, cx| {
              let layouts = KeyboardLayout::handle(None, cx)?;
              Ok(Outcome::Push(
                Choices {
                  id: "keyboard:layout",
                  items: Rc::new(move |_| {
                    layouts
                      .iter()
                      .enumerate()
                      .map(|(i, l)| (i.to_string().into(), l.clone().into()))
                      .collect()
                  }),
                  current: Rc::new(|_| None),
                  apply: Rc::new(|index, cx| {
                    KeyboardLayout::handle(Some(index.to_string()), cx).map(|_| ())
                  }),
                  preview: None,
                  icon: Icon::Named(IconName::Keyboard),
                }
                .view(
                  t!("app.launcher.windows.keyboard_layout"),
                  t!("app.launcher.windows.search_layouts"),
                ),
              ))
            }),
            Action::sync(
              "next",
              t!("app.launcher.windows.next_layout"),
              |_, _, cx| {
                KeyboardLayout::handle(Some("next".into()), cx)?;
                Ok(Outcome::close())
              },
            )
            .shortcut("ctrl-shift-n"),
          ]
        },
      ),
    ],
    cx,
  );
}
