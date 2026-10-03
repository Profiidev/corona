use std::time::Duration;

use gpui_kit::{Context, Task, Window};

pub trait TickerExt<T> {
  fn ticker(&mut self, every: Duration, f: impl Fn(&mut T, &mut Context<T>) + 'static) -> Task<()>;

  fn ticker_in(
    &mut self,
    window: &Window,
    every: Duration,
    f: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
  ) -> Task<()>;
}

impl<T: 'static> TickerExt<T> for Context<'_, T> {
  fn ticker(&mut self, every: Duration, f: impl Fn(&mut T, &mut Context<T>) + 'static) -> Task<()> {
    self.spawn(async move |this, cx| {
      loop {
        cx.background_executor().timer(every).await;
        if this.update(cx, |this, cx| f(this, cx)).is_err() {
          break;
        }
      }
    })
  }

  fn ticker_in(
    &mut self,
    window: &Window,
    every: Duration,
    f: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
  ) -> Task<()> {
    self.spawn_in(window, async move |this, cx| {
      loop {
        cx.background_executor().timer(every).await;
        if this
          .update_in(cx, |this, window, cx| f(this, window, cx))
          .is_err()
        {
          break;
        }
      }
    })
  }
}
