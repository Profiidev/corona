//! `Background` (solid, gradient and pattern fills) and `PathBuilder` with
//! `window.paint_path`, mirroring gpui's.

use gpui_kit::{
  Background, Canvas, ColorSpace, LinearColorStop, PathBuilder, canvas, checkerboard,
  linear_color_stop, linear_gradient, pattern_slash, point, px, solid_background,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, JsLifetime, Object, Result, Value,
  class::Trace,
  function::{Opt, This},
};

use crate::{El, Named, el::new_el, style::Color};

/// A fill: `Background.solid(color)`, `Background.linear_gradient(..)`, ..
#[derive(Trace, JsLifetime, Clone, Copy)]
#[rquickjs::class(rename = "Background")]
pub struct JsBackground {
  #[qjs(skip_trace)]
  pub(crate) bg: Background,
}

#[rquickjs::methods]
impl JsBackground {
  /// Scales the opacity of every color.
  pub fn opacity(&self, factor: f32) -> Self {
    Self {
      bg: self.bg.opacity(factor),
    }
  }

  /// The space a gradient interpolates in: `"srgb"` or `"oklab"`.
  pub fn color_space(&self, ctx: Ctx<'_>, space: String) -> Result<Self> {
    let space = match space.as_str() {
      "srgb" => ColorSpace::Srgb,
      "oklab" => ColorSpace::Oklab,
      _ => {
        return Err(Exception::throw_type(
          &ctx,
          "expected \"srgb\" or \"oklab\"",
        ));
      }
    };
    Ok(Self {
      bg: self.bg.color_space(space),
    })
  }
}

impl Named for JsBackground {
  const TS: &'static str = "Background";
}

/// A gradient stop: `Background.stop(color, percentage)`
#[derive(Trace, JsLifetime, Clone, Copy)]
#[rquickjs::class(rename = "BackgroundStop")]
pub struct Stop {
  #[qjs(skip_trace)]
  stop: LinearColorStop,
}

/// A color or a stop, the end of a gradient
struct End(LinearColorStop);

impl<'js> FromJs<'js> for End {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match Class::<Stop>::from_js(ctx, value.clone()) {
      Ok(stop) => Ok(End(stop.borrow().stop)),
      Err(_) => Ok(End(linear_color_stop(Color::from_js(ctx, value)?.0, 0.))),
    }
  }
}

/// The `Background` export
pub(crate) fn backgrounds<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  let bg = |bg| JsBackground { bg };
  o.set(
    "solid",
    Function::new(ctx.clone(), move |c: Color| bg(solid_background(c.0)))?,
  )?;
  o.set(
    "stop",
    Function::new(ctx.clone(), |c: Color, percentage: f32| Stop {
      stop: linear_color_stop(c.0, percentage),
    })?,
  )?;
  o.set(
    "linear_gradient",
    Function::new(ctx.clone(), move |angle: f32, from: End, to: End| {
      let mut to = to.0;
      // a bare color ends the gradient
      if to.percentage == 0. {
        to.percentage = 1.;
      }
      bg(linear_gradient(angle, from.0, to))
    })?,
  )?;
  o.set(
    "pattern_slash",
    Function::new(ctx.clone(), move |c: Color, width: f32, interval: f32| {
      bg(pattern_slash(c.0, width, interval))
    })?,
  )?;
  o.set(
    "checkerboard",
    Function::new(ctx.clone(), move |c: Color, size: f32| {
      bg(checkerboard(c.0, size))
    })?,
  )?;
  Ok(o)
}

#[derive(Clone)]
enum Op {
  Move(f32, f32),
  Line(f32, f32),
  Curve([f32; 4]),
  Cubic([f32; 6]),
  Arc([f32; 5], bool, bool),
  Polygon(Vec<(f32, f32)>, bool),
  Close,
}

/// Geometry: `PathBuilder.fill()` or `PathBuilder.stroke(width)`, then `move_to`, .., `build()`
#[derive(Trace, JsLifetime, Clone, Default)]
#[rquickjs::class(rename = "PathBuilder")]
pub struct JsPathBuilder {
  #[qjs(skip_trace)]
  stroke: Option<f32>,
  #[qjs(skip_trace)]
  dash: Vec<f32>,
  #[qjs(skip_trace)]
  ops: Vec<Op>,
}

/// What `PathBuilder.build()` makes, for `window.paint_path`
#[derive(Trace, JsLifetime, Clone)]
#[rquickjs::class(rename = "Path")]
pub struct JsPath {
  #[qjs(skip_trace)]
  path: JsPathBuilder,
}

impl Named for JsPath {
  const TS: &'static str = "Path";
}

fn op<'js>(this: This<Class<'js, JsPathBuilder>>, op: Op) -> Class<'js, JsPathBuilder> {
  this.borrow_mut().ops.push(op);
  this.0
}

#[rquickjs::methods]
impl JsPathBuilder {
  pub fn move_to<'js>(this: This<Class<'js, Self>>, x: f32, y: f32) -> Class<'js, Self> {
    op(this, Op::Move(x, y))
  }

  pub fn line_to<'js>(this: This<Class<'js, Self>>, x: f32, y: f32) -> Class<'js, Self> {
    op(this, Op::Line(x, y))
  }

  pub fn curve_to<'js>(
    this: This<Class<'js, Self>>,
    to_x: f32,
    to_y: f32,
    control_x: f32,
    control_y: f32,
  ) -> Class<'js, Self> {
    op(this, Op::Curve([to_x, to_y, control_x, control_y]))
  }

  #[allow(clippy::too_many_arguments)]
  pub fn cubic_bezier_to<'js>(
    this: This<Class<'js, Self>>,
    to_x: f32,
    to_y: f32,
    control_a_x: f32,
    control_a_y: f32,
    control_b_x: f32,
    control_b_y: f32,
  ) -> Class<'js, Self> {
    let p = [
      to_x,
      to_y,
      control_a_x,
      control_a_y,
      control_b_x,
      control_b_y,
    ];
    op(this, Op::Cubic(p))
  }

  #[allow(clippy::too_many_arguments)]
  pub fn arc_to<'js>(
    this: This<Class<'js, Self>>,
    radius_x: f32,
    radius_y: f32,
    rotation: f32,
    large_arc: bool,
    sweep: bool,
    to_x: f32,
    to_y: f32,
  ) -> Class<'js, Self> {
    let p = [radius_x, radius_y, rotation, to_x, to_y];
    op(this, Op::Arc(p, large_arc, sweep))
  }

  pub fn add_polygon<'js>(
    this: This<Class<'js, Self>>,
    points: Vec<Vec<f32>>,
    closed: Opt<bool>,
  ) -> Class<'js, Self> {
    let points = points.iter().map(|p| {
      (
        p.first().copied().unwrap_or(0.),
        p.get(1).copied().unwrap_or(0.),
      )
    });
    op(
      this,
      Op::Polygon(points.collect(), closed.0.unwrap_or(true)),
    )
  }

  pub fn close<'js>(this: This<Class<'js, Self>>) -> Class<'js, Self> {
    op(this, Op::Close)
  }

  pub fn dash_array<'js>(this: This<Class<'js, Self>>, values: Vec<f32>) -> Class<'js, Self> {
    this.borrow_mut().dash = values;
    this.0
  }

  pub fn build(&self) -> JsPath {
    JsPath { path: self.clone() }
  }
}

/// The `PathBuilder` export
pub(crate) fn path_builders<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("fill", Function::new(ctx.clone(), JsPathBuilder::default)?)?;
  o.set(
    "stroke",
    Function::new(ctx.clone(), |width: f32| JsPathBuilder {
      stroke: Some(width),
      ..Default::default()
    })?,
  )?;
  Ok(o)
}

crate::node!(Canvas<()>, "canvas", style);

/// An element painting `path` with `fill`, its coordinates relative to the element
pub(crate) fn paint_path<'js>(
  ctx: &Ctx<'js>,
  path: &JsPath,
  fill: Background,
) -> Result<Class<'js, El>> {
  let path = path.path.clone();
  let el = canvas(
    |_, _, _| (),
    move |bounds, _, window, _| {
      let mut b = match path.stroke {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
      };
      if !path.dash.is_empty() {
        b = b.dash_array(&path.dash.iter().map(|d| px(*d)).collect::<Vec<_>>());
      }
      let o = bounds.origin;
      let p = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
      for op in &path.ops {
        match op {
          Op::Move(x, y) => b.move_to(p(*x, *y)),
          Op::Line(x, y) => b.line_to(p(*x, *y)),
          Op::Curve([x, y, cx, cy]) => b.curve_to(p(*x, *y), p(*cx, *cy)),
          Op::Cubic([x, y, ax, ay, bx, by]) => {
            b.cubic_bezier_to(p(*x, *y), p(*ax, *ay), p(*bx, *by))
          }
          Op::Arc([rx, ry, rot, x, y], large, sweep) => {
            b.arc_to(point(px(*rx), px(*ry)), px(*rot), *large, *sweep, p(*x, *y))
          }
          Op::Polygon(points, closed) => {
            let points: Vec<_> = points.iter().map(|(x, y)| p(*x, *y)).collect();
            b.add_polygon(&points, *closed)
          }
          Op::Close => b.close(),
        }
      }
      match b.build() {
        Ok(path) => window.paint_path(path, fill),
        Err(e) => tracing::warn!("paint_path: {e}"),
      }
    },
  );
  new_el(ctx, el)
}
