//! Event handlers (`on_click`, `on_key_down`, ..) and the payloads their handlers get.

use gpui_kit::{
  Action, Modifiers, MouseButton, NavigationDirection, Pixels, Point, ScrollDelta, SharedString,
  TouchPhase,
};
use rquickjs::{Ctx, Exception, FromJs, IntoJs, Result, Value};

use crate::{
  TsType,
  el::{On, Payload, interact},
  frame,
};

/// The one gpui action type a script names: its name is the identity.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptAction(pub SharedString);

impl Action for ScriptAction {
  fn boxed_clone(&self) -> Box<dyn Action> {
    Box::new(self.clone())
  }

  fn partial_eq(&self, action: &dyn Action) -> bool {
    action
      .as_any()
      .downcast_ref::<Self>()
      .is_some_and(|other| other == self)
  }

  fn name(&self) -> &'static str {
    Self::name_for_type()
  }

  fn name_for_type() -> &'static str {
    "corona_js::ScriptAction"
  }

  fn build(value: gpui_kit::private::serde_json::Value) -> anyhow::Result<Box<dyn Action>> {
    let name = value.get("name").and_then(|n| n.as_str());
    let name = name.ok_or_else(|| anyhow::anyhow!("a script action needs `{{\"name\": ..}}`"))?;
    Ok(Box::new(Self(name.to_owned().into())))
  }
}

#[derive(IntoJs, Clone, Copy)]
pub struct Mods {
  shift: bool,
  control: bool,
  alt: bool,
  /// Command on macOS, Super elsewhere
  platform: bool,
  function: bool,
}

impl From<Modifiers> for Mods {
  fn from(m: Modifiers) -> Self {
    Self {
      shift: m.shift,
      control: m.control,
      alt: m.alt,
      platform: m.platform,
      function: m.function,
    }
  }
}

#[derive(IntoJs, Clone, Copy)]
pub struct Pt {
  x: f32,
  y: f32,
}

impl From<Point<Pixels>> for Pt {
  fn from(p: Point<Pixels>) -> Self {
    Self {
      x: p.x.into(),
      y: p.y.into(),
    }
  }
}

#[derive(IntoJs)]
pub struct ClickEvent {
  click_count: u32,
  position: Pt,
  modifiers: Mods,
}

impl From<&gpui_kit::ClickEvent> for ClickEvent {
  fn from(e: &gpui_kit::ClickEvent) -> Self {
    Self {
      click_count: e.click_count() as u32,
      position: e.position().into(),
      modifiers: e.modifiers().into(),
    }
  }
}

#[derive(IntoJs)]
pub struct MouseButtonEvent {
  button: String,
  click_count: u32,
  position: Pt,
  modifiers: Mods,
}

#[derive(IntoJs)]
pub struct MouseMoveEvent {
  position: Pt,
  pressed_button: Option<String>,
  modifiers: Mods,
}

#[derive(IntoJs)]
pub struct ScrollWheelEvent {
  /// In pixels, whichever unit the device reported
  delta: Pt,
  delta_lines: Option<Pt>,
  touch_phase: String,
  position: Pt,
  modifiers: Mods,
}

#[derive(IntoJs)]
pub struct KeyEvent {
  key: String,
  /// The chord as a key binding spells it, with `cmd` for the platform key everywhere
  keystroke: String,
  key_char: Option<String>,
  modifiers: Mods,
  is_held: Option<bool>,
}

#[derive(IntoJs)]
pub struct Capslock {
  on: bool,
}

#[derive(IntoJs)]
pub struct ModifiersChangedEvent {
  modifiers: Mods,
  capslock: Capslock,
}

#[derive(IntoJs)]
pub struct ActionEvent {
  action: String,
}

macro_rules! payloads {
  ($($ty:ty => $ts:literal),* $(,)?) => {
    $(impl Payload for $ty {
      const TS: &'static str = $ts;
    })*
  };
}

payloads! {
  ClickEvent => "ClickEvent",
  MouseButtonEvent => "MouseButtonEvent",
  MouseMoveEvent => "MouseMoveEvent",
  ScrollWheelEvent => "ScrollWheelEvent",
  KeyEvent => "KeyEvent",
  ModifiersChangedEvent => "ModifiersChangedEvent",
  ActionEvent => "ActionEvent",
  bool => "boolean",
}

/// The event payload types
pub(crate) const D_TS: &str = r#"  export interface Modifiers {
    shift: boolean;
    control: boolean;
    alt: boolean;
    /** Command on macOS, Super elsewhere. */
    platform: boolean;
    function: boolean;
  }
  export interface Point { x: number; y: number; }
  export interface Size { width: number; height: number; }
  export interface Bounds extends Point, Size {}
  export type MouseButton = "left" | "right" | "middle" | "back" | "forward";
  /** What an `on_click` handler gets. Keyboard activation counts as one. */
  export interface ClickEvent { click_count: number; position: Point; modifiers: Modifiers; }
  /** What `on_mouse_down`, `on_mouse_up` and `on_mouse_down_out` handlers get. */
  export interface MouseButtonEvent {
    button: MouseButton;
    /** `2` on a double click. */
    click_count: number;
    position: Point;
    modifiers: Modifiers;
  }
  /** What an `on_mouse_move` handler gets. `position` is window-relative. */
  export interface MouseMoveEvent { position: Point; pressed_button?: MouseButton; modifiers: Modifiers; }
  /** What an `on_scroll_wheel` handler gets. */
  export interface ScrollWheelEvent {
    /** The distance in pixels, whichever unit the device reported. */
    delta: Point;
    /** The same distance in lines, when the device reported lines. */
    delta_lines?: Point;
    touch_phase: "started" | "moved" | "ended" | "cancelled";
    position: Point;
    modifiers: Modifiers;
  }
  /** What `on_key_down` and `on_key_up` handlers get. */
  export interface KeyEvent {
    /** The key, e.g. `"s"` or `"escape"`. */
    key: string;
    /** The chord as a key binding spells it, `cmd` for the platform key everywhere: `"cmd-shift-s"`. */
    keystroke: string;
    /** The character this keystroke types, if any. */
    key_char?: string;
    modifiers: Modifiers;
    /** Whether the key is held down. Absent on `on_key_up`. */
    is_held?: boolean;
  }
  /** What an `on_modifiers_changed` handler gets. */
  export interface ModifiersChangedEvent { modifiers: Modifiers; capslock: { on: boolean }; }
  /** What an `on_action` handler gets. */
  export interface ActionEvent { action: string; }
"#;

fn button_name(button: MouseButton) -> String {
  match button {
    MouseButton::Left => "left",
    MouseButton::Right => "right",
    MouseButton::Middle => "middle",
    MouseButton::Navigate(NavigationDirection::Back) => "back",
    MouseButton::Navigate(NavigationDirection::Forward) => "forward",
  }
  .into()
}

/// A mouse button by name
pub struct Button(MouseButton);

impl<'js> FromJs<'js> for Button {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let name = String::from_js(ctx, value)?;
    Ok(Button(match name.as_str() {
      "left" => MouseButton::Left,
      "right" => MouseButton::Right,
      "middle" => MouseButton::Middle,
      "back" => MouseButton::Navigate(NavigationDirection::Back),
      "forward" => MouseButton::Navigate(NavigationDirection::Forward),
      _ => {
        let names = ["left", "right", "middle", "back", "forward"];
        return Err(Exception::throw_type(
          ctx,
          &crate::style::unknown("mouse button", &name, &names),
        ));
      }
    }))
  }
}

impl TsType for Button {
  fn ts() -> String {
    "MouseButton".into()
  }
}

/// `ctrl-alt-shift-cmd-fn-key`
pub(crate) fn keystroke(m: &Modifiers, key: &str) -> String {
  let mut out = String::new();
  for (held, name) in [
    (m.control, "ctrl-"),
    (m.alt, "alt-"),
    (m.shift, "shift-"),
    (m.platform, "cmd-"),
    (m.function, "fn-"),
  ] {
    if held {
      out.push_str(name);
    }
  }
  out + key
}

fn key_event(k: &gpui_kit::Keystroke, is_held: Option<bool>) -> KeyEvent {
  KeyEvent {
    key: k.key.clone(),
    keystroke: keystroke(&k.modifiers, &k.key),
    key_char: k.key_char.clone(),
    modifiers: k.modifiers.into(),
    is_held,
  }
}

crate::el_methods! {
  /// `handler(event, cx)` on a click, or keyboard activation.
  fn on_click(handler: On<'js, ClickEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::ClickEvent| ClickEvent::from(e));
    interact(c, n, |mut i| {
      i.0.on_click(l);
      i
    })
  };
  /// `handler(hovered, cx)` when the pointer enters and leaves.
  fn on_hover(handler: On<'js, bool>) |c, n| {
    let l = frame().listener(handler.0, |hovered: &bool| *hovered);
    interact(c, n, |mut i| {
      i.0.on_hover(l);
      i
    })
  };
  /// `handler(event, cx)` when `button` is pressed over this element.
  fn on_mouse_down(button: Button, handler: On<'js, MouseButtonEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::MouseDownEvent| MouseButtonEvent {
      button: button_name(e.button),
      click_count: e.click_count as u32,
      position: e.position.into(),
      modifiers: e.modifiers.into(),
    });
    interact(c, n, |mut i| {
      i.0.on_mouse_down(button.0, l);
      i
    })
  };
  /// `handler(event, cx)` when `button` is released over this element.
  fn on_mouse_up(button: Button, handler: On<'js, MouseButtonEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::MouseUpEvent| MouseButtonEvent {
      button: button_name(e.button),
      click_count: e.click_count as u32,
      position: e.position.into(),
      modifiers: e.modifiers.into(),
    });
    interact(c, n, |mut i| {
      i.0.on_mouse_up(button.0, l);
      i
    })
  };
  /// `handler(event, cx)` on a press anywhere outside this element.
  fn on_mouse_down_out(handler: On<'js, MouseButtonEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::MouseDownEvent| MouseButtonEvent {
      button: button_name(e.button),
      click_count: e.click_count as u32,
      position: e.position.into(),
      modifiers: e.modifiers.into(),
    });
    interact(c, n, |mut i| {
      i.0.on_mouse_down_out(l);
      i
    })
  };
  /// `handler(event, cx)` while the pointer moves over this element.
  fn on_mouse_move(handler: On<'js, MouseMoveEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::MouseMoveEvent| MouseMoveEvent {
      position: e.position.into(),
      pressed_button: e.pressed_button.map(button_name),
      modifiers: e.modifiers.into(),
    });
    interact(c, n, |mut i| {
      i.0.on_mouse_move(l);
      i
    })
  };
  /// `handler(event, cx)` on wheel and trackpad scrolling over this element.
  fn on_scroll_wheel(handler: On<'js, ScrollWheelEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::ScrollWheelEvent| ScrollWheelEvent {
      // gpui's default line height, the event has no window to ask
      delta: e.delta.pixel_delta(gpui_kit::px(20.)).into(),
      delta_lines: match e.delta {
        ScrollDelta::Lines(p) => Some(Pt { x: p.x, y: p.y }),
        ScrollDelta::Pixels(_) => None,
      },
      touch_phase: match e.touch_phase {
        TouchPhase::Started => "started",
        TouchPhase::Moved => "moved",
        TouchPhase::Ended => "ended",
        TouchPhase::Cancelled => "cancelled",
      }
      .into(),
      position: e.position.into(),
      modifiers: e.modifiers.into(),
    });
    interact(c, n, |mut i| {
      i.0.on_scroll_wheel(l);
      i
    })
  };
  /// `handler(event, cx)` on a key press while this element or a descendant has focus.
  fn on_key_down(handler: On<'js, KeyEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::KeyDownEvent| {
      key_event(&e.keystroke, Some(e.is_held))
    });
    interact(c, n, |mut i| {
      i.0.on_key_down(l);
      i
    })
  };
  /// `handler(event, cx)` on a key release while this element or a descendant has focus.
  fn on_key_up(handler: On<'js, KeyEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::KeyUpEvent| key_event(&e.keystroke, None));
    interact(c, n, |mut i| {
      i.0.on_key_up(l);
      i
    })
  };
  /// `handler(event, cx)` when a modifier key changes while focused.
  fn on_modifiers_changed(handler: On<'js, ModifiersChangedEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &gpui_kit::ModifiersChangedEvent| {
      ModifiersChangedEvent {
        modifiers: e.modifiers.into(),
        capslock: Capslock { on: e.capslock.on },
      }
    });
    interact(c, n, |mut i| {
      i.0.on_modifiers_changed(l);
      i
    })
  };
  /// `handler(event, cx)` when the action named `action` is dispatched to this element or
  /// inside it, by a key binding (`cx.bind_keys`) or `window.dispatch_action`.
  fn on_action(action: String, handler: On<'js, ActionEvent>) |c, n| {
    let l = frame().listener(handler.0, |e: &ScriptAction| ActionEvent { action: e.0.to_string() });
    interact(c, n, |mut i| {
      i.0.on_action(move |a: &ScriptAction, window, cx| match a.0.as_ref() == action {
        true => l(a, window, cx),
        false => cx.propagate(),
      });
      i
    })
  };
}
