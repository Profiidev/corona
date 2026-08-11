use std::{cell::RefCell, rc::Rc};

use corona::{Corona, api::widget::WidgetHandle};
use slint::ComponentHandle;
use smithay_client_toolkit::shell::wlr_layer::Anchor;

mod ui;

#[derive(Default)]
struct PanelState {
  widget: Option<WidgetHandle>,
  component: Option<slint::Weak<ui::panel::Panel>>,
}

fn main() {
  tracing_subscriber::fmt::init();

  let mut corona = Corona::init().expect("Failed to initialize Corona state");
  let handle = corona.handle();
  let panel = Rc::new(RefCell::new(PanelState::default()));

  for output in corona.outputs() {
    let handle = handle.clone();
    let wl_output = output.clone();
    let panel = panel.clone();

    corona
      .widget_builder()
      .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
      .height(30)
      .exclusive_zone(30)
      .build(&output, move |b: &mut ui::bar::Bar| {
        b.on_clicked(move || {
          let wl_output = wl_output.clone();
          let panel = panel.clone();
          let handle = handle.clone();

          handle.clone().defer(move |corona| {
            let open = panel
              .borrow()
              .component
              .as_ref()
              .and_then(|component| component.upgrade());

            if let Some(component) = open {
              component.invoke_toggle_open();
              return;
            }

            let widget = corona
              .widget_builder()
              .width(200)
              .height(200)
              .anchor(Anchor::TOP | Anchor::LEFT)
              .build(&wl_output, {
                let panel = panel.clone();
                move |p: &mut ui::panel::Panel| {
                  panel.borrow_mut().component = Some(p.as_weak());

                  p.on_clicked(|| {
                    println!("Panel clicked");
                  });

                  p.on_closed(move || {
                    let panel = panel.clone();
                    handle.defer(move |corona| {
                      let mut panel = panel.borrow_mut();
                      panel.component = None;
                      if let Some(widget) = panel.widget.take() {
                        corona.destroy_widget(widget);
                      }
                    });
                  });
                }
              })
              .expect("Failed to create panel widget");

            panel.borrow_mut().widget = Some(widget);
          });
        });
      })
      .expect("Failed to create widget");
  }

  corona.run().expect("Failed to run Corona event loop");
}
