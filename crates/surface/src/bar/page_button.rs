use std::marker::PhantomData;

use gpui_kit::{AnyElement, App, Context, IntoElement, Render, Subscription, Window, div};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  bar::{Button, Widget},
  control_center::{ControlCenterPage, Standalone},
};

/// A bar button opening the page `P` on its own, named after it
pub struct PageButton<P>(PhantomData<fn() -> P>);

impl<P: ControlCenterPage> Widget for PageButton<P> {
  const NAME: &'static str = P::ID;
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    Self(PhantomData)
  }
}

impl<P: ControlCenterPage> Render for PageButton<P> {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    button::<_, P>(cx)
  }
}

/// A page whose bar button can hide while nothing goes on there
pub trait IdlePage: ControlCenterPage {
  fn busy(cx: &App) -> bool;
  /// Re-renders the button whenever `busy` may change
  fn observe(cx: &mut Context<IdlePageButton<Self>>) -> Vec<Subscription>
  where
    Self: Sized;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IdleOptions {
  /// only show while the page is busy
  pub hide_when_idle: bool,
}

/// A [`PageButton`] that can hide while its page is idle
pub struct IdlePageButton<P> {
  options: IdleOptions,
  _subscriptions: Vec<Subscription>,
  page: PhantomData<fn() -> P>,
}

impl<P: IdlePage> Widget for IdlePageButton<P> {
  const NAME: &'static str = P::ID;
  type Options = IdleOptions;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: IdleOptions) -> Self {
    Self {
      options,
      _subscriptions: P::observe(cx),
      page: PhantomData,
    }
  }
}

impl<P: IdlePage> Render for IdlePageButton<P> {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if self.options.hide_when_idle && !P::busy(cx) {
      return div().into_any_element();
    }
    button::<_, P>(cx)
  }
}

fn button<W: Widget, P: ControlCenterPage>(cx: &mut Context<W>) -> AnyElement {
  Button::<_, Standalone<P>>::new(cx, format!("{}-button", P::ID), P::icon()).into_any_element()
}
