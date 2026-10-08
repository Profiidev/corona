use corona_components::async_listener::AsyncListenerExt;
use corona_power::PowerExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  base::StyledExt,
  component::{
    Sizable, Theme,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};

use crate::{
  control_center::power::{PowerPanel, card},
  icons::power_profile,
};
use rust_i18n::t;

impl PowerPanel {
  pub fn profiles(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let profiles = cx.power().profiles(cx)?;

    Some(
      card(cx)
        .child(
          div()
            .text_sm()
            .font_bold()
            .child(t!("app.power.profile.title")),
        )
        .child(
          div()
            .flex()
            .gap_2()
            .children(profiles.available.iter().map(|name| {
              let (icon, label) = power_profile(name);
              Button::new(format!("profile-{name}"))
                .icon(icon)
                .label(label)
                .small()
                .flex_1()
                .cursor_pointer()
                .when(*name == profiles.active, |b| b.primary())
                .on_click(cx.async_listener(
                  {
                    let name = name.clone();
                    move |_, _, _, cx| cx.power().set_profile(name.clone())
                  },
                  Self::show_error,
                ))
            })),
        )
        .when_some(profiles.degraded.as_ref(), |d, reason| {
          d.child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(t!(
                "app.power.profile.degraded",
                reason = reason.replace('-', " ")
              )),
          )
        }),
    )
  }
}
