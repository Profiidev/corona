//! Edge detection and measurements on a frozen frame, all in device pixels

use corona_capture::image::{Rgba, RgbaImage};

/// Pixels `start..=end` on one axis
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
  pub start: u32,
  pub end: u32,
}

impl Span {
  pub fn len(self) -> u32 {
    self.end - self.start + 1
  }
}

/// A live or pinned measurement: a box when `boxed`, else lines through `at`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
  pub at: (u32, u32),
  pub h: Option<Span>,
  pub v: Option<Span>,
  pub boxed: bool,
}

/// Whether any RGB channel of `a` and `b` is more than `tolerance` apart
fn differs(a: Rgba<u8>, b: Rgba<u8>, tolerance: u8) -> bool {
  (0..3).any(|i| a.0[i].abs_diff(b.0[i]) > tolerance)
}

/// The run of pixels around `at` that look like it, along x when `horizontal`, else y
fn span(image: &RgbaImage, (x, y): (u32, u32), horizontal: bool, tolerance: u8) -> Span {
  let base = *image.get_pixel(x, y);
  let (pos, max) = if horizontal {
    (x, image.width())
  } else {
    (y, image.height())
  };
  let same = |i: &u32| {
    let p = if horizontal {
      image.get_pixel(*i, y)
    } else {
      image.get_pixel(x, *i)
    };
    !differs(*p, base, tolerance)
  };
  Span {
    start: (0..pos).rev().take_while(same).last().unwrap_or(pos),
    end: (pos + 1..max).take_while(same).last().unwrap_or(pos),
  }
}

impl Measurement {
  /// The lines from `at` to the nearest edges on the `(horizontal, vertical)` axes
  pub fn spacing(image: &RgbaImage, at: (u32, u32), axes: (bool, bool), tolerance: u8) -> Self {
    Self {
      at,
      h: axes.0.then(|| span(image, at, true, tolerance)),
      v: axes.1.then(|| span(image, at, false, tolerance)),
      boxed: false,
    }
  }

  /// The box between the pixel corners `a` and `b`, none when it has no area
  pub fn bounds(a: (u32, u32), b: (u32, u32)) -> Option<Self> {
    let axis = |a: u32, b: u32| {
      (a != b).then(|| Span {
        start: a.min(b),
        end: a.max(b) - 1,
      })
    };
    Some(Self {
      at: b,
      h: Some(axis(a.0, b.0)?),
      v: Some(axis(a.1, b.1)?),
      boxed: true,
    })
  }

  pub fn label(&self) -> String {
    match (self.h, self.v) {
      (Some(h), Some(v)) => format!("{} × {}", h.len(), v.len()),
      (Some(s), None) | (None, Some(s)) => s.len().to_string(),
      (None, None) => String::new(),
    }
  }

  /// Whether `p` lies on the box or within `slack` of a line
  pub fn hit(&self, (x, y): (f32, f32), slack: f32) -> bool {
    let near = |s: Span, v: f32| (s.start as f32 - slack..=(s.end + 1) as f32 + slack).contains(&v);
    let across = |line: u32, v: f32| (v - (line as f32 + 0.5)).abs() <= slack;
    match (self.boxed, self.h, self.v) {
      (true, Some(h), Some(v)) => near(h, x) && near(v, y),
      _ => {
        self.h.is_some_and(|h| near(h, x) && across(self.at.1, y))
          || self.v.is_some_and(|v| near(v, y) && across(self.at.0, x))
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
  const GREY: Rgba<u8> = Rgba([240, 240, 240, 255]);
  const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

  /// 20×10 white with a black column at x = 15 and a black row at y = 2
  fn image() -> RgbaImage {
    RgbaImage::from_fn(20, 10, |x, y| if x == 15 || y == 2 { BLACK } else { WHITE })
  }

  #[test]
  fn spacing_stops_at_edges() {
    let m = Measurement::spacing(&image(), (5, 6), (true, true), 30);
    assert_eq!(m.h, Some(Span { start: 0, end: 14 }));
    assert_eq!(m.v, Some(Span { start: 3, end: 9 }));
    assert_eq!(m.label(), "15 × 7");
  }

  #[test]
  fn tolerance_decides_what_is_an_edge() {
    let mut img = RgbaImage::from_pixel(10, 1, WHITE);
    img.put_pixel(7, 0, GREY);
    let h = |tol| {
      Measurement::spacing(&img, (2, 0), (true, false), tol)
        .h
        .unwrap()
    };
    assert_eq!(h(10).end, 6);
    assert_eq!(h(15).end, 9);
  }

  #[test]
  fn single_axis_and_edge_pixel() {
    let m = Measurement::spacing(&image(), (15, 6), (false, true), 0);
    assert_eq!(m.h, None);
    // the black column runs through the black row, so it spans the full height
    assert_eq!(m.v, Some(Span { start: 0, end: 9 }));
    assert_eq!(m.label(), "10");
    let lone = Measurement::spacing(&image(), (0, 2), (false, true), 0);
    assert_eq!(lone.v.unwrap().len(), 1);
  }

  #[test]
  fn bounds_need_area() {
    assert_eq!(Measurement::bounds((3, 3), (3, 9)), None);
    assert_eq!(Measurement::bounds((3, 3), (3, 3)), None);
    let m = Measurement::bounds((10, 8), (2, 3)).unwrap();
    assert_eq!(m.label(), "8 × 5");
    assert_eq!(m.h, Some(Span { start: 2, end: 9 }));
  }

  #[test]
  fn hit_boxes_and_lines() {
    let b = Measurement::bounds((10, 10), (20, 20)).unwrap();
    assert!(b.hit((15., 15.), 2.));
    assert!(b.hit((9., 21.), 2.));
    assert!(!b.hit((5., 15.), 2.));

    let l = Measurement::spacing(&image(), (5, 6), (true, false), 30);
    assert!(l.hit((10., 6.5), 2.));
    assert!(l.hit((10., 8.), 2.));
    assert!(!l.hit((10., 9.), 2.));
    assert!(!l.hit((18., 6.5), 2.));
  }
}
