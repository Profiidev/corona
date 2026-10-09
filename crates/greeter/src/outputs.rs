use anyhow::Result;
use gpui_kit::{Bounds, Pixels, Size, point, px};
use wayland_client::{
  Connection, Dispatch, QueueHandle,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_output::WlOutput, wl_registry::WlRegistry},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
  zxdg_output_manager_v1::ZxdgOutputManagerV1,
  zxdg_output_v1::{self, ZxdgOutputV1},
};

/// The outputs by their index in the registry
struct Outputs(Vec<Output>);

#[derive(Default)]
struct Output {
  name: Option<String>,
  position: (i32, i32),
  size: (i32, i32),
}

/// Each output's name and place in the layout. gpui reads `wl_output`, whose
/// position wlroots always sends as 0,0; xdg-output has the real one.
pub fn layout() -> Result<Vec<(String, Bounds<Pixels>)>> {
  let conn = Connection::connect_to_env()?;
  let (globals, mut queue) = registry_queue_init::<Outputs>(&conn)?;
  let qh = queue.handle();
  let manager: ZxdgOutputManagerV1 = globals.bind(&qh, 2..=3, ())?;
  let outputs = globals
    .contents()
    .clone_list()
    .into_iter()
    .filter(|g| g.interface == "wl_output")
    .enumerate()
    .map(|(i, g)| {
      let output: WlOutput = globals.registry().bind(g.name, 1, &qh, ());
      manager.get_xdg_output(&output, &qh, i)
    })
    .collect::<Vec<_>>();
  let mut state = Outputs(outputs.iter().map(|_| Output::default()).collect());
  queue.roundtrip(&mut state)?;
  Ok(named(state.0))
}

/// The outputs that told their name, which every xdg-output v2+ does
fn named(outputs: Vec<Output>) -> Vec<(String, Bounds<Pixels>)> {
  (outputs.into_iter())
    .filter_map(|o| {
      let name = o.name?;
      let bounds = Bounds {
        origin: point(px(o.position.0 as f32), px(o.position.1 as f32)),
        size: Size::new(px(o.size.0 as f32), px(o.size.1 as f32)),
      };
      Some((name, bounds))
    })
    .collect()
}

impl Output {
  fn apply(&mut self, event: zxdg_output_v1::Event) {
    match event {
      zxdg_output_v1::Event::Name { name } => self.name = Some(name),
      zxdg_output_v1::Event::LogicalPosition { x, y } => self.position = (x, y),
      zxdg_output_v1::Event::LogicalSize { width, height } => self.size = (width, height),
      _ => {}
    }
  }
}

impl Dispatch<ZxdgOutputV1, usize> for Outputs {
  fn event(
    state: &mut Self,
    _: &ZxdgOutputV1,
    event: zxdg_output_v1::Event,
    &i: &usize,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    state.0[i].apply(event);
  }
}

impl Dispatch<WlRegistry, GlobalListContents> for Outputs {
  fn event(
    _: &mut Self,
    _: &WlRegistry,
    _: <WlRegistry as wayland_client::Proxy>::Event,
    _: &GlobalListContents,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

wayland_client::delegate_noop!(Outputs: ignore WlOutput);
wayland_client::delegate_noop!(Outputs: ZxdgOutputManagerV1);

#[cfg(test)]
mod tests {
  use super::*;

  fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
      origin: point(px(x), px(y)),
      size: Size::new(px(w), px(h)),
    }
  }

  #[test]
  fn events_fill_an_output() {
    use zxdg_output_v1::Event;
    let mut output = Output::default();
    output.apply(Event::LogicalPosition { x: -1920, y: 120 });
    output.apply(Event::LogicalSize {
      width: 1600,
      height: 1000,
    });
    output.apply(Event::Name {
      name: "eDP-1".into(),
    });
    // later events win
    output.apply(Event::LogicalPosition { x: 1920, y: 0 });
    output.apply(Event::Description {
      description: "ignored".into(),
    });
    output.apply(Event::Done);
    assert_eq!(
      named(vec![output]),
      [("eDP-1".into(), bounds(1920., 0., 1600., 1000.))]
    );
  }

  #[test]
  fn nameless_outputs_are_left_out() {
    let named_one = Output {
      name: Some("DP-1".into()),
      position: (0, 0),
      size: (2560, 1440),
    };
    let names = named(vec![Output::default(), named_one])
      .into_iter()
      .map(|(name, _)| name)
      .collect::<Vec<_>>();
    assert_eq!(names, ["DP-1"]);
    assert!(named(Vec::new()).is_empty());
  }

  #[test]
  fn layout_needs_a_compositor() {
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "/nonexistent/corona-test") };
    assert!(layout().is_err());
  }
}
