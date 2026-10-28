//! Style methods: every no-argument method of gpui's `Styled` and gpui-base's `StyledExt`
//! (the checked-in table in `styles.rs`, kept in step by a test against gpui's inspector
//! reflection), the methods with arguments, and the values they take: lengths
//! (`4`, `"4px"`, `"1rem"`, `"50%"`, `"auto"`, spacing tokens) and colors (theme tokens,
//! `"#rrggbb"`, `0xrrggbb`, a `Background`).

use gpui_kit::{
  AbsoluteLength, DefiniteLength, Hsla, Length, Pixels, StyleRefinement, Styled as _,
  base::{StyledExt as _, Theme},
  px, relative, rems, rgb,
};
use rquickjs::{Class, Ctx, Exception, FromJs, Function, Object, Result, Value, function::This};

use crate::{El, TsType, background::JsBackground, el::restyle, with_node};

/// `nullary! { w_0 w_neg_0 .. }`: the no-argument style methods by name
macro_rules! nullary {
  ($($name:ident)*) => {
    /// Every no-argument style method
    pub(crate) const NULLARY: &[(&str, fn(StyleRefinement) -> StyleRefinement)] =
      &[$((stringify!($name), |s| s.$name())),*];
  };
}

include!("styles.rs");

/// The color tokens of the theme, as `gpui_base::ColorTokens` names them
pub(crate) const COLOR_TOKENS: &[&str] = &[
  "background",
  "foreground",
  "surface",
  "surface_foreground",
  "primary",
  "primary_foreground",
  "secondary",
  "secondary_foreground",
  "muted",
  "muted_foreground",
  "accent",
  "accent_foreground",
  "destructive",
  "destructive_foreground",
  "border",
  "input",
  "ring",
  "selection",
];

pub(crate) const SPACING_TOKENS: &[&str] = &["xxs", "xs", "sm", "md", "lg", "xl", "xxl"];
pub(crate) const RADIUS_TOKENS: &[&str] = &["none", "sm", "md", "lg", "xl", "full"];

/// The active theme
pub(crate) fn theme() -> Theme {
  crate::scope::try_app()
    .map(|cx| Theme::global(cx))
    .unwrap_or_default()
}

pub(crate) fn color_token(theme: &Theme, name: &str) -> Option<Hsla> {
  let c = &theme.tokens.colors;
  Some(match name {
    "background" => c.background,
    "foreground" => c.foreground,
    "surface" => c.surface,
    "surface_foreground" => c.surface_foreground,
    "primary" => c.primary,
    "primary_foreground" => c.primary_foreground,
    "secondary" => c.secondary,
    "secondary_foreground" => c.secondary_foreground,
    "muted" => c.muted,
    "muted_foreground" => c.muted_foreground,
    "accent" => c.accent,
    "accent_foreground" => c.accent_foreground,
    "destructive" => c.destructive,
    "destructive_foreground" => c.destructive_foreground,
    "border" => c.border,
    "input" => c.input,
    "ring" => c.ring,
    "selection" => c.selection,
    _ => return None,
  })
}

pub(crate) fn spacing_token(theme: &Theme, name: &str) -> Option<Pixels> {
  let s = &theme.tokens.spacing;
  Some(match name {
    "xxs" => s.xxs,
    "xs" => s.xs,
    "sm" => s.sm,
    "md" => s.md,
    "lg" => s.lg,
    "xl" => s.xl,
    "xxl" => s.xxl,
    _ => return None,
  })
}

pub(crate) fn radius_token(theme: &Theme, name: &str) -> Option<Pixels> {
  let r = &theme.tokens.radius;
  Some(match name {
    "none" => r.none,
    "sm" => r.sm,
    "md" => r.md,
    "lg" => r.lg,
    "xl" => r.xl,
    "full" => r.full,
    _ => return None,
  })
}

/// The known name closest to `name`, if close enough to be a typo
fn suggest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
  let best = known.iter().map(|k| (distance(name, k), *k)).min();
  best
    .filter(|(d, _)| *d <= name.len().max(2) / 2)
    .map(|(_, k)| k)
}

/// `unknown color "primay", did you mean "primary"?`
pub(crate) fn unknown(what: &str, name: &str, known: &[&str]) -> String {
  match suggest(name, known) {
    Some(k) => format!("unknown {what} \"{name}\", did you mean \"{k}\"?"),
    None => format!("unknown {what} \"{name}\""),
  }
}

/// Levenshtein distance
fn distance(a: &str, b: &str) -> usize {
  let b: Vec<char> = b.chars().collect();
  let mut row: Vec<usize> = (0..=b.len()).collect();
  for (i, ca) in a.chars().enumerate() {
    let mut prev = row[0];
    row[0] = i + 1;
    for (j, cb) in b.iter().enumerate() {
      let next = (prev + (ca != *cb) as usize)
        .min(row[j] + 1)
        .min(row[j + 1] + 1);
      prev = row[j + 1];
      row[j + 1] = next;
    }
  }
  row[b.len()]
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`
pub(crate) fn hex(text: &str) -> Option<Hsla> {
  let digits = text.strip_prefix('#')?;
  let n = u32::from_str_radix(digits, 16).ok()?;
  Some(match digits.len() {
    3 => {
      let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
      rgb(((r * 0x11) << 16) | ((g * 0x11) << 8) | (b * 0x11)).into()
    }
    6 => rgb(n).into(),
    8 => gpui_kit::rgba(n).into(),
    _ => return None,
  })
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque
pub(crate) fn to_hex(color: Hsla) -> String {
  let c = color.to_rgb();
  let [r, g, b, a] = [c.r, c.g, c.b, c.a].map(|v| (v * 255.).round() as u8);
  match a {
    255 => format!("#{r:02x}{g:02x}{b:02x}"),
    a => format!("#{r:02x}{g:02x}{b:02x}{a:02x}"),
  }
}

/// A color: a theme token, `"#rrggbb"` or `0xrrggbb`
pub struct Color(pub Hsla);

impl<'js> FromJs<'js> for Color {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    if let Some(n) = value.as_number() {
      return Ok(Color(rgb(n as u32).into()));
    }
    let Some(text) = value.as_string().map(|s| s.to_string()).transpose()? else {
      return Err(Exception::throw_type(ctx, "expected a color"));
    };
    if text.starts_with('#') {
      return hex(&text).map(Color).ok_or_else(|| {
        Exception::throw_type(ctx, &format!("invalid color \"{text}\", expected #rrggbb"))
      });
    }
    color_token(&theme(), &text)
      .map(Color)
      .ok_or_else(|| Exception::throw_type(ctx, &unknown("color", &text, COLOR_TOKENS)))
  }
}

impl TsType for Color {
  fn ts() -> String {
    "Color".into()
  }
}

/// What a background is painted with: a [`Color`] or a `Background`
pub struct Fill(pub gpui_kit::Background);

impl<'js> FromJs<'js> for Fill {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match Class::<JsBackground>::from_js(ctx, value.clone()) {
      Ok(bg) => Ok(Fill(bg.borrow().bg)),
      Err(_) => Ok(Fill(Color::from_js(ctx, value)?.0.into())),
    }
  }
}

impl TsType for Fill {
  fn ts() -> String {
    "Color | Background".into()
  }
}

/// Parses a length: a number of pixels, `"Npx"`, `"Nrem"`, `"N%"`, `"auto"` or a token.
fn length<'js>(
  ctx: &Ctx<'js>,
  value: Value<'js>,
  auto: bool,
  percent: bool,
  tokens: Tokens,
) -> Result<Length> {
  if let Some(n) = value.as_number() {
    return Ok(px(n as f32).into());
  }
  let text = String::from_js(ctx, value)
    .map_err(|_| Exception::throw_type(ctx, "expected a length: a number or a string"))?;
  let number = |unit: &str| text.strip_suffix(unit)?.trim().parse::<f32>().ok();
  let parsed: Option<Length> = if auto && text == "auto" {
    Some(Length::Auto)
  } else if let Some(n) = number("px") {
    Some(px(n).into())
  } else if let Some(n) = number("rem") {
    Some(rems(n).into())
  } else if let Some(n) = number("%").filter(|_| percent) {
    Some(relative(n / 100.).into())
  } else {
    (tokens.1)(&theme(), &text).map(Into::into)
  };
  parsed.ok_or_else(|| {
    let mut forms = vec!["4", "\"4px\"", "\"1rem\""];
    forms.extend(percent.then_some("\"50%\""));
    forms.extend(auto.then_some("\"auto\""));
    let mut message = format!("invalid length \"{text}\", expected {}", forms.join(", "));
    if !tokens.0.is_empty() {
      message.push_str(" or a token");
      if let Some(k) = suggest(&text, tokens.0) {
        message = format!("{message}, did you mean \"{k}\"?");
      }
    }
    Exception::throw_type(ctx, &message)
  })
}

/// The token names a length takes and how they resolve
type Tokens = (&'static [&'static str], fn(&Theme, &str) -> Option<Pixels>);

const SPACING: Tokens = (SPACING_TOKENS, spacing_token);
const RADIUS: Tokens = (RADIUS_TOKENS, radius_token);
const NONE: Tokens = (&[], |_, _| None);

/// A `Length`: pixels, rems, a percentage, `"auto"` or a spacing token
pub struct Len(pub Length);

impl<'js> FromJs<'js> for Len {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    length(ctx, value, true, true, SPACING).map(Len)
  }
}

impl TsType for Len {
  fn ts() -> String {
    "Length".into()
  }
}

/// A `DefiniteLength`: pixels, rems, a percentage or a spacing token
pub struct Def(pub DefiniteLength);

impl<'js> FromJs<'js> for Def {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match length(ctx, value, false, true, SPACING)? {
      Length::Definite(d) => Ok(Def(d)),
      Length::Auto => unreachable!("auto is refused"),
    }
  }
}

impl TsType for Def {
  fn ts() -> String {
    "DefiniteLength".into()
  }
}

fn absolute<'js>(ctx: &Ctx<'js>, value: Value<'js>, tokens: Tokens) -> Result<AbsoluteLength> {
  match length(ctx, value, false, false, tokens)? {
    Length::Definite(DefiniteLength::Absolute(a)) => Ok(a),
    _ => unreachable!("percentages and auto are refused"),
  }
}

/// An `AbsoluteLength`: pixels or rems
pub struct Abs(pub AbsoluteLength);

impl<'js> FromJs<'js> for Abs {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    absolute(ctx, value, NONE).map(Abs)
  }
}

impl TsType for Abs {
  fn ts() -> String {
    "AbsoluteLength".into()
  }
}

/// A corner radius: pixels, rems or a radius token
pub struct Radius(pub AbsoluteLength);

impl<'js> FromJs<'js> for Radius {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    absolute(ctx, value, RADIUS).map(Radius)
  }
}

impl TsType for Radius {
  fn ts() -> String {
    "AbsoluteLength | RadiusToken".into()
  }
}

/// A line height: a bare number is a multiplier, a string a length
pub struct LineHeight(pub DefiniteLength);

impl<'js> FromJs<'js> for LineHeight {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match value.as_number() {
      Some(n) => Ok(LineHeight(relative(n as f32))),
      None => Def::from_js(ctx, value).map(|d| LineHeight(d.0)),
    }
  }
}

impl TsType for LineHeight {
  fn ts() -> String {
    "number | DefiniteLength".into()
  }
}

mod params {
  use gpui_kit::{FontWeight, Styled};

  use super::{Abs, Color, Def, Fill, Len, LineHeight, Radius};
  use crate::{Opt, el::restyle};

  crate::el_methods! {
    /// Sets the width.
    fn w(v: Len) |c, n| restyle(c, n, |s| s.w(v.0));
    /// Sets the height.
    fn h(v: Len) |c, n| restyle(c, n, |s| s.h(v.0));
    /// Sets the width and the height together.
    fn size(v: Len) |c, n| restyle(c, n, |s| s.size(v.0));
    /// Sets the minimum width.
    fn min_w(v: Len) |c, n| restyle(c, n, |s| s.min_w(v.0));
    /// Sets the minimum height.
    fn min_h(v: Len) |c, n| restyle(c, n, |s| s.min_h(v.0));
    /// Sets the minimum width and height together.
    fn min_size(v: Len) |c, n| restyle(c, n, |s| s.min_size(v.0));
    /// Sets the maximum width.
    fn max_w(v: Len) |c, n| restyle(c, n, |s| s.max_w(v.0));
    /// Sets the maximum height.
    fn max_h(v: Len) |c, n| restyle(c, n, |s| s.max_h(v.0));
    /// Sets the maximum width and height together.
    fn max_size(v: Len) |c, n| restyle(c, n, |s| s.max_size(v.0));
    /// Sets the padding on all four sides.
    fn p(v: Def) |c, n| restyle(c, n, |s| s.p(v.0));
    /// Sets the padding on the left and right.
    fn px(v: Def) |c, n| restyle(c, n, |s| s.px(v.0));
    /// Sets the padding on the top and bottom.
    fn py(v: Def) |c, n| restyle(c, n, |s| s.py(v.0));
    /// Sets the padding on the top.
    fn pt(v: Def) |c, n| restyle(c, n, |s| s.pt(v.0));
    /// Sets the padding on the bottom.
    fn pb(v: Def) |c, n| restyle(c, n, |s| s.pb(v.0));
    /// Sets the padding on the left.
    fn pl(v: Def) |c, n| restyle(c, n, |s| s.pl(v.0));
    /// Sets the padding on the right.
    fn pr(v: Def) |c, n| restyle(c, n, |s| s.pr(v.0));
    /// Sets the margin on all four sides.
    fn m(v: Len) |c, n| restyle(c, n, |s| s.m(v.0));
    /// Sets the margin on the left and right.
    fn mx(v: Len) |c, n| restyle(c, n, |s| s.mx(v.0));
    /// Sets the margin on the top and bottom.
    fn my(v: Len) |c, n| restyle(c, n, |s| s.my(v.0));
    /// Sets the margin on the top.
    fn mt(v: Len) |c, n| restyle(c, n, |s| s.mt(v.0));
    /// Sets the margin on the bottom.
    fn mb(v: Len) |c, n| restyle(c, n, |s| s.mb(v.0));
    /// Sets the margin on the left.
    fn ml(v: Len) |c, n| restyle(c, n, |s| s.ml(v.0));
    /// Sets the margin on the right.
    fn mr(v: Len) |c, n| restyle(c, n, |s| s.mr(v.0));
    /// Sets all four offsets of a positioned element.
    fn inset(v: Len) |c, n| restyle(c, n, |s| s.inset(v.0));
    /// Sets the top offset of a positioned element.
    fn top(v: Len) |c, n| restyle(c, n, |s| s.top(v.0));
    /// Sets the bottom offset of a positioned element.
    fn bottom(v: Len) |c, n| restyle(c, n, |s| s.bottom(v.0));
    /// Sets the left offset of a positioned element.
    fn left(v: Len) |c, n| restyle(c, n, |s| s.left(v.0));
    /// Sets the right offset of a positioned element.
    fn right(v: Len) |c, n| restyle(c, n, |s| s.right(v.0));
    /// Sets the gap between children on both axes.
    fn gap(v: Def) |c, n| restyle(c, n, |s| s.gap(v.0));
    /// Sets the gap between columns.
    fn gap_x(v: Def) |c, n| restyle(c, n, |s| s.gap_x(v.0));
    /// Sets the gap between rows.
    fn gap_y(v: Def) |c, n| restyle(c, n, |s| s.gap_y(v.0));
    /// Sets how much of the free space this child takes, 1 when left out.
    fn flex_grow(v: Opt<f32>) |c, n| restyle(c, n, |mut s| {
      s.flex_grow = Some(v.0.unwrap_or(1.));
      s
    });
    /// Sets how readily this child gives space back, 1 when left out.
    fn flex_shrink(v: Opt<f32>) |c, n| restyle(c, n, |mut s| {
      s.flex_shrink = Some(v.0.unwrap_or(1.));
      s
    });
    /// Sets the size this child starts from before growing or shrinking.
    fn flex_basis(v: Len) |c, n| restyle(c, n, |s| s.flex_basis(v.0));
    /// Sets the background.
    fn bg(v: Fill) |c, n| restyle(c, n, |s| s.bg(v.0));
    /// Sets the text color, which children inherit.
    fn text_color(v: Color) |c, n| restyle(c, n, |s| s.text_color(v.0));
    /// Sets the background painted behind the text itself.
    fn text_bg(v: Color) |c, n| restyle(c, n, |s| s.text_bg(v.0));
    /// Sets the font size.
    fn text_size(v: Abs) |c, n| restyle(c, n, |s| s.text_size(v.0));
    /// Sets the font family.
    fn font_family(v: String) |c, n| restyle(c, n, |s| s.font_family(v));
    /// Sets the font weight, 100 to 900.
    fn font_weight(v: f32) |c, n| restyle(c, n, |s| s.font_weight(FontWeight(v)));
    /// Sets the line height. A bare number is a multiplier (`1.45`), a string a length.
    fn line_height(v: LineHeight) |c, n| restyle(c, n, |s| s.line_height(v.0));
    /// Sets the opacity of the element and everything in it, from 0 to 1.
    fn opacity(v: f32) |c, n| restyle(c, n, |s| s.opacity(v));
    /// Sets the border width on all four sides. Draws nothing without a color.
    fn border(v: Abs) |c, n| restyle(c, n, |s| s.border(v.0));
    /// Sets the border width on the top.
    fn border_t(v: Abs) |c, n| restyle(c, n, |s| s.border_t(v.0));
    /// Sets the border width on the bottom.
    fn border_b(v: Abs) |c, n| restyle(c, n, |s| s.border_b(v.0));
    /// Sets the border width on the left.
    fn border_l(v: Abs) |c, n| restyle(c, n, |s| s.border_l(v.0));
    /// Sets the border width on the right.
    fn border_r(v: Abs) |c, n| restyle(c, n, |s| s.border_r(v.0));
    /// Sets the border width on the left and right.
    fn border_x(v: Abs) |c, n| restyle(c, n, |s| s.border_x(v.0));
    /// Sets the border width on the top and bottom.
    fn border_y(v: Abs) |c, n| restyle(c, n, |s| s.border_y(v.0));
    /// Sets the border color. Draws nothing without a width.
    fn border_color(v: Color) |c, n| restyle(c, n, |s| s.border_color(v.0));
    /// Sets the corner radius on all four corners.
    fn rounded(v: Radius) |c, n| restyle(c, n, |s| s.rounded(v.0));
    /// Sets the corner radius on the two top corners.
    fn rounded_t(v: Radius) |c, n| restyle(c, n, |s| s.rounded_t(v.0));
    /// Sets the corner radius on the two bottom corners.
    fn rounded_b(v: Radius) |c, n| restyle(c, n, |s| s.rounded_b(v.0));
    /// Sets the corner radius on the two left corners.
    fn rounded_l(v: Radius) |c, n| restyle(c, n, |s| s.rounded_l(v.0));
    /// Sets the corner radius on the two right corners.
    fn rounded_r(v: Radius) |c, n| restyle(c, n, |s| s.rounded_r(v.0));
    /// Sets the corner radius on the top-left corner.
    fn rounded_tl(v: Radius) |c, n| restyle(c, n, |s| s.rounded_tl(v.0));
    /// Sets the corner radius on the top-right corner.
    fn rounded_tr(v: Radius) |c, n| restyle(c, n, |s| s.rounded_tr(v.0));
    /// Sets the corner radius on the bottom-left corner.
    fn rounded_bl(v: Radius) |c, n| restyle(c, n, |s| s.rounded_bl(v.0));
    /// Sets the corner radius on the bottom-right corner.
    fn rounded_br(v: Radius) |c, n| restyle(c, n, |s| s.rounded_br(v.0));
  }
}

/// Puts every style method on the element prototype.
pub(crate) fn install<'js>(proto: &Object<'js>) -> Result<()> {
  for (name, f) in NULLARY {
    let f = *f;
    let method = Function::new(proto.ctx().clone(), move |this: This<Class<'js, El>>| {
      with_node(&this.0, |ctx, n| restyle(ctx, n, f))?;
      Ok::<_, rquickjs::Error>(this.0)
    })?;
    proto.set(*name, method)?;
  }
  params::install(proto)
}

/// The style members of `interface Element`. The no-argument ones are one mapped type, as
/// documenting them one by one would repeat gpui's few doc lines thousands of times.
pub(crate) fn d_ts() -> String {
  params::d_ts()
}

/// `type NullaryStyle = "w_0" | ..`
pub(crate) fn nullary_d_ts() -> String {
  let names: Vec<String> = NULLARY
    .iter()
    .map(|(name, _)| format!("    | \"{name}\"\n"))
    .collect();
  format!(
    "  /** Every no-argument style method of gpui's `Styled` and gpui-base's `StyledExt`. */\n  export type NullaryStyle =\n{};\n",
    names.concat().trim_end()
  )
}
