//! The Devices tab: drives grouped internal and removable, their partitions with a button each

use std::rc::Rc;

use corona_udisks::{Block, Drive, MountOptions, UdisksExt, UnmountOptions};
use gpui_kit::{
  AnyElement, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
  Styled, TestSupportExt, Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;

use crate::control_center::disks::{
  DisksPanel,
  dialogs::Dialog,
  model::{DriveView, Health, PartView, health, percent, system_block, usage},
  types::{fs_name, size},
  ui::{dot, icon, note, ok_color, partition_map, section, usage_bar, warn_color},
};

/// `NVMe`, `USB`, `ATA · 5400 rpm`
pub(super) fn bus(drive: &Drive) -> String {
  let mut bus = match (drive.nvme.is_some(), drive.connection_bus.as_str()) {
    (true, _) => "NVMe".to_string(),
    (_, "") if drive.ata.is_some() => "ATA".into(),
    (_, "") => String::new(),
    (_, other) => other.to_uppercase(),
  };
  if drive.rotation_rate > 0 {
    bus = format!("{bus} · {} rpm", drive.rotation_rate);
  }
  bus.trim_start_matches(" · ").to_string()
}

pub(super) fn drive_icon(drive: &Drive) -> IconName {
  match (drive.is_removable(), drive.nvme.is_some()) {
    (true, _) => IconName::Usb,
    (_, true) => IconName::Cpu,
    _ => IconName::HardDrive,
  }
}

/// `/dev/sda · 2 TB · ATA · 5400 rpm`
pub(super) fn drive_line(view: &DriveView) -> String {
  let mut parts = Vec::new();
  if let Some(disk) = &view.disk {
    parts.push(disk.device.clone());
  }
  parts.push(size(view.drive.size));
  let bus = bus(&view.drive);
  if !bus.is_empty() {
    parts.push(bus);
  }
  parts.join(" · ")
}

pub(super) fn segments(view: &DriveView) -> Vec<(f32, f32, bool)> {
  let total = view.drive.size.max(1) as f32;
  view
    .parts
    .iter()
    .map(|p| (p.block.size as f32 / total, p.hue, p.is_on()))
    .collect()
}

/// A partition's icon by what it is: locked or unlocked LUKS, swap, a filesystem
pub(super) fn part_icon(block: &Block, on: bool) -> IconName {
  match (&block.encrypted, &block.swapspace, &block.filesystem) {
    (Some(_), _, _) if on => IconName::LockOpen,
    (Some(_), _, _) => IconName::Lock,
    (_, Some(_), _) => IconName::Layers,
    (_, _, Some(_)) => IconName::Folder,
    _ => IconName::File,
  }
}

impl DisksPanel {
  pub(super) fn devices_tab(&self, cx: &Context<'_, Self>) -> Vec<AnyElement> {
    let views = self.drive_views(cx);
    if views.is_empty() {
      return vec![
        section("", "", None, [note(t!("app.disks.no_devices"), cx)], cx).into_any_element(),
      ];
    }
    let (removable, internal): (Vec<_>, Vec<_>) =
      views.into_iter().partition(|v| v.drive.is_removable());
    [
      (t!("app.disks.internal"), internal),
      (t!("app.disks.removable"), removable),
    ]
    .into_iter()
    .filter(|(_, views)| !views.is_empty())
    .map(|(title, views)| {
      let hint = t!("app.disks.drives", count = views.len());
      section(
        title,
        hint,
        None,
        views.iter().map(|v| self.drive_card(v, cx)),
        cx,
      )
      .into_any_element()
    })
    .collect()
  }

  fn drive_card(&self, view: &DriveView, cx: &Context<'_, Self>) -> AnyElement {
    let theme = cx.theme();
    let drive = &view.drive;
    let path = drive.path.clone();
    let right = match drive.smart().and_then(|s| s.temperature_celsius) {
      Some(c) => format!("{c:.0} °C"),
      None if drive.is_removable() => drive.connection_bus.to_uppercase(),
      None => String::new(),
    };
    let health_dot = match health(drive) {
      Health::Ok => Some(ok_color()),
      Health::Warn => Some(warn_color()),
      Health::Err => Some(theme.colors.danger),
      Health::Unknown => None,
    };
    let mut rows: Vec<AnyElement> = Vec::new();
    for part in &view.parts {
      rows.push(self.part_row(part, None, cx));
      if let Some(clear) = &part.cleartext {
        rows.push(self.part_row(part, Some(clear), cx));
      }
    }
    div()
      .flex()
      .flex_col()
      .gap_1()
      .p_1()
      .rounded_lg()
      .bg(theme.colors.background)
      .child(
        div()
          .id(gpui_kit::ElementId::Name(
            format!("disks-drive-{}", drive.id).into(),
          ))
          .test_support()
          .flex()
          .items_center()
          .gap_2()
          .p_1()
          .rounded_md()
          .cursor_pointer()
          .hover(|s| s.bg(theme.colors.muted))
          .on_click(cx.listener(move |this, _, _, cx| this.open_drive(path.clone(), cx)))
          .child(icon(drive_icon(drive)))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().truncate().child(drive.name()))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(drive_line(view)),
              ),
          )
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(right),
          )
          .children(health_dot.map(dot)),
      )
      .when(!view.parts.is_empty(), |d| {
        d.child(div().px_1().child(partition_map(&segments(view), cx)))
      })
      .children(rows)
      .into_any_element()
  }

  /// A partition's row, or with `child` its unlocked cleartext's, indented
  pub(super) fn part_row(
    &self,
    part: &PartView,
    child: Option<&Block>,
    cx: &Context<'_, Self>,
  ) -> AnyElement {
    let theme = cx.theme();
    let shown = child.unwrap_or(&part.block);
    let encrypted = child.is_none() && part.block.encrypted.is_some();
    let on = match child {
      Some(c) => c.is_mounted(),
      None => part.is_on(),
    };
    let mount = shown
      .filesystem
      .as_ref()
      .and_then(|f| f.mount_points.first().cloned());
    let mut sub = format!(
      "{} · {}",
      shown.device.trim_start_matches("/dev/"),
      fs_name(&shown.id_type)
    );
    if let (Some(mount), false) = (&mount, encrypted) {
      sub = format!("{sub} · {mount}");
    }
    let usage = mount.as_deref().filter(|_| !encrypted).and_then(usage);
    let open = shown.path.clone();
    let id = shown
      .path
      .as_str()
      .rsplit('/')
      .next()
      .unwrap_or_default()
      .to_string();
    div()
      .flex()
      .flex_col()
      .gap_1()
      .child(
        div()
          .id(gpui_kit::ElementId::Name(format!("disks-part-{id}").into()))
          .test_support()
          .flex()
          .items_center()
          .gap_2()
          .p_1()
          .when(child.is_some(), |d| d.pl_8())
          .rounded_md()
          .cursor_pointer()
          .hover(|s| s.bg(theme.colors.muted))
          .on_click(cx.listener(move |this, _, _, cx| this.open_block(open.clone(), cx)))
          .child(icon(part_icon(shown, on)))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(
                div()
                  .text_sm()
                  .truncate()
                  .child(shown.name().trim_start_matches("/dev/").to_string()),
              )
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(sub),
              ),
          )
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(size(shown.size)),
          )
          .children(self.primary_button(part, child, &id, on, false, cx)),
      )
      .when_some(usage, |d, (used, total)| {
        d.child(
          div()
            .px_1()
            .when(child.is_some(), |d| d.pl_8())
            .child(usage_bar(
              percent(used, total),
              t!(
                "app.disks.used",
                percent = format!("{:.0}", percent(used, total))
              ),
              size(total),
              cx,
            )),
        )
      })
      .into_any_element()
  }

  /// Mount / Unmount, Unlock / Lock, Start / Stop by what the block is; none for a bare block
  pub(super) fn primary_button(
    &self,
    part: &PartView,
    child: Option<&Block>,
    id: &str,
    on: bool,
    large: bool,
    cx: &Context<'_, Self>,
  ) -> Option<AnyElement> {
    let block = child.unwrap_or(&part.block).clone();
    let encrypted = child.is_none() && block.encrypted.is_some();
    let swap = block.swapspace.is_some();
    if !encrypted && !swap && block.filesystem.is_none() {
      return None;
    }
    let (label, icon) = match (encrypted, swap, on) {
      (true, _, true) => (t!("app.disks.lock"), IconName::Lock),
      (true, _, false) => (t!("app.disks.unlock"), IconName::LockOpen),
      (_, true, true) => (t!("app.disks.stop"), IconName::Square),
      (_, true, false) => (t!("app.disks.start"), IconName::Play),
      (_, _, true) => (t!("app.disks.unmount"), IconName::Eject),
      (_, _, false) => (t!("app.disks.mount"), IconName::Play),
    };
    let cleartext = part.cleartext.clone();
    let title = format!("{label} · {}", block.name());
    // the running system's own: nothing to unmount, lock or stop
    let system = match child {
      Some(c) => system_block(c),
      None => part.is_system(),
    };
    let button = Button::new(gpui_kit::ElementId::Name(
      format!("disks-primary-{id}").into(),
    ))
    .label(label)
    .disabled(system)
    .when(system, |b| b.tooltip(t!("app.disks.system_hint")))
    .when(large, |b| b.icon(icon))
    .when(!large, |b| b.xsmall())
    .when(large, |b| b.small())
    .when(!on, |b| b.primary())
    .cursor_pointer()
    .on_click(cx.listener(move |this, _, window: &mut Window, cx| {
      // the row under the button opens the partition page
      cx.stop_propagation();
      let path = block.path.clone();
      let objects = vec![path.clone()];
      match (encrypted, swap, on) {
        (true, _, false) => this.dialog = Some(Dialog::unlock(path, window, cx)),
        (true, _, true) => {
          let cleartext = cleartext.clone();
          this.act(
            title.clone(),
            objects,
            Rc::new(move |cx| {
              let udisks = cx.udisks().clone();
              let unmount = cleartext
                .as_ref()
                .filter(|c| c.is_mounted())
                .map(|c| udisks.unmount(&c.path, UnmountOptions::default()));
              let lock = udisks.lock(&path);
              Box::pin(async move {
                if let Some(unmount) = unmount {
                  unmount.await?;
                }
                lock.await
              })
            }),
            cx,
          );
        }
        (_, true, true) => this.act(
          title.clone(),
          objects,
          Rc::new(move |cx| Box::pin(cx.udisks().swap_stop(&path))),
          cx,
        ),
        (_, true, false) => this.act(
          title.clone(),
          objects,
          Rc::new(move |cx| Box::pin(cx.udisks().swap_start(&path))),
          cx,
        ),
        (_, _, true) => this.act(
          title.clone(),
          objects,
          Rc::new(move |cx| Box::pin(cx.udisks().unmount(&path, UnmountOptions::default()))),
          cx,
        ),
        (_, _, false) => this.act(
          title.clone(),
          objects,
          Rc::new(move |cx| {
            let mount = cx.udisks().mount(&path, MountOptions::default());
            Box::pin(async move { mount.await.map(drop) })
          }),
          cx,
        ),
      }
      cx.notify();
    }));
    // a disabled button lets the click through, the row under it must not get it either way
    Some(
      div()
        .id(gpui_kit::ElementId::Name(
          format!("disks-primary-wrap-{id}").into(),
        ))
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(button)
        .into_any_element(),
    )
  }
}
