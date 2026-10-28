//! What host functions reach while JS runs: raw pointers set only by [`Enter`].

use std::{cell::Cell, marker::PhantomData, ptr};

use gpui_kit::{App, Window};

use crate::Frame;

type Ptrs = (*mut App, *mut Window, *mut Frame);

thread_local! {
  static SCOPE: Cell<Ptrs> = const { Cell::new((ptr::null_mut(), ptr::null_mut(), ptr::null_mut())) };
}

/// Sets the scope for one call into JS (init, render, event dispatch, driving futures),
/// and restores the outer one on drop, also on panic. Borrows what it points to, so
/// nothing else touches it meanwhile.
pub struct Enter<'a> {
  outer: Ptrs,
  _borrow: PhantomData<&'a mut ()>,
}

impl<'a> Enter<'a> {
  pub fn new(
    app: &'a mut App,
    window: Option<&'a mut Window>,
    frame: Option<&'a mut Frame>,
  ) -> Self {
    let window = window.map_or(ptr::null_mut(), ptr::from_mut);
    let frame = frame.map_or(ptr::null_mut(), ptr::from_mut);
    Self {
      outer: SCOPE.replace((app, window, frame)),
      _borrow: PhantomData,
    }
  }
}

impl Drop for Enter<'_> {
  fn drop(&mut self) {
    SCOPE.set(self.outer);
  }
}

/// The app of the current call into JS
pub fn app() -> &'static mut App {
  let p = SCOPE.get().0;
  assert!(!p.is_null(), "host fn called outside init/render/dispatch");
  unsafe { &mut *p }
}

/// The window of the current init, render or event dispatch
pub fn window() -> &'static mut Window {
  let p = SCOPE.get().1;
  assert!(
    !p.is_null(),
    "host fn needs a window, called outside init/render/dispatch"
  );
  unsafe { &mut *p }
}

/// The element arena of the current render
pub fn frame() -> &'static mut Frame {
  let p = SCOPE.get().2;
  assert!(!p.is_null(), "element built outside render");
  unsafe { &mut *p }
}
