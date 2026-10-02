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
