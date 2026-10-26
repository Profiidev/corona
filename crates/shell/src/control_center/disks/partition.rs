//! A block's page: its primary action, details and the actions of what it is

use std::rc::Rc;

use corona_launcher::Tone;
use corona_udisks::UdisksExt;
use gpui_kit::{
  AnyElement, Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{ActiveTheme, Sizable, button::Button},
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::control_center::disks::{
  DisksPanel,
  devices::part_icon,
  dialogs::{DialogKind, LabelTarget, ResizeTarget},
  jobs::DisksLog,
  model::{percent, usage},
  state_tag,
  types::{fs_name, partition_type_name, size, table_name},
  ui::{icon, kv, section, tag, usage_bar},
};

/// The actions left on a partition the running system uses
const SYSTEM_SAFE: [&str; 3] = ["change-passphrase", "backup", "benchmark"];

impl DisksPanel {
  pub(super) fn block_page(
    &mut self,
    path: &OwnedObjectPath,
    cx: &mut Context<Self>,
  ) -> Vec<AnyElement> {
    let Some((drive, part)) = self.find_part(path, cx) else {
      return Vec::new();
    };
    let theme = cx.theme();
    let block = part.block.clone();
    let table_kind = drive
      .as_ref()
      .and_then(|d| d.disk.as_ref())
      .and_then(|d| d.partition_table.as_ref())
      .map(|t| t.kind.clone())
      .unwrap_or_default();
    let on = part.is_on();
    let encrypted = block.encrypted.is_some();
    let swap = block.swapspace.is_some();
    let mount = block
      .filesystem
      .as_ref()
      .and_then(|f| f.mount_points.first().cloned());
    let (state, tone) = state_tag(&part);
    let system_tag = part.is_system();

    let usage = mount.as_deref().and_then(usage);
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
          .child(icon(part_icon(&block, on)))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().truncate().child(block.name().to_string()))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(format!(
                    "{} · {} · {}",
                    block.device.trim_start_matches("/dev/"),
                    fs_name(&block.id_type),
                    size(block.size)
                  )),
              ),
          )
          .when(system_tag, |d| {
            d.child(tag(t!("app.disks.system"), Tone::Warn, cx))
          })
          .child(tag(state, tone, cx)),
      )
      .when_some(usage.zip(mount.clone()), |d, ((used, total), mount)| {
        d.child(usage_bar(
          percent(used, total),
          t!(
            "app.disks.used",
            percent = format!("{:.0}", percent(used, total))
          ),
          mount,
          cx,
        ))
      });

    let id = block
      .path
      .as_str()
      .rsplit('/')
      .next()
      .unwrap_or_default()
      .to_string();
    let mut buttons: Vec<AnyElement> = self
      .primary_button(&part, None, &format!("page-{id}"), on, true, cx)
      .into_iter()
      .collect();
    if let Some(mount) = mount.clone() {
      buttons.push(
        Button::new("disks-open-folder")
          .label(t!("app.disks.open_folder"))
          .icon(IconName::FolderOpen)
          .small()
          .cursor_pointer()
          // the default file manager, a file:// url goes to the browser
          .on_click(move |_, _, cx| cx.open_with_system(std::path::Path::new(&mount)))
          .into_any_element(),
      );
    }

    let partition = block.partition.clone();
    let mount_text = match (&mount, on) {
      (Some(m), true) => m.clone(),
      _ => t!("app.disks.not_mounted").to_string(),
    };
    let mut details = vec![
      kv(t!("app.disks.device"), block.device.clone(), None, cx),
      kv(
        "UUID",
        if block.id_uuid.is_empty() {
          "—".into()
        } else {
          block.id_uuid.clone()
        },
        None,
        cx,
      ),
      kv(
        t!("app.disks.label"),
        if block.id_label.is_empty() {
          "—".into()
        } else {
          block.id_label.clone()
        },
        None,
        cx,
      ),
      kv(
        t!("app.disks.filesystem"),
        fs_name(&block.id_type).to_string(),
        None,
        cx,
      ),
    ];
    if block.filesystem.is_some() {
      details.push(kv(t!("app.disks.mount_point"), mount_text, None, cx));
    }
    details.push(kv(t!("app.disks.size"), size(block.size), None, cx));
    if let Some(p) = &partition {
      details.push(kv(
        t!("app.disks.partition_type"),
        partition_type_name(&table_kind, &p.kind),
        None,
        cx,
      ));
      if !p.name.is_empty() {
        details.push(kv(t!("app.disks.partition_name"), p.name.clone(), None, cx));
      }
      details.push(kv(
        t!("app.disks.flags_key"),
        format!("0x{:x}", p.flags),
        None,
        cx,
      ));
    }
    if let Some(e) = &block.encrypted
      && !e.hint_encryption_type.is_empty()
    {
      details.push(kv(
        t!("app.disks.encryption"),
        e.hint_encryption_type.to_uppercase(),
        None,
        cx,
      ));
    }

    let mut page = vec![
      summary.into_any_element(),
      div()
        .flex()
        .flex_shrink_0()
        .gap_2()
        .children(buttons)
        .into_any_element(),
      section(t!("app.disks.details"), "", None, details, cx).into_any_element(),
    ];
    let caps = self.caps(&block.id_type);
    // what the running system uses only reads, backs up and changes its passphrase
    let system = part.is_system();
    let row = |this: &Self,
               key: &'static str,
               icon,
               label,
               enabled,
               danger,
               kind: DialogKind,
               cx: &Context<Self>| {
      let path = block.path.clone();
      let enabled = enabled && (!system || SYSTEM_SAFE.contains(&key));
      this.action_row(
        format!("disks-{key}"),
        icon,
        label,
        enabled,
        danger,
        move |this, window, cx| this.dialog_for(kind.clone(), path.clone(), window, cx),
        cx,
      )
    };

    if block.filesystem.is_some() {
      let path = block.path.clone();
      let check = self.action_row(
        "disks-check",
        IconName::Check,
        t!("app.disks.check"),
        caps.check && !on,
        false,
        move |this, _, cx| this.check(path.clone(), false, cx),
        cx,
      );
      let path = block.path.clone();
      let repair = self.action_row(
        "disks-repair",
        IconName::Wrench,
        t!("app.disks.repair"),
        caps.repair && !on,
        false,
        move |this, _, cx| this.check(path.clone(), true, cx),
        cx,
      );
      page.push(
        section(
          t!("app.disks.filesystem"),
          fs_name(&block.id_type).to_string(),
          None,
          [
            row(
              self,
              "rename",
              IconName::Pencil,
              t!("app.disks.rename"),
              true,
              false,
              DialogKind::Label(LabelTarget::Filesystem),
              cx,
            ),
            row(
              self,
              "resize-fs",
              IconName::Scaling,
              t!("app.disks.resize_fs"),
              caps.resize,
              false,
              DialogKind::Resize(ResizeTarget::Filesystem),
              cx,
            ),
            check,
            repair,
          ],
          cx,
        )
        .into_any_element(),
      );
    }
    if encrypted {
      page.push(
        section(
          t!("app.disks.encryption"),
          "LUKS",
          None,
          [
            row(
              self,
              "change-passphrase",
              IconName::KeyRound,
              t!("app.disks.change_passphrase"),
              true,
              false,
              DialogKind::ChangePassphrase,
              cx,
            ),
            row(
              self,
              "resize-luks",
              IconName::Scaling,
              t!("app.disks.resize_luks"),
              on,
              false,
              DialogKind::Resize(ResizeTarget::Encrypted),
              cx,
            ),
          ],
          cx,
        )
        .into_any_element(),
      );
    }
    if swap {
      page.push(
        section(
          t!("app.disks.swap"),
          "",
          None,
          [row(
            self,
            "swap-label",
            IconName::Pencil,
            t!("app.disks.rename"),
            true,
            false,
            DialogKind::Label(LabelTarget::Swap),
            cx,
          )],
          cx,
        )
        .into_any_element(),
      );
    }
    if partition.is_some() {
      page.push(
        section(
          t!("app.disks.partition"),
          table_name(&table_kind).to_string(),
          None,
          [
            row(
              self,
              "part-type",
              IconName::Tag,
              t!("app.disks.change_type"),
              true,
              false,
              DialogKind::PartitionType,
              cx,
            ),
            row(
              self,
              "part-name",
              IconName::Pencil,
              t!("app.disks.change_name"),
              table_kind == "gpt",
              false,
              DialogKind::Label(LabelTarget::PartitionName),
              cx,
            ),
            row(
              self,
              "part-flags",
              IconName::Tag,
              t!("app.disks.edit_flags"),
              true,
              false,
              DialogKind::Flags,
              cx,
            ),
            row(
              self,
              "resize-part",
              IconName::Scaling,
              t!("app.disks.resize_part"),
              !on,
              false,
              DialogKind::Resize(ResizeTarget::Partition),
              cx,
            ),
            row(
              self,
              "delete-part",
              IconName::Trash,
              t!("app.disks.delete"),
              !on,
              true,
              DialogKind::DeletePartition,
              cx,
            ),
          ],
          cx,
        )
        .into_any_element(),
      );
    }
    let path = block.path.clone();
    let rescan = self.action_row(
      "disks-block-rescan",
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
    );
    page.push(
      section(
        t!("app.disks.device_section"),
        "",
        None,
        [
          row(
            self,
            "format",
            IconName::HardDrive,
            t!("app.disks.format"),
            !on && !block.read_only,
            true,
            DialogKind::Format,
            cx,
          ),
          row(
            self,
            "backup",
            IconName::Download,
            t!("app.disks.backup"),
            true,
            false,
            DialogKind::Backup,
            cx,
          ),
          row(
            self,
            "restore",
            IconName::Upload,
            t!("app.disks.restore"),
            !on && !block.read_only,
            true,
            DialogKind::Restore,
            cx,
          ),
          row(
            self,
            "benchmark",
            IconName::Gauge,
            t!("app.disks.benchmark"),
            true,
            false,
            DialogKind::Benchmark,
            cx,
          ),
          rescan,
        ],
        cx,
      )
      .into_any_element(),
    );
    page
  }

  /// Checks or repairs, the result in a dialog
  fn check(&mut self, path: OwnedObjectPath, repair: bool, cx: &mut Context<Self>) {
    let udisks = cx.udisks().clone();
    let name = udisks
      .block(&path, cx)
      .map(|b| b.name().to_string())
      .unwrap_or_default();
    let title = match repair {
      true => format!("{} · {name}", t!("app.disks.repair")),
      false => format!("{} · {name}", t!("app.disks.check")),
    };
    let started = std::time::Instant::now();
    let this = cx.entity().downgrade();
    let target = path.clone();
    DisksLog::run_then(
      cx,
      title,
      vec![path.clone()],
      Rc::new(move |cx| {
        let udisks = cx.udisks();
        match repair {
          true => {
            let repair = udisks.repair(&target);
            Box::pin(async move { repair.await.map(|ok| ok.to_string()) })
          }
          false => {
            let check = udisks.check(&target);
            Box::pin(async move { check.await.map(|ok| ok.to_string()) })
          }
        }
      }),
      move |ok, cx| {
        let clean = ok == "true";
        this
          .update(cx, |this, cx| {
            this.dialog = Some(super::dialogs::Dialog::check_result(
              path.clone(),
              repair,
              clean,
              started.elapsed(),
            ));
            cx.notify();
          })
          .ok();
      },
    );
  }
}
