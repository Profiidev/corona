use std::{cell::RefCell, rc::Rc};

use corona::{
  Corona,
  api::widget::{Anchor, WidgetHandle},
  slint::ComponentHandle,
};
use slint::{ModelRc, ToSharedString, VecModel};

mod ui;

#[derive(Default)]
struct PanelState {
  widget: Option<WidgetHandle>,
  component: Option<slint::Weak<ui::panel::Panel>>,
}

fn main() {
  tracing_subscriber::fmt::init();

  let corona = Corona::init().expect("Failed to initialize Corona state");
  let panel = Rc::new(RefCell::new(PanelState::default()));

  for output in corona.outputs() {
    let corona = corona.clone();
    let wl_output = output.clone();
    let panel = panel.clone();

    corona
      .widget_builder()
      .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
      .height(50)
      .exclusive_zone(50)
      .build(&output, move |b: &mut ui::bar::Bar| {
        let workspaces = ModelRc::new(VecModel::from(
          corona
            .workspace_list()
            .unwrap()
            .into_iter()
            .map(|i| i.name.to_shared_string())
            .collect::<Vec<_>>(),
        ));
        b.set_workspaces(workspaces);

        let corona_ = corona.clone();
        b.on_workspaceClicked(move |workspace: slint::SharedString| {
          if let Err(e) = corona_.dispatch_workspace(workspace.to_string()) {
            tracing::error!("Failed to dispatch workspace: {e}");
          }
        });

        let corona = corona.clone();
        b.on_clicked(move || {
          let open = panel
            .borrow()
            .component
            .as_ref()
            .and_then(|component| component.upgrade());

          if let Some(component) = open {
            component.invoke_toggle_open();
            return;
          }

          let wl_output = wl_output.clone();
          let panel_for_widget = panel.clone();
          let corona_for_widget = corona.clone();

          let widget = corona
            .widget_builder()
            .width(200)
            .height(200)
            .anchor(Anchor::TOP | Anchor::LEFT)
            .build(&wl_output, move |p: &mut ui::panel::Panel| {
              panel_for_widget.borrow_mut().component = Some(p.as_weak());

              p.on_clicked(|| {
                println!("Panel clicked");
              });

              p.on_closed(move || {
                let mut panel = panel_for_widget.borrow_mut();
                panel.component = None;
                if let Some(widget) = panel.widget.take() {
                  corona_for_widget.destroy_widget(widget);
                }
              });
            })
            .expect("Failed to create panel widget");

          panel.borrow_mut().widget = Some(widget);
        });
      })
      .expect("Failed to create widget");
  }

  corona.run().expect("Failed to run Corona event loop");
}
