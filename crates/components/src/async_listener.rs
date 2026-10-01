use std::rc::Rc;

use gpui_kit::{App, Context, Window};

pub trait AsyncListenerExt<T> {
  /// like `cx.listener`, `start` runs on the event and returns a future, `done` gets its output
  /// once it resolves, the view is notified after both
  fn async_listener<E: ?Sized, F: Future + 'static>(
    &self,
    start: impl Fn(&mut T, &E, &mut Window, &mut Context<T>) -> F + 'static,
    done: impl Fn(&mut T, F::Output, &mut Context<T>) + 'static,
  ) -> impl Fn(&E, &mut Window, &mut App) + 'static;
}

impl<T: 'static> AsyncListenerExt<T> for Context<'_, T> {
  fn async_listener<E: ?Sized, F: Future + 'static>(
    &self,
    start: impl Fn(&mut T, &E, &mut Window, &mut Context<T>) -> F + 'static,
    done: impl Fn(&mut T, F::Output, &mut Context<T>) + 'static,
  ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let done = Rc::new(done);
    self.listener(move |this, event, window, cx| {
      let task = start(this, event, window, cx);
      let done = done.clone();
      cx.spawn(async move |this, cx| {
        let output = task.await;
        this
          .update(cx, |this, cx| {
            done(this, output, cx);
            cx.notify();
          })
          .ok();
      })
      .detach();
      cx.notify();
    })
  }
}
