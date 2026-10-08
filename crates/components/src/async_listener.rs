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

#[cfg(test)]
mod tests {
  use std::{cell::Cell, rc::Rc};

  use gpui_kit::{self as gpui, AppContext, IntoElement, TestAppContext, div};

  use super::*;
  use crate::test_view;

  type Listener = Box<dyn Fn(&u32, &mut Window, &mut App)>;

  #[derive(Default)]
  struct Counter {
    started: Vec<u32>,
    done: Vec<u32>,
  }

  fn listener(counter: &gpui_kit::Entity<Counter>, cx: &mut TestAppContext) -> Listener {
    counter.update(cx, |_, cx| {
      Box::new(cx.async_listener(
        |this: &mut Counter, event: &u32, _, _| {
          this.started.push(*event);
          let event = *event;
          async move { event * 10 }
        },
        |this, output, _| this.done.push(output),
      )) as Listener
    })
  }

  #[gpui::test]
  fn runs_start_then_done_and_notifies_after_each(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, _| div().into_any_element());
    let counter = cx.new(|_| Counter::default());
    let notified = Rc::new(Cell::new(0));
    cx.update(|cx| {
      let notified = notified.clone();
      cx.observe(&counter, move |_, _| notified.set(notified.get() + 1))
        .detach();
    });

    let listener = listener(&counter, cx);
    cx.update_window(handle, |_, window, cx| {
      listener(&1, window, cx);
      listener(&2, window, cx);
    })
    .unwrap();
    counter.read_with(cx, |c, _| {
      assert_eq!(c.started, [1, 2]);
      assert!(c.done.is_empty());
    });
    let after_start = notified.get();
    assert!(after_start >= 1);
    cx.run_until_parked();
    counter.read_with(cx, |c, _| assert_eq!(c.done, [10, 20]));
    assert!(notified.get() > after_start);
  }

  #[gpui::test]
  fn done_is_skipped_once_the_view_is_gone(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, _| div().into_any_element());
    let counter = cx.new(|_| Counter::default());
    let listener = listener(&counter, cx);
    cx.update_window(handle, |_, window, cx| listener(&1, window, cx))
      .unwrap();
    let weak = counter.downgrade();
    drop(counter);
    // nothing to update: must not panic
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
  }
}
