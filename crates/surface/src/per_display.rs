use std::{
  collections::{HashMap, HashSet},
  rc::Rc,
  time::Duration,
};

use corona_compositor::CompositorExt;
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

  for display in current.difference(&wanted) {
    let windows = this.update(cx, |this, _| this.windows.remove(display));
    for handle in windows.into_iter().flatten() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
  }
  for display in wanted.difference(&current) {
    let windows = open(cx, *display);
    this.update(cx, |this, _| this.windows.insert(*display, windows));
  }
}
