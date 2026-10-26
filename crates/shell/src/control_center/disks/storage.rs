//! The Storage tab: loop devices and swap spaces

use std::rc::Rc;

use corona_launcher::Tone;
use corona_udisks::{Block, UdisksExt};
use gpui_kit::{
  AnyElement, Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    switch::Switch,
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;

use crate::control_center::disks::{
  DisksPanel,
  dialogs::{Dialog, DialogKind, LabelTarget},
  types::size,
  ui::{TileContent, note, section, tag, tile},
};

fn short(block: &Block) -> String {
  block.device.trim_start_matches("/dev/").to_string()
}

impl DisksPanel {
  pub(super) fn storage_tab(&self, cx: &Context<'_, Self>) -> Vec<AnyElement> {
    let blocks = cx.udisks().list_blocks(cx);
    let loops: Vec<&Block> = blocks
      .iter()
      // an unused loop device has no backing file
      .filter(|b| {
        b.loop_device
          .as_ref()
          .is_some_and(|l| !l.backing_file.is_empty())
      })
      .collect();
    let swaps: Vec<&Block> = blocks.iter().filter(|b| b.swapspace.is_some()).collect();

    let setup = Button::new("disks-loop-setup")
      .label(t!("app.disks.loop_setup_button"))
      .icon(IconName::Plus)
      .xsmall()
      .cursor_pointer()
      .on_click(cx.listener(|this, _, window, cx| {
        this.dialog = Some(Dialog::loop_setup(window, cx));
        cx.notify();
      }))
      .into_any_element();
    let mut loop_rows: Vec<AnyElement> = loops.iter().map(|b| self.loop_row(b, cx)).collect();
    if loop_rows.is_empty() {
      loop_rows.push(note(t!("app.disks.no_loops"), cx));
    }
    let mut swap_rows: Vec<AnyElement> = swaps.iter().map(|b| self.swap_row(b, cx)).collect();
    if swap_rows.is_empty() {
      swap_rows.push(note(t!("app.disks.no_swap"), cx));
    }
    vec![
      section(
        t!("app.disks.loops"),
        t!("app.disks.attached", count = loops.len()),
        Some(setup),
        loop_rows,
        cx,
      )
      .into_any_element(),
      section(
        t!("app.disks.swap"),
        t!("app.disks.spaces", count = swaps.len()),
        None,
        swap_rows,
        cx,
      )
      .into_any_element(),
    ]
  }

  fn loop_row(&self, block: &Block, cx: &Context<'_, Self>) -> AnyElement {
    let theme = cx.theme();
    let backing = block.loop_device.clone().unwrap_or_default();
    let file = std::path::Path::new(&backing.backing_file)
      .file_name()
      .map(|f| f.to_string_lossy().into_owned())
      .unwrap_or(backing.backing_file.clone());
    let mut sub = vec![file, size(block.size)];
    if block.read_only {
      sub.push(t!("app.disks.read_only").into());
    }
    let id = short(block);
    let (path, detach_path) = (block.path.clone(), block.path.clone());
    let name = short(block);
    div()
      .flex()
      .items_center()
      .gap_2()
      .p_1()
      .child(tile(85., TileContent::Icon(IconName::Layers), 28.))
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(div().text_sm().truncate().child(name.clone()))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(sub.join(" · ")),
          ),
      )
      .child(
        Switch::new(gpui_kit::ElementId::Name(
          format!("disks-autoclear-{id}").into(),
        ))
        .checked(backing.autoclear)
        .small()
        .tooltip(t!("app.disks.autoclear"))
        .on_click(cx.listener(move |this, on: &bool, _, cx| {
          let (path, on) = (path.clone(), *on);
          this.act(
            t!("app.disks.autoclear").into(),
            vec![path.clone()],
            Rc::new(move |cx| Box::pin(cx.udisks().loop_set_autoclear(&path, on))),
            cx,
          );
        })),
      )
      .child(
        Button::new(gpui_kit::ElementId::Name(
          format!("disks-detach-{id}").into(),
        ))
        .label(t!("app.disks.detach"))
        .xsmall()
        .danger()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
          let path = detach_path.clone();
          this.act(
            format!("{} · {name}", t!("app.disks.detach")),
            vec![path.clone()],
            Rc::new(move |cx| Box::pin(cx.udisks().loop_delete(&path))),
            cx,
          );
        })),
      )
      .into_any_element()
  }

  fn swap_row(&self, block: &Block, cx: &Context<'_, Self>) -> AnyElement {
    let theme = cx.theme();
    let active = block.swapspace.as_ref().is_some_and(|s| s.active);
    let id = short(block);
    let path = block.path.clone();
    let label_path = block.path.clone();
    let name = block.name().trim_start_matches("/dev/").to_string();
    let mut sub = vec![size(block.size)];
    if !block.id_label.is_empty() && block.id_label != name {
      sub.insert(0, short(block));
    }
    let (state, tone) = match active {
      true => (t!("app.disks.state.active"), Tone::Ok),
      false => (t!("app.disks.state.inactive"), Tone::Off),
    };
    div()
      .flex()
      .items_center()
      .gap_2()
      .p_1()
      .child(tile(320., TileContent::Icon(IconName::Layers), 28.))
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(div().text_sm().truncate().child(name.clone()))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(sub.join(" · ")),
          ),
      )
      .child(tag(state, tone, cx))
      .child(
        Button::new(gpui_kit::ElementId::Name(
          format!("disks-swap-label-{id}").into(),
        ))
        .icon(IconName::Pencil)
        .xsmall()
        .ghost()
        .cursor_pointer()
        .tooltip(t!("app.disks.rename"))
        .on_click(cx.listener(move |this, _, window, cx| {
          this.dialog_for(
            DialogKind::Label(LabelTarget::Swap),
            label_path.clone(),
            window,
            cx,
          )
        })),
      )
      .child(
        Button::new(gpui_kit::ElementId::Name(
          format!("disks-swap-toggle-{id}").into(),
        ))
        .label(match active {
          true => t!("app.disks.stop"),
          false => t!("app.disks.start"),
        })
        .xsmall()
        .when(!active, |b| b.primary())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
          let path = path.clone();
          let title = format!("{} · {name}", t!("app.disks.swap"));
          this.act(
            title,
            vec![path.clone()],
            Rc::new(move |cx| match active {
              true => Box::pin(cx.udisks().swap_stop(&path)),
              false => Box::pin(cx.udisks().swap_start(&path)),
            }),
            cx,
          );
        })),
      )
      .into_any_element()
  }
}
