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

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, AppContext, TestAppContext};

  use super::*;

  struct Counter {
    ticks: usize,
    task: Option<Task<()>>,
  }

  const EVERY: Duration = Duration::from_secs(1);

  fn counter(cx: &mut TestAppContext) -> gpui_kit::Entity<Counter> {
    cx.new(|cx| Counter {
      ticks: 0,
      task: Some(
        cx.ticker(EVERY, |this: &mut Counter, _: &mut Context<Counter>| {
          this.ticks += 1
        }),
      ),
    })
  }

  fn ticks(counter: &gpui_kit::Entity<Counter>, cx: &mut TestAppContext) -> usize {
    counter.read_with(cx, |c, _| c.ticks)
  }

  #[gpui::test]
  fn ticks_once_per_period(cx: &mut TestAppContext) {
    let counter = counter(cx);
    cx.run_until_parked();
    // no tick before the first period
    assert_eq!(ticks(&counter, cx), 0);
    cx.executor()
      .advance_clock(EVERY - Duration::from_millis(1));
    assert_eq!(ticks(&counter, cx), 0);
    cx.executor().advance_clock(Duration::from_millis(1));
    assert_eq!(ticks(&counter, cx), 1);
    cx.executor().advance_clock(EVERY * 3);
    assert_eq!(ticks(&counter, cx), 4);
  }

  #[gpui::test]
  fn dropping_the_task_stops_it(cx: &mut TestAppContext) {
    let counter = counter(cx);
    cx.executor().advance_clock(EVERY);
    counter.update(cx, |c, _| c.task = None);
    cx.executor().advance_clock(EVERY * 5);
    assert_eq!(ticks(&counter, cx), 1);
  }

  #[gpui::test]
  fn stops_with_its_entity(cx: &mut TestAppContext) {
    let task = {
      let counter = cx.new(|_| Counter {
        ticks: 0,
        task: None,
      });
      counter.update(cx, |_, cx| {
        cx.ticker(EVERY, |this: &mut Counter, _: &mut Context<Counter>| {
          this.ticks += 1
        })
      })
    };
    cx.run_until_parked();
    // the entity is gone: the next tick ends the loop and the task completes
    cx.executor().advance_clock(EVERY);
    assert_eq!(
      futures_lite::future::block_on(futures_lite::future::poll_once(task)),
      Some(())
    );
  }

  #[gpui::test]
  fn ticks_in_a_window(cx: &mut TestAppContext) {
    struct View {
      ticks: usize,
      _task: Task<()>,
    }
    impl gpui_kit::Render for View {
      fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
      }
    }
    let window = cx.add_window(|window, cx| View {
      ticks: 0,
      _task: cx.ticker_in(
        window,
        EVERY,
        |this: &mut View, _: &mut Window, _: &mut Context<View>| this.ticks += 1,
      ),
    });
    cx.executor().advance_clock(EVERY * 2);
    let ticks = window.read_with(cx, |v, _| v.ticks).unwrap();
    assert_eq!(ticks, 2);
  }

  #[gpui::test]
  fn stops_with_its_window(cx: &mut TestAppContext) {
    struct View;
    impl gpui_kit::Render for View {
      fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
      }
    }
    let window = cx.add_window(|_, _| View);
    let task = window
      .update(cx, |_, window, cx| {
        cx.ticker_in(
          window,
          EVERY,
          |_: &mut View, _: &mut Window, _: &mut Context<View>| {},
        )
      })
      .unwrap();
    window
      .update(cx, |_, window, _| window.remove_window())
      .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(EVERY);
    assert_eq!(
      futures_lite::future::block_on(futures_lite::future::poll_once(task)),
      Some(())
    );
  }
}
