use gpui_kit::{Bounds, Pixels, Point, Size, layer_shell::Anchor, point, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Placement {
  Top,
  Bottom,
  Left,
  Right,
}

impl Placement {
  pub fn anchor(&self) -> Anchor {
    Anchor::all()
      - match self {
        Placement::Top => Anchor::BOTTOM,
        Placement::Bottom => Anchor::TOP,
        Placement::Left => Anchor::RIGHT,
        Placement::Right => Anchor::LEFT,
      }
  }

  pub fn size(&self, height: f32, width: f32) -> Size<Pixels> {
    match self {
      Placement::Top | Placement::Bottom => Size::new(px(width), px(height)),
      Placement::Left | Placement::Right => Size::new(px(height), px(width)),
    }
  }

  pub fn is_horizontal(&self) -> bool {
    matches!(self, Placement::Top | Placement::Bottom)
  }

  pub fn is_vertical(&self) -> bool {
    matches!(self, Placement::Left | Placement::Right)
  }

  /// Whether mapping bar-local space into window coordinates reflects the
  /// plane, which reverses the direction an arc sweeps.
  pub fn mirrored(&self) -> bool {
    matches!(self, Placement::Bottom | Placement::Left)
  }
}

pub trait PlacmentBounds {
  /// A point `along` the bar from its start, and `across` from the screen edge
  /// the bar is anchored to, in window coordinates.
  fn point_p(&self, p: Placement, along: Pixels, across: Pixels) -> Point<Pixels>;

  /// Length along the bar, and depth from the anchored edge.
  fn extent_p(&self, p: Placement) -> (Pixels, Pixels);
}

impl PlacmentBounds for Bounds<Pixels> {
  fn point_p(&self, p: Placement, along: Pixels, across: Pixels) -> Point<Pixels> {
    match p {
      Placement::Top => point(self.left() + along, self.top() + across),
      Placement::Bottom => point(self.left() + along, self.bottom() - across),
      Placement::Left => point(self.left() + across, self.top() + along),
      Placement::Right => point(self.right() - across, self.top() + along),
    }
  }

  fn extent_p(&self, p: Placement) -> (Pixels, Pixels) {
    if p.is_horizontal() {
      (self.size.width, self.size.height)
    } else {
      (self.size.height, self.size.width)
    }
  }
}
