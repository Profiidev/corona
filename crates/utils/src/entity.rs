use gpui_kit::{AppContext, Entity};

pub trait WriteChangedExt<T> {
  fn write_changed<C: AppContext>(&self, cx: &mut C, next: T);
}

impl<T: PartialEq + 'static> WriteChangedExt<T> for Entity<T> {
  fn write_changed<C: AppContext>(&self, cx: &mut C, next: T) {
    self.update(cx, |value, cx| {
      if *value != next {
        *value = next;
        cx.notify();
      }
    });
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::Cell, rc::Rc};

  use gpui_kit::{self as gpui, AppContext, TestAppContext};

  use super::*;

  fn counted<T: 'static>(cx: &mut TestAppContext, value: T) -> (Entity<T>, Rc<Cell<usize>>) {
    let entity = cx.new(|_| value);
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    cx.update(|cx| {
      cx.observe(&entity, move |_, _| seen.set(seen.get() + 1))
        .detach()
    });
    (entity, count)
  }

  #[gpui::test]
  fn notifies_only_on_change(cx: &mut TestAppContext) {
    let (entity, count) = counted(cx, 1);
    entity.write_changed(cx, 1);
    cx.run_until_parked();
    assert_eq!(count.get(), 0);
    entity.write_changed(cx, 2);
    cx.run_until_parked();
    assert_eq!((count.get(), entity.read_with(cx, |v, _| *v)), (1, 2));
    entity.write_changed(cx, 2);
    cx.run_until_parked();
    assert_eq!(count.get(), 1);
  }

  #[gpui::test]
  fn nan_never_equals(cx: &mut TestAppContext) {
    let (entity, count) = counted(cx, f32::NAN);
    entity.write_changed(cx, f32::NAN);
    cx.run_until_parked();
    assert_eq!(count.get(), 1);
  }
}
