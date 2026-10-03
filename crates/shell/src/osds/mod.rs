use std::time::Duration;

use gpui_kit::{App, Entity};

mod bluetooth;
mod brightness;
mod dnd;
mod power_profile;
mod privacy;
mod view;
mod volume;
mod wifi;

const SETTLE: Duration = Duration::from_secs(3);

pub fn init(cx: &mut App) {
  cx.spawn(async move |cx| {
    cx.background_executor().timer(SETTLE).await;
    cx.update(|cx| {
      volume::init(cx);
      brightness::init(cx);
      wifi::init(cx);
      bluetooth::init(cx);
      power_profile::init(cx);
      dnd::init(cx);
      privacy::init(cx);
    });
  })
  .detach();
}

fn on_change<E: 'static, K: PartialEq + 'static>(
  entity: &Entity<E>,
  cx: &mut App,
  read: impl Fn(&App) -> Option<K> + 'static,
  show: impl Fn(&K, &K, &mut App) + 'static,
) {
  let mut last = read(cx);
  cx.observe(entity, move |_, cx| {
    let next = read(cx);
    if next == last {
      return;
    }
    if let (Some(prev), Some(next)) = (&last, &next) {
      show(prev, next, cx);
    }
    last = next;
  })
  .detach();
}
