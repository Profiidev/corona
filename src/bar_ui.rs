//! Wraps either the compiled `Bar` component or (with `--features hot-reload`) an interpreted
//! `ComponentInstance` behind one set of setters, so the rest of the app doesn't care which mode
//! built the UI. This is the only place the two modes need different code (plan §5).

use slint::{ModelRc, VecModel};

use crate::events::WorkspaceInfo;
use crate::ui::bar::{Bar, WorkspaceItem};

pub enum BarUi {
  Compiled(Bar),
  #[cfg(feature = "hot-reload")]
  Interpreted(slint_interpreter::ComponentInstance),
}

impl BarUi {
  pub fn set_workspaces(&self, workspaces: &[WorkspaceInfo]) {
    match self {
      BarUi::Compiled(bar) => {
        let items: Vec<WorkspaceItem> = workspaces
          .iter()
          .map(|w| WorkspaceItem {
            id: w.id,
            name: w.name.as_str().into(),
            active: w.active,
          })
          .collect();
        bar.set_workspaces(ModelRc::new(VecModel::from(items)));
      }
      #[cfg(feature = "hot-reload")]
      BarUi::Interpreted(instance) => {
        use slint_interpreter::{Struct, Value};
        let items: Vec<Value> = workspaces
          .iter()
          .map(|w| {
            let mut s = Struct::default();
            s.set_field("id".into(), Value::Number(w.id as f64));
            s.set_field("name".into(), Value::String(w.name.as_str().into()));
            s.set_field("active".into(), Value::Bool(w.active));
            Value::Struct(s)
          })
          .collect();
        let _ = instance.set_property(
          "workspaces",
          Value::Model(ModelRc::new(VecModel::from(items))),
        );
      }
    }
  }

  pub fn set_active_window_title(&self, title: &str) {
    match self {
      BarUi::Compiled(bar) => bar.set_active_window_title(title.into()),
      #[cfg(feature = "hot-reload")]
      BarUi::Interpreted(instance) => {
        let _ = instance.set_property(
          "active-window-title",
          slint_interpreter::Value::String(title.into()),
        );
      }
    }
  }

  pub fn on_workspace_clicked(&self, f: impl Fn(i32) + 'static) {
    match self {
      BarUi::Compiled(bar) => bar.on_workspace_clicked(f),
      #[cfg(feature = "hot-reload")]
      BarUi::Interpreted(instance) => {
        let _ = instance.set_callback("workspace-clicked", move |args| {
          if let Some(slint_interpreter::Value::Number(id)) = args.first() {
            f(*id as i32);
          }
          slint_interpreter::Value::Void
        });
      }
    }
  }

  pub fn set_clock(&self, clock: &str) {
    match self {
      BarUi::Compiled(bar) => bar.set_clock(clock.into()),
      #[cfg(feature = "hot-reload")]
      BarUi::Interpreted(instance) => {
        let _ = instance.set_property("clock", slint_interpreter::Value::String(clock.into()));
      }
    }
  }
}
