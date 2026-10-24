use std::time::Duration;

use corona_config::ConfigProvider;
use gpui_kit::{
  AnyElement, App, AppContext, BorrowAppContext, Bounds, Context, FontWeight, InteractiveElement,
  IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Render,
  StatefulInteractiveElement, Styled, Subscription, Task, Window,
  assets::IconName,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px, size,
};
use rust_i18n::t;

use super::{Offset, style};
use crate::{
  Osk, OskExt,
  keymap::Mods,
  layouts::{self, Key, Kind, Layer, Special},
  predict::{Chip, Chips, current_word},
  wayland::Command,
};
use corona_surface::input_region::InputRegion;

/// A held key repeats, or opens its accents, after this long
const HOLD: Duration = Duration::from_millis(420);
const REPEAT: Duration = Duration::from_millis(55);

/// Row and column of a key
type At = (usize, usize);

/// Modifiers a latched key adds to the next one; AltGr picks characters instead
const SENT_MODS: Mods = Mods::CTRL.union(Mods::ALT).union(Mods::SUPER);

struct Popup {
  key: At,
  alternates: Vec<char>,
}

pub struct Keyboard {
  layer: Layer,
  /// One-shot, cleared by the next key
  shift: bool,
  caps: bool,
  /// Ctrl, Alt, Super and AltGr, cleared by the next key
  latched: Mods,
  pressed: Option<At>,
  /// Repeats the held key, or opens its accents
  hold: Option<Task<()>>,
  /// A key with accents types when released before they open
  pending: Option<At>,
  popup: Option<Popup>,
  /// The pointer's place minus the offset when the drag started
  drag: Option<Point<Pixels>>,
  chips: Chips,
  predicting: Option<Task<()>>,
  /// What this keyboard typed since the field changed, for apps that do not
  /// say what is before the cursor
  typed: String,
  input_region: InputRegion,
  _field: Subscription,
}

impl Keyboard {
  pub fn new(cx: &mut Context<Self>) -> Self {
    let field = cx.osk().field.clone();
    let _field = cx.observe(&field, |this, _, cx| {
      this.typed.clear();
      this.predict(cx);
    });
    // the dictionary may have loaded or changed language
    cx.observe_global::<Osk>(|this, cx| this.predict(cx))
      .detach();
    let mut keyboard = Self {
      layer: Layer::Letters,
      shift: false,
      caps: false,
      latched: Mods::empty(),
      pressed: None,
      hold: None,
      pending: None,
      popup: None,
      drag: None,
      chips: Chips::default(),
      predicting: None,
      typed: String::new(),
      input_region: InputRegion::default(),
      _field,
    };
    keyboard.predict(cx);
    keyboard
  }

  fn rows(&self, cx: &App) -> Vec<Vec<Key>> {
    layouts::rows(cx.osk().layout, self.layer)
  }

  /// What a character key types now
  fn char_of(&self, key: &Key) -> Option<char> {
    let Kind::Char {
      base,
      shifted,
      altgr,
    } = key.kind
    else {
      return None;
    };
    if let Some(altgr) = altgr.filter(|_| self.latched.contains(Mods::ALTGR)) {
      return Some(altgr);
    }
    let upper = match key.is_letter() {
      true => self.shift ^ self.caps,
      false => self.shift,
    };
    Some(if upper { shifted } else { base })
  }

  fn upper(&self) -> bool {
    self.shift ^ self.caps
  }

  /// What is before the cursor: the app's text, else what was typed here
  fn context(&self, cx: &App) -> String {
    let field = cx.osk().field.read(cx);
    field
      .as_ref()
      .and_then(|f| f.before_cursor.clone())
      .unwrap_or_else(|| self.typed.clone())
  }

  /// Nothing is suggested or learned in passwords, PINs and the like
  fn private(&self, cx: &App) -> bool {
    cx.osk().field.read(cx).as_ref().is_some_and(|f| f.private)
  }

  fn predict(&mut self, cx: &mut Context<Self>) {
    let config = &cx.config().osk;
    let (complete, spell) = (config.suggestions, config.spell_check);
    let predictor = cx.osk().predictor().filter(|_| !self.private(cx));
    let Some(predictor) = predictor else {
      self.chips = Chips::default();
      self.predicting = None;
      cx.notify();
      return;
    };
    let before = self.context(cx);
    let chips = cx
      .background_spawn(async move { predictor.lock().unwrap().predict(&before, complete, spell) });
    // a newer prediction drops this one
    self.predicting = Some(cx.spawn(async move |this, cx| {
      let chips = chips.await;
      let _ = this.update(cx, |this, cx| {
        this.chips = chips;
        cx.notify();
      });
    }));
  }

  /// Learns the word that ends before the cursor, when it is a real word
  fn learn_ended(&self, cx: &mut Context<Self>) {
    if !cx.config().osk.learn_words || self.private(cx) {
      return;
    }
    let context = self.context(cx);
    let (_, word) = current_word(&context);
    let Some(predictor) = cx.osk().predictor() else {
      return;
    };
    // misspellings are only learned when kept from the strip
    if predictor.lock().unwrap().knows(word) {
      let word = word.to_string();
      cx.update_global::<Osk, _>(|osk, cx| osk.learn(word, cx));
    }
  }

  /// Ends the one-shot Shift and the latched modifiers
  fn release_mods(&mut self) {
    self.shift = false;
    self.latched = Mods::empty();
  }

  fn type_char(&mut self, c: char, cx: &mut Context<Self>) {
    let mods = self.latched & SENT_MODS;
    if mods.is_empty() {
      if !c.is_alphabetic() && c != '\'' {
        self.learn_ended(cx);
      }
      self.typed.push(c);
    } else {
      self.typed.clear();
    }
    cx.osk().send(Command::Type {
      text: c.to_string(),
      mods,
    });
    self.release_mods();
    self.predict(cx);
    cx.notify();
  }

  fn special(&mut self, special: Special, cx: &mut Context<Self>) {
    let latch = match special {
      Special::Ctrl => Mods::CTRL,
      Special::Alt => Mods::ALT,
      Special::Super => Mods::SUPER,
      Special::AltGr => Mods::ALTGR,
      _ => Mods::empty(),
    };
    match special {
      Special::Shift => self.shift = !self.shift,
      Special::Caps => self.caps = !self.caps,
      Special::SwitchLayer => {
        self.layer = match self.layer {
          Layer::Letters => Layer::Symbols,
          Layer::Symbols => Layer::Letters,
        }
      }
      _ if !latch.is_empty() => self.latched.toggle(latch),
      Special::Space if (self.latched & SENT_MODS).is_empty() => return self.type_char(' ', cx),
      _ => {
        if matches!(special, Special::Enter | Special::Tab) {
          self.learn_ended(cx);
        }
        let mut mods = self.latched & SENT_MODS;
        mods.set(Mods::SHIFT, self.shift);
        if let Some(code) = special.evdev() {
          cx.osk().send(Command::Key { code, mods });
        }
        match special {
          Special::Backspace => drop(self.typed.pop()),
          _ => self.typed.clear(),
        }
        self.release_mods();
        self.predict(cx);
      }
    }
    cx.notify();
  }

  /// Runs `action` again and again while the key stays held
  fn repeat(
    &mut self,
    action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    cx: &mut Context<Self>,
  ) {
    self.hold = Some(cx.spawn(async move |this, cx| {
      cx.background_executor().timer(HOLD).await;
      loop {
        if this.update(cx, |this, cx| action(this, cx)).is_err() {
          return;
        }
        cx.background_executor().timer(REPEAT).await;
      }
    }));
  }

  fn down(&mut self, at: At, key: Key, cx: &mut Context<Self>) {
    self.pressed = Some(at);
    match key.kind {
      Kind::Char { base, .. } => {
        let Some(c) = self.char_of(&key) else {
          return;
        };
        let alternates = match self.layer {
          Layer::Letters => layouts::alternates(cx.osk().language, base),
          Layer::Symbols => "",
        };
        if alternates.is_empty() {
          self.type_char(c, cx);
          self.repeat(move |this, cx| this.type_char(c, cx), cx);
          return;
        }
        let upper = self.upper();
        let alternates: Vec<char> = alternates
          .chars()
          .flat_map(|a| match upper {
            true => a.to_uppercase().collect::<Vec<_>>(),
            false => vec![a],
          })
          .collect();
        self.pending = Some(at);
        self.hold = Some(cx.spawn(async move |this, cx| {
          cx.background_executor().timer(HOLD).await;
          let _ = this.update(cx, |this, cx| {
            this.pending = None;
            this.popup = Some(Popup {
              key: at,
              alternates,
            });
            cx.notify();
          });
        }));
      }
      Kind::Special(special) => {
        self.special(special, cx);
        if special.repeats() {
          self.repeat(move |this, cx| this.special(special, cx), cx);
        }
      }
    }
    cx.notify();
  }

  fn up(&mut self, cx: &mut Context<Self>) {
    self.hold = None;
    self.drag = None;
    // released before its accents opened: a tap
    if let Some((row, column)) = self.pending.take()
      && let Some(c) = self
        .rows(cx)
        .get(row)
        .and_then(|r| r.get(column))
        .and_then(|k| self.char_of(k))
    {
      self.type_char(c, cx);
    }
    self.pressed = None;
    cx.notify();
  }

  /// The pointer left a key: it stops repeating, a tap waiting for release is off
  fn left(&mut self, at: At, cx: &mut Context<Self>) {
    if self.pressed == Some(at) {
      self.hold = None;
      self.pending = None;
      self.pressed = None;
      cx.notify();
    }
  }

  fn pick(&mut self, chip: Chip, cx: &mut Context<Self>) {
    if !chip.delete.is_empty() || !chip.insert.is_empty() {
      cx.osk().send(Command::Replace {
        delete: chip.delete.clone(),
        insert: chip.insert.clone(),
      });
    }
    if self.typed.ends_with(&chip.delete) {
      self.typed.truncate(self.typed.len() - chip.delete.len());
      self.typed.push_str(&chip.insert);
    } else {
      self.typed.clear();
    }
    if cx.config().osk.learn_words && !self.private(cx) {
      cx.update_global::<Osk, _>(|osk, cx| osk.learn(chip.word, cx));
    }
    self.chips = Chips::default();
    self.predict(cx);
    cx.notify();
  }

  fn render_key(&self, at: At, key: Key, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme();
    let pressed = self.pressed == Some(at);
    let shows_altgr =
      matches!(key.kind, Kind::Char { altgr: Some(_), .. }) && self.latched.contains(Mods::ALTGR);
    let active = shows_altgr
      || match key.kind {
        Kind::Special(Special::Shift) => self.shift,
        Kind::Special(Special::Caps) => self.caps,
        Kind::Special(Special::Ctrl) => self.latched.contains(Mods::CTRL),
        Kind::Special(Special::Alt) => self.latched.contains(Mods::ALT),
        Kind::Special(Special::Super) => self.latched.contains(Mods::SUPER),
        Kind::Special(Special::AltGr) => self.latched.contains(Mods::ALTGR),
        _ => false,
      };
    let fill = match key.kind {
      _ if pressed => style::pressed_key(cx),
      _ if active => style::active_key(cx),
      Kind::Special(Special::Enter) => style::enter_key(cx),
      Kind::Char { .. } => style::char_key(cx),
      Kind::Special(_) => style::special_key(cx),
    };

    let label: AnyElement = match key.kind {
      Kind::Char { .. } => div()
        .text_size(px(18.))
        .font_weight(FontWeight(450.))
        .child(self.char_of(&key).unwrap_or_default().to_string())
        .into_any_element(),
      Kind::Special(special) => match icon(special) {
        Some(icon) => Icon::new(icon).with_size(px(20.)).into_any_element(),
        None => div()
          .text_size(px(13.))
          .font_weight(FontWeight::MEDIUM)
          .when(!active, |d| d.text_color(theme.colors.muted_foreground))
          .child(self.special_label(special, cx))
          .into_any_element(),
      },
    };
    // the shifted character, small in the corner of symbol keys
    let corner = match key.kind {
      Kind::Char { shifted, .. } if !key.is_letter() && !self.shift && !shows_altgr => {
        Some(shifted)
      }
      _ => None,
    };

    div()
      .id(at.0 * 32 + at.1)
      .relative()
      .w(width)
      .h(px(style::KEY_HEIGHT))
      .flex()
      .items_center()
      .justify_center()
      .rounded(px(style::KEY_RADIUS))
      .border_1()
      .border_color(style::key_border(cx))
      .bg(fill)
      .shadow(style::key_shadow(pressed))
      .when(pressed, |d| d.top(px(1.)))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| this.down(at, key, cx)),
      )
      .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
        if !hovered {
          this.left(at, cx)
        }
      }))
      .child(label)
      .children(corner.map(|c| {
        div()
          .absolute()
          .top(px(4.))
          .right(px(7.))
          .text_size(px(11.5))
          .line_height(px(12.))
          .text_color(theme.colors.muted_foreground)
          .child(c.to_string())
      }))
      .children(
        self
          .popup
          .as_ref()
          .filter(|p| p.key == at)
          .map(|p| self.render_popup(&p.alternates, width, cx)),
      )
      .into_any_element()
  }

  fn special_label(&self, special: Special, cx: &App) -> String {
    match special {
      Special::Ctrl => t!("app.osk.ctrl").to_string(),
      Special::Delete => t!("app.osk.delete").to_string(),
      Special::Escape => "Esc".to_string(),
      Special::Super => "Super".to_string(),
      Special::Alt => "Alt".to_string(),
      Special::AltGr => "AltGr".to_string(),
      Special::F(n) => format!("F{n}"),
      Special::Space => cx.osk().language.name().to_string(),
      Special::SwitchLayer => match self.layer {
        Layer::Letters => "?123".to_string(),
        Layer::Symbols => "ABC".to_string(),
      },
      _ => String::new(),
    }
  }

  fn render_popup(
    &self,
    alternates: &[char],
    key_width: Pixels,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let count = alternates.len() as f32;
    let width = count * style::POPUP_KEY_WIDTH + (count - 1.) * 4. + 2. * style::POPUP_PADDING;
    let options = alternates.iter().map(|&c| {
      div()
        .w(px(style::POPUP_KEY_WIDTH))
        .h(px(style::POPUP_KEY_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .text_size(px(20.))
        .hover(|d| d.bg(style::hover(cx)))
        .on_mouse_down(
          MouseButton::Left,
          cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.popup = None;
            this.type_char(c, cx);
          }),
        )
        .child(c.to_string())
    });
    div()
      .absolute()
      .bottom(px(style::POPUP_BOTTOM))
      .left((key_width - px(width)) / 2.)
      .flex()
      .gap(px(4.))
      .p(px(style::POPUP_PADDING))
      .rounded(px(12.))
      .border_1()
      .border_color(cx.theme().colors.border)
      .bg(style::popup_fill(cx))
      .occlude()
      .children(options)
      .into_any_element()
  }

  fn render_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let chips = self.chips.iter().enumerate().map(|(slot, chip)| {
      let best = slot == 1;
      div()
        .flex_1()
        .max_w(px(180.))
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .text_size(px(15.))
        .overflow_hidden()
        .when_some(chip.clone(), |d, chip| {
          d.when(best, |d| d.bg(style::best_chip(cx)))
            .hover(|d| d.bg(style::hover(cx)))
            .child(chip.label.clone())
            .on_mouse_down(
              MouseButton::Left,
              cx.listener(move |this, _: &MouseDownEvent, _, cx| this.pick(chip.clone(), cx)),
            )
        })
    });
    let button = |d: gpui_kit::Div| {
      d.h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .hover(|d| d.bg(style::hover(cx)))
    };
    div()
      .h(px(style::STRIP_HEIGHT))
      .flex()
      .items_center()
      .gap(px(10.))
      .child(
        div()
          .flex_1()
          .min_w_0()
          .flex()
          .items_center()
          .justify_center()
          .gap(px(4.))
          .children(chips),
      )
      .child(
        button(div())
          .text_size(px(12.5))
          .font_weight(FontWeight::MEDIUM)
          .child("Esc")
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| {
              let code = Special::Escape.evdev().unwrap_or_default();
              cx.osk().send(Command::Key {
                code,
                mods: Mods::empty(),
              });
            }),
          ),
      )
      .child(
        button(div())
          .child(Icon::new(IconName::ChevronDown).with_size(px(16.)))
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| {
              cx.update_global::<Osk, _>(|osk, cx| osk.hide(cx))
            }),
          ),
      )
  }
}

fn icon(special: Special) -> Option<IconName> {
  Some(match special {
    Special::Shift => IconName::ArrowBigUp,
    Special::Caps => IconName::ArrowBigUpDash,
    Special::Backspace => IconName::Delete,
    Special::Enter => IconName::CornerDownLeft,
    Special::Tab => IconName::ArrowRightToLine,
    Special::Left => IconName::ArrowLeft,
    Special::Right => IconName::ArrowRight,
    Special::Up => IconName::ArrowUp,
    Special::Down => IconName::ArrowDown,
    _ => return None,
  })
}

impl Render for Keyboard {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let viewport = window.viewport_size();
    let width = px(style::WIDTH).min(viewport.width - px(2. * style::SCREEN_GAP));
    let height = px(style::HEIGHT);
    let home = Point::new(
      (viewport.width - width) / 2.,
      viewport.height - height - px(style::SCREEN_GAP),
    );
    let offset = cx.default_global::<Offset>().0;
    let origin = Point::new(
      (home.x + offset.x).clamp(px(0.), (viewport.width - width).max(px(0.))),
      (home.y + offset.y).clamp(px(0.), (viewport.height - height).max(px(0.))),
    );
    let mut region = Bounds::new(origin, size(width, height));
    if self.popup.is_some() {
      region.origin.y -= px(style::POPUP_REACH);
      region.size.height += px(style::POPUP_REACH);
    }
    self.input_region.set(region, window);

    let inner = width - px(2. * style::PADDING_X);
    let rows = self.rows(cx).into_iter().enumerate().map(|(r, keys)| {
      let gaps = px(style::GAP) * (keys.len() as f32 - 1.);
      let unit = (inner - gaps) / layouts::ROW_UNITS;
      let keys: Vec<AnyElement> = keys
        .into_iter()
        .enumerate()
        .map(|(c, key)| self.render_key((r, c), key, unit * key.width, cx))
        .collect();
      div().flex().gap(px(style::GAP)).children(keys)
    });
    let rows: Vec<_> = rows.collect();

    div()
      .size_full()
      .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.up(cx)))
      .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
        if let Some(start) = this.drag
          && e.dragging()
        {
          cx.set_global(Offset(e.position - start));
          cx.notify();
        }
      }))
      .child(
        div()
          .absolute()
          .left(origin.x)
          .top(origin.y)
          .w(width)
          .h(height)
          .flex()
          .flex_col()
          .gap(px(style::GAP))
          .px(px(style::PADDING_X))
          .pb(px(style::PADDING_BOTTOM))
          .rounded(px(style::RADIUS))
          .border_1()
          .border_color(cx.theme().colors.border)
          .bg(style::card_fill(cx))
          .text_color(cx.theme().foreground)
          // a tap anywhere else dismisses the accents
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
              if this.popup.take().is_some() {
                cx.notify();
              }
            }),
          )
          .child(
            div()
              .h(px(style::GRIP_HEIGHT))
              .flex()
              .items_end()
              .justify_center()
              .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                  this.drag = Some(e.position - offset);
                }),
              )
              .child(
                div()
                  .w(px(36.))
                  .h(px(4.))
                  .rounded_full()
                  .bg(style::grip(cx)),
              ),
          )
          .child(self.render_strip(cx))
          .children(rows),
      )
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, Entity, TestAppContext};

  use super::*;
  use crate::{layouts::Layout, test_setup};

  struct Test {
    keyboard: Entity<Keyboard>,
    sent: flume::Receiver<Command>,
  }

  impl Test {
    fn new(layout: Layout, cx: &mut TestAppContext) -> Self {
      let sent = test_setup(layout, cx);
      let keyboard = cx.new(Keyboard::new);
      Self { keyboard, sent }
    }

    /// Presses the key showing `label` as its base, or the special key
    fn down(&self, key: impl Into<Find>, cx: &mut TestAppContext) {
      let find = key.into();
      self.keyboard.update(cx, |kb, cx| {
        let (at, key) = kb
          .rows(cx)
          .into_iter()
          .enumerate()
          .flat_map(|(r, row)| row.into_iter().enumerate().map(move |(c, k)| ((r, c), k)))
          .find(|(_, k)| match (k.kind, &find) {
            (Kind::Char { base, .. }, Find::Char(c)) => base == *c,
            (Kind::Special(s), Find::Special(f)) => s == *f,
            _ => false,
          })
          .expect("no such key");
        kb.down(at, key, cx)
      });
      cx.run_until_parked();
    }

    fn up(&self, cx: &mut TestAppContext) {
      self.keyboard.update(cx, |kb, cx| kb.up(cx));
      cx.run_until_parked();
    }

    fn tap(&self, key: impl Into<Find>, cx: &mut TestAppContext) {
      self.down(key, cx);
      self.up(cx);
    }

    /// Text typed, with any modifiers, and evdev keys pressed since the last call
    fn sent(&self) -> Vec<String> {
      self
        .sent
        .try_iter()
        .filter_map(|c| match c {
          Command::Type { text, mods } if mods.is_empty() => Some(text),
          Command::Type { text, mods } => Some(format!("{mods:?}+{text}")),
          Command::Key { code, .. } => Some(format!("key {code}")),
          _ => None,
        })
        .collect()
    }
  }

  enum Find {
    Char(char),
    Special(Special),
  }

  impl From<char> for Find {
    fn from(c: char) -> Self {
      Find::Char(c)
    }
  }

  impl From<Special> for Find {
    fn from(s: Special) -> Self {
      Find::Special(s)
    }
  }

  fn hold(by: Duration, cx: &mut TestAppContext) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
  }

  #[gpui::test]
  fn shift_is_one_shot_and_caps_stays(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwerty, cx);
    t.tap(Special::Shift, cx);
    t.tap('q', cx);
    t.tap('q', cx);
    t.tap(Special::Shift, cx);
    t.tap('1', cx);
    assert_eq!(t.sent(), ["Q", "q", "!"]);

    t.tap(Special::Caps, cx);
    t.tap('q', cx);
    t.tap('w', cx);
    // caps leaves symbols alone, shift with caps is lower case
    t.tap('1', cx);
    t.tap(Special::Shift, cx);
    t.tap('q', cx);
    assert_eq!(t.sent(), ["Q", "W", "1", "q"]);
  }

  #[gpui::test]
  fn latched_modifiers_go_with_the_next_key(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwerty, cx);
    t.tap(Special::Ctrl, cx);
    t.tap(Special::Shift, cx);
    t.tap('t', cx);
    t.tap('t', cx);
    assert_eq!(t.sent(), ["Mods(CTRL)+T", "t"]);
    // tapping again lets go
    t.tap(Special::Alt, cx);
    t.tap(Special::Alt, cx);
    t.tap('t', cx);
    assert_eq!(t.sent(), ["t"]);
  }

  #[gpui::test]
  fn altgr_picks_the_third_character(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwertz, cx);
    t.tap(Special::AltGr, cx);
    t.tap('q', cx);
    t.tap('q', cx);
    assert_eq!(t.sent(), ["@", "q"]);
  }

  #[gpui::test]
  fn accents_open_on_hold(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwerty, cx);
    // a tap types on release
    t.down('e', cx);
    assert!(t.sent().is_empty());
    t.up(cx);
    assert_eq!(t.sent(), ["e"]);

    t.tap(Special::Shift, cx);
    t.down('e', cx);
    hold(HOLD, cx);
    t.up(cx);
    assert!(t.sent().is_empty());
    let alternates = t.keyboard.read_with(cx, |kb, _| {
      kb.popup
        .as_ref()
        .map(|p| p.alternates.iter().collect::<String>())
    });
    assert_eq!(alternates.as_deref(), Some("ÉÈÊË€"));
  }

  #[gpui::test]
  fn held_keys_repeat_until_released(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwerty, cx);
    t.down(Special::Backspace, cx);
    hold(HOLD, cx);
    hold(REPEAT, cx);
    t.up(cx);
    hold(HOLD, cx);
    assert_eq!(t.sent(), ["key 14", "key 14", "key 14"]);
    // Enter does not repeat
    t.down(Special::Enter, cx);
    hold(HOLD * 2, cx);
    t.up(cx);
    assert_eq!(t.sent(), ["key 28"]);
  }

  #[gpui::test]
  fn symbols_layer_switches_back(cx: &mut TestAppContext) {
    let t = Test::new(Layout::Qwerty, cx);
    t.tap(Special::SwitchLayer, cx);
    t.tap('€', cx);
    t.tap(Special::F(5), cx);
    t.tap(Special::SwitchLayer, cx);
    t.tap('q', cx);
    assert_eq!(t.sent(), ["€", "key 63", "q"]);
  }
}
