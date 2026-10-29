//! Form controls: buttons, toggles, text fields, sliders, pickers, selects and forms.
//!
//! `"gpui-component"` exports the styled widgets ([`FAMILY`]), `"gpui-base"` the unstyled
//! primitives of the same names ([`BASE`]), which a script draws itself. Both take the same
//! retained text, slider and code states.

use gpui_kit::{
  AnyElement, App, AppContext, Axis, ElementId, Entity, Focusable, Hsla, IntoElement, Keystroke,
  ParentElement, Refineable, RenderOnce, SharedString, StyleRefinement, Styled, WeakEntity, Window,
  base::{self, CheckboxState, StepAction},
  component::{
    Disableable, FocusableExt, Selectable, Sizable,
    button::{Button, ButtonVariant, ButtonVariants, Toggle, ToggleVariants},
    checkbox::Checkbox,
    clipboard::Clipboard,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    combobox::{Combobox, ComboboxEvent, ComboboxState},
    date_picker::{DatePicker, DatePickerEvent, DatePickerState},
    form::{Field, Form},
    input::{
      Input, InputEvent, InputGroup, InputGroupAddon, InputGroupAddonAlignment, InputGroupButton,
      InputGroupText, InputState, NumberInput, OtpEvent, OtpInput, OtpState, Textarea,
      TextareaState,
    },
    kbd::Kbd,
    label::Label,
    link::Link,
    radio::{Radio, RadioGroup},
    rating::Rating,
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
    slider::{Slider, SliderEvent, SliderScale, SliderState, SliderValue},
    switch::Switch,
    time_field::{TimeField, TimeFieldEvent, TimeFieldState},
  },
  div, px, relative,
};
use rquickjs::{Class, Coerced, Ctx, Exception, FromJs, Function, IntoJs, Object, Result, Value};

use super::{Family, SizeArg, ctor, declare, is, listen, on_entity, take};
use crate::{
  Child, Color, El, Id, Node, On, Opt, Payload, ScriptView, app, el::styled, events::ClickEvent,
  frame, frame_or_throw, map, miss, new_el, runtime::runtime, view::window_or_throw,
};

crate::node!(Button, "Button", style, interactive, parent);
crate::node!(Link, "Link", style, parent);
crate::node!(Checkbox, "Checkbox", style, interactive, parent);
crate::node!(Switch, "Switch", style);
crate::node!(Radio, "Radio", style, interactive, parent);
crate::node!(RadioGroup, "RadioGroup", style);
crate::node!(Toggle, "Toggle", style, parent);
crate::node!(Label, "Label", style);
crate::node!(Kbd, "Kbd", style);
crate::node!(Clipboard, "Clipboard");
crate::node!(Rating, "Rating", style);
crate::node!(NumberInput, "NumberInput", style);
crate::node!(OtpInput, "OtpInput");
crate::node!(Slider, "Slider", style);
crate::node!(ColorPicker, "ColorPicker", style);
crate::node!(DatePicker, "DatePicker", style);
crate::node!(TimeField, "TimeField", style);
crate::node!(Select<Items>, "Select", style);
crate::node!(Form, "Form", style);
crate::node!(Field, "Field", style, parent);
crate::node!(InputGroup, "InputGroup", style);
crate::node!(InputGroupAddon, "InputGroupAddon", style, parent);
crate::node!(
  InputGroupButton,
  "InputGroupButton",
  style,
  interactive,
  parent
);
crate::node!(InputGroupText, "InputGroupText", style, parent);

crate::node!(base::Button, "Button", style, interactive, parent);
crate::node!(base::Link, "Link", style, interactive, parent);
crate::node!(base::Checkbox, "Checkbox", style, interactive, parent);
crate::node!(base::Switch, "Switch", style, interactive, parent);
crate::node!(base::Radio, "Radio", style, interactive, parent);
crate::node!(base::Toggle, "Toggle", style, interactive, parent);
crate::node!(base::RadioGroup, "RadioGroup", style, interactive, parent);
crate::node!(base::ToggleGroup, "ToggleGroup", style, interactive, parent);
crate::node!(base::Select, "Select", style, parent);
crate::node!(base::Combobox, "Combobox", style, parent);
crate::node!(base::DatePicker, "DatePicker", style, interactive, parent);
crate::node!(base::Input, "Input");
crate::node!(base::Textarea, "Textarea");
crate::node!(base::NumberInput, "NumberInput", style, parent);
crate::node!(base::Slider, "Slider", style, parent);
crate::node!(base::SliderTrack, "SliderTrack", style, interactive, parent);
crate::node!(Indicator, "SliderIndicator", style, parent);
crate::node!(Thumb, "SliderThumb", style, parent);
crate::node!(Cells, "OtpInput", style, parent);

/// A widget with the retained state it was made from, for methods that reach the state
pub struct Stated<T, S: 'static> {
  kind: &'static str,
  el: T,
  state: Entity<S>,
}

type TextIn = Stated<Input, InputState>;
type TextArea = Stated<Textarea, TextareaState>;
type Combo = Stated<Combobox<Items>, ComboboxState<Items>>;

impl<T, S> Stated<T, S> {
  fn with(mut self, f: impl FnOnce(T) -> T) -> Self {
    self.el = f(self.el);
    self
  }
}

impl<T: IntoElement + Styled + 'static, S: 'static> Node for Stated<T, S> {
  fn kind(&self) -> &'static str {
    self.kind
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.el))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.el.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
    self
  }
}

/// Every kind this family makes
const KINDS: &[&str] = &[
  "Button",
  "Link",
  "Checkbox",
  "Switch",
  "Radio",
  "RadioGroup",
  "Toggle",
  "ToggleGroup",
  "Label",
  "Kbd",
  "Clipboard",
  "Rating",
  "Input",
  "NumberInput",
  "Textarea",
  "OtpInput",
  "Slider",
  "SliderTrack",
  "SliderIndicator",
  "SliderThumb",
  "ColorPicker",
  "DatePicker",
  "TimeField",
  "Select",
  "Combobox",
  "Form",
  "Field",
  "InputGroup",
  "InputGroupAddon",
  "InputGroupButton",
  "InputGroupText",
];

choice! {
  AxisArg("axis"): Axis { "horizontal" => Axis::Horizontal, "vertical" => Axis::Vertical }
  Align("align"): &'static str { "start" => "start", "center" => "center", "end" => "end" }
  AddonAlign("align"): InputGroupAddonAlignment {
    "inline-start" => InputGroupAddonAlignment::InlineStart,
    "inline-end" => InputGroupAddonAlignment::InlineEnd,
    "block-start" => InputGroupAddonAlignment::BlockStart,
    "block-end" => InputGroupAddonAlignment::BlockEnd,
  }
  Variant("variant"): ButtonVariant {
    "default" => ButtonVariant::default(),
    "primary" => ButtonVariant::Primary,
    "secondary" => ButtonVariant::Secondary,
    "danger" => ButtonVariant::Danger,
    "warning" => ButtonVariant::Warning,
    "success" => ButtonVariant::Success,
    "info" => ButtonVariant::Info,
    "ghost" => ButtonVariant::Ghost,
    "link" => ButtonVariant::Link,
    "text" => ButtonVariant::Text,
  }
}

/// A string or a number, whichever the kind takes
pub struct Scalar<'js>(Value<'js>);

impl<'js> FromJs<'js> for Scalar<'js> {
  fn from_js(_: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Scalar(value))
  }
}

impl crate::TsType for Scalar<'_> {
  fn ts() -> String {
    "string | number".into()
  }
}

/// The payload types of the handlers here
macro_rules! markers {
  ($($ty:ident => $ts:literal),* $(,)?) => {
    $(pub struct $ty;
    impl Payload for $ty {
      const TS: &'static str = $ts;
    })*
  };
}

markers! {
  Changed => "any",
  Step => "\"increment\" | \"decrement\"",
  Nothing => "{}",
}

/// A `Slider` part of `gpui-base` that the shell places: the fill of the indicator, the thumb
#[derive(IntoElement)]
pub struct Indicator {
  inner: base::SliderIndicator,
  state: Entity<base::slider::SliderState>,
  axis: Axis,
  range: Option<StyleRefinement>,
}

#[derive(IntoElement)]
pub struct Thumb {
  inner: base::SliderThumb,
  state: Entity<base::slider::SliderState>,
  axis: Axis,
  start: bool,
}

impl Styled for Indicator {
  fn style(&mut self) -> &mut StyleRefinement {
    self.inner.style()
  }
}

impl ParentElement for Indicator {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.inner.extend(elements)
  }
}

impl Styled for Thumb {
  fn style(&mut self) -> &mut StyleRefinement {
    self.inner.style()
  }
}

impl ParentElement for Thumb {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.inner.extend(elements)
  }
}

impl RenderOnce for Indicator {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let range = self.state.read(cx).percentage();
    let Some(style) = self.range else {
      return self.inner;
    };
    let mut fill = div().absolute();
    *Styled::style(&mut fill) = style;
    let fill = match self.axis {
      Axis::Horizontal => (fill.top_0().bottom_0())
        .left(relative(range.start))
        .w(relative(range.end - range.start)),
      Axis::Vertical => (fill.left_0().right_0())
        .bottom(relative(range.start))
        .h(relative(range.end - range.start)),
    };
    self.inner.child(fill)
  }
}

impl RenderOnce for Thumb {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let range = self.state.read(cx).percentage();
    let at = relative(if self.start { range.start } else { range.end });
    let thumb = self.inner.axis(self.axis).start(self.start).absolute();
    match self.axis {
      Axis::Horizontal => thumb.left(at),
      Axis::Vertical => thumb.bottom(at),
    }
  }
}

/// The cells of a `gpui-base` `OtpInput`, drawn from its state each frame
#[derive(IntoElement)]
pub struct Cells {
  inner: base::OtpInput,
  state: Entity<OtpState>,
  cell: StyleRefinement,
  active: StyleRefinement,
  caret: Option<StyleRefinement>,
  disabled: bool,
}

impl Styled for Cells {
  fn style(&mut self) -> &mut StyleRefinement {
    self.inner.style()
  }
}

impl ParentElement for Cells {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.inner.extend(elements)
  }
}

impl RenderOnce for Cells {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let state = self.state.read(cx);
    let (value, len, masked) = (state.value().clone(), state.len(), state.is_masked());
    let focused = state.focus_handle(cx).is_focused(window);
    let blink = state.cursor_visible(cx);
    let mut cells = Vec::with_capacity(len);
    for (i, digit) in (0..len).map(|i| (i, value.chars().nth(i))) {
      let mut cell = div();
      *Styled::style(&mut cell) = self.cell.clone();
      let active = focused && !self.disabled && i == value.chars().count();
      if active {
        Styled::style(&mut cell).refine(&self.active);
      }
      let cell = match (digit, &self.caret) {
        (Some(_), _) if masked => cell.child("•"),
        (Some(d), _) => cell.child(d.to_string()),
        (None, Some(caret)) if active && blink => {
          let mut mark = div();
          *Styled::style(&mut mark) = caret.clone();
          cell.child(mark)
        }
        _ => cell,
      };
      cells.push(cell.into_any_element());
    }
    let mut inner = self.inner.disabled(self.disabled);
    inner.extend(cells);
    inner
  }
}

/// Puts a `gpui-base` `NumberInput` step button around `look`
fn step_button(look: AnyElement) -> impl FnOnce(base::Button) -> base::Button + 'static {
  move |b| b.child(look)
}

mod methods {
  use super::*;

  crate::el_methods! {
    kinds: KINDS;
    /// The visible label of a button, check, switch, radio, toggle, color picker or field.
    fn label(text: String) |c, n| pick!(c, n,
      Button => |b: Button| b.label(text),
      Checkbox => |b: Checkbox| b.label(text),
      Switch => |b: Switch| b.label(text),
      Radio => |b: Radio| b.label(text),
      Toggle => |b: Toggle| b.label(text),
      ColorPicker => |b: ColorPicker| b.label(text),
      Field => |b: Field| b.label(text),
      InputGroupButton => |b: InputGroupButton| b.label(text),
    );
    /// An icon of an `InputGroupButton`, by asset path.
    fn icon(path: String) |c, n| {
      let icon = gpui_kit::component::Icon::empty().path(path);
      pick!(c, n, InputGroupButton => |b: InputGroupButton| b.icon(icon))
    };
    /// Concise hover help.
    fn tooltip(text: String) |c, n| pick!(c, n,
      Button => |b: Button| b.tooltip(text),
      Checkbox => |b: Checkbox| b.tooltip(text),
      Switch => |b: Switch| b.tooltip(text),
      Toggle => |b: Toggle| b.tooltip(text),
      Clipboard => |b: Clipboard| b.tooltip(text),
      InputGroupButton => |b: InputGroupButton| b.tooltip(text),
    );
    /// The controlled value of a `Checkbox`, `Switch`, `Radio` or `Toggle`.
    fn checked(value: bool) |c, n| pick!(c, n,
      Checkbox => |b: Checkbox| b.checked(value),
      Switch => |b: Switch| b.checked(value),
      Radio => |b: Radio| b.checked(value),
      Toggle => |b: Toggle| b.checked(value),
      base::Checkbox => |b: base::Checkbox| b.checked(value),
      base::Switch => |b: base::Switch| b.checked(value),
      base::Radio => |b: base::Radio| b.checked(value),
    );
    /// The controlled state of a `gpui-base` `Toggle`: a button that stays down.
    fn pressed(value: bool) |c, n| pick!(c, n, base::Toggle => |b: base::Toggle| b.pressed(value));
    /// `handler(value, cx)` with the new value: `checked` of a check, switch, radio or toggle,
    /// the index of a `RadioGroup`, the stars of a `Rating`. The script owns the value.
    fn on_change(handler: On<'js, Changed>) |c, n| {
      let h = handler.0;
      if is::<TextIn>(&**n) || is::<TextArea>(&**n) {
        return text_on_change(c, n, h);
      }
      let flag = |h: Function<'js>| {
        let l = listen(h, |v: &bool| *v);
        move |v: bool, _: &gpui_kit::ClickEvent, w: &mut Window, cx: &mut App| l(&v, w, cx)
      };
      pick!(c, n,
        Checkbox => |b: Checkbox| b.on_change(listen(h, |v: &bool| *v)),
        Switch => |b: Switch| b.on_change(listen(h, |v: &bool| *v)),
        Radio => |b: Radio| b.on_change(listen(h, |v: &bool| *v)),
        Toggle => |b: Toggle| b.on_click(listen(h, |v: &bool| *v)),
        RadioGroup => |b: RadioGroup| b.on_change(listen(h, |i: &usize| *i as u32)),
        Rating => |b: Rating| b.on_click(listen(h, |v: &usize| *v as u32)),
        base::Switch => |b: base::Switch| b.on_change(flag(h)),
        base::Radio => |b: base::Radio| b.on_change(flag(h)),
        base::Toggle => |b: base::Toggle| b.on_change(flag(h)),
        base::Checkbox => |b: base::Checkbox| {
          let l = listen(h, |v: &bool| *v);
          b.on_change(move |s, _, w, cx| l(&(s == CheckboxState::Checked), w, cx))
        },
      )
    };
    /// Blocks interaction and draws the control disabled.
    fn disabled(value: bool) |c, n| pick!(c, n,
      Button => |b: Button| b.disabled(value),
      Link => |b: Link| b.disabled(value),
      Checkbox => |b: Checkbox| b.disabled(value),
      Switch => |b: Switch| b.disabled(value),
      Radio => |b: Radio| b.disabled(value),
      RadioGroup => |b: RadioGroup| b.disabled(value),
      Toggle => |b: Toggle| b.disabled(value),
      Rating => |b: Rating| b.disabled(value),
      TextIn => |b: TextIn| b.with(|b| b.disabled(value)),
      NumberInput => |b: NumberInput| b.disabled(value),
      TextArea => |b: TextArea| b.with(|b| b.disabled(value)),
      OtpInput => |b: OtpInput| b.disabled(value),
      Slider => |b: Slider| b.disabled(value),
      DatePicker => |b: DatePicker| b.disabled(value),
      TimeField => |b: TimeField| b.disabled(value),
      Select<Items> => |b: Select<Items>| b.disabled(value),
      Combo => |b: Combo| b.with(|b| b.disabled(value)),
      InputGroup => |b: InputGroup| b.disabled(value),
      InputGroupButton => |b: InputGroupButton| b.disabled(value),
      base::Button => |b: base::Button| b.disabled(value),
      base::Link => |b: base::Link| b.disabled(value),
      base::Checkbox => |b: base::Checkbox| b.disabled(value),
      base::Switch => |b: base::Switch| b.disabled(value),
      base::Radio => |b: base::Radio| b.disabled(value),
      base::Toggle => |b: base::Toggle| b.disabled(value),
      base::Select => |b: base::Select| b.disabled(value),
      base::Combobox => |b: base::Combobox| b.disabled(value),
      base::DatePicker => |b: base::DatePicker| b.disabled(value),
      base::NumberInput => |b: base::NumberInput| b.disabled(value),
      base::Slider => |b: base::Slider| b.disabled(value),
      base::SliderTrack => |b: base::SliderTrack| b.disabled(value),
      Thumb => |mut b: Thumb| {
        b.inner = b.inner.disabled(value);
        b
      },
      Cells => |mut b: Cells| {
        b.disabled = value;
        b
      },
    );
    /// The selected state of a `Button`.
    fn selected(value: bool) |c, n| pick!(c, n,
      Button => |b: Button| b.selected(value),
      InputGroupButton => |b: InputGroupButton| b.selected(value),
      base::Button => |b: base::Button| b.selected(value),
    );
    /// Shows a spinner and blocks clicks.
    fn loading(value: bool) |c, n| pick!(c, n,
      Button => |b: Button| b.loading(value),
      InputGroupButton => |b: InputGroupButton| b.loading(value),
    );
    /// The semantic control size.
    fn size(size: SizeArg<'js>) |c, n| {
      let s = size.get(c, &**n)?;
      pick!(c, n,
        Button => |b: Button| b.with_size(s),
        Checkbox => |b: Checkbox| b.with_size(s),
        Switch => |b: Switch| b.with_size(s),
        Radio => |b: Radio| b.with_size(s),
        Toggle => |b: Toggle| b.with_size(s),
        Rating => |b: Rating| b.with_size(s),
        Form => |b: Form| b.with_size(s),
        InputGroup => |b: InputGroup| b.with_size(s),
        InputGroupButton => |b: InputGroupButton| b.with_size(s),
      )
    };
    /// The outlined look of a button, toggle or keycap.
    fn outline() |c, n| pick!(c, n,
      Button => |b: Button| b.outline(),
      Toggle => |b: Toggle| b.outline(),
      Kbd => |b: Kbd| b.outline(),
      InputGroupButton => |b: InputGroupButton| b.outline(),
    );
    /// The primary action look.
    fn primary() |c, n| pick!(c, n, Button => |b: Button| b.primary());
    /// The secondary look of a `Button`, or the muted secondary text of a `Label`.
    fn secondary(text: Opt<String>) |c, n| match text.0 {
      Some(text) => pick!(c, n, Label => |b: Label| b.secondary(text)),
      None => pick!(c, n, Button => |b: Button| b.secondary()),
    };
    /// The destructive action look.
    fn danger() |c, n| pick!(c, n, Button => |b: Button| b.danger());
    /// The success look.
    fn success() |c, n| pick!(c, n, Button => |b: Button| b.success());
    /// The warning look.
    fn warning() |c, n| pick!(c, n, Button => |b: Button| b.warning());
    /// The quiet look of a button.
    fn ghost() |c, n| pick!(c, n, Button => |b: Button| b.ghost());
    /// The look of a link.
    fn link() |c, n| pick!(c, n, Button => |b: Button| b.link());
    /// Tighter padding.
    fn compact() |c, n| pick!(c, n, Button => |b: Button| b.compact());
    /// A native button variant of an `InputGroupButton`.
    fn variant(variant: Variant) |c, n| {
      pick!(c, n, InputGroupButton => |b: InputGroupButton| b.with_variant(variant.0))
    };
    /// The absolute HTTP(S) URL a `Link` opens.
    fn href(url: String) |c, n| pick!(c, n,
      Link => |b: Link| b.href(url),
      base::Link => |b: base::Link| b.href(url),
    );
    /// The checked radio of a `RadioGroup`, by index.
    fn selected_index(index: u32) |c, n| {
      pick!(c, n, RadioGroup => |b: RadioGroup| b.selected_index(Some(index as usize)))
    };
    /// The orientation a `gpui-base` `RadioGroup` or `ToggleGroup` announces.
    fn axis(axis: AxisArg) |c, n| pick!(c, n,
      base::RadioGroup => |b: base::RadioGroup| b.axis(axis.0),
      base::ToggleGroup => |b: base::ToggleGroup| b.axis(axis.0),
      base::Slider => |b: base::Slider| b.axis(axis.0),
      base::SliderTrack => |b: base::SliderTrack| b.axis(axis.0),
      Indicator => |mut b: Indicator| {
        b.axis = axis.0;
        b
      },
      Thumb => |mut b: Thumb| {
        b.axis = axis.0;
        b
      },
    );
    /// This item's one-based position and its collection's size, announced.
    fn set_position(position: u32, size: u32) |c, n| {
      let (p, s) = (position as usize, size as usize);
      pick!(c, n, base::Radio => |b: base::Radio| b.set_position(p, s))
    };
    /// Draws a `Label`'s text as a password.
    fn masked(value: bool) |c, n| pick!(c, n, Label => |b: Label| b.masked(value));
    /// Highlights the matches of `text` in a `Label`.
    fn highlights(text: String) |c, n| pick!(c, n, Label => |b: Label| b.highlights(text));
    /// Whether a keycap or text area draws its frame.
    fn appearance(value: bool) |c, n| pick!(c, n,
      Kbd => |b: Kbd| b.appearance(value),
      TextArea => |b: TextArea| b.with(|b| b.appearance(value)),
    );
    /// What a `Clipboard` copies, or the stars of a `Rating`.
    fn value(value: Scalar<'js>) |c, n| {
      if is::<Clipboard>(&**n) {
        let text = Coerced::<String>::from_js(c, value.0)?.0;
        return map(c, n, |b: Clipboard| b.value(text));
      }
      let stars = u32::from_js(c, value.0)? as usize;
      pick!(c, n, Rating => |b: Rating| b.value(stars))
    };
    /// How many stars a `Rating` has.
    fn max(count: u32) |c, n| pick!(c, n, Rating => |b: Rating| b.max(count as usize));
    /// The accent color of a `Rating`.
    fn color(color: Color) |c, n| pick!(c, n, Rating => |b: Rating| b.color(color.0));
    /// Draws a `Textarea`'s border.
    fn bordered(value: bool) |c, n| pick!(c, n, TextArea => |b: TextArea| b.with(|b| b.bordered(value)));
    /// Allows selecting and copying but no edits.
    fn readonly(value: bool) |c, n| pick!(c, n,
      TextIn => |b: TextIn| b.with(|b| b.readonly(value)),
      TextArea => |b: TextArea| b.with(|b| b.readonly(value)),
      InputGroup => |b: InputGroup| b.readonly(value),
    );
    /// The prompt shown while empty.
    fn placeholder(text: String) |c, n| pick!(c, n,
      NumberInput => |b: NumberInput| b.placeholder(text),
      DatePicker => |b: DatePicker| b.placeholder(text),
      Select<Items> => |b: Select<Items>| b.placeholder(text),
      Combo => |b: Combo| b.with(|b| b.placeholder(text)),
    );
    /// The prompt of a `Combobox`'s search field.
    fn search_placeholder(text: String) |c, n| {
      pick!(c, n, Combo => |b: Combo| b.with(|b| b.search_placeholder(text)))
    };
    /// The width of a `Select`'s or `Combobox`'s menu, in pixels.
    fn menu_width(width: f32) |c, n| pick!(c, n,
      Select<Items> => |b: Select<Items>| b.menu_width(px(width)),
      Combo => |b: Combo| b.with(|b| b.menu_width(px(width))),
    );
    /// Splits an `OtpInput`'s cells into this many groups.
    fn groups(count: u32) |c, n| pick!(c, n, OtpInput => |b: OtpInput| b.groups(count as usize));
    /// A vertical `Slider`.
    fn vertical() |c, n| pick!(c, n, Slider => |b: Slider| b.vertical());
    /// Fills a `Slider` from the other end.
    fn reverse() |c, n| pick!(c, n, Slider => |b: Slider| b.reverse());
    /// Draws an `InputGroup` as invalid.
    fn invalid(value: bool) |c, n| pick!(c, n, InputGroup => |b: InputGroup| b.invalid(value));
    /// Draws the focus ring of an `InputGroup`.
    fn focus_ring(value: bool) |c, n| {
      pick!(c, n, InputGroup => |b: InputGroup| b.focus_ring(value))
    };
    /// Supporting text under a `Field`'s control.
    fn description(text: String) |c, n| pick!(c, n, Field => |b: Field| b.description(text));
    /// Marks a `Field` as required.
    fn required(value: bool) |c, n| pick!(c, n, Field => |b: Field| b.required(value));
    /// Indents an unlabeled `Field` of a horizontal form like the labeled ones.
    fn label_indent(value: bool) |c, n| pick!(c, n, Field => |b: Field| b.label_indent(value));
    /// How many grid columns a `Field` spans.
    fn col_span(span: u32) |c, n| pick!(c, n, Field => |b: Field| b.col_span(span as u16));
    /// Where a `Field` aligns its control.
    fn align(to: Scalar<'js>) |c, n| {
      if is::<InputGroupAddon>(&**n) {
        let side = AddonAlign::from_js(c, to.0)?.0;
        return map(c, n, |b: InputGroupAddon| b.align(side));
      }
      let to = Align::from_js(c, to.0)?.0;
      pick!(c, n, Field => |b: Field| match to {
        "center" => b.items_center(),
        "end" => b.items_end(),
        _ => b.items_start(),
      })
    };
    /// How many columns a `Form` lays its fields out in.
    fn columns(count: u32) |c, n| pick!(c, n, Form => |b: Form| b.columns(count as usize));
    /// The label width of a horizontal `Form`, in pixels.
    fn label_width(width: f32) |c, n| pick!(c, n, Form => |b: Form| b.label_width(px(width)));
    /// The text control of an `InputGroup` (`InputGroupInput` or `InputGroupTextarea`), or
    /// the editor slot of a `gpui-base` `NumberInput`.
    fn input(control: Child<'js>) |c, n| {
      if is::<base::NumberInput>(&**n) {
        let el = frame().child(c, control)?;
        return map(c, n, |b: base::NumberInput| b.input(el));
      }
      if !is::<InputGroup>(&**n) {
        return Err(miss(c, n.kind()));
      }
      const WHAT: &str = "input() takes an InputGroupInput or an InputGroupTextarea";
      let Child::El(el) = control else {
        return Err(Exception::throw_type(c, WHAT));
      };
      let node = frame_or_throw(c)?.take(c, &el.borrow())?.into_any();
      match node.downcast::<TextIn>() {
        Ok(input) => map(c, n, |g: InputGroup| g.input(input.el)),
        Err(node) => match node.downcast::<TextArea>() {
          Ok(area) => map(c, n, |g: InputGroup| g.input(area.el)),
          Err(_) => Err(Exception::throw_type(c, WHAT)),
        },
      }
    };
    /// Appends an `InputGroupAddon` to an `InputGroup`.
    fn addon(addon: Child<'js>) |c, n| {
      if !is::<InputGroup>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let addon = take::<InputGroupAddon>(c, addon, "addon() takes an InputGroupAddon")?;
      map(c, n, |g: InputGroup| g.addon(addon))
    };
    /// Names the control for accessibility.
    fn aria_label(label: String) |c, n| pick!(c, n,
      TextIn => |b: TextIn| b.with(|b| b.aria_label(label)),
      TextArea => |b: TextArea| b.with(|b| b.aria_label(label)),
      InputGroup => |b: InputGroup| b.aria_label(label),
      InputGroupButton => |b: InputGroupButton| b.accessibility_label(label),
    );
    /// What a screen reader announces.
    fn accessibility_label(label: String) |c, n| pick!(c, n,
      Radio => |b: Radio| b.accessibility_label(label),
      ColorPicker => |b: ColorPicker| b.accessibility_label(label),
      base::Button => |b: base::Button| b.accessibility_label(label),
      base::Link => |b: base::Link| b.accessibility_label(label),
      base::Checkbox => |b: base::Checkbox| b.accessibility_label(label),
      base::Switch => |b: base::Switch| b.accessibility_label(label),
      base::Radio => |b: base::Radio| b.accessibility_label(label),
      base::Toggle => |b: base::Toggle| b.accessibility_label(label),
      base::Select => |b: base::Select| b.accessibility_label(label),
    );
    /// Whether Tab lands on a `Radio`.
    fn tab_stop(value: bool) |c, n| pick!(c, n,
      Radio => |b: Radio| b.tab_stop(value),
      base::Radio => |b: base::Radio| b.tab_stop(value),
    );
    /// `handler(event, cx)` on activation.
    fn on_click(handler: On<'js, ClickEvent>) |c, n| {
      let h = handler.0;
      let l = || listen(h, |e: &gpui_kit::ClickEvent| ClickEvent::from(e));
      pick!(c, n,
        Button => |b: Button| b.on_click(l()),
        Link => |b: Link| b.on_click(l()),
        InputGroupButton => |b: InputGroupButton| b.on_click(l()),
        base::Button => |b: base::Button| b.on_click(l()),
      )
    };
    /// Whether a `gpui-base` `Select`, `Combobox` or `DatePicker` is showing its list.
    fn open(value: bool) |c, n| pick!(c, n,
      base::Select => |b: base::Select| b.open(value),
      base::Combobox => |b: base::Combobox| b.open(value),
      base::DatePicker => |b: base::DatePicker| b.open(value),
    );
    /// `handler(open, cx)` when something other than the script opened or closed it.
    fn on_open_change(handler: On<'js, bool>) |c, n| {
      let h = handler.0;
      let l = || {
        let l = listen(h, |v: &bool| *v);
        move |v: bool, w: &mut Window, cx: &mut App| l(&v, w, cx)
      };
      pick!(c, n,
        base::Select => |b: base::Select| b.on_open_change(l()),
        base::Combobox => |b: base::Combobox| b.on_open_change(l()),
        base::DatePicker => |b: base::DatePicker| b.on_open_change(l()),
      )
    };
    /// `handler(_, cx)` on Enter in an open `gpui-base` `Select` or `Combobox`.
    fn on_confirm(handler: On<'js, Nothing>) |c, n| {
      let h = handler.0;
      let l = || {
        let l = listen(h, |_: &()| Empty {});
        move |w: &mut Window, cx: &mut App| l(&(), w, cx)
      };
      pick!(c, n,
        base::Select => |b: base::Select| b.on_confirm(l()),
        base::Combobox => |b: base::Combobox| b.on_confirm(l()),
      )
    };
    /// `handler(_, cx)` on Escape in an open `gpui-base` `Select` or `Combobox`.
    fn on_dismiss(handler: On<'js, Nothing>) |c, n| {
      let h = handler.0;
      let l = || {
        let l = listen(h, |_: &()| Empty {});
        move |w: &mut Window, cx: &mut App| l(&(), w, cx)
      };
      pick!(c, n,
        base::Select => |b: base::Select| b.on_dismiss(l()),
        base::Combobox => |b: base::Combobox| b.on_dismiss(l()),
      )
    };
    /// The handle a `gpui-base` `Select` or `Combobox` moves the keyboard to when it opens.
    fn content_focus_handle(handle: Class<'js, crate::FocusHandle>) |c, n| {
      let h = handle.borrow().handle(c)?;
      pick!(c, n,
        base::Select => |b: base::Select| b.content_focus_handle(&h),
        base::Combobox => |b: base::Combobox| b.content_focus_handle(&h),
      )
    };
    /// Which thumb of a range slider a `SliderThumb` is.
    fn start(value: bool) |c, n| pick!(c, n, Thumb => |mut b: Thumb| {
      b.start = value;
      b
    });
    /// How the filled part of a `SliderIndicator` looks: `declare(el)` styles it.
    fn range_style(declare: Function<'js>) |c, n| {
      if !is::<Indicator>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let style = styled(c, declare)?;
      map(c, n, |mut b: Indicator| {
        b.range = Some(style);
        b
      })
    };
    /// How every cell of a `gpui-base` `OtpInput` looks.
    fn cell_style(declare: Function<'js>) |c, n| {
      if !is::<Cells>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let style = styled(c, declare)?;
      map(c, n, |mut b: Cells| {
        b.cell = style;
        b
      })
    };
    /// What the cell the next digit lands in adds to `cell_style`.
    fn cell_active_style(declare: Function<'js>) |c, n| {
      if !is::<Cells>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let style = styled(c, declare)?;
      map(c, n, |mut b: Cells| {
        b.active = style;
        b
      })
    };
    /// The blinking mark in that cell while it is empty.
    fn caret_style(declare: Function<'js>) |c, n| {
      if !is::<Cells>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let style = styled(c, declare)?;
      map(c, n, |mut b: Cells| {
        b.caret = Some(style);
        b
      })
    };
    /// The look of a `gpui-base` `NumberInput`'s decrement button.
    fn decrement_button(look: Child<'js>) |c, n| {
      if !is::<base::NumberInput>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let look = frame().child(c, look)?;
      map(c, n, |b: base::NumberInput| b.decrement_button(step_button(look)))
    };
    /// The look of a `gpui-base` `NumberInput`'s increment button.
    fn increment_button(look: Child<'js>) |c, n| {
      if !is::<base::NumberInput>(&**n) {
        return Err(miss(c, n.kind()));
      }
      let look = frame().child(c, look)?;
      map(c, n, |b: base::NumberInput| b.increment_button(step_button(look)))
    };
    /// Stacks both step buttons to the right of the text.
    fn controls_right() |c, n| {
      pick!(c, n, base::NumberInput => |b: base::NumberInput| b.controls_right())
    };
    /// `handler(action, cx)` on a step; replaces the built-in stepping.
    fn on_step(handler: On<'js, Step>) |c, n| {
      let h = handler.0;
      pick!(c, n, base::NumberInput => |b: base::NumberInput| {
        let l = listen(h, |a: &StepAction| match a {
          StepAction::Increment => "increment",
          StepAction::Decrement => "decrement",
        });
        b.on_step(move |a, w, cx| l(&a, w, cx))
      })
    };
  }

  /// `on_change(text => ..)` of an `Input` or `Textarea`: edits of its retained state
  fn text_on_change<'js>(c: &Ctx<'js>, n: &mut Box<dyn Node>, h: Function<'js>) -> Result<()> {
    let any = &**n as &dyn std::any::Any;
    if let Some(input) = any.downcast_ref::<TextIn>() {
      return on_text(c, &input.state.clone(), h);
    }
    match any.downcast_ref::<TextArea>() {
      Some(area) => on_text(c, &area.state.clone(), h),
      None => Err(miss(c, n.kind())),
    }
  }

  fn on_text<'js, M: gpui_kit::component::input::InputModeKind + 'static>(
    c: &Ctx<'js>,
    state: &Entity<base::input::InputBaseState<M>>,
    h: Function<'js>,
  ) -> Result<()>
  where
    base::input::InputBaseState<M>: gpui_kit::EventEmitter<InputEvent>,
  {
    on_entity(c, state, "change", h, |s, e: &InputEvent, cx| match e {
      InputEvent::Change => Some(s.read(cx).value().to_string()),
      _ => None,
    })
  }
}

/// Children a `RadioGroup` or `Form` of `gpui-component` takes: `Radio`s, `Field`s. Typed
/// as the generic `child`.
#[allow(dead_code)]
mod children {
  use super::*;

  crate::el_methods! {
    kinds: &["RadioGroup", "Form"];
    fn child(child: Child<'js>) |c, n| typed_child(c, n, child);
    fn children(children: Vec<Child<'js>>) |c, n| {
      children.into_iter().try_for_each(|child| typed_child(c, n, child))
    };
  }

  /// A `Radio` (or its label) of a `RadioGroup`, a `Field` of a `Form`
  fn typed_child(c: &Ctx<'_>, n: &mut Box<dyn Node>, child: Child<'_>) -> Result<()> {
    if is::<RadioGroup>(&**n) {
      let radio = match child {
        Child::Text(text) => Radio::new(SharedString::from(text.clone())).label(text),
        child => take::<Radio>(c, child, "a RadioGroup takes Radio children")?,
      };
      return map(c, n, |g: RadioGroup| g.child(radio));
    }
    if is::<Form>(&**n) {
      let field = take::<Field>(c, child, "a Form takes Field children")?;
      return map(c, n, |f: Form| f.child(field));
    }
    Err(miss(c, n.kind()))
  }
}

fn install(proto: &Object<'_>) -> Result<()> {
  methods::install(proto)?;
  children::install(proto)
}

// Retained states

/// Updates a state with the window of the current call
fn update<T: 'static>(
  ctx: &Ctx<'_>,
  state: &crate::Retained<T>,
  f: impl FnOnce(&mut T, &mut Window, &mut gpui_kit::Context<T>),
) -> Result<()> {
  let window = window_or_throw(ctx)?;
  state.entity(ctx)?.update(app(), |s, cx| f(s, window, cx));
  Ok(())
}

/// What `on` of a text state gets: the keys held for `submit`
#[derive(rquickjs::IntoJs, Default)]
pub struct TextEvent {
  secondary: Option<bool>,
  shift: Option<bool>,
}

fn text_event(e: &InputEvent, _: &App) -> Option<(&'static str, TextEvent)> {
  Some(match e {
    InputEvent::Change => ("change", TextEvent::default()),
    InputEvent::PressEnter { secondary, shift } => {
      let (secondary, shift) = (Some(*secondary), Some(*shift));
      ("submit", TextEvent { secondary, shift })
    }
    InputEvent::Focus => ("focus", TextEvent::default()),
    InputEvent::Blur => ("blur", TextEvent::default()),
  })
}

const TEXT_EVENTS: &[&str] = &["change", "submit", "focus", "blur"];

/// The options of a text state: `(placeholder?, value?)` or `{ placeholder, value, rows }`
fn text_options<'js>(
  ctx: &Ctx<'js>,
  first: Opt<Value<'js>>,
  second: Opt<String>,
) -> Result<(Option<String>, Option<String>, Option<u32>)> {
  match first.0 {
    Some(v) if v.is_object() => {
      let o = Object::from_js(ctx, v)?;
      Ok((o.get("placeholder")?, o.get("value")?, o.get("rows")?))
    }
    Some(v) => Ok((Option::from_js(ctx, v)?, second.0, None)),
    None => Ok((None, second.0, None)),
  }
}

state!(JsInputState, InputState, "InputState");

/// `InputState(placeholder?, value?)`, or `InputState.new({ placeholder, value })`
fn input_state<'js>(
  ctx: Ctx<'js>,
  first: Opt<Value<'js>>,
  value: Opt<String>,
) -> Result<Class<'js, JsInputState>> {
  let (placeholder, value, _) = text_options(&ctx, first, value)?;
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| {
    let state = InputState::new(window, cx).placeholder(placeholder.unwrap_or_default());
    state.default_value(value.unwrap_or_default())
  });
  JsInputState::instance(&ctx, state)
}

#[rquickjs::methods]
impl JsInputState {
  pub fn value(&self, ctx: Ctx<'_>) -> Result<String> {
    Ok(self.state.entity(&ctx)?.read(app()).value().to_string())
  }

  /// Replaces the text; emits no `change`.
  pub fn set_value(&self, ctx: Ctx<'_>, value: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_value(value, w, cx))
  }

  /// Replaces the selection with `text`, as typing it would.
  pub fn replace(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.replace(text, w, cx))
  }

  pub fn set_placeholder(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_placeholder(text, w, cx))
  }

  pub fn set_masked(&self, ctx: Ctx<'_>, masked: bool) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_masked(masked, w, cx))
  }

  pub fn set_loading(&self, ctx: Ctx<'_>, loading: bool) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_loading(loading, w, cx))
  }

  pub fn set_step(&self, ctx: Ctx<'_>, step: Option<f64>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| {
      s.set_step(step.map(Into::into), w, cx)
    })
  }

  pub fn set_min(&self, ctx: Ctx<'_>, min: Option<f64>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_min(min, w, cx))
  }

  pub fn set_max(&self, ctx: Ctx<'_>, max: Option<f64>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_max(max, w, cx))
  }

  pub fn focus(&self, ctx: Ctx<'_>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.focus(w, cx))
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, TEXT_EVENTS, name, f, text_event)
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

state!(JsTextareaState, TextareaState, "TextareaState");

/// `TextareaState(value?)`, or `TextareaState.new({ placeholder, value, rows })`
fn textarea_state<'js>(
  ctx: Ctx<'js>,
  first: Opt<Value<'js>>,
) -> Result<Class<'js, JsTextareaState>> {
  // a lone string is the value here, not the placeholder
  let (placeholder, value, rows) = match first.0 {
    Some(v) if !v.is_object() => (None, Option::from_js(&ctx, v)?, None),
    first => text_options(&ctx, Opt(first), Opt(None))?,
  };
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| {
    let state = TextareaState::new(window, cx).placeholder(placeholder.unwrap_or_default());
    let state = state.default_value(value.unwrap_or_default());
    match rows {
      Some(rows) => state.rows(rows as usize),
      None => state,
    }
  });
  JsTextareaState::instance(&ctx, state)
}

#[rquickjs::methods]
impl JsTextareaState {
  pub fn value(&self, ctx: Ctx<'_>) -> Result<String> {
    Ok(self.state.entity(&ctx)?.read(app()).value().to_string())
  }

  pub fn set_value(&self, ctx: Ctx<'_>, value: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_value(value, w, cx))
  }

  pub fn replace(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.replace(text, w, cx))
  }

  pub fn set_placeholder(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_placeholder(text, w, cx))
  }

  pub fn set_rows(&self, ctx: Ctx<'_>, rows: u32) -> Result<()> {
    update(&ctx, &self.state, |s, _, cx| s.set_rows(rows as usize, cx))
  }

  pub fn set_auto_grow(&self, ctx: Ctx<'_>, min_rows: u32, max_rows: u32) -> Result<()> {
    let (min, max) = (min_rows as usize, max_rows as usize);
    update(&ctx, &self.state, |s, _, cx| s.set_auto_grow(min, max, cx))
  }

  pub fn set_soft_wrap(&self, ctx: Ctx<'_>, wrap: bool) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_soft_wrap(wrap, w, cx))
  }

  pub fn focus(&self, ctx: Ctx<'_>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.focus(w, cx))
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, TEXT_EVENTS, name, f, text_event)
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// A slider value: a number, or `[start, end]`
pub struct SliderVal(SliderValue);

impl<'js> IntoJs<'js> for SliderVal {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self.0 {
      SliderValue::Single(v) => v.into_js(ctx),
      SliderValue::Range(a, b) => [a, b].into_js(ctx),
    }
  }
}

impl<'js> FromJs<'js> for SliderVal {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(SliderVal(match value.is_array() {
      true => {
        let v = Vec::<f32>::from_js(ctx, value)?;
        let (a, b) = (v.first().copied(), v.get(1).copied());
        SliderValue::Range(a.unwrap_or_default(), b.unwrap_or_default())
      }
      false => SliderValue::Single(f32::from_js(ctx, value)?),
    }))
  }
}

state!(JsSliderState, SliderState, "SliderState");

/// `SliderState(value?)`, or `SliderState.new({ min, max, step, scale, value })`
fn slider_state<'js>(ctx: Ctx<'js>, first: Opt<Value<'js>>) -> Result<Class<'js, JsSliderState>> {
  let mut state = SliderState::new();
  match first.0 {
    Some(v) if v.is_object() && !v.is_array() => {
      let o = Object::from_js(&ctx, v)?;
      if let Some(min) = o.get::<_, Option<f32>>("min")? {
        state = state.min(min);
      }
      if let Some(max) = o.get::<_, Option<f32>>("max")? {
        state = state.max(max);
      }
      if let Some(step) = o.get::<_, Option<f32>>("step")? {
        state = state.step(step);
      }
      if o.get::<_, Option<String>>("scale")?.as_deref() == Some("logarithmic") {
        state = state.scale(SliderScale::Logarithmic);
      }
      if let Some(value) = o.get::<_, Option<SliderVal>>("value")? {
        state = state.default_value(value.0);
      }
    }
    Some(v) if !v.is_undefined() && !v.is_null() => {
      state = state.default_value(SliderVal::from_js(&ctx, v)?.0);
    }
    _ => {}
  }
  JsSliderState::instance(&ctx, app().new(|_| state))
}

#[rquickjs::methods]
impl JsSliderState {
  /// A number, or `[start, end]` for a range.
  pub fn value(&self, ctx: Ctx<'_>) -> Result<SliderVal> {
    Ok(SliderVal(self.state.entity(&ctx)?.read(app()).value()))
  }

  pub fn set_value(&self, ctx: Ctx<'_>, value: SliderVal) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_value(value.0, w, cx))
  }

  pub fn min_value(&self, ctx: Ctx<'_>) -> Result<f32> {
    Ok(self.state.entity(&ctx)?.read(app()).min_value())
  }

  pub fn max_value(&self, ctx: Ctx<'_>) -> Result<f32> {
    Ok(self.state.entity(&ctx)?.read(app()).max_value())
  }

  pub fn step_value(&self, ctx: Ctx<'_>) -> Result<f32> {
    Ok(self.state.entity(&ctx)?.read(app()).step_value())
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(
      &ctx,
      &["change", "release"],
      name,
      f,
      |e: &SliderEvent, _| {
        Some(match e {
          SliderEvent::Change(v) => ("change", SliderVal(*v)),
          SliderEvent::Release(v) => ("release", SliderVal(*v)),
        })
      },
    )
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// What `on` of an `OtpState` gets: nothing, read the value from the state
#[derive(rquickjs::IntoJs, Clone)]
pub struct Empty {}

state!(JsOtpState, OtpState, "OtpState");

/// `OtpState(length, { value, masked }?)`
fn otp_state<'js>(
  ctx: Ctx<'js>,
  length: u32,
  options: Opt<Object<'js>>,
) -> Result<Class<'js, JsOtpState>> {
  if !(1..=64).contains(&length) {
    return Err(Exception::throw_range(
      &ctx,
      "an OtpState has 1 to 64 cells",
    ));
  }
  let (value, masked) = match &options.0 {
    Some(o) => (o.get::<_, Option<String>>("value")?, o.get("masked")?),
    None => (None, None),
  };
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| {
    let state = OtpState::new(length as usize, window, cx).masked(masked.unwrap_or(false));
    state.default_value(value.unwrap_or_default())
  });
  JsOtpState::instance(&ctx, state)
}

#[rquickjs::methods]
impl JsOtpState {
  pub fn value(&self, ctx: Ctx<'_>) -> Result<String> {
    Ok(self.state.entity(&ctx)?.read(app()).value().to_string())
  }

  pub fn set_value(&self, ctx: Ctx<'_>, value: String) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_value(value, w, cx))
  }

  pub fn len(&self, ctx: Ctx<'_>) -> Result<u32> {
    Ok(self.state.entity(&ctx)?.read(app()).len() as u32)
  }

  pub fn is_masked(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(self.state.entity(&ctx)?.read(app()).is_masked())
  }

  pub fn set_masked(&self, ctx: Ctx<'_>, masked: bool) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.set_masked(masked, w, cx))
  }

  pub fn focus(&self, ctx: Ctx<'_>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| s.focus(w, cx))
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    let names = ["change", "complete", "focus", "blur"];
    (self.state).on(&ctx, &names, name, f, |e: &OtpEvent, _| {
      let name = match e {
        OtpEvent::Change => "change",
        OtpEvent::Complete => "complete",
        OtpEvent::Focus => "focus",
        OtpEvent::Blur => "blur",
      };
      Some((name, Empty {}))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

fn hex(color: Option<Hsla>) -> Option<String> {
  color.map(crate::style::to_hex)
}

state!(JsColorPickerState, ColorPickerState, "ColorPickerState");

fn color_picker_state(ctx: Ctx<'_>) -> Result<Class<'_, JsColorPickerState>> {
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| ColorPickerState::new(window, cx));
  JsColorPickerState::instance(&ctx, state)
}

#[rquickjs::methods]
impl JsColorPickerState {
  /// The color as `#rrggbb[aa]`.
  pub fn value(&self, ctx: Ctx<'_>) -> Result<Option<String>> {
    Ok(hex(self.state.entity(&ctx)?.read(app()).value()))
  }

  pub fn set_value(&self, ctx: Ctx<'_>, color: Option<Color>) -> Result<()> {
    update(&ctx, &self.state, |s, w, cx| match color {
      Some(color) => s.set_value(color.0, w, cx),
      None => s.clear_value(w, cx),
    })
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |e: &ColorPickerEvent, _| {
      let ColorPickerEvent::Change(color) = e;
      Some(("change", hex(*color)))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

state!(JsDatePickerState, DatePickerState, "DatePickerState");

fn date_picker_state(ctx: Ctx<'_>) -> Result<Class<'_, JsDatePickerState>> {
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| DatePickerState::new(window, cx));
  JsDatePickerState::instance(&ctx, state)
}

const DATE: &str = "%Y-%m-%d";

#[rquickjs::methods]
impl JsDatePickerState {
  /// The picked date as `YYYY-MM-DD`.
  pub fn value(&self, ctx: Ctx<'_>) -> Result<Option<String>> {
    let date = self.state.entity(&ctx)?.read(app()).date_time();
    Ok(date.format(DATE).map(Into::into))
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |e: &DatePickerEvent, _| {
      let DatePickerEvent::Change(date) = e;
      Some(("change", date.format(DATE).map(String::from)))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

state!(JsTimeFieldState, TimeFieldState, "TimeFieldState");

fn time_field_state(ctx: Ctx<'_>) -> Result<Class<'_, JsTimeFieldState>> {
  let window = window_or_throw(&ctx)?;
  let state = app().new(|cx| TimeFieldState::new(window, cx));
  JsTimeFieldState::instance(&ctx, state)
}

#[rquickjs::methods]
impl JsTimeFieldState {
  /// The time as `HH:MM:SS`.
  pub fn value(&self, ctx: Ctx<'_>) -> Result<String> {
    Ok(self.state.entity(&ctx)?.read(app()).time().to_string())
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |e: &TimeFieldEvent, _| {
      let TimeFieldEvent::Change(time) = e;
      Some(("change", time.to_string()))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// A row of a `Select` or `Combobox`: `{ id, label, disabled? }`, kept as JSON for
/// `render_row`
#[derive(Clone)]
pub struct Item {
  id: SharedString,
  label: SharedString,
  disabled: bool,
  /// The row's JSON and where to render it: the view and the `render_row` slot
  row: Option<(String, WeakEntity<ScriptView>, u64)>,
}

impl SelectItem for Item {
  type Value = SharedString;

  fn title(&self) -> SharedString {
    self.label.clone()
  }

  fn value(&self) -> &SharedString {
    &self.id
  }

  fn disabled(&self) -> bool {
    self.disabled
  }

  fn render(&self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let rendered = self.row.as_ref().and_then(|(json, view, key)| {
      let view = view.upgrade()?;
      let json = json.clone();
      let out = view.update(cx, |view, cx| {
        let args = move |a: &mut rquickjs::function::Args<'_>| {
          let row = a.ctx().json_parse(json)?;
          a.push_arg(row)
        };
        view.render_content(*key, None, window, cx, args, |ctx, frame, value| {
          match value.is_null() || value.is_undefined() {
            true => Ok(None),
            false => frame.child(ctx, Child::from_js(ctx, value)?).map(Some),
          }
        })
      });
      out.unwrap_or_else(|e| Some(format!("render_row failed: {e}").into_any_element()))
    });
    rendered.unwrap_or_else(|| self.label.clone().into_any_element())
  }
}

type Items = SearchableVec<Item>;

/// The rows `rows()` returns, rendered by the slot `key` of `view` when there is one
fn items<'js>(
  ctx: &Ctx<'js>,
  rows: Function<'js>,
  render: Option<(WeakEntity<ScriptView>, u64)>,
) -> Result<Vec<Item>> {
  let rows: Vec<Object<'js>> = rows.call(())?;
  rows
    .into_iter()
    .map(|o| {
      let id: String = o.get("id")?;
      let label: Option<String> = o.get("label")?;
      let json = match &render {
        Some((view, key)) => {
          let json = ctx.json_stringify(o.clone())?;
          let json = json.map(|s| s.to_string()).transpose()?.unwrap_or_default();
          Some((json, view.clone(), *key))
        }
        None => None,
      };
      Ok(Item {
        label: label.unwrap_or_else(|| id.clone()).into(),
        id: id.into(),
        disabled: o.get::<_, Option<bool>>("disabled")?.unwrap_or(false),
        row: json,
      })
    })
    .collect()
}

/// `Select(id, rows, render_row, on_select)`: a native select over a rows snapshot
fn select<'js>(
  ctx: Ctx<'js>,
  id: String,
  rows: Function<'js>,
  render_row: Function<'js>,
  on_select: Function<'js>,
) -> Result<Class<'js, El>> {
  let view = frame_or_throw(&ctx)?.view.clone();
  let rt = runtime(&ctx)?;
  let key = rt.slot(view.entity_id(), render_row, true);
  let items = items(&ctx, rows, Some((view.clone(), key)))?;
  let window = window_or_throw(&ctx)?;
  let state: Entity<SelectState<Items>> =
    rt.keyed(view.entity_id(), format!("Select:{id}"), || {
      app().new(|cx| SelectState::new(SearchableVec::new(Vec::new()), None, window, cx))
    });
  state.update(app(), |s, cx| {
    s.set_items(SearchableVec::new(items), window, cx)
  });
  on_entity(
    &ctx,
    &state,
    "select",
    on_select,
    |_, e: &SelectEvent<Items>, _| {
      let SelectEvent::Confirm(v) = e;
      v.as_ref().map(|v| v.to_string())
    },
  )?;
  new_el(&ctx, Select::new(&state))
}

/// `Combobox(id, rows, on_change, on_confirm)`: a single-select searchable combobox
fn combobox<'js>(
  ctx: Ctx<'js>,
  id: String,
  rows: Function<'js>,
  on_change: Function<'js>,
  on_confirm: Function<'js>,
) -> Result<Class<'js, El>> {
  let view = frame_or_throw(&ctx)?.view.entity_id();
  let rt = runtime(&ctx)?;
  let items = items(&ctx, rows, None)?;
  let window = window_or_throw(&ctx)?;
  let state: Entity<ComboboxState<Items>> = rt.keyed(view, format!("Combobox:{id}"), || {
    app().new(|cx| {
      let state = ComboboxState::new(SearchableVec::new(Vec::new()), Vec::new(), window, cx);
      state.searchable(true)
    })
  });
  state.update(app(), |s, cx| {
    s.set_items(SearchableVec::new(items), window, cx)
  });
  let first = |v: &Vec<SharedString>| v.first().map(|v| v.to_string()).unwrap_or_default();
  on_entity(
    &ctx,
    &state,
    "change",
    on_change,
    move |_, e: &ComboboxEvent<Items>, _| match e {
      ComboboxEvent::Change(v) => Some(first(v)),
      _ => None,
    },
  )?;
  on_entity(
    &ctx,
    &state,
    "confirm",
    on_confirm,
    move |_, e: &ComboboxEvent<Items>, _| match e {
      ComboboxEvent::Confirm(v) => Some(first(v)),
      _ => None,
    },
  )?;
  let el = Combobox::new(&state);
  new_el(
    &ctx,
    Stated {
      kind: "Combobox",
      el,
      state,
    },
  )
}

fn text_in(state: &Entity<InputState>) -> TextIn {
  let (kind, el, state) = ("Input", Input::new(state), state.clone());
  Stated { kind, el, state }
}

fn text_area(state: &Entity<TextareaState>) -> TextArea {
  let (kind, el, state) = ("Textarea", Textarea::new(state), state.clone());
  Stated { kind, el, state }
}

/// `Kbd(keystroke)`
fn kbd(ctx: Ctx<'_>, keys: String) -> Result<Class<'_, El>> {
  let stroke =
    Keystroke::parse(&keys).map_err(|e| Exception::throw_type(&ctx, &format!("Kbd: {e}")))?;
  new_el(&ctx, Kbd::new(stroke))
}

macro_rules! by_id {
  ($out:ident, $ctx:ident: $($name:literal => $make:expr),* $(,)?) => {$(
    $out.push(ctor($ctx, $name, |ctx: Ctx<'js>, id: Id| {
      let id: ElementId = id.into_inner();
      new_el(&ctx, $make(id))
    })?);
  )*};
}

macro_rules! by_state {
  ($out:ident, $ctx:ident: $($name:literal => $state:ident => $make:expr),* $(,)?) => {$(
    $out.push(ctor($ctx, $name, |ctx: Ctx<'js>, state: Class<'js, $state>| {
      let state = $state::get(&state)?;
      new_el(&ctx, $make(&state))
    })?);
  )*};
}

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let mut out = Vec::new();
  by_id!(out, ctx:
    "Button" => Button::new,
    "Link" => Link::new,
    "Checkbox" => Checkbox::new,
    "Switch" => Switch::new,
    "Radio" => Radio::new,
    "RadioGroup" => RadioGroup::new,
    "HorizontalRadioGroup" => RadioGroup::horizontal,
    "Toggle" => Toggle::new,
    "Clipboard" => Clipboard::new,
    "Rating" => Rating::new,
    "InputGroup" => InputGroup::new,
    "InputGroupAddon" => InputGroupAddon::new,
    "InputGroupButton" => InputGroupButton::new,
  );
  by_state!(out, ctx:
    "Input" => JsInputState => text_in,
    "NumberInput" => JsInputState => NumberInput::new,
    "InputGroupInput" => JsInputState => text_in,
    "Textarea" => JsTextareaState => text_area,
    "InputGroupTextarea" => JsTextareaState => text_area,
    "OtpInput" => JsOtpState => OtpInput::new,
    "Slider" => JsSliderState => Slider::new,
    "ColorPicker" => JsColorPickerState => ColorPicker::new,
    "DatePicker" => JsDatePickerState => DatePicker::new,
    "TimeField" => JsTimeFieldState => TimeField::new,
  );
  out.extend([
    ctor(ctx, "Label", |ctx: Ctx<'js>, text: String| {
      new_el(&ctx, Label::new(text))
    })?,
    ctor(ctx, "Kbd", kbd)?,
    ctor(ctx, "Field", |ctx: Ctx<'js>| new_el(&ctx, Field::new()))?,
    ctor(ctx, "Form", |ctx: Ctx<'js>| new_el(&ctx, Form::new()))?,
    ctor(ctx, "VForm", |ctx: Ctx<'js>| new_el(&ctx, Form::vertical()))?,
    ctor(ctx, "HForm", |ctx: Ctx<'js>| {
      new_el(&ctx, Form::horizontal())
    })?,
    ctor(ctx, "InputGroupText", |ctx: Ctx<'js>| {
      new_el(&ctx, InputGroupText::new())
    })?,
    ctor(ctx, "Select", select)?,
    ctor(ctx, "Combobox", combobox)?,
    ctor(ctx, "InputState", input_state)?,
    ctor(ctx, "TextareaState", textarea_state)?,
    ctor(ctx, "SliderState", slider_state)?,
    ctor(ctx, "OtpState", otp_state)?,
    ctor(ctx, "ColorPickerState", color_picker_state)?,
    ctor(ctx, "DatePickerState", date_picker_state)?,
    ctor(ctx, "TimeFieldState", time_field_state)?,
  ]);
  Ok(out)
}

/// A `gpui-base` slider part
fn slider_part<'js>(
  ctx: &Ctx<'js>,
  name: &'static str,
  make: fn(&Entity<SliderState>) -> Box<dyn Node>,
) -> Result<(&'static str, Value<'js>)> {
  ctor(
    ctx,
    name,
    move |ctx: Ctx<'js>, state: Class<'js, JsSliderState>| {
      let state = JsSliderState::get(&state)?;
      let el = crate::el::new_boxed(&ctx, make(&state))?;
      Ok::<_, rquickjs::Error>(el)
    },
  )
}

fn base_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let mut out = Vec::new();
  by_id!(out, ctx:
    "Button" => base::Button::new,
    "Link" => base::Link::new,
    "Checkbox" => base::Checkbox::new,
    "Switch" => base::Switch::new,
    "Radio" => base::Radio::new,
    "Toggle" => base::Toggle::new,
    "RadioGroup" => base::RadioGroup::new,
    "ToggleGroup" => base::ToggleGroup::new,
    "Select" => base::Select::new,
    "Combobox" => base::Combobox::new,
  );
  by_state!(out, ctx:
    "Input" => JsInputState => base::Input::new,
    "NumberInput" => JsInputState => base::NumberInput::new,
    "Textarea" => JsTextareaState => base::Textarea::new,
  );
  out.extend([
    ctor(
      ctx,
      "DatePicker",
      |ctx: Ctx<'js>, id: Id, handle: Class<'js, crate::FocusHandle>| {
        let handle = handle.borrow().handle(&ctx)?;
        new_el(&ctx, base::DatePicker::new(id.into_inner(), &handle))
      },
    )?,
    ctor(
      ctx,
      "OtpInput",
      |ctx: Ctx<'js>, state: Class<'js, JsOtpState>| {
        let state = JsOtpState::get(&state)?;
        let cells = Cells {
          inner: base::OtpInput::new(&state),
          state,
          cell: StyleRefinement::default(),
          active: StyleRefinement::default(),
          caret: None,
          disabled: false,
        };
        new_el(&ctx, cells)
      },
    )?,
    slider_part(ctx, "Slider", |s| Box::new(base::Slider::new(s)))?,
    slider_part(ctx, "SliderTrack", |s| Box::new(base::SliderTrack::new(s)))?,
    slider_part(ctx, "SliderIndicator", |s| {
      Box::new(Indicator {
        inner: base::SliderIndicator::new(s),
        state: s.clone(),
        axis: Axis::Horizontal,
        range: None,
      })
    })?,
    slider_part(ctx, "SliderThumb", |s| {
      Box::new(Thumb {
        inner: base::SliderThumb::new(s),
        state: s.clone(),
        axis: Axis::Horizontal,
        start: false,
      })
    })?,
    ctor(ctx, "InputState", input_state)?,
    ctor(ctx, "TextareaState", textarea_state)?,
    ctor(ctx, "SliderState", slider_state)?,
    ctor(ctx, "OtpState", otp_state)?,
  ]);
  Ok(out)
}

const NAMES: &[&str] = &[
  "Button",
  "Link",
  "Checkbox",
  "Switch",
  "Radio",
  "RadioGroup",
  "HorizontalRadioGroup",
  "Toggle",
  "Clipboard",
  "Rating",
  "InputGroup",
  "InputGroupAddon",
  "InputGroupButton",
  "Input",
  "NumberInput",
  "InputGroupInput",
  "Textarea",
  "InputGroupTextarea",
  "OtpInput",
  "Slider",
  "ColorPicker",
  "DatePicker",
  "TimeField",
  "Label",
  "Kbd",
  "Field",
  "Form",
  "VForm",
  "HForm",
  "InputGroupText",
  "Select",
  "Combobox",
  "InputState",
  "TextareaState",
  "SliderState",
  "OtpState",
  "ColorPickerState",
  "DatePickerState",
  "TimeFieldState",
];

const BASE_NAMES: &[&str] = &[
  "Button",
  "Link",
  "Checkbox",
  "Switch",
  "Radio",
  "Toggle",
  "RadioGroup",
  "ToggleGroup",
  "Select",
  "Combobox",
  "Input",
  "NumberInput",
  "Textarea",
  "DatePicker",
  "OtpInput",
  "Slider",
  "SliderTrack",
  "SliderIndicator",
  "SliderThumb",
  "InputState",
  "TextareaState",
  "SliderState",
  "OtpState",
];

fn d_ts() -> String {
  let mut out = String::new();
  for (name, doc) in [
    ("Button", "A styled command button."),
    ("Link", "An external-resource link."),
    (
      "Checkbox",
      "A controlled check box: `checked(..)` in, `on_change(..)` out.",
    ),
    (
      "Switch",
      "A controlled switch: `checked(..)` in, `on_change(..)` out.",
    ),
    ("Radio", "One option of a radio group."),
    (
      "RadioGroup",
      "A column of `Radio`s; `on_change(index => ..)`.",
    ),
    (
      "HorizontalRadioGroup",
      "A row of `Radio`s; `on_change(index => ..)`.",
    ),
    (
      "Toggle",
      "A button that stays down: `checked(..)` in, `on_change(..)` out.",
    ),
    ("Clipboard", "A button copying `value(..)`."),
    (
      "Rating",
      "Stars: `value(..)`, `max(..)`, `on_change(stars => ..)`.",
    ),
    (
      "InputGroup",
      "A frame around `input(..)` and its `addon(..)`s.",
    ),
    (
      "InputGroupAddon",
      "An addon of an `InputGroup`, at `align(..)`.",
    ),
    (
      "InputGroupButton",
      "A compact button inside an `InputGroup`.",
    ),
  ] {
    out += &declare(doc, name, "id: string | number");
  }
  for (name, state, doc) in [
    ("Input", "InputState", "The frame around retained text."),
    (
      "NumberInput",
      "InputState",
      "A spinbutton over an `InputState`.",
    ),
    (
      "InputGroupInput",
      "InputState",
      "The single-line control of an `InputGroup`.",
    ),
    (
      "Textarea",
      "TextareaState",
      "The frame around retained multi-line text.",
    ),
    (
      "InputGroupTextarea",
      "TextareaState",
      "The multi-line control of an `InputGroup`.",
    ),
    ("OtpInput", "OtpState", "A fixed-length code."),
    (
      "Slider",
      "SliderState",
      "A slider over a retained `SliderState`.",
    ),
    ("ColorPicker", "ColorPickerState", "A color picker."),
    ("DatePicker", "DatePickerState", "A date picker."),
    ("TimeField", "TimeFieldState", "A time of day field."),
  ] {
    out += &declare(doc, name, &format!("state: {state}"));
  }
  out += &declare("A text label.", "Label", "text: string");
  out += &declare(
    "A keyboard shortcut keycap, e.g. `\"ctrl-s\"`.",
    "Kbd",
    "keystroke: string",
  );
  for (name, doc) in [
    ("Field", "A labeled form field."),
    ("Form", "A form of `Field`s."),
    ("VForm", "A form laying out its labels above."),
    ("HForm", "A form laying out its labels beside."),
    ("InputGroupText", "Muted text inside an `InputGroup`."),
  ] {
    out += &declare(doc, name, "");
  }
  out += &declare(
    "A native select over the `{ id, label, disabled? }` rows `rows()` returns.",
    "Select",
    "id: string, rows: () => readonly SelectRow[], render_row: (row: any) => Element | null, \
     on_select: (value: string, cx: Context) => void",
  );
  out += &declare(
    "A single-select searchable combobox over the rows `rows()` returns.",
    "Combobox",
    "id: string, rows: () => readonly SelectRow[], on_change: (value: string, cx: Context) => \
     void, on_confirm: (value: string, cx: Context) => void",
  );
  out + STATES_D_TS
}

fn base_d_ts() -> String {
  let mut out = String::new();
  for (name, doc) in [
    (
      "Button",
      "Activation, focus, disabled and selected state. No styling.",
    ),
    (
      "Link",
      "An external HTTP(S) resource opened through the system browser.",
    ),
    (
      "Checkbox",
      "A controlled toggle. No styling: draw the indicator yourself.",
    ),
    ("Switch", "A controlled switch. No styling."),
    ("Radio", "One option in a radio group. No styling."),
    (
      "Toggle",
      "A button that stays down: `pressed(..)` in, `on_change(..)` out.",
    ),
    ("RadioGroup", "A set of radios, announced as one group."),
    ("ToggleGroup", "A set of toggles, announced as a toolbar."),
    (
      "Select",
      "A select root: the semantics and the keyboard, none of the picture.",
    ),
    (
      "Combobox",
      "A combobox root whose trigger is an editable field.",
    ),
  ] {
    out += &declare(doc, name, "id: string | number");
  }
  out += &declare(
    "A date-picker root; it holds no date.",
    "DatePicker",
    "id: string | number, focus_handle: FocusHandle",
  );
  for (name, state, doc) in [
    ("Input", "InputState", "The bare editor of retained text."),
    (
      "NumberInput",
      "InputState",
      "A spinbutton; give it `decrement_button`/`increment_button`.",
    ),
    (
      "Textarea",
      "TextareaState",
      "The bare editor of retained multi-line text.",
    ),
    (
      "OtpInput",
      "OtpState",
      "A code drawn cell by cell: `cell_style(..)`, `caret_style(..)`.",
    ),
    (
      "Slider",
      "SliderState",
      "The slider root: announces the value.",
    ),
    ("SliderTrack", "SliderState", "The press and drag surface."),
    (
      "SliderIndicator",
      "SliderState",
      "The groove; `range_style(..)` draws the fill.",
    ),
    (
      "SliderThumb",
      "SliderState",
      "The knob; `start(true)` is a range's lower one.",
    ),
  ] {
    out += &declare(doc, name, &format!("state: {state}"));
  }
  let states = "InputState, TextareaState, SliderState, OtpState, SliderValue, TextEvent";
  out += &format!("  import {{ {states} }} from \"gpui-component\";\n  export {{ {states} }};\n");
  out
}

/// The retained states, written out: they are rquickjs classes, not table rows
const STATES_D_TS: &str = r#"  /** A row of a `Select` or `Combobox`. */
  export interface SelectRow { id: string; label: string; disabled?: boolean; }
  /** What a text state's `on` handlers get; `submit` carries the keys held with Enter. */
  export interface TextEvent { secondary?: boolean; shift?: boolean; }
  /** Retained text of an `Input` or `NumberInput`. Make it in `init` or a handler. */
  export interface InputState {
    value(): string;
    /** Replaces the text; emits no `change`. */
    set_value(value: string): void;
    /** Replaces the selection with `text`, as typing it would. */
    replace(text: string): void;
    set_placeholder(text: string): void;
    set_masked(masked: boolean): void;
    set_loading(loading: boolean): void;
    /** How far a `NumberInput` steps; `null` stops stepping. */
    set_step(step: number | null): void;
    set_min(min: number | null): void;
    set_max(max: number | null): void;
    focus(): void;
    on(event: "change" | "submit" | "focus" | "blur", handler: (event: TextEvent, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const InputState: {
    (placeholder?: string, initial_value?: string): InputState;
    new: (options?: { placeholder?: string; value?: string }) => InputState;
  };
  /** Retained text of a `Textarea`. */
  export interface TextareaState {
    value(): string;
    set_value(value: string): void;
    replace(text: string): void;
    set_placeholder(text: string): void;
    set_rows(rows: number): void;
    set_auto_grow(min_rows: number, max_rows: number): void;
    set_soft_wrap(wrap: boolean): void;
    focus(): void;
    on(event: "change" | "submit" | "focus" | "blur", handler: (event: TextEvent, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const TextareaState: {
    (initial_value?: string): TextareaState;
    new: (options?: { placeholder?: string; value?: string; rows?: number }) => TextareaState;
  };
  /** One thumb, or the two ends of a range. */
  export type SliderValue = number | [number, number];
  /** Where a `Slider` is; a drag writes it without rendering the script. */
  export interface SliderState {
    value(): SliderValue;
    set_value(value: SliderValue): void;
    min_value(): number;
    max_value(): number;
    step_value(): number;
    /** `change` on every step of a drag, `release` once when it ends. */
    on(event: "change" | "release", handler: (value: SliderValue, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const SliderState: {
    (initial_value?: number): SliderState;
    new: (options?: { min?: number; max?: number; step?: number; scale?: "linear" | "logarithmic"; value?: SliderValue }) => SliderState;
  };
  /** The code an `OtpInput` edits; its length is fixed. */
  export interface OtpState {
    value(): string;
    set_value(value: string): void;
    len(): number;
    is_masked(): boolean;
    set_masked(masked: boolean): void;
    focus(): void;
    on(event: "change" | "complete" | "focus" | "blur", handler: (event: {}, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const OtpState: {
    (length: number, options?: { value?: string; masked?: boolean }): OtpState;
    new: (length: number, options?: { value?: string; masked?: boolean }) => OtpState;
  };
  export interface ColorPickerState {
    /** The color as `#rrggbb[aa]`. */
    value(): string | undefined;
    set_value(color: import("gpui-kit").Color | null): void;
    on(event: "change", handler: (color: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function ColorPickerState(): ColorPickerState;
  export interface DatePickerState {
    /** The picked date as `YYYY-MM-DD`. */
    value(): string | undefined;
    on(event: "change", handler: (date: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function DatePickerState(): DatePickerState;
  export interface TimeFieldState {
    /** The time as `HH:MM:SS`. */
    value(): string;
    on(event: "change", handler: (time: string, cx: Context) => void): boolean;
    release(): boolean;
  }
  export function TimeFieldState(): TimeFieldState;
"#;

fn el_d_ts() -> String {
  methods::d_ts()
}

pub(crate) const FAMILY: Family = Family {
  module: "gpui-component",
  names: NAMES,
  exports,
  methods: install,
  d_ts,
  el_d_ts,
};

pub(crate) const BASE: Family = Family {
  module: "gpui-base",
  names: BASE_NAMES,
  exports: base_exports,
  methods: |_| Ok(()),
  d_ts: base_d_ts,
  el_d_ts: String::new,
};
