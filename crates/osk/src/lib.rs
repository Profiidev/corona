//! The on-screen keyboard: a floating key board that types into the focused
//! window without taking its focus, shown when a text field gets focus

pub mod cli;
pub mod commands;
pub mod hinge;
pub mod keymap;
pub mod layouts;
pub mod predict;
pub mod ui;
pub mod wayland;

use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use corona_compositor::CompositorExt;
use corona_config::{
  ConfigProvider, observe_section,
  osk::{AutoShow, OskLanguage, OskLayout, TabletSource},
};
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global, Task};

use crate::{
  layouts::Layout,
  predict::{Language, Predictor, clear_learned_files},
  wayland::{Command, Commands, Event, Field},
};

rust_i18n::i18n!("../../assets/locales", fallback = "en");

/// Focus moving from one field to the next deactivates first; a hide waits
/// this long for the next field
const HIDE_DELAY: Duration = Duration::from_millis(200);
/// Learned words are written once typing pauses this long
const SAVE_DELAY: Duration = Duration::from_secs(2);
/// How often the hinge angle is read
const HINGE_POLL: Duration = Duration::from_millis(500);

pub struct Osk {
  /// On screen
  pub visible: Entity<bool>,
  /// Tablet mode, set over IPC from a Hyprland switch bind
  pub tablet: Entity<bool>,
  /// The focused text field
  pub field: Entity<Option<Field>>,
  commands: Option<Commands>,
  /// Shown by a focused field, so leaving it hides again
  auto_shown: bool,
  hide_later: Option<Task<()>>,
  /// The keys shown and the layout the virtual keyboard has
  pub layout: Layout,
  /// The language of suggestions and accents
  pub language: Language,
  predictor: Option<Arc<Mutex<Predictor>>>,
  loading: Option<Task<()>>,
  save_later: Option<Task<()>>,
  /// Watches the hinge sensor while it is the tablet source
  hinge: Option<Task<()>>,
}

impl Global for Osk {}

/// Starts the keyboard and registers its IPC commands
pub fn init(cx: &mut App, _: &corona_utils::Buses) {
  start(cx);
  commands::register(cx);
}

/// Connects to the compositor and follows its keyboard layout and the settings
fn start(cx: &mut App) {
  let enabled = cx.config().osk.enabled;
  let (events, events_rx) = flume::unbounded();
  let commands = wayland::spawn(enabled, events).log_err().ok();
  cx.spawn(async move |cx| {
    while let Ok(event) = events_rx.recv_async().await {
      cx.update(|cx| on_event(event, cx));
    }
  })
  .detach();

  let osk = Osk {
    visible: cx.new(|_| false),
    tablet: cx.new(|_| false),
    field: cx.new(|_| None),
    commands,
    auto_shown: false,
    hide_later: None,
    layout: resolve_layout(cx),
    language: resolve_language(cx),
    predictor: None,
    loading: None,
    save_later: None,
    hinge: None,
  };
  osk.send(Command::Keymap(osk.layout.xkb()));
  let visible = osk.visible.clone();
  cx.set_global(osk);
  ui::watch_closed(visible, cx);
  sync(cx);

  let keyboard_layout = cx.compositor().keyboard_layout.clone();
  cx.observe(&keyboard_layout, |_, cx| sync(cx)).detach();
  observe_section(cx, |c| &c.osk, |_, cx| sync(cx));
  observe_section(
    cx,
    |c| &c.osk.enabled,
    |_, _| tracing::warn!("osk.enabled applies after a restart"),
  );
}

/// The keys shown, from the settings or the compositor's layout
fn resolve_layout(cx: &App) -> Layout {
  match cx.config().osk.layout {
    OskLayout::Auto => Layout::from_compositor(cx.compositor().keyboard_layout(cx)),
    OskLayout::Qwerty => Layout::Qwerty,
    OskLayout::Qwertz => Layout::Qwertz,
  }
}

fn resolve_language(cx: &App) -> Language {
  match cx.config().osk.language {
    OskLanguage::Auto if resolve_layout(cx) == Layout::Qwertz => Language::De,
    OskLanguage::Auto | OskLanguage::En => Language::En,
    OskLanguage::De => Language::De,
  }
}

/// Whether a focused field shows the keyboard
pub fn shows_on_focus(auto_show: AutoShow, tablet: bool) -> bool {
  match auto_show {
    AutoShow::Never => false,
    AutoShow::Always => true,
    AutoShow::TabletMode => tablet,
  }
}

/// Brings the keymap and dictionary in line with the layout and language
fn sync(cx: &mut App) {
  let layout = resolve_layout(cx);
  let language = resolve_language(cx);
  cx.update_global::<Osk, _>(|osk, cx| {
    if osk.layout != layout {
      osk.layout = layout;
      osk.send(Command::Keymap(layout.xkb()));
    }
    if osk.language != language {
      osk.language = language;
      if osk.predictor.is_some() || osk.loading.is_some() {
        osk.load_predictor(language, cx);
      }
    }
    match cx.config().osk.tablet_source {
      TabletSource::Hinge if osk.hinge.is_none() => osk.hinge = Some(watch_hinge(cx)),
      TabletSource::Hinge => {}
      TabletSource::Ipc => osk.hinge = None,
    }
  });
  cx.refresh_windows();
}

/// Sets tablet mode whenever the hinge crosses a threshold, so IPC holds
/// until it moves again
fn watch_hinge(cx: &mut App) -> Task<()> {
  cx.spawn(async move |cx| {
    let mut hinge = None;
    let mut tablet = None;
    loop {
      let angle = cx
        .background_spawn(async move {
          let hinge = hinge.or_else(|| hinge::Hinge::find(hinge::IIO_DEVICES.as_ref()));
          let angle = hinge.as_ref().and_then(hinge::Hinge::angle);
          // a sensor that stopped answering is looked for again: its number can change
          (angle.and(hinge), angle)
        })
        .await;
      hinge = angle.0;
      match angle.1 {
        Some(angle) => {
          let now = hinge::is_tablet(angle, tablet.unwrap_or(false));
          if tablet != Some(now) {
            tablet = Some(now);
            cx.update(|cx| cx.update_global::<Osk, _>(|osk, cx| osk.set_tablet(now, cx)));
          }
        }
        None if tablet.is_none() => {
          tracing::warn!(
            "no hinge sensor in {}, tablet mode stays off",
            hinge::IIO_DEVICES
          );
          return;
        }
        None => {}
      }
      cx.background_executor().timer(HINGE_POLL).await;
    }
  })
}

fn on_event(event: Event, cx: &mut App) {
  cx.update_global::<Osk, _>(|osk, cx| match event {
    Event::Unavailable => {
      tracing::warn!("another input method holds the seat, the keyboard shows only on request");
      osk.field.write_changed(cx, None);
    }
    Event::Stopped => {
      tracing::warn!("the on-screen keyboard thread stopped, keys are no longer sent");
      osk.commands = None;
      osk.field.write_changed(cx, None);
    }
    Event::Field(field) => {
      let focused = field.is_some();
      // every done of the same field reports again, which must not reopen a hidden keyboard
      let gained = focused && osk.field.read(cx).is_none();
      osk.field.write_changed(cx, field);
      if focused {
        osk.hide_later = None;
        let wanted = shows_on_focus(cx.config().osk.auto_show, *osk.tablet.read(cx));
        if gained && wanted && !*osk.visible.read(cx) {
          osk.auto_shown = true;
          osk.open(cx);
        }
      } else if osk.auto_shown {
        osk.hide_later = Some(cx.spawn(async move |cx| {
          cx.background_executor().timer(HIDE_DELAY).await;
          cx.update(|cx| cx.update_global::<Osk, _>(|osk, cx| osk.hide(cx)));
        }));
      }
    }
  });
}

impl Osk {
  pub fn send(&self, command: Command) {
    if let Some(commands) = &self.commands {
      commands.send(command);
    }
  }

  pub fn is_visible(&self, cx: &App) -> bool {
    *self.visible.read(cx)
  }

  pub fn show(&mut self, cx: &mut App) {
    self.auto_shown = false;
    self.open(cx);
  }

  pub fn hide(&mut self, cx: &mut App) {
    self.auto_shown = false;
    self.hide_later = None;
    // not from inside the keyboard's own window, which cannot be updated then;
    // ordered with open's deferred write
    let visible = self.visible.clone();
    cx.defer(move |cx| {
      ui::close(cx);
      visible.write_changed(cx, false);
    });
  }

  pub fn toggle(&mut self, cx: &mut App) {
    match self.is_visible(cx) {
      true => self.hide(cx),
      false => self.show(cx),
    }
  }

  /// Entering arms showing on focus, and shows now when a field has it;
  /// leaving hides
  pub fn set_tablet(&mut self, on: bool, cx: &mut App) {
    let was = *self.tablet.read(cx);
    self.tablet.write_changed(cx, on);
    let focused = self.field.read(cx).is_some();
    let auto_show = cx.config().osk.auto_show;
    if !on {
      // only leaving tablet mode hides, not the first reading of a laptop
      if was {
        self.hide(cx);
      }
    } else if focused && shows_on_focus(auto_show, true) && !self.is_visible(cx) {
      self.auto_shown = true;
      self.open(cx);
    }
  }

  fn open(&mut self, cx: &mut App) {
    self.hide_later = None;
    if self.predictor.is_none() && self.loading.is_none() {
      self.load_predictor(self.language, cx);
    }
    // not while the caller holds this global: the new view reads it
    let visible = self.visible.clone();
    cx.defer(move |cx| {
      if ui::open(cx).log_err().is_ok() {
        visible.write_changed(cx, true);
      }
    });
  }

  fn load_predictor(&mut self, language: Language, cx: &mut App) {
    let load = cx.background_spawn(async move { Predictor::new(language) });
    self.loading = Some(cx.spawn(async move |cx| {
      let predictor = Arc::new(Mutex::new(load.await));
      cx.update(|cx| {
        cx.update_global::<Osk, _>(|osk, _| {
          osk.predictor = Some(predictor);
          osk.loading = None;
        });
        cx.refresh_windows();
      });
    }));
  }

  /// The dictionary, once loaded
  pub fn predictor(&self) -> Option<Arc<Mutex<Predictor>>> {
    self.predictor.clone()
  }

  /// Counts a word the user typed, written to disk once typing pauses
  pub fn learn(&mut self, word: String, cx: &mut App) {
    let Some(predictor) = self.predictor.clone() else {
      return;
    };
    predictor.lock().unwrap().learn(&word);
    self.save_later = Some(cx.spawn(async move |cx| {
      cx.background_executor().timer(SAVE_DELAY).await;
      // copied, so the file write holds no lock
      let snapshot = predictor.lock().unwrap().clone();
      let save = cx.background_spawn(async move { snapshot.save() });
      let _ = save.await.log_err();
    }));
  }

  /// Forgets every learned word
  pub fn clear_learned(&mut self, cx: &mut App) {
    self.save_later = None;
    let predictor = self.predictor.clone();
    cx.background_spawn(async move {
      let cleared = match predictor {
        Some(p) => p.lock().unwrap().clear_learned(),
        None => clear_learned_files(),
      };
      let _ = cleared.log_err();
    })
    .detach();
  }
}

corona_utils::global_ext!(
  /// Reads the keyboard; changes go through `cx.update_global::<Osk, _>`
  OskExt,
  osk,
  Osk
);

/// The keyboard global with its commands in a queue, no compositor needed
#[cfg(test)]
pub(crate) fn test_setup(
  layout: Layout,
  cx: &mut gpui_kit::TestAppContext,
) -> flume::Receiver<Command> {
  cx.update(|cx| {
    cx.set_global(corona_config::Config::default());
    let (commands, rx) = wayland::channel();
    let osk = Osk {
      visible: cx.new(|_| false),
      tablet: cx.new(|_| false),
      field: cx.new(|_| None),
      commands: Some(commands),
      auto_shown: false,
      hide_later: None,
      layout,
      language: Language::En,
      predictor: None,
      loading: None,
      save_later: None,
      hinge: None,
    };
    cx.set_global(osk);
    rx
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn focus_shows_it_as_configured() {
    assert!(!shows_on_focus(AutoShow::Never, true));
    assert!(shows_on_focus(AutoShow::Always, false));
    assert!(shows_on_focus(AutoShow::TabletMode, true));
    assert!(!shows_on_focus(AutoShow::TabletMode, false));
  }
}
