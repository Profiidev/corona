use gpui_kit::{
  App, Entity, IntoElement, ParentElement, RenderOnce, Styled, Window,
  assets::IconName,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div, px,
};

#[derive(IntoElement)]
pub struct ControlPanelNav {
  state: Entity<ControlPanelNavState>,
}

pub struct ControlPanelNavState {
  selected: ControlPanelNavItem,
}

impl ControlPanelNav {
  pub fn new(state: &Entity<ControlPanelNavState>) -> Self {
    ControlPanelNav {
      state: state.clone(),
    }
  }
}

impl ControlPanelNavState {
  pub fn new() -> Self {
    ControlPanelNavState {
      selected: ControlPanelNavItem::Dashboard,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPanelNavItem {
  Dashboard,
  Audio,
  Network,
}

impl ControlPanelNavItem {
  fn as_str(&self) -> &'static str {
    match self {
      ControlPanelNavItem::Dashboard => "dashboard",
      ControlPanelNavItem::Audio => "audio",
      ControlPanelNavItem::Network => "network",
    }
  }

  fn icon(&self) -> IconName {
    match self {
      ControlPanelNavItem::Dashboard => IconName::LayoutDashboard,
      ControlPanelNavItem::Audio => IconName::Volume2,
      ControlPanelNavItem::Network => IconName::Wifi,
    }
  }

  fn tooltip(&self) -> &'static str {
    match self {
      ControlPanelNavItem::Dashboard => "Dashboard",
      ControlPanelNavItem::Audio => "Audio",
      ControlPanelNavItem::Network => "Network",
    }
  }

  fn iter() -> impl Iterator<Item = ControlPanelNavItem> {
    vec![
      ControlPanelNavItem::Dashboard,
      ControlPanelNavItem::Audio,
      ControlPanelNavItem::Network,
    ]
    .into_iter()
  }
}

impl RenderOnce for ControlPanelNav {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let selected = self.state.read(cx).selected;

    div()
      .w(px(48.))
      .h_full()
      .p_2()
      .flex()
      .flex_col()
      .gap_1()
      .rounded_2xl()
      .bg(theme.tokens.sidebar)
      .children(ControlPanelNavItem::iter().map(|v| {
        Button::new(v.as_str())
          .with_variant(if selected == v {
            ButtonVariant::Primary
          } else {
            ButtonVariant::Ghost
          })
          .tooltip(v.tooltip())
          .cursor_pointer()
          .icon(v.icon())
          .on_click({
            let state = self.state.clone();
            move |_, _, cx| {
              state.update(cx, |this, _| this.selected = v);
            }
          })
      }))
  }
}
