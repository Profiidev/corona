use corona_printing::PrintingExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window, assets::IconName, div};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::control_center::{PrintersPanel, Standalone};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
  /// only show while a print job or a scan runs
  pub hide_when_idle: bool,
}

pub struct PrintersButton {
  options: Options,
  _subscriptions: [Subscription; 2],
}

impl Widget for PrintersButton {
  const NAME: &'static str = "printers";
  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let printing = cx.printing().clone();
    Self {
      options,
      _subscriptions: [
        cx.observe(&printing.jobs, |_, _, cx| cx.notify()),
        cx.observe(&printing.scan, |_, _, cx| cx.notify()),
      ],
    }
  }
}

impl Render for PrintersButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if self.options.hide_when_idle && !cx.printing().busy(cx) {
      return div().into_any_element();
    }
    Button::<_, Standalone<PrintersPanel>>::new(cx, "printers-button", IconName::Printer)
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use corona_printing::{Job, JobState, Printing};
  use gpui_kit::{self as gpui, AppContext, TestAppContext, WindowOptions, test::TestWindowExt};

  use super::*;

  fn shown(cx: &mut TestAppContext, hide_when_idle: bool, jobs: Vec<Job>) -> bool {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      Printing::fake(cx, vec![], jobs, vec![], vec![]);
    });
    let (window, _) = cx
      .update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
          cx.new(|cx| PrintersButton::init(cx, Uuid::nil(), Options { hide_when_idle }))
        })
      })
      .unwrap();
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      window.try_find("printers-button").is_some()
    })
    .unwrap()
  }

  fn job(state: JobState) -> Job {
    Job {
      id: 1,
      name: "doc".into(),
      printer: "Office".into(),
      state,
      pages_done: None,
      pages_total: None,
      created: None,
    }
  }

  #[gpui::test]
  fn always_shown_by_default(cx: &mut TestAppContext) {
    assert!(shown(cx, false, vec![]));
  }

  #[gpui::test]
  fn hidden_when_idle(cx: &mut TestAppContext) {
    assert!(!shown(cx, true, vec![job(JobState::Completed)]));
  }

  #[gpui::test]
  fn shown_while_printing(cx: &mut TestAppContext) {
    assert!(shown(cx, true, vec![job(JobState::Processing)]));
  }
}
