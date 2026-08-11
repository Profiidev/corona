use std::{cell::RefCell, rc::Rc};

use corona::Corona;
use smithay_client_toolkit::shell::wlr_layer::Anchor;

mod ui;

fn main() {
  tracing_subscriber::fmt::init();

  let mut corona = Corona::init().expect("Failed to initialize Corona state");
  let handle = corona.handle();
  let panel_handle = Rc::new(RefCell::new(None));

  for output in corona.outputs() {
    let handle = handle.clone();
    let wl_output = output.clone();
    let panel_handle = panel_handle.clone();
    corona
      .widget_builder()
      .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
      .height(30)
      .exclusive_zone(30)
      .build(&output, move |b: &mut ui::bar::Bar| {
        b.on_clicked(move || {
          let wl_output = wl_output.clone();
          let panel_handle = panel_handle.clone();
          handle.defer(move |corona| {
            if let Some(panel) = panel_handle.borrow_mut().take() {
              corona.destroy_widget(panel);
              return;
            }

            let panel = corona
              .widget_builder()
              .width(200)
              .height(200)
              .anchor(Anchor::TOP | Anchor::LEFT)
              .build(&wl_output, |p: &mut ui::panel::Panel| {
                p.on_clicked(move || {
                  println!("Panel clicked");
                });
              })
              .expect("Failed to create panel widget");

            *panel_handle.borrow_mut() = Some(panel);
          });
        });
      })
      .expect("Failed to create widget");
  }

  corona.run().expect("Failed to run Corona event loop");
}
