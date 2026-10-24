//! The on-screen keyboard: a floating key board that types into the focused
//! window without taking its focus, shown when a text field gets focus

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
  osk::{AutoShow, OskLanguage, OskLayout},
};
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global, Task};

use crate::{
  layouts::Layout,
  predict::{Language, Predictor},
  wayland::{Command, Commands, Event, Field},
};

rust_i18n::i18n!("../../assets/locales", fallback = "en");

/// Focus moving from one field to the next deactivates first; a hide waits
/// this long for the next field
const HIDE_DELAY: Duration = Duration::from_millis(200);
/// Learned words are written once typing pauses this long
const SAVE_DELAY: Duration = Duration::from_secs(2);

pub struct Osk {
  /// On screen
  pub visible: Entity<bool>,
  /// Tablet mode, set over IPC from a Hyprland switch bind
  pub tablet: Entity<bool>,
  /// The focused text field
  pub field: Entity<Option<Field>>,
  /// The seat's input method is this keyboard's, so focused fields are known
  pub input_method: Entity<bool>,
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
}

impl Global for Osk {}

/// Connects to the compositor and follows its keyboard layout and the settings
pub fn init(cx: &mut App) {
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
    input_method: cx.new(|_| enabled),
    commands,
    auto_shown: false,
    hide_later: None,
    layout: resolve_layout(cx),
    language: resolve_language(cx),
    predictor: None,
    loading: None,
    save_later: None,
  };
  osk.send(Command::Keymap(osk.layout.xkb()));
  cx.set_global(osk);

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
      if osk.predictor.is_some() {
        osk.load_predictor(language, cx);
      }
    }
  });
  cx.refresh_windows();
}

fn on_event(event: Event, cx: &mut App) {
  cx.update_global::<Osk, _>(|osk, cx| match event {
    Event::Unavailable => {
      tracing::warn!("another input method holds the seat, the keyboard shows only on request");
      osk.input_method.write_changed(cx, false);
    }
    Event::Field(field) => {
      let focused = field.is_some();
      osk.field.write_changed(cx, field);
      if focused {
        osk.hide_later = None;
        let wanted = shows_on_focus(cx.config().osk.auto_show, *osk.tablet.read(cx));
        if wanted && !*osk.visible.read(cx) {
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
    // not from inside the keyboard's own window, which cannot be updated then
    cx.defer(ui::close);
    self.visible.write_changed(cx, false);
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
    self.tablet.write_changed(cx, on);
    let focused = self.field.read(cx).is_some();
    let auto_show = cx.config().osk.auto_show;
    if !on {
      self.hide(cx);
    } else if focused && shows_on_focus(auto_show, true) && !self.is_visible(cx) {
      self.auto_shown = true;
      self.open(cx);
    }
  }

  fn open(&mut self, cx: &mut App) {
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
      let save = cx.background_spawn(async move { predictor.lock().unwrap().save() });
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
        None => Predictor::new(Language::En).clear_learned(),
      };
      let _ = cleared.log_err();
    })
    .detach();
  }
}

/// Reads the keyboard; changes go through `cx.update_global::<Osk, _>`
pub trait OskExt {
  fn osk(&self) -> &Osk;
}

impl OskExt for App {
  fn osk(&self) -> &Osk {
    self.global::<Osk>()
  }
}

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
      input_method: cx.new(|_| true),
      commands: Some(commands),
      auto_shown: false,
      hide_later: None,
      layout,
      language: Language::En,
      predictor: None,
      loading: None,
      save_later: None,
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
