//! The theme a script reads (`cx.theme()`) and sets (`set_theme` of `corona/base`): gpui-base's
//! semantic tokens, which gpui-component keeps in step with corona's theme.

use gpui_kit::{
  FontWeight, Pixels,
  base::{TextStyleToken, Theme, ThemeAppearance},
  px,
};
use rquickjs::{Ctx, Exception, Object, Result};

use crate::{
  app,
  element::style::{
    COLOR_TOKENS, Finite, RADIUS_TOKENS, SPACING_TOKENS, color_token, hex, radius_token,
    spacing_token, theme, to_hex,
  },
};

const TEXT_STEPS: &[&str] = &["xs", "sm", "md", "lg", "xl", "mono_md"];

fn text_step(theme: &Theme, name: &str) -> TextStyleToken {
  let t = &theme.tokens.typography;
  match name {
    "xs" => t.xs,
    "sm" => t.sm,
    "md" => t.md,
    "lg" => t.lg,
    "xl" => t.xl,
    _ => t.mono_md,
  }
}

fn text_step_mut<'a>(theme: &'a mut Theme, name: &str) -> &'a mut TextStyleToken {
  let t = &mut theme.tokens.typography;
  match name {
    "xs" => &mut t.xs,
    "sm" => &mut t.sm,
    "md" => &mut t.md,
    "lg" => &mut t.lg,
    "xl" => &mut t.xl,
    _ => &mut t.mono_md,
  }
}

fn pixels(p: Pixels) -> f32 {
  p.into()
}

/// What `cx.theme()` returns
pub(crate) fn theme_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let theme = theme();
  let o = Object::new(ctx.clone())?;
  let dark = theme.appearance == ThemeAppearance::Dark;
  o.set("appearance", if dark { "dark" } else { "light" })?;
  o.set("is_dark", dark)?;
  let colors = Object::new(ctx.clone())?;
  for name in COLOR_TOKENS {
    let hex = to_hex(color_token(&theme, name).expect("token"));
    colors.set(*name, hex.clone())?;
    o.set(*name, hex)?;
  }
  o.set("colors", colors)?;
  let spacing = Object::new(ctx.clone())?;
  for name in SPACING_TOKENS {
    spacing.set(*name, pixels(spacing_token(&theme, name).expect("token")))?;
  }
  o.set("spacing", spacing)?;
  let radius = Object::new(ctx.clone())?;
  for name in RADIUS_TOKENS {
    radius.set(*name, pixels(radius_token(&theme, name).expect("token")))?;
  }
  o.set("radius", radius)?;
  let typography = Object::new(ctx.clone())?;
  typography.set("sans", theme.tokens.typography.sans.to_string())?;
  typography.set("mono", theme.tokens.typography.mono.to_string())?;
  for name in TEXT_STEPS {
    let step = text_step(&theme, name);
    let s = Object::new(ctx.clone())?;
    s.set("size", pixels(step.size))?;
    s.set("line_height", pixels(step.line_height))?;
    s.set("weight", step.weight.0)?;
    typography.set(*name, s)?;
  }
  o.set("typography", typography)?;
  Ok(o)
}

/// A finite, non-negative pixel value: the theme feeds layout of every window
pub(crate) fn size(ctx: &Ctx<'_>, v: Finite) -> Result<f32> {
  match v.0 {
    n if n >= 0.0 => Ok(n),
    _ => Err(Exception::throw_type(ctx, "expected a non-negative number")),
  }
}

/// `set_theme({ appearance, tokens: { colors, spacing, radius, typography? } })`: replaces the
/// tokens it names and redraws every window. Not while rendering.
pub(crate) fn set_theme<'js>(ctx: Ctx<'js>, value: Object<'js>) -> Result<()> {
  if crate::runtime::scope::try_frame().is_some() {
    return Err(Exception::throw_type(
      &ctx,
      "set_theme while rendering would loop",
    ));
  }
  let mut theme = theme();
  let bad = |what: String| Exception::throw_type(&ctx, &what);
  match value.get::<_, Option<String>>("appearance")?.as_deref() {
    Some("dark") => theme.appearance = ThemeAppearance::Dark,
    Some("light") => theme.appearance = ThemeAppearance::Light,
    None => {}
    Some(other) => {
      return Err(bad(format!(
        "appearance \"{other}\", expected light or dark"
      )));
    }
  }
  let tokens: Object = value.get("tokens")?;
  if let Some(colors) = tokens.get::<_, Option<Object>>("colors")? {
    for key in colors.keys::<String>() {
      let key = key?;
      let value: String = colors.get(&key)?;
      let color =
        hex(&value).ok_or_else(|| bad(format!("color {key}: \"{value}\" is no #rrggbb")))?;
      let c = &mut theme.tokens.colors;
      *match key.as_str() {
        "background" => &mut c.background,
        "foreground" => &mut c.foreground,
        "surface" => &mut c.surface,
        "surface_foreground" => &mut c.surface_foreground,
        "primary" => &mut c.primary,
        "primary_foreground" => &mut c.primary_foreground,
        "secondary" => &mut c.secondary,
        "secondary_foreground" => &mut c.secondary_foreground,
        "muted" => &mut c.muted,
        "muted_foreground" => &mut c.muted_foreground,
        "accent" => &mut c.accent,
        "accent_foreground" => &mut c.accent_foreground,
        "destructive" => &mut c.destructive,
        "destructive_foreground" => &mut c.destructive_foreground,
        "border" => &mut c.border,
        "input" => &mut c.input,
        "ring" => &mut c.ring,
        "selection" => &mut c.selection,
        _ => {
          return Err(bad(crate::element::style::unknown(
            "color token",
            &key,
            COLOR_TOKENS,
          )));
        }
      } = color;
    }
  }
  if let Some(spacing) = tokens.get::<_, Option<Object>>("spacing")? {
    for key in spacing.keys::<String>() {
      let key = key?;
      let s = &mut theme.tokens.spacing;
      *match key.as_str() {
        "xxs" => &mut s.xxs,
        "xs" => &mut s.xs,
        "sm" => &mut s.sm,
        "md" => &mut s.md,
        "lg" => &mut s.lg,
        "xl" => &mut s.xl,
        "xxl" => &mut s.xxl,
        _ => {
          return Err(bad(crate::element::style::unknown(
            "spacing token",
            &key,
            SPACING_TOKENS,
          )));
        }
      } = px(size(&ctx, spacing.get(&key)?)?);
    }
  }
  if let Some(radius) = tokens.get::<_, Option<Object>>("radius")? {
    for key in radius.keys::<String>() {
      let key = key?;
      let r = &mut theme.tokens.radius;
      *match key.as_str() {
        "none" => &mut r.none,
        "sm" => &mut r.sm,
        "md" => &mut r.md,
        "lg" => &mut r.lg,
        "xl" => &mut r.xl,
        "full" => &mut r.full,
        _ => {
          return Err(bad(crate::element::style::unknown(
            "radius token",
            &key,
            RADIUS_TOKENS,
          )));
        }
      } = px(size(&ctx, radius.get(&key)?)?);
    }
  }
  if let Some(typography) = tokens.get::<_, Option<Object>>("typography")? {
    if let Some(sans) = typography.get::<_, Option<String>>("sans")? {
      theme.tokens.typography.sans = sans.into();
    }
    if let Some(mono) = typography.get::<_, Option<String>>("mono")? {
      theme.tokens.typography.mono = mono.into();
    }
    for name in TEXT_STEPS {
      let Some(step) = typography.get::<_, Option<Object>>(*name)? else {
        continue;
      };
      let token = text_step_mut(&mut theme, name);
      if let Some(v) = step.get::<_, Option<Finite>>("size")? {
        token.size = px(size(&ctx, v)?);
      }
      if let Some(v) = step.get::<_, Option<Finite>>("line_height")? {
        token.line_height = px(size(&ctx, v)?);
      }
      if let Some(Finite(weight)) = step.get::<_, Option<Finite>>("weight")? {
        token.weight = FontWeight(weight);
      }
    }
  }
  let cx = app();
  *Theme::global_mut(cx) = theme;
  cx.refresh_windows();
  Ok(())
}

/// The theme's TypeScript, in `corona/base`
pub(crate) fn d_ts() -> String {
  let union = |names: &[&str]| -> String {
    names
      .iter()
      .map(|n| format!("\"{n}\""))
      .collect::<Vec<_>>()
      .join(" | ")
  };
  format!(
    r#"  export type ColorTokens = {{ readonly [Role in import("corona/kit").ColorToken]: import("corona/kit").Color }};
  export type SpacingTokens = {{ readonly [Step in {spacing}]: number }};
  export type RadiusTokens = {{ readonly [Step in {radius}]: number }};
  export interface TextStyleToken {{
    readonly size: number;
    readonly line_height: number;
    /** 100 is thin, 400 regular, 700 bold. */
    readonly weight: number;
  }}
  export type TypographyTokens = {{ readonly sans: string; readonly mono: string }} & {{
    readonly [Step in {steps}]: TextStyleToken;
  }};
  export interface SemanticThemeTokens {{
    readonly colors: ColorTokens;
    readonly spacing: SpacingTokens;
    readonly radius: RadiusTokens;
    readonly typography: TypographyTokens;
  }}
  /** The semantic tokens plus the appearance; the colors also directly. Read-only. */
  export interface Theme extends SemanticThemeTokens, ColorTokens {{
    readonly appearance: "light" | "dark";
    readonly is_dark: boolean;
  }}
  /** Replaces the tokens it names and redraws every window. Not while rendering. */
  export function set_theme(theme: {{
    readonly appearance?: "light" | "dark";
    readonly tokens: {{
      readonly colors?: Partial<Record<import("corona/kit").ColorToken, `#${{string}}`>>;
      readonly spacing?: Partial<SpacingTokens>;
      readonly radius?: Partial<RadiusTokens>;
      readonly typography?: {{ readonly sans?: string; readonly mono?: string }} & {{
        readonly [Step in {steps}]?: Partial<TextStyleToken>;
      }};
    }};
  }}): void;
"#,
    spacing = union(SPACING_TOKENS),
    radius = union(RADIUS_TOKENS),
    steps = union(TEXT_STEPS),
  )
}
