use std::{cell::Cell, rc::Rc};

use anyhow::{Result, bail};
use corona_config::placement::Placement;
use corona_js::{Cx, Data, Json, ModuleBuilder, Throws, host_module, track};
use corona_surface::{bar::BarState, panel::PanelState};
use gpui_kit::{App, EntityId};
use serde::Serialize;
use specta::{Type, specta};

/// The side of the screen a bar is on.
#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Side {
  Top,
  Bottom,
  Left,
  Right,
}

/// The bar a widget is shown in.
#[derive(Serialize, Type)]
struct Bar {
  side: Side,
  /// Left or right: widgets stack top to bottom.
  vertical: bool,
  /// Without a pill of its own: in a group, or the bar has capsules off.
  bare: bool,
}

impl Bar {
  fn new(placement: Placement, bare: bool) -> Self {
    let side = match placement {
      Placement::Top => Side::Top,
      Placement::Bottom => Side::Bottom,
      Placement::Left => Side::Left,
      Placement::Right => Side::Right,
    };
    Self {
      side,
      vertical: !placement.is_horizontal(),
      bare,
    }
  }
}

/// The plugin's place in the shell's surfaces: its panels, by the names its
/// manifest gives them, and the bar its widget is in
pub struct Own {
  pub id: String,
  pub panels: Vec<String>,
  /// The bar widget the script is shown as, once it is one
  pub opener: Rc<Cell<Option<EntityId>>>,
  /// A widget's options from the bar config
  pub options: Option<serde_json::Value>,
}

impl Own {
  fn panel(&self, name: &str) -> Result<String> {
    if !self.panels.iter().any(|p| p == name) {
      bail!("this plugin has no panel `{name}`");
    }
    Ok(format!("{}:{name}", self.id))
  }

  /// Shows panel `name` once the script is done: a panel opens a window
  fn show(&self, name: &str, toggle: bool, cx: &mut App) -> Result<()> {
    let (full, opener) = (self.panel(name)?, self.opener.clone());
    cx.defer(move |cx| {
      if let Err(e) = PanelState::show_at(&full, opener.get(), toggle, cx) {
        tracing::error!("panel `{full}`: {e:#}");
      }
    });
    Ok(())
  }
}

/// The names of this plugin's panels.
#[specta]
fn panels(own: Data<Own>) -> Vec<String> {
  own.panels.clone()
}

/// Opens panel `name`, at this widget when called from one, or closes it when it is open
/// there.
#[specta]
fn toggle_panel(mut cx: Cx, own: Data<Own>, name: String) -> Throws<()> {
  own.show(&name, true, &mut cx).into()
}

/// Opens panel `name`, at this widget when called from one; an open one stays open.
#[specta]
fn open_panel(mut cx: Cx, own: Data<Own>, name: String) -> Throws<()> {
  own.show(&name, false, &mut cx).into()
}

/// Closes panel `name` when it is open.
#[specta]
fn close_panel(mut cx: Cx, own: Data<Own>, name: String) -> Throws<()> {
  own
    .panel(&name)
    .map(|full| {
      cx.defer(move |cx| {
        PanelState::close(&full, cx).ok();
      })
    })
    .into()
}

/// Whether panel `name` is open or opening. A view that asks renders again as it opens
/// and closes.
#[specta]
fn is_panel_open(cx: Cx, own: Data<Own>, name: String) -> Throws<bool> {
  let open = |full: String| {
    if !cx.has_global::<PanelState>() {
      return false;
    }
    track(&PanelState::open_panels(&cx));
    PanelState::is_open(&full, &cx)
  };
  own.panel(&name).map(open).into()
}

/// This widget's options from the bar config; null in a panel or service, or when the
/// config gives none.
#[specta]
fn options(own: Data<Own>) -> Json<Option<serde_json::Value>> {
  Json(own.options.clone())
}

/// The bar this widget is in; null outside of one, like in a panel.
#[specta]
fn bar(cx: Cx, own: Data<Own>) -> Json<Option<Bar>> {
  let place = (own.opener.get()).and_then(|widget| BarState::widget_place(widget, &cx));
  Json(place.map(|(placement, bare)| Bar::new(placement, bare)))
}

/// `corona/surface`: the plugin's panels and the bar its widget is in. Every plugin has
/// it, for its own panels only.
pub fn module(own: Own, m: &mut ModuleBuilder) {
  m.provide(own);
  host_module!(m, {
    sync: [panels, toggle_panel, open_panel, close_panel, is_panel_open, options, bar],
  });
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn bars() {
    for (placement, side, vertical) in [
      (Placement::Top, "top", false),
      (Placement::Bottom, "bottom", false),
      (Placement::Left, "left", true),
      (Placement::Right, "right", true),
    ] {
      assert_eq!(
        serde_json::to_value(Bar::new(placement, true)).unwrap(),
        json!({ "side": side, "vertical": vertical, "bare": true })
      );
    }
  }

  #[test]
  fn only_own_panels_under_the_plugin_id() {
    let own = Own {
      id: "a".into(),
      panels: vec!["p".into()],
      opener: Rc::default(),
      options: None,
    };
    assert_eq!(own.panel("p").unwrap(), "a:p");
    // not another plugin's, named in full
    for name in ["q", "a:p", "b:p", ""] {
      let error = own.panel(name).unwrap_err().to_string();
      assert!(error.contains("no panel"), "{name}: {error}");
    }
  }
}
