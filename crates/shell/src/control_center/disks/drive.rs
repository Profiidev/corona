//! A drive's page: Overview with its facts and actions, Health with SMART

use std::rc::Rc;

use corona_launcher::Tone;
use corona_udisks::{AtaAttribute, Drive, SmartAttributes, UdisksExt};
use gpui_kit::{
  AnyElement, Context, IntoElement, ParentElement, Styled, Window,
  assets::IconName,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::control_center::disks::{
  DisksPanel, DriveTab,
  devices::{drive_icon, drive_line, segments},
  dialogs::{Dialog, DialogKind},
  jobs::DisksLog,
  model::{Health, free_spaces, health},
  types::{size, table_name},
  ui::{dot, icon, kv, ok_color, partition_map, section, tag, warn_color},
};

/// ATA attributes whose raw value above 0 means wear: reallocated, pending, uncorrectable
const WEAR: [u8; 4] = [5, 196, 197, 198];

pub(super) fn attribute_state(a: &AtaAttribute) -> Health {
  match () {
    _ if a.threshold > 0 && a.value >= 0 && a.value <= a.threshold => Health::Err,
    _ if WEAR.contains(&a.id) && a.pretty > 0 => Health::Warn,
    _ => Health::Ok,
  }
}

impl DisksPanel {
  pub(super) fn drive_page(
    &mut self,
    path: &OwnedObjectPath,
    cx: &mut Context<Self>,
  ) -> Vec<AnyElement> {
    let Some(view) = self
      .drive_views(cx)
      .into_iter()
      .find(|v| &v.drive.path == path)
    else {
      return Vec::new();
    };
    let theme = cx.theme();
    let drive = &view.drive;
    let (state, tone) = match health(drive) {
      Health::Ok => (t!("app.disks.healthy"), Tone::Ok),
      Health::Warn => (t!("app.disks.warning"), Tone::Warn),
      Health::Err => (t!("app.disks.failing"), Tone::Err),
      Health::Unknown => ("".into(), Tone::Off),
    };
    let summary = div()
      .flex()
      .flex_col()
      .flex_shrink_0()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .items_center()
          .gap_2()
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
                  .child(drive_line(&view)),
              ),
          )
          .when(!state.is_empty(), |d| d.child(tag(state, tone, cx))),
      )
      .when(!view.parts.is_empty(), |d| {
        d.child(partition_map(&segments(&view), cx))
      });
    let sub_tabs = div().flex().flex_shrink_0().gap_1().children(
      [
        (
          DriveTab::Overview,
          "disks-drive-tab-overview",
          t!("app.disks.overview"),
        ),
        (
          DriveTab::Health,
          "disks-drive-tab-health",
          t!("app.disks.health"),
        ),
      ]
      .map(|(tab, id, label)| {
        let path = path.clone();
        Button::new(id)
          .label(label)
          .small()
          .flex_1()
          .cursor_pointer()
          .when(tab == self.drive_tab, |b| b.primary())
          .on_click(cx.listener(move |this, _, _, cx| {
            this.drive_tab = tab;
            if tab == DriveTab::Health && this.smart.as_ref().is_none_or(|(p, _)| p != &path) {
              this.refresh_smart(path.clone(), cx);
            }
            cx.notify();
          }))
      }),
    );
    let mut page = vec![summary.into_any_element(), sub_tabs.into_any_element()];
    match self.drive_tab {
      DriveTab::Overview => page.extend(self.overview(&view.drive, view.disk.as_ref(), cx)),
      DriveTab::Health => page.extend(self.health(drive, cx)),
    }
    page
  }

  fn overview(
    &self,
    drive: &Drive,
    disk: Option<&corona_udisks::Block>,
    cx: &Context<'_, Self>,
  ) -> Vec<AnyElement> {
    let table = disk
      .and_then(|d| d.partition_table.as_ref())
      .map(|t| t.kind.clone())
      .unwrap_or_default();
    let yes_no = |b: bool| match b {
      true => t!("app.disks.yes"),
      false => t!("app.disks.no"),
    };
    let media = match (drive.media.is_empty(), drive.rotation_rate) {
      (false, _) => drive.media.clone(),
      (true, rpm) if rpm > 0 => t!("app.disks.media_rotating").to_string(),
      (true, 0) => t!("app.disks.media_solid").to_string(),
      _ => "—".into(),
    };
    let facts = [
      (
        t!("app.disks.device"),
        disk.map(|d| d.device.clone()).unwrap_or_default(),
      ),
      (t!("app.disks.model"), drive.name()),
      (t!("app.disks.serial"), drive.serial.clone()),
      (t!("app.disks.firmware"), drive.revision.clone()),
      (t!("app.disks.size"), size(drive.size)),
      (t!("app.disks.media"), media),
      (
        t!("app.disks.partition_table"),
        table_name(&table).to_string(),
      ),
      (
        t!("app.disks.removable_key"),
        yes_no(drive.is_removable()).to_string(),
      ),
      (t!("app.disks.identifier"), drive.id.clone()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(k, v)| kv(k, v, None, cx));

    let mut actions = Vec::new();
    if let Some(disk) = disk {
      let free = match &disk.partition_table {
        Some(_) => {
          let taken: Vec<(u64, u64)> = cx
            .udisks()
            .list_blocks(cx)
            .iter()
            .filter_map(|b| b.partition.as_ref())
            .filter(|p| p.table.as_ref() == Some(&disk.path))
            .map(|p| (p.offset, p.size))
            .collect();
          !free_spaces(disk.size, &taken).is_empty()
        }
        None => false,
      };
      let path = disk.path.clone();
      actions.push(self.action_row(
        "disks-create-part",
        IconName::Plus,
        t!("app.disks.create_partition"),
        free,
        false,
        {
          let path = path.clone();
          move |this, window, cx| {
            this.dialog = Some(Dialog::new(
              DialogKind::CreatePartition,
              path.clone(),
              window,
              cx,
            ))
          }
        },
        cx,
      ));
      actions.push(self.action_row(
        "disks-drive-backup",
        IconName::Download,
        t!("app.disks.backup_drive"),
        true,
        false,
        {
          let path = path.clone();
          move |this, window, cx| {
            this.dialog = Some(Dialog::new(DialogKind::Backup, path.clone(), window, cx))
          }
        },
        cx,
      ));
      actions.push(self.action_row(
        "disks-drive-benchmark",
        IconName::Gauge,
        t!("app.disks.benchmark"),
        true,
        false,
        {
          let path = path.clone();
          move |this, window, cx| {
            this.dialog = Some(Dialog::new(DialogKind::Benchmark, path.clone(), window, cx))
          }
        },
        cx,
      ));
      actions.push(self.action_row(
        "disks-drive-rescan",
        IconName::ScanLine,
        t!("app.disks.rescan"),
        true,
        false,
        move |this, _, cx| {
          let path = path.clone();
          this.act(
            t!("app.disks.rescan").into(),
            vec![path.clone()],
            Rc::new(move |cx| Box::pin(cx.udisks().rescan(&path))),
            cx,
          )
        },
        cx,
      ));
    }
    let system = self
      .drive_views(cx)
      .iter()
      .any(|v| v.drive.path == drive.path && v.is_system());
    let drive_path = drive.path.clone();
    let drive_path_ok = drive.path.clone();
    let name = drive.name();
    actions.push(match drive.is_removable() {
      true => self.action_row(
        "disks-eject",
        IconName::Eject,
        t!("app.disks.eject"),
        !system,
        false,
        move |_, _, cx| {
          let this = cx.entity().downgrade();
          let path = drive_path.clone();
          let path_ok = drive_path_ok.clone();
          let name = name.clone();
          DisksLog::run_then(
            cx,
            format!("{} · {name}", t!("app.disks.eject")),
            vec![path.clone()],
            Rc::new(move |cx| {
              let eject = cx.udisks().eject_drive(&path, cx);
              Box::pin(async move { eject.await.map(|()| String::new()) })
            }),
            {
              let name = name.clone();
              move |_, cx| {
                let (name, path) = (name.clone(), path_ok.clone());
                this
                  .update(cx, |this, cx| {
                    this.dialog = Some(Dialog::ejected(name, path));
                    cx.notify();
                  })
                  .ok();
              }
            },
          );
        },
        cx,
      ),
      false => self.action_row(
        "disks-power-off",
        IconName::Power,
        t!("app.disks.power_off"),
        false,
        false,
        |_, _, _| {},
        cx,
      ),
    });
    vec![
      section(t!("app.disks.drive"), "", None, facts, cx).into_any_element(),
      section(t!("app.disks.actions"), "", None, actions, cx).into_any_element(),
    ]
  }

  pub(super) fn refresh_smart(&mut self, path: OwnedObjectPath, cx: &mut Context<Self>) {
    self.refreshing = true;
    let udisks = cx.udisks().clone();
    let refresh = udisks.refresh_smart(&path, cx);
    cx.spawn(async move |this, cx| {
      let _ = refresh.await;
      let attributes = this
        .update(cx, |_, cx| cx.udisks().smart_attributes(&path, cx))
        .ok();
      let attributes = match attributes {
        Some(a) => a.await.ok(),
        None => None,
      };
      this
        .update(cx, |this, cx| {
          this.refreshing = false;
          this.smart = Some((path, attributes));
          cx.notify();
        })
        .ok();
    })
    .detach();
  }

  fn health(&self, drive: &Drive, cx: &Context<'_, Self>) -> Vec<AnyElement> {
    let theme = cx.theme();
    let Some(smart) = drive.smart() else {
      return vec![
        section(
          t!("app.disks.smart"),
          "",
          None,
          [kv(
            t!("app.disks.status"),
            t!("app.disks.no_smart"),
            None,
            cx,
          )],
          cx,
        )
        .into_any_element(),
      ];
    };
    let attributes = self
      .smart
      .as_ref()
      .filter(|(p, _)| p == &drive.path)
      .and_then(|(_, a)| a.clone());
    let state = health(drive);
    let (status, color) = match state {
      Health::Ok => (t!("app.disks.passed"), ok_color()),
      Health::Warn => (t!("app.disks.warning"), warn_color()),
      _ => (t!("app.disks.failing"), theme.colors.danger),
    };
    let third = match &attributes {
      Some(SmartAttributes::Nvme(log)) => (t!("app.disks.wear"), format!("{} %", log.percent_used)),
      _ => (
        t!("app.disks.power_on"),
        smart
          .power_on_hours
          .map(|h| t!("app.disks.hours", count = h).to_string())
          .unwrap_or_else(|| "—".into()),
      ),
    };
    let stats = [
      (t!("app.disks.status"), status.to_string(), Some(color)),
      (
        t!("app.disks.temperature"),
        smart
          .temperature_celsius
          .map(|c| format!("{c:.0} °C"))
          .unwrap_or_else(|| "—".into()),
        None,
      ),
      (third.0, third.1, None),
    ];
    let stats = div().flex().gap_2().children(stats.map(|(k, v, c)| {
      div()
        .flex()
        .flex_col()
        .flex_1()
        .p_2()
        .rounded_lg()
        .bg(theme.colors.background)
        .child(
          div()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(k),
        )
        .child(
          div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
            .text_color(c.unwrap_or(theme.colors.foreground))
            .child(v),
        )
    }));
    let path = drive.path.clone();
    let refresh = Button::new("disks-smart-refresh")
      .label(t!("app.disks.refresh"))
      .icon(IconName::RefreshCw)
      .small()
      .loading(self.refreshing)
      .cursor_pointer()
      .on_click(cx.listener(move |this, _, _, cx| this.refresh_smart(path.clone(), cx)));
    let mut sections = vec![
      section(
        t!("app.disks.smart"),
        status,
        None,
        [
          stats.into_any_element(),
          div().flex().child(refresh).into_any_element(),
        ],
        cx,
      )
      .into_any_element(),
    ];
    match attributes {
      Some(SmartAttributes::Nvme(log)) => {
        let warning = drive
          .nvme
          .as_ref()
          .map(|n| n.smart_critical_warning.join(", "))
          .filter(|w| !w.is_empty());
        let rows = [
          (
            t!("app.disks.critical_warning"),
            warning
              .clone()
              .unwrap_or_else(|| t!("app.disks.none").into()),
            Some(if warning.is_some() {
              theme.colors.danger
            } else {
              ok_color()
            }),
          ),
          (
            t!("app.disks.spare"),
            t!(
              "app.disks.spare_value",
              spare = log.avail_spare,
              threshold = log.spare_thresh
            )
            .to_string(),
            None,
          ),
          (
            t!("app.disks.wear"),
            format!("{} %", log.percent_used),
            None,
          ),
          (
            t!("app.disks.data_written"),
            size(log.total_data_written),
            None,
          ),
          (t!("app.disks.data_read"), size(log.total_data_read), None),
          (
            t!("app.disks.unsafe_shutdowns"),
            log.unsafe_shutdowns.to_string(),
            None,
          ),
          (
            t!("app.disks.media_errors"),
            log.media_errors.to_string(),
            Some(if log.media_errors > 0 {
              warn_color()
            } else {
              ok_color()
            }),
          ),
          (
            t!("app.disks.busy_time"),
            t!("app.disks.minutes", count = log.ctrl_busy_time).to_string(),
            None,
          ),
        ]
        .into_iter()
        .map(|(k, v, c)| kv(k, v, c, cx));
        sections.push(section(t!("app.disks.nvme_log"), "", None, rows, cx).into_any_element());
      }
      Some(SmartAttributes::Ata(attributes)) => {
        let head = attribute_line(
          [
            "ID",
            &t!("app.disks.attribute"),
            &t!("app.disks.value"),
            &t!("app.disks.worst"),
            &t!("app.disks.threshold"),
            &t!("app.disks.raw"),
          ],
          None,
          true,
          cx,
        );
        let rows = attributes.iter().map(|a| {
          let color = match attribute_state(a) {
            Health::Err => theme.colors.danger,
            Health::Warn => warn_color(),
            _ => ok_color(),
          };
          attribute_line(
            [
              &a.id.to_string(),
              &a.name,
              &a.value.to_string(),
              &a.worst.to_string(),
              &a.threshold.to_string(),
              &a.pretty.to_string(),
            ],
            Some(color),
            false,
            cx,
          )
        });
        sections.push(
          section(
            t!("app.disks.ata_attributes"),
            "",
            None,
            std::iter::once(head).chain(rows),
            cx,
          )
          .into_any_element(),
        );
      }
      None => {}
    }
    // udisks keeps no history of earlier tests, only the last result
    if !smart.selftest_status.is_empty() {
      let ok = matches!(smart.selftest_status.as_str(), "success" | "inprogress");
      sections.push(
        section(
          t!("app.disks.selftest"),
          "",
          None,
          [kv(
            t!("app.disks.last_result"),
            selftest_name(&smart.selftest_status),
            Some(if ok { ok_color() } else { warn_color() }),
            cx,
          )],
          cx,
        )
        .into_any_element(),
      );
    }
    sections
  }

  pub(super) fn dialog_for(
    &mut self,
    kind: DialogKind,
    block: OwnedObjectPath,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if kind == DialogKind::Format {
      for f in crate::control_center::disks::types::FORMATS {
        self.load_caps(f, cx);
      }
    }
    self.dialog = Some(Dialog::new(kind, block, window, cx));
    cx.notify();
  }
}

/// The self-test results udisks reports, for ATA and NVMe
pub const SELFTEST: [&str; 11] = [
  "success",
  "inprogress",
  "aborted",
  "interrupted",
  "fatal",
  "fatal_error",
  "error_unknown",
  "error_electrical",
  "error_servo",
  "error_read",
  "error_handling",
];

fn selftest_name(status: &str) -> String {
  let key = format!("app.disks.selftest_status.{status}");
  match crate::_rust_i18n_try_translate(&rust_i18n::locale(), &key) {
    Some(name) if SELFTEST.contains(&status) => name.to_string(),
    _ => status.to_string(),
  }
}

fn attribute_line(
  cells: [&str; 6],
  color: Option<gpui_kit::Hsla>,
  head: bool,
  cx: &Context<'_, DisksPanel>,
) -> AnyElement {
  let theme = cx.theme();
  let widths = [28., 0., 40., 40., 40., 56.];
  div()
    .flex()
    .items_center()
    .gap_1()
    .px_1()
    .text_xs()
    .when(head, |d| d.text_color(theme.colors.muted_foreground))
    .child(div().w(px(10.)).children(color.map(dot)))
    .children(cells.into_iter().zip(widths).map(|(cell, w)| {
      let d = div().truncate().child(cell.to_string());
      match w {
        0. => d.flex_1().min_w_0(),
        w => d.w(px(w)).flex_shrink_0().text_right(),
      }
    }))
    .into_any_element()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn attr(id: u8, value: i32, threshold: i32, pretty: i64) -> AtaAttribute {
    AtaAttribute {
      id,
      name: String::new(),
      flags: 0,
      value,
      worst: value,
      threshold,
      pretty,
      pretty_unit: 1,
    }
  }

  #[test]
  fn attribute_states() {
    assert_eq!(attribute_state(&attr(9, 80, 0, 18204)), Health::Ok);
    assert_eq!(attribute_state(&attr(5, 192, 140, 8)), Health::Warn);
    assert_eq!(attribute_state(&attr(5, 100, 140, 900)), Health::Err);
    assert_eq!(attribute_state(&attr(197, 200, 0, 0)), Health::Ok);
    // unknown normalized value is no failure
    assert_eq!(attribute_state(&attr(1, -1, 50, 0)), Health::Ok);
  }
}
