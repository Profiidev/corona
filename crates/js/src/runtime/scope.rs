//! What host functions reach while JS runs: raw pointers set only by [`Enter`].

use std::{
  cell::Cell,
  marker::PhantomData,
  ops::{Deref, DerefMut},
  ptr::{self, NonNull},
  time::Instant,
};

use gpui_kit::{App, Window};

use crate::{Frame, runtime::limits};

type Ptrs = (*mut App, *mut Window, *mut Frame);

thread_local! {
  static SCOPE: Cell<Ptrs> = const { Cell::new((ptr::null_mut(), ptr::null_mut(), ptr::null_mut())) };
  /// Which of the three are held by a [`Scoped`], as bits
  static TAKEN: Cell<u8> = const { Cell::new(0) };
}

/// Sets the scope for one call into JS (init, render, event dispatch, driving futures),
/// and restores the outer one on drop, also on panic. Borrows what it points to, so
/// nothing else touches it meanwhile.
pub struct Enter<'a> {
  outer: Ptrs,
  outer_taken: u8,
  deadline: Option<Instant>,
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
      outer_taken: TAKEN.replace(0),
      deadline: limits::start(),
      _borrow: PhantomData,
    }
  }
}

impl Drop for Enter<'_> {
  fn drop(&mut self) {
    SCOPE.set(self.outer);
    TAKEN.set(self.outer_taken);
    limits::restore(self.deadline);
  }
}

/// Exclusive access to the app, window or frame of the current call into JS. While it
/// lives, a second one of the same kind (also across a call back into JS) finds nothing
/// instead of aliasing a `&mut`. Not `'static`: it ends with the statement or scope that
/// holds it, and that is inside the [`Enter`].
pub struct Scoped<T: 'static> {
  ptr: NonNull<T>,
  bit: u8,
}

const APP: u8 = 1;
const WINDOW: u8 = 2;
const FRAME: u8 = 4;

impl<T> Scoped<T> {
  fn take(ptr: *mut T, bit: u8) -> Option<Self> {
    let taken = TAKEN.get();
    if taken & bit != 0 {
      return None;
    }
    let ptr = NonNull::new(ptr)?;
    TAKEN.set(taken | bit);
    Some(Self { ptr, bit })
  }
}

impl<T> Deref for Scoped<T> {
  type Target = T;
  fn deref(&self) -> &T {
    // SAFETY: set by a live `Enter`, which borrows the target; `take` made this the only
    // access until drop, and the thread-local keeps it on its thread
    unsafe { self.ptr.as_ref() }
  }
}

impl<T> DerefMut for Scoped<T> {
  fn deref_mut(&mut self) -> &mut T {
    // SAFETY: as in `deref`
    unsafe { self.ptr.as_mut() }
  }
}

impl<T> Drop for Scoped<T> {
  fn drop(&mut self) {
    TAKEN.set(TAKEN.get() & !self.bit);
  }
}

/// The app of the current call into JS, without taking it: for [`Cx`](crate::Cx) and
/// [`Glob`](crate::Glob), which live across the polls of async host fns and so cannot hold
/// a [`Scoped`].
///
/// # Safety
/// The caller must not keep the reference past the call into JS (the pointer is only valid
/// while the [`Enter`] lives), nor use it while another such reference is in use.
pub(crate) unsafe fn app_unchecked<'a>() -> &'a mut App {
  let p = SCOPE.get().0;
  assert!(
    !p.is_null() && TAKEN.get() & APP == 0,
    "host fn called outside init/render/dispatch, or the app is in use"
  );
  // SAFETY: not null, so set by a live `Enter`, and no `Scoped` holds it; the rest is the
  // caller's
  unsafe { &mut *p }
}

/// The app of the current call into JS
pub fn app() -> Scoped<App> {
  try_app().expect("host fn called outside init/render/dispatch, or the app is in use")
}

/// The app of the current call into JS, if any and not in use
pub(crate) fn try_app() -> Option<Scoped<App>> {
  Scoped::take(SCOPE.get().0, APP)
}

/// The window of the current init, render or event dispatch, if any and not in use
pub(crate) fn try_window() -> Option<Scoped<Window>> {
  Scoped::take(SCOPE.get().1, WINDOW)
}

/// The window of the current init, render or event dispatch
pub fn window() -> Scoped<Window> {
  try_window()
    .expect("host fn needs a window, called outside init/render/dispatch, or it is in use")
}

/// The element arena of the current render
pub fn frame() -> Scoped<Frame> {
  try_frame().expect("element built outside render, or the frame is in use")
}

pub(crate) fn try_frame() -> Option<Scoped<Frame>> {
  Scoped::take(SCOPE.get().2, FRAME)
}

/// Whether an init, render or event dispatch has a window, also while it is in use
pub(crate) fn has_window() -> bool {
  !SCOPE.get().1.is_null()
}

/// Whether a render is running, also while its frame is in use
pub(crate) fn rendering() -> bool {
  !SCOPE.get().2.is_null()
}
