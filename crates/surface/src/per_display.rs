use std::{
  collections::{HashMap, HashSet},
  rc::Rc,
  time::Duration,
};

use corona_compositor::CompositorExt;
use corona_config::{Config, ConfigProvider};
use corona_utils::display::display_uuid;
use gpui_kit::{AnyWindowHandle, App, AppContext, DisplayId, Entity, Subscription, WeakEntity};
use uuid::Uuid;

const DISPLAY_WAIT_TICK: Duration = Duration::from_millis(16);
const DISPLAY_WAIT_TICKS: usize = 60;

type Open = Rc<dyn Fn(&mut App, DisplayId) -> Vec<AnyWindowHandle>>;

pub struct PerDisplay {
  open: Open,
  windows: HashMap<DisplayId, Vec<AnyWindowHandle>>,
  _subscription: Subscription,
}

impl PerDisplay {
  pub fn new(
    cx: &mut App,
    open: impl Fn(&mut App, DisplayId) -> Vec<AnyWindowHandle> + 'static,
  ) -> Entity<Self> {
    let monitors = cx.compositor().monitors.clone();
    let this = cx.new(|cx| Self {
      open: Rc::new(open),
      windows: HashMap::new(),
      _subscription: cx.observe(&monitors, |_, _, cx| reconcile_soon(cx.weak_entity(), cx)),
    });
    reconcile_soon(this.downgrade(), cx);
    this
  }

  /// Closes every window it opened; it opens no more once dropped.
  pub fn close(this: Entity<Self>, cx: &mut App) {
    let windows = this.update(cx, |this, _| std::mem::take(&mut this.windows));
    for handle in windows.into_values().flatten() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
  }

  /// Keeps the windows `open` makes on every display, opened again once what `watch` reads
  /// from the config changes; `open` gives `None` for none at all
  pub fn follow<T: PartialEq + 'static, F>(
    cx: &mut App,
    watch: fn(&Config) -> T,
    open: impl Fn(&mut App) -> Option<F> + 'static,
  ) where
    F: Fn(&mut App, DisplayId) -> Vec<AnyWindowHandle> + 'static,
  {
    let mut displays = open(cx).map(|f| Self::new(cx, f));
    let mut last = watch(cx.config());
    cx.observe_global::<Config>(move |cx| {
      let now = watch(cx.config());
      if now == last {
        return;
      }
      last = now;
      if let Some(displays) = displays.take() {
        Self::close(displays, cx);
      }
      displays = open(cx).map(|f| Self::new(cx, f));
    })
    .detach();
  }

  pub fn windows(&self, display: DisplayId) -> &[AnyWindowHandle] {
    self.windows.get(&display).map_or(&[], Vec::as_slice)
  }
}

fn reconcile_soon(this: WeakEntity<PerDisplay>, cx: &mut App) {
  cx.spawn(async move |cx| {
    for _ in 0..DISPLAY_WAIT_TICKS {
      if cx.update(displays_ready) {
        break;
      }
      cx.background_executor().timer(DISPLAY_WAIT_TICK).await;
    }
    cx.update(|cx| reconcile(this, cx));
  })
  .detach();
}

fn displays_ready(cx: &mut App) -> bool {
  let displays: HashSet<Uuid> = cx.displays().iter().filter_map(|d| d.uuid().ok()).collect();
  cx.compositor()
    .list_monitors(cx)
    .iter()
    .filter(|m| !m.disabled)
    .all(|m| displays.contains(&display_uuid(&m.name)))
}

fn reconcile(this: WeakEntity<PerDisplay>, cx: &mut App) {
  let Some(this) = this.upgrade() else {
    return;
  };
  let displays: HashMap<Uuid, DisplayId> = cx
    .displays()
    .iter()
    .filter_map(|d| Some((d.uuid().ok()?, d.id())))
    .collect();
  let wanted: HashSet<DisplayId> = cx
    .compositor()
    .list_monitors(cx)
    .iter()
    .filter(|m| !m.disabled)
    .filter_map(|m| displays.get(&display_uuid(&m.name)).copied())
    .collect();

  let (open, current) = this.read_with(cx, |this, _| {
    (
      this.open.clone(),
      this.windows.keys().copied().collect::<HashSet<_>>(),
    )
  });

  let (close, add) = diff(&current, &wanted);
  for display in close {
    let windows = this.update(cx, |this, _| this.windows.remove(&display));
    for handle in windows.into_iter().flatten() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
  }
  for display in add {
    let windows = open(cx, display);
    if !windows.is_empty() {
      this.update(cx, |this, _| this.windows.insert(display, windows));
    }
  }
}

/// The displays whose windows go, and the ones that get windows
fn diff(
  current: &HashSet<DisplayId>,
  wanted: &HashSet<DisplayId>,
) -> (Vec<DisplayId>, Vec<DisplayId>) {
  (
    current.difference(wanted).copied().collect(),
    wanted.difference(current).copied().collect(),
  )
}

#[cfg(test)]
mod tests {
  use std::cell::Cell;

  use gpui_kit::TestAppContext;

  use super::*;
  use crate::{
    test_support::{self, plain_window, windows},
    testing::{FakeCompositor, monitor},
  };

  fn ids(ids: &[u64]) -> HashSet<DisplayId> {
    ids.iter().map(|&i| DisplayId::new(i)).collect()
  }

  fn sorted(mut v: Vec<DisplayId>) -> Vec<DisplayId> {
    v.sort_by_key(|d| u64::from(*d));
    v
  }

  #[test]
  fn diff_closes_the_gone_and_opens_the_new() {
    let (close, open) = diff(&ids(&[1, 2, 3]), &ids(&[2, 3, 4, 5]));
    assert_eq!(close, [DisplayId::new(1)]);
    assert_eq!(sorted(open), [DisplayId::new(4), DisplayId::new(5)]);
    // reconciled already: nothing to do
    let (close, open) = diff(&ids(&[1, 2]), &ids(&[1, 2]));
    assert!(close.is_empty() && open.is_empty());
    let (close, open) = diff(&ids(&[]), &ids(&[]));
    assert!(close.is_empty() && open.is_empty());
  }

  fn per_display(cx: &mut TestAppContext) -> (Entity<PerDisplay>, Rc<Cell<usize>>) {
    let opened = Rc::new(Cell::new(0));
    let count = opened.clone();
    let this = cx.update(|cx| {
      PerDisplay::new(cx, move |_, _| {
        count.set(count.get() + 1);
        Vec::new()
      })
    });
    (this, opened)
  }

  #[gpui_kit::test]
  fn opens_nothing_without_a_matching_monitor(cx: &mut TestAppContext) {
    let fake = FakeCompositor {
      monitors: vec![monitor("DP-1")],
      ..Default::default()
    };
    crate::testing::setup(fake, cx);
    cx.update(|cx| crate::init(cx).unwrap());
    let (this, opened) = per_display(cx);
    // waits for the monitor's display, then gives up
    cx.executor()
      .advance_clock(DISPLAY_WAIT_TICK * DISPLAY_WAIT_TICKS as u32);
    cx.run_until_parked();
    assert_eq!(opened.get(), 0);
    cx.update(|cx| {
      let display = cx.displays()[0].id();
      assert!(this.read(cx).windows(display).is_empty());
    });
  }

  #[gpui_kit::test]
  fn reconcile_closes_windows_on_unwanted_displays(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let (this, opened) = per_display(cx);
    cx.run_until_parked();
    let before = windows(cx);
    let handle = plain_window(cx);
    let display = DisplayId::new(99);
    this.update(cx, |this, _| this.windows.insert(display, vec![handle]));
    assert_eq!(this.read_with(cx, |this, _| this.windows(display).len()), 1);

    cx.update(|cx| reconcile(this.downgrade(), cx));
    cx.run_until_parked();
    assert!(this.read_with(cx, |this, _| this.windows(display).is_empty()));
    assert_eq!(windows(cx), before);
    assert_eq!(opened.get(), 0);
  }

  #[gpui_kit::test]
  fn close_removes_every_window(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let (this, _) = per_display(cx);
    let before = windows(cx);
    let handles = vec![plain_window(cx), plain_window(cx)];
    this.update(cx, |this, _| {
      this.windows.insert(DisplayId::new(1), handles)
    });
    cx.update(|cx| PerDisplay::close(this.clone(), cx));
    cx.run_until_parked();
    assert_eq!(windows(cx), before);
    assert!(this.read_with(cx, |this, _| this.windows.is_empty()));
  }

  #[gpui_kit::test]
  fn follow_opens_again_only_when_the_watched_part_changes(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let opened = Rc::new(Cell::new(0));
    let count = opened.clone();
    cx.update(|cx| {
      PerDisplay::follow(
        cx,
        |c| c.wallpaper.path.clone(),
        move |cx| {
          count.set(count.get() + 1);
          cx.config()
            .wallpaper
            .path
            .is_some()
            .then_some(|_: &mut App, _| Vec::new())
        },
      )
    });
    assert_eq!(opened.get(), 1);
    cx.update(|cx| cx.global_mut::<Config>().theme.screen_corners ^= true);
    assert_eq!(opened.get(), 1);
    cx.update(|cx| cx.global_mut::<Config>().wallpaper.path = Some("a.png".into()));
    assert_eq!(opened.get(), 2);
  }

  #[gpui_kit::test]
  fn a_dropped_set_is_not_reconciled(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let (this, opened) = per_display(cx);
    let weak = this.downgrade();
    drop(this);
    cx.update(|cx| reconcile(weak, cx));
    cx.run_until_parked();
    assert_eq!(opened.get(), 0);
  }
}
