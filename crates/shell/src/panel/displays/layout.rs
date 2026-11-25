//! The maths of the layout map, free of any UI so it can be tested

/// A monitor in layout space: logical pixels, top-left origin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
  pub x: i32,
  pub y: i32,
  pub w: i32,
  pub h: i32,
}

impl Rect {
  pub fn right(&self) -> i32 {
    self.x + self.w
  }

  pub fn bottom(&self) -> i32 {
    self.y + self.h
  }

  /// Share area, which Hyprland does not allow between monitors
  pub fn overlaps(&self, o: &Rect) -> bool {
    self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
  }

  /// Touch along an edge for more than a point. The cursor only crosses
  /// between monitors there; at a corner, or across a gap, it stops.
  pub fn adjoins(&self, o: &Rect) -> bool {
    let side = |a1: i32, a2: i32, b1: i32, b2: i32| a1.max(b1) < a2.min(b2);
    let vertical =
      (self.right() == o.x || o.right() == self.x) && side(self.y, self.bottom(), o.y, o.bottom());
    let horizontal =
      (self.bottom() == o.y || o.bottom() == self.y) && side(self.x, self.right(), o.x, o.right());
    vertical || horizontal
  }
}

/// Indices of rects that overlap another
pub(crate) fn overlapping(rects: &[Rect]) -> Vec<usize> {
  (0..rects.len())
    .filter(|&i| (0..rects.len()).any(|j| i != j && rects[i].overlaps(&rects[j])))
    .collect()
}

/// Indices of rects the cursor cannot reach from the others: not joined by
/// shared edges, directly or through other monitors, to the first one
pub(crate) fn unreachable(rects: &[Rect]) -> Vec<usize> {
  if rects.len() < 2 {
    return Vec::new();
  }
  let mut seen = vec![false; rects.len()];
  let mut stack = vec![0];
  seen[0] = true;
  while let Some(i) = stack.pop() {
    for j in 0..rects.len() {
      if !seen[j] && rects[i].adjoins(&rects[j]) {
        seen[j] = true;
        stack.push(j);
      }
    }
  }
  (0..rects.len()).filter(|&i| !seen[i]).collect()
}

/// How layout space maps onto the map widget
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
  pub k: f32,
  world_x: f32,
  world_y: f32,
  off_x: f32,
  off_y: f32,
}

impl View {
  /// The window is every monitor at `rects` with room around it, so a monitor
  /// can be dragged off to any side. Built once, when the view opens, so the
  /// map does not rescale under the pointer.
  pub fn new(rects: &[Rect], map_w: f32, map_h: f32) -> Self {
    let x0 = rects.iter().map(|r| r.x).min().unwrap_or(0) as f32;
    let y0 = rects.iter().map(|r| r.y).min().unwrap_or(0) as f32;
    let x1 = rects.iter().map(|r| r.right()).max().unwrap_or(1) as f32;
    let y1 = rects.iter().map(|r| r.bottom()).max().unwrap_or(1) as f32;
    let pad_x = ((x1 - x0) * 0.25).max(400.);
    let pad_y = ((y1 - y0) * 0.35).max(400.);
    let (world_x, world_y) = (x0 - pad_x, y0 - pad_y);
    let (ww, wh) = (x1 - x0 + 2. * pad_x, y1 - y0 + 2. * pad_y);
    let k = (map_w / ww).min(map_h / wh);
    Self {
      k,
      world_x,
      world_y,
      off_x: (map_w - ww * k) / 2.,
      off_y: (map_h - wh * k) / 2.,
    }
  }

  /// Map position and size of `r`
  pub fn to_map(self, r: &Rect) -> (f32, f32, f32, f32) {
    (
      self.off_x + (r.x as f32 - self.world_x) * self.k,
      self.off_y + (r.y as f32 - self.world_y) * self.k,
      r.w as f32 * self.k,
      r.h as f32 * self.k,
    )
  }
}

fn snap_axis(v: f32, candidates: &[i32], threshold: f32) -> f32 {
  candidates
    .iter()
    .map(|&c| c as f32)
    .filter(|c| (v - c).abs() < threshold)
    .min_by(|a, b| (v - a).abs().total_cmp(&(v - b).abs()))
    .unwrap_or(v)
}

/// Where a drag puts `me` (at `start` when it began) after the pointer moved
/// `dx`, `dy` map pixels: moved by `d / k`, then snapped to the edges and
/// alignments of `others` within 8 map pixels, rounded to whole pixels
pub(crate) fn drag(
  me: &Rect,
  start: (i32, i32),
  dx: f32,
  dy: f32,
  k: f32,
  others: &[Rect],
) -> (i32, i32) {
  let (x, y) = (start.0 as f32 + dx / k, start.1 as f32 + dy / k);
  let th = 8. / k;
  let (mut cx, mut cy) = (Vec::new(), Vec::new());
  for o in others {
    cx.extend([o.right(), o.x - me.w, o.x, o.right() - me.w]);
    cy.extend([o.bottom(), o.y - me.h, o.y, o.bottom() - me.h]);
  }
  (
    snap_axis(x, &cx, th).round() as i32,
    snap_axis(y, &cy, th).round() as i32,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  const fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
  }

  #[test]
  fn overlap_needs_shared_area() {
    assert!(r(0, 0, 100, 100).overlaps(&r(50, 50, 100, 100)));
    // touching edges and corners share no area
    assert!(!r(0, 0, 100, 100).overlaps(&r(100, 0, 100, 100)));
    assert!(!r(0, 0, 100, 100).overlaps(&r(100, 100, 100, 100)));
    assert_eq!(
      overlapping(&[r(0, 0, 100, 100), r(50, 0, 100, 100), r(500, 0, 10, 10)]),
      [0, 1]
    );
  }

  #[test]
  fn adjoin_needs_a_shared_edge_segment() {
    let a = r(0, 0, 1920, 1080);
    assert!(a.adjoins(&r(1920, 0, 1280, 1024)));
    assert!(a.adjoins(&r(1920, 1000, 800, 600)), "partial edge");
    assert!(a.adjoins(&r(-500, 1080, 800, 600)), "below");
    assert!(!a.adjoins(&r(1920, 1080, 800, 600)), "corner only");
    assert!(!a.adjoins(&r(1921, 0, 800, 600)), "one pixel gap");
    assert!(!a.adjoins(&r(1920, 1080 + 1, 800, 600)));
  }

  #[test]
  fn unreachable_follows_chains() {
    let chain = [r(0, 0, 100, 100), r(100, 0, 100, 100), r(200, 0, 100, 100)];
    assert!(unreachable(&chain).is_empty());
    let gap = [r(0, 0, 100, 100), r(100, 0, 100, 100), r(300, 0, 100, 100)];
    assert_eq!(unreachable(&gap), [2]);
    assert!(unreachable(&[r(0, 0, 1, 1)]).is_empty());
  }

  #[test]
  fn view_fits_pads_and_centres() {
    let rects = [r(0, 0, 1920, 1080), r(1920, 0, 1920, 1080)];
    let v = View::new(&rects, 494., 330.);
    // world: 3840 + 2*960 wide, 1080 + 2*400 high
    let (ww, wh) = (5760., 1880.);
    assert!((v.k - (494_f32 / ww).min(330_f32 / wh)).abs() < 1e-6);
    let (x, y, w, h) = v.to_map(&rects[0]);
    assert!((w - 1920. * v.k).abs() < 1e-3 && (h - 1080. * v.k).abs() < 1e-3);
    // both rects sit inside the map and the pair is centred
    let (x2, _, w2, _) = v.to_map(&rects[1]);
    assert!(x >= 0. && x2 + w2 <= 494. + 1e-3 && y >= 0.);
    assert!((x + (x2 + w2 - x) / 2. - 247.).abs() < 1e-2);
  }

  #[test]
  fn view_keeps_a_lone_monitor_from_filling_the_map() {
    let v = View::new(&[r(0, 0, 1000, 1000)], 494., 330.);
    let (_, _, w, h) = v.to_map(&r(0, 0, 1000, 1000));
    assert!(w < 494. && h < 330.);
  }

  #[test]
  fn drag_moves_by_pointer_over_scale() {
    let me = r(0, 0, 100, 100);
    assert_eq!(drag(&me, (0, 0), 50., -25., 0.5, &[]), (100, -50));
  }

  #[test]
  fn drag_snaps_to_edges_and_alignments() {
    let me = r(0, 0, 1000, 500);
    let other = r(2000, 0, 1000, 800);
    // k = 0.1: threshold 80 layout px. Left edge near the other's, snaps to 2000 - 1000 = 1000 (right to left)
    assert_eq!(drag(&me, (0, 0), 98., 7., 0.1, &[other]), (1000, 0));
    // far away: no snap, just rounding
    assert_eq!(drag(&me, (0, 0), 0., 300., 0.1, &[other]), (0, 3000));
    // bottom aligns with the other's bottom: y = 800 - 500
    assert_eq!(drag(&me, (0, 0), 0., 33., 0.1, &[other]).1, 300);
  }
}
