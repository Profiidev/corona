use std::time::Duration;

use gpui_kit::{
  App, AppContext, Context, Entity, EventEmitter, Subscription, Window,
  base::slider::{SliderEvent, SliderState},
};

/// A slider whose drag reaches its backend at most once per `throttle`, the final value on
/// release. It follows the drag, not the backend, until released
pub struct ThrottledSlider {
  state: Entity<SliderState>,
  /// None hands every change on
  throttle: Option<Duration>,
  dragging: bool,
  pending: Option<f32>,
  _subscription: Subscription,
}

/// The slider moved; `Some` when the backend should take the value now
pub struct Moved(pub Option<f32>);

impl EventEmitter<Moved> for ThrottledSlider {}

impl ThrottledSlider {
  pub fn new(state: SliderState, throttle: Option<Duration>, cx: &mut Context<Self>) -> Self {
    let state = cx.new(|_| state);
    let subscription = cx.subscribe(&state, |this, _, event: &SliderEvent, cx| {
      this.on_event(event, cx)
    });
    Self {
      state,
      throttle,
      dragging: false,
      pending: None,
      _subscription: subscription,
    }
  }

  pub fn state(&self) -> &Entity<SliderState> {
    &self.state
  }

  /// Where a drag is, else the backend's `value`
  pub fn shown(&self, value: f32, cx: &App) -> f32 {
    match self.dragging {
      true => self.state.read(cx).value().start(),
      false => value,
    }
  }

  /// Moves to the backend's `value`, unless a drag holds it
  pub fn sync(&self, value: f32, window: &mut Window, cx: &mut Context<Self>) {
    if !self.dragging {
      self
        .state
        .update(cx, |state, cx| state.set_value(value, window, cx));
    }
  }

  fn on_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
    match event {
      SliderEvent::Change(value) => {
        self.dragging = true;
        let Some(throttle) = self.throttle else {
          cx.emit(Moved(Some(value.start())));
          return;
        };
        cx.emit(Moved(None));
        if self.pending.replace(value.start()).is_none() {
          cx.spawn(async move |this, cx| {
            cx.background_executor().timer(throttle).await;
            let _ = this.update(cx, |this, cx| {
              if let Some(value) = this.pending.take() {
                cx.emit(Moved(Some(value)));
              }
            });
          })
          .detach();
        }
      }
      SliderEvent::Release(value) => {
        self.dragging = false;
        self.pending = None;
        cx.emit(Moved(Some(value.start())));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, rc::Rc};

  use gpui_kit::{TestAppContext, base::slider::SliderValue};

  use super::*;

  #[gpui_kit::test]
  fn throttles_the_drag_and_sets_on_release(cx: &mut TestAppContext) {
    let slider =
      cx.new(|cx| ThrottledSlider::new(SliderState::new(), Some(Duration::from_millis(100)), cx));
    let set = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
      let set = set.clone();
      cx.subscribe(&slider, move |_, Moved(value), _| {
        set.borrow_mut().extend(*value)
      })
      .detach();
    });
    let state = slider.read_with(cx, |s, _| s.state().clone());
    let emit = |event: SliderEvent, cx: &mut TestAppContext| {
      state.update(cx, |_, cx| cx.emit(event));
    };

    emit(SliderEvent::Change(SliderValue::from(10.)), cx);
    emit(SliderEvent::Change(SliderValue::from(20.)), cx);
    assert!(slider.read_with(cx, |s, _| s.dragging));
    assert!(set.borrow().is_empty());
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(*set.borrow(), [20.]);

    emit(SliderEvent::Change(SliderValue::from(30.)), cx);
    emit(SliderEvent::Release(SliderValue::from(40.)), cx);
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    // the pending 30 is dropped for the release
    assert_eq!(*set.borrow(), [20., 40.]);
    assert!(!slider.read_with(cx, |s, _| s.dragging));
  }
}
