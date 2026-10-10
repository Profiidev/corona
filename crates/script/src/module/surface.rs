use std::{cell::Cell, rc::Rc};

use anyhow::{Result, bail};
use corona_config::placement::Placement;
use corona_macros::named;
use corona_surface::{bar::BarState, panel::PanelState};
use gpui_kit::{App, EntityId};
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Module},
  module::{Subscribe, Subscriptions, watch},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  OpenPanels,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Surface(value)
  }
}

/// The side of the screen a bar is on.
#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Side {
  Top,
  Bottom,
  Left,
  Right,
}

/// The bar a widget is shown in.
#[derive(Serialize, TS)]
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
struct Own {
  id: String,
  panels: Vec<String>,
  /// The bar widget the script is shown as, once it is one
  opener: Rc<Cell<Option<EntityId>>>,
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

/// `corona/surface`: the plugin's panels and the bar its widget is in. Every
/// plugin has it, for its own panels only.
pub fn module(
  id: &str,
  panels: Vec<String>,
  opener: Rc<Cell<Option<EntityId>>>,
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  cx: &mut App,
) -> HostModule {
  let own = Rc::new(Own {
    id: id.to_string(),
    panels,
    opener,
  });
  if cx.has_global::<PanelState>() {
    subs.push(watch(
      reads,
      Updates::OpenPanels.into(),
      PanelState::open_panels(cx),
    ));
  }
  let reads = reads.clone();
  let (list, toggle, open, close, is_open, bar) = (
    own.clone(),
    own.clone(),
    own.clone(),
    own.clone(),
    own.clone(),
    own,
  );

  Module::new("corona/surface")
    .func(named!(
      "panels",
      /// The names of this plugin's panels.
      move || list.panels.clone()
    ))
    .func(named!(
      "togglePanel",
      /// Opens panel `name`, at this widget when called from one, or closes
      /// it when it is open there.
      move |cx: &mut App, name: String| toggle.show(&name, true, cx)
    ))
    .func(named!(
      "openPanel",
      /// Opens panel `name`, at this widget when called from one; an open
      /// one stays open.
      move |cx: &mut App, name: String| open.show(&name, false, cx)
    ))
    .func(named!(
      "closePanel",
      /// Closes panel `name` when it is open.
      move |cx: &mut App, name: String| -> Result<()> {
        let full = close.panel(&name)?;
        cx.defer(move |cx| {
          PanelState::close(&full, cx).ok();
        });
        Ok(())
      }
    ))
    .func(named!(
      "isPanelOpen",
      /// Whether panel `name` is open or opening. A view that asks renders
      /// again as it opens and closes.
      move |cx: Cx, name: String| -> Result<bool> {
        let full = is_open.panel(&name)?;
        reads.record(Updates::OpenPanels.into());
        Ok(cx.has_global::<PanelState>() && PanelState::is_open(&full, &cx))
      }
    ))
    .func(named!(
      "bar",
      /// The bar this widget is in; null outside of one, like in a panel.
      move |cx: Cx| {
        let widget = bar.opener.get()?;
        let (placement, bare) = BarState::widget_place(widget, &cx)?;
        Some(Bar::new(placement, bare))
      }
    ))
    .into()
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
    };
    assert_eq!(own.panel("p").unwrap(), "a:p");
    // not another plugin's, named in full
    for name in ["q", "a:p", "b:p", ""] {
      let error = own.panel(name).unwrap_err().to_string();
      assert!(error.contains("no panel"), "{name}: {error}");
    }
  }
}
