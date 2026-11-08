//! The printers bar button

use corona_surface::bar::{IdlePage, IdlePageButton};
use gpui_kit::{App, Context, Subscription};

use crate::panel::printers::PrintersPanel;
use corona_printing::PrintingExt;

/// Busy while a print job or a scan runs
impl IdlePage for PrintersPanel {
  fn busy(cx: &App) -> bool {
    cx.printing().busy(cx)
  }

  fn observe(cx: &mut Context<IdlePageButton<Self>>) -> Vec<Subscription> {
    let printing = cx.printing().clone();
    vec![
      cx.observe(&printing.jobs, |_, _, cx| cx.notify()),
      cx.observe(&printing.scan, |_, _, cx| cx.notify()),
    ]
  }
}

#[cfg(test)]
mod tests {
  use corona_printing::{Job, JobState, Printing};
  use gpui_kit::{self as gpui, AppContext, TestAppContext, WindowOptions, test::TestWindowExt};

  use super::*;
  use corona_surface::bar::{IdleOptions, Widget};
  use uuid::Uuid;

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
          cx.new(|cx| {
            IdlePageButton::<PrintersPanel>::init(cx, Uuid::nil(), IdleOptions { hide_when_idle })
          })
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
