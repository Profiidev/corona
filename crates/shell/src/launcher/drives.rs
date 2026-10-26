//! Drives through UDisks2: mount, unmount, unlock, lock, eject and their SMART health

use std::rc::Rc;

use corona_launcher::{
  Accessory, Action, Command, DetailView, Field, FieldKind, FormView, Icon, ListView, Meta,
  Outcome, Tone, View,
  kit::{ListSource, Notify, opener, watch},
  registry, simple,
};
use corona_udisks::{Drive, MountOptions, SmartAttributes, UdisksExt, UnmountOptions, Volume};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

fn size(bytes: u64) -> String {
  let gb = bytes as f64 / 1e9;
  match gb >= 1. {
    true => format!("{gb:.1} GB"),
    false => format!("{:.0} MB", bytes as f64 / 1e6),
  }
}

fn observe(notify: Notify, cx: &mut App) -> Vec<Subscription> {
  let udisks = cx.udisks().clone();
  vec![
    watch(&udisks.volumes, &notify, cx),
    watch(&udisks.drives, &notify, cx),
  ]
}

fn volume_meta(v: &Volume, icon: IconName) -> Meta {
  let subtitle = [v.device.clone(), v.model.clone(), size(v.size)]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
  Meta {
    title: v.label.clone().into(),
    subtitle: Some(subtitle.into()),
    icon: Icon::Named(icon),
    keywords: vec![v.device.clone().into(), v.label.clone().into()],
    accessories: v
      .mount_points
      .first()
      .map(|m| Accessory::Tag(m.clone().into(), Tone::Ok))
      .into_iter()
      .collect(),
    ..Default::default()
  }
}

/// A row for the volume, `action` run on it
fn volume_row(
  volume: &Volume,
  icon: IconName,
  action: impl Fn(&Volume) -> Action + 'static,
) -> Rc<dyn Command> {
  let (v, volume) = (volume.clone(), volume.clone());
  simple(
    format!("drive:{}", v.device),
    move |_| volume_meta(&v, icon),
    move |_| vec![action(&volume)],
  )
}

fn mount(volume: &Volume) -> Action {
  let block = volume.block.clone();
  Action::spawn(
    "mount",
    t!("app.launcher.drives.mount_one"),
    move |_, cx| {
      let task = cx.udisks().mount(&block, MountOptions::default());
      async move {
        let at = task.await?;
        Ok(Outcome::hud(t!("app.launcher.drives.mounted", path = at)))
      }
    },
  )
}

fn unmount(volume: &Volume) -> Action {
  let (block, name) = (volume.block.clone(), volume.label.clone());
  Action::spawn(
    "unmount",
    t!("app.launcher.drives.unmount_one"),
    move |_, cx| {
      let task = cx.udisks().unmount(&block, UnmountOptions::default());
      let name = name.clone();
      async move {
        task.await?;
        Ok(Outcome::hud(t!(
          "app.launcher.drives.unmounted",
          name = name
        )))
      }
    },
  )
}

fn unlock(volume: &Volume) -> Action {
  let volume = volume.clone();
  Action::sync(
    "unlock",
    t!("app.launcher.drives.unlock_one"),
    move |_, _, _| Ok(Outcome::Push(passphrase_form(&volume))),
  )
}

/// Asks for the passphrase, then unlocks and mounts
fn passphrase_form(volume: &Volume) -> View {
  let block = volume.block.clone();
  View::Form(FormView {
    title: t!("app.launcher.drives.unlock").into(),
    fields: vec![
      Field::new(
        "passphrase",
        t!("app.launcher.drives.passphrase"),
        FieldKind::Password,
      )
      .required(),
    ],
    submit_label: t!("app.launcher.drives.unlock_one").into(),
    submit: Rc::new(move |values, _, cx| {
      let passphrase = values.get("passphrase").cloned().unwrap_or_default();
      let task = cx.udisks().unlock_and_mount(&block, passphrase);
      cx.spawn(async move |_| {
        let at = task.await?;
        Ok(Outcome::hud(t!("app.launcher.drives.mounted", path = at)))
      })
    }),
    cancel: None,
    message: Some(
      t!(
        "app.launcher.drives.passphrase_for",
        name = volume.label.clone()
      )
      .into(),
    ),
  })
}

fn lock(volume: &Volume) -> Action {
  let volume = volume.clone();
  Action::spawn("lock", t!("app.launcher.drives.lock_one"), move |_, cx| {
    let task = cx.udisks().unmount_and_lock(&volume, cx);
    let name = volume.label.clone();
    async move {
      task.await?;
      Ok(Outcome::hud(t!("app.launcher.drives.locked", name = name)))
    }
  })
}

/// A list of the volumes `keep` picks
fn volumes_view(
  id: &'static str,
  title: SharedString,
  keep: fn(&Volume) -> bool,
  icon: IconName,
  action: fn(&Volume) -> Action,
) -> View {
  let source = ListSource::new(id, move |_, cx| {
    cx.udisks()
      .list_volumes(cx)
      .iter()
      .filter(|v| keep(v))
      .map(|v| volume_row(v, icon, action))
      .collect()
  })
  .observe(observe);
  ListView::new(title, t!("app.launcher.drives.search"), Rc::new(source))
    .empty(t!("app.launcher.drives.no_drives"))
    .into_view()
}

/// The drives holding a volume, by path order
fn drives_with_volumes(cx: &App) -> Vec<Drive> {
  let udisks = cx.udisks();
  udisks
    .list_drives(cx)
    .iter()
    .filter(|d| {
      udisks
        .list_volumes(cx)
        .iter()
        .any(|v| v.drive.as_ref() == Some(&d.path))
    })
    .cloned()
    .collect()
}

fn drive_meta(d: &Drive, icon: IconName) -> Meta {
  Meta {
    title: d.name().into(),
    subtitle: Some(size(d.size).into()),
    icon: Icon::Named(icon),
    keywords: vec![d.vendor.clone().into(), d.model.clone().into()],
    ..Default::default()
  }
}

fn eject_row(drive: &Drive) -> Rc<dyn Command> {
  let (d, path) = (drive.clone(), drive.path.clone());
  simple(
    format!("drive:{}", drive.id),
    move |_| drive_meta(&d, IconName::HardDriveUpload),
    move |_| {
      let path = path.clone();
      vec![Action::spawn(
        "eject",
        t!("app.launcher.drives.eject_one"),
        move |_, cx| {
          let task = cx.udisks().eject_drive(&path, cx);
          async move {
            task.await?;
            Ok(Outcome::hud(t!("app.launcher.drives.ejected")))
          }
        },
      )]
    },
  )
}

fn health_row(drive: &Drive) -> Rc<dyn Command> {
  let d = drive.clone();
  let failing = drive.smart().is_some_and(|s| s.failing);
  let drive = drive.clone();
  simple(
    format!("drive-health:{}", drive.id),
    move |_| {
      let mut meta = drive_meta(&d, IconName::HeartPulse);
      if failing {
        meta.accessories = vec![Accessory::Tag(
          t!("app.launcher.drives.failing").into(),
          Tone::Err,
        )];
      }
      meta
    },
    move |_| {
      vec![open_health(
        &drive,
        "health",
        t!("app.launcher.drives.health").into(),
      )]
    },
  )
}

/// Asks the drive for fresh SMART data and its attributes, then shows them
fn open_health(drive: &Drive, id: &'static str, title: SharedString) -> Action {
  let drive = drive.clone();
  Action::spawn(id, title, move |_, cx| {
    let refresh = cx.udisks().refresh_smart(&drive.path, cx);
    let attributes = cx.udisks().smart_attributes(&drive.path, cx);
    let known = drive.clone();
    async move {
      // a refresh it could not do still shows the last data
      let fresh = refresh.await.ok().flatten().unwrap_or(known);
      let attributes = attributes.await.ok();
      Ok(Outcome::Push(health_view(&fresh, attributes.as_ref())))
    }
  })
}

fn row(rows: &mut Vec<(SharedString, SharedString)>, key: &str, value: impl Into<SharedString>) {
  rows.push((t!(key).into(), value.into()));
}

fn health_view(drive: &Drive, attributes: Option<&SmartAttributes>) -> View {
  let smart = drive.smart();
  let mut rows: Vec<(SharedString, SharedString)> = Vec::new();
  let mut body = format!("# {}\n", drive.name());
  if let Some(smart) = &smart {
    let state = match smart.failing {
      true => t!("app.launcher.drives.failing"),
      false => t!("app.launcher.drives.healthy"),
    };
    body.push_str(&format!("\n**{state}**\n"));
    if let Some(celsius) = smart.temperature_celsius {
      row(
        &mut rows,
        "app.launcher.drives.temperature",
        format!("{celsius:.0} °C"),
      );
    }
    if let Some(hours) = smart.power_on_hours {
      let hours = t!("app.launcher.drives.hours", count = hours);
      row(&mut rows, "app.launcher.drives.power_on", hours.to_string());
    }
    // udisks keeps no self-test history, only the last result
    if !smart.selftest_status.is_empty() {
      row(
        &mut rows,
        "app.launcher.drives.selftest",
        smart.selftest_status.clone(),
      );
    }
  }
  if let Some(ata) = &drive.ata {
    if ata.smart_num_bad_sectors >= 0 {
      row(
        &mut rows,
        "app.launcher.drives.bad_sectors",
        ata.smart_num_bad_sectors.to_string(),
      );
    }
    row(
      &mut rows,
      "app.launcher.drives.attributes_failing",
      ata.smart_num_attributes_failing.to_string(),
    );
  }
  if let Some(nvme) = &drive.nvme
    && !nvme.smart_critical_warning.is_empty()
  {
    row(
      &mut rows,
      "app.launcher.drives.critical_warning",
      nvme.smart_critical_warning.join(", "),
    );
  }
  match attributes {
    Some(SmartAttributes::Ata(attributes)) if !attributes.is_empty() => {
      body.push_str(&format!(
        "\n## {}\n\n| ID | {} | {} | {} | {} | {} |\n|---|---|---|---|---|---|\n",
        t!("app.launcher.drives.attributes"),
        t!("app.launcher.drives.attribute"),
        t!("app.launcher.drives.value"),
        t!("app.launcher.drives.worst"),
        t!("app.launcher.drives.threshold"),
        t!("app.launcher.drives.raw"),
      ));
      for a in attributes {
        body.push_str(&format!(
          "| {} | {} | {} | {} | {} | {} |\n",
          a.id, a.name, a.value, a.worst, a.threshold, a.pretty
        ));
      }
    }
    Some(SmartAttributes::Nvme(log)) => {
      row(
        &mut rows,
        "app.launcher.drives.spare",
        format!("{} %", log.avail_spare),
      );
      row(
        &mut rows,
        "app.launcher.drives.percent_used",
        format!("{} %", log.percent_used),
      );
      row(
        &mut rows,
        "app.launcher.drives.data_read",
        size(log.total_data_read),
      );
      row(
        &mut rows,
        "app.launcher.drives.data_written",
        size(log.total_data_written),
      );
      row(
        &mut rows,
        "app.launcher.drives.power_cycles",
        log.power_cycles.to_string(),
      );
      row(
        &mut rows,
        "app.launcher.drives.unsafe_shutdowns",
        log.unsafe_shutdowns.to_string(),
      );
      row(
        &mut rows,
        "app.launcher.drives.media_errors",
        log.media_errors.to_string(),
      );
      row(
        &mut rows,
        "app.launcher.drives.error_log_entries",
        log.num_err_log_entries.to_string(),
      );
    }
    _ => {}
  }
  if !drive.serial.is_empty() {
    row(
      &mut rows,
      "app.launcher.drives.serial",
      drive.serial.clone(),
    );
  }
  row(&mut rows, "app.launcher.drives.size", size(drive.size));

  View::Detail(DetailView {
    title: t!("app.launcher.drives.health").into(),
    markdown: body.into(),
    metadata: rows,
    actions: match smart {
      Some(_) => vec![open_health(
        drive,
        "refresh",
        t!("app.launcher.drives.refresh").into(),
      )],
      None => Vec::new(),
    },
  })
}

fn drives_view(
  id: &'static str,
  title: SharedString,
  drives: fn(&App) -> Vec<Drive>,
  row: fn(&Drive) -> Rc<dyn Command>,
) -> View {
  let source =
    ListSource::new(id, move |_, cx| drives(cx).iter().map(row).collect()).observe(observe);
  ListView::new(title, t!("app.launcher.drives.search"), Rc::new(source))
    .empty(t!("app.launcher.drives.no_drives"))
    .into_view()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      opener(
        "drives:mount",
        t!("app.launcher.drives.mount"),
        Icon::Tile(IconName::HardDriveDownload, 30.),
        &["usb", "drive", "disk", "mount"],
        |_| {
          volumes_view(
            "drives:mountable",
            t!("app.launcher.drives.mount").into(),
            |v| !v.is_mounted() && !v.is_locked(),
            IconName::HardDrive,
            mount,
          )
        },
      ),
      opener(
        "drives:unmount",
        t!("app.launcher.drives.unmount"),
        Icon::Tile(IconName::HardDrive, 30.),
        &["usb", "drive", "disk", "unmount"],
        |_| {
          volumes_view(
            "drives:mounted",
            t!("app.launcher.drives.unmount").into(),
            Volume::is_mounted,
            IconName::HardDrive,
            unmount,
          )
        },
      ),
      opener(
        "drives:unlock",
        t!("app.launcher.drives.unlock"),
        Icon::Tile(IconName::LockOpen, 30.),
        &["usb", "drive", "encrypted", "luks", "unlock", "decrypt"],
        |_| {
          volumes_view(
            "drives:locked",
            t!("app.launcher.drives.unlock").into(),
            Volume::is_locked,
            IconName::Lock,
            unlock,
          )
        },
      ),
      opener(
        "drives:lock",
        t!("app.launcher.drives.lock"),
        Icon::Tile(IconName::Lock, 30.),
        &["usb", "drive", "encrypted", "luks", "lock"],
        |_| {
          volumes_view(
            "drives:unlocked",
            t!("app.launcher.drives.lock").into(),
            |v| v.lock.is_some() && !v.is_locked(),
            IconName::LockOpen,
            lock,
          )
        },
      ),
      opener(
        "drives:eject",
        t!("app.launcher.drives.eject"),
        Icon::Tile(IconName::HardDriveUpload, 30.),
        &["usb", "drive", "disk", "unmount", "eject", "power off"],
        |_| {
          drives_view(
            "drives:ejectable",
            t!("app.launcher.drives.eject").into(),
            drives_with_volumes,
            eject_row,
          )
        },
      ),
      opener(
        "drives:health",
        t!("app.launcher.drives.health"),
        Icon::Tile(IconName::HeartPulse, 0.),
        &[
          "drive",
          "disk",
          "smart",
          "health",
          "ssd",
          "nvme",
          "temperature",
        ],
        |_| {
          drives_view(
            "drives:smart",
            t!("app.launcher.drives.health").into(),
            |cx| {
              cx.udisks()
                .list_drives(cx)
                .iter()
                .filter(|d| d.smart().is_some())
                .cloned()
                .collect()
            },
            health_row,
          )
        },
      ),
    ],
    cx,
  );
}
