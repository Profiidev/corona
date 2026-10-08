use std::time::Duration;

use gpui_kit::{App, Entity};

mod bluetooth;
mod brightness;
mod dnd;
mod keyboard;
pub(crate) mod lock_keys;
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
      keyboard::init(cx);
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

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::{self as gpui, AppContext as _, TestAppContext};
  use std::{cell::RefCell, rc::Rc};

  type Shown = Rc<RefCell<Vec<(i32, i32)>>>;

  fn watch(cx: &mut TestAppContext, start: Option<i32>) -> (Entity<Option<i32>>, Shown) {
    let shown = Shown::default();
    let entity = cx.new(|_| start);
    let (e, s) = (entity.clone(), shown.clone());
    cx.update(|cx| {
      on_change(
        &e.clone(),
        cx,
        move |cx| *e.read(cx),
        move |prev, next, _| s.borrow_mut().push((*prev, *next)),
      )
    });
    (entity, shown)
  }

  fn set(entity: &Entity<Option<i32>>, value: Option<i32>, cx: &mut TestAppContext) {
    entity.update(cx, |v, cx| {
      *v = value;
      cx.notify();
    });
    cx.run_until_parked();
  }

  #[gpui::test]
  fn shows_changes_only(cx: &mut TestAppContext) {
    let (entity, shown) = watch(cx, Some(1));
    set(&entity, Some(1), cx);
    assert!(shown.borrow().is_empty());
    set(&entity, Some(2), cx);
    set(&entity, Some(5), cx);
    assert_eq!(*shown.borrow(), [(1, 2), (2, 5)]);
  }

  #[gpui::test]
  fn appearing_or_vanishing_is_not_shown(cx: &mut TestAppContext) {
    let (entity, shown) = watch(cx, None);
    set(&entity, Some(3), cx);
    set(&entity, None, cx);
    assert!(shown.borrow().is_empty());
    // the last value was still kept
    set(&entity, Some(4), cx);
    set(&entity, Some(6), cx);
    assert_eq!(*shown.borrow(), [(4, 6)]);
  }
}
