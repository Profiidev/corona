//! A notification when a drive is plugged in, with Mount, Unlock… and Open Disks

use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  rc::Rc,
};

use crate::{MountOptions, Udisks, UdisksExt, Volume};
use corona_notifications::{Action, Notifications, Notify, Urgency};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use corona_surface::{control_center::Standalone, panel::AppPanelExt};

use crate::ui::page::DisksPanel;
use crate::ui::page::{DisksRequest, Request, types::size};

const MOUNT: &str = "mount";
const UNLOCK: &str = "unlock";
const OPEN: &str = "open";

/// What a notification's buttons act on
#[derive(Clone, Debug, PartialEq)]
struct Sent {
  drive: OwnedObjectPath,
  /// The volume Mount or Unlock… is for
  volume: Option<OwnedObjectPath>,
}

/// The first action of a new drive's notification, by its volumes
#[derive(Clone, Debug, PartialEq)]
enum Offer {
  Mount(OwnedObjectPath),
  Unlock(OwnedObjectPath),
  Open,
}

fn offer(volumes: &[&Volume]) -> Offer {
  match volumes {
    [one] if one.is_locked() => Offer::Unlock(one.block.clone()),
    [one] if !one.is_mounted() => Offer::Mount(one.block.clone()),
    _ => Offer::Open,
  }
}

fn open(request: Request, cx: &mut App) {
  cx.default_global::<DisksRequest>().0 = Some(request);
  let _ = cx.open_panel::<Standalone<DisksPanel>>().log_err();
}

pub fn init(cx: &mut App) {
  let (Some(udisks), Some(notifications)) = (
    cx.try_global::<Udisks>().cloned(),
    cx.try_global::<Notifications>().cloned(),
  ) else {
    return;
  };
  // ponytail: grows per unclicked notification, drop on NotificationClosed once notifications
  // exposes it
  let sent: Rc<RefCell<HashMap<u32, Sent>>> = Rc::default();
  // None until udisksd runs: its drives were there already
  let seen: Rc<RefCell<Option<HashSet<OwnedObjectPath>>>> = Rc::default();
  let drives_now = |cx: &App| -> HashSet<OwnedObjectPath> {
    cx.udisks()
      .list_drives(cx)
      .iter()
      .map(|d| d.path.clone())
      .collect()
  };
  if udisks.running(cx) {
    *seen.borrow_mut() = Some(drives_now(cx));
  }
  {
    // running is published after the drives and volumes, so this is the baseline
    let seen = seen.clone();
    cx.observe(&udisks.running, move |_, cx| {
      *seen.borrow_mut() = cx.udisks().running(cx).then(|| drives_now(cx));
    })
    .detach();
  }
  {
    let sent = sent.clone();
    cx.observe(&udisks.volumes, move |_, cx| {
      let udisks = cx.udisks();
      let drives = drives_now(cx);
      let mut seen = seen.borrow_mut();
      let Some(known) = seen.as_mut() else {
        return;
      };
      // a drive is new once it shows a volume: a blank one waits for its first
      let volumes = udisks.list_volumes(cx);
      let new: Vec<OwnedObjectPath> = drives
        .iter()
        .filter(|d| !known.contains(*d))
        .filter(|d| volumes.iter().any(|v| v.drive.as_ref() == Some(*d)))
        .cloned()
        .collect();
      known.retain(|d| drives.contains(d));
      known.extend(new.iter().cloned());
      for drive in new {
        notify(drive, sent.clone(), cx);
      }
    })
    .detach();
  }

  let actions = notifications.action_invoked();
  cx.spawn(async move |cx| {
    let mut actions = match actions.await {
      Ok(actions) => std::pin::pin!(actions),
      Err(e) => return tracing::error!("drive notifications: {e:#}"),
    };
    use futures::StreamExt;
    while let Some((notification, key)) = actions.next().await {
      let Some(target) = sent.borrow_mut().remove(&notification) else {
        continue;
      };
      cx.update(|cx| match (key.as_str(), target.volume) {
        (MOUNT, Some(volume)) => {
          let mount = cx.udisks().mount(&volume, MountOptions::default());
          cx.spawn(async move |_| drop(mount.await.log_err()))
            .detach();
        }
        (UNLOCK, Some(volume)) => open(Request::Unlock(volume), cx),
        // Open, or the notification itself clicked
        _ => open(Request::Drive(target.drive), cx),
      });
    }
  })
  .detach();
}

fn notify(drive: OwnedObjectPath, sent: Rc<RefCell<HashMap<u32, Sent>>>, cx: &mut App) {
  let udisks = cx.udisks();
  let Some(d) = udisks.drive(&drive, cx) else {
    return;
  };
  let volumes: Vec<&Volume> = udisks
    .list_volumes(cx)
    .iter()
    .filter(|v| v.drive.as_ref() == Some(&drive))
    .collect();
  let mut actions = Vec::new();
  let volume = match offer(&volumes) {
    Offer::Mount(block) => {
      actions.push(Action {
        key: MOUNT.into(),
        label: t!("app.disks.mount").into(),
      });
      Some(block)
    }
    Offer::Unlock(block) => {
      actions.push(Action {
        key: UNLOCK.into(),
        label: t!("app.disks.hotplug.unlock").into(),
      });
      Some(block)
    }
    Offer::Open => None,
  };
  actions.push(Action {
    key: OPEN.into(),
    label: t!("app.disks.hotplug.open").into(),
  });
  let body = volumes
    .iter()
    .map(|v| v.label.clone())
    .collect::<Vec<_>>()
    .join(", ");
  let send = cx.global::<Notifications>().send_notify(Notify {
    app_name: corona_config::APP_NAME.into(),
    app_icon: "drive-removable-media".into(),
    summary: t!(
      "app.disks.hotplug.connected",
      name = d.name(),
      size = size(d.size)
    )
    .into(),
    body,
    actions,
    urgency: Urgency::Normal,
    hints: HashMap::new(),
    timeout: -1,
  });
  cx.spawn(async move |_| match send.await {
    Ok(id) => drop(sent.borrow_mut().insert(id, Sent { drive, volume })),
    Err(e) => tracing::warn!("drive notification: {e:#}"),
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use crate::Lock;

  use super::*;

  fn volume(name: &str, mounted: bool, lock: Option<Lock>) -> Volume {
    Volume {
      block: OwnedObjectPath::try_from(format!("/b/{name}")).unwrap(),
      drive: None,
      device: format!("/dev/{name}"),
      label: name.into(),
      model: String::new(),
      size: 0,
      mount_points: if mounted { vec!["/m".into()] } else { vec![] },
      removable: true,
      can_power_off: true,
      lock,
    }
  }

  #[test]
  fn offers_by_volumes() {
    let stick = volume("sdb1", false, None);
    assert_eq!(offer(&[&stick]), Offer::Mount(stick.block.clone()));
    let locked = volume("sdc1", false, Some(Lock::Locked));
    assert_eq!(offer(&[&locked]), Offer::Unlock(locked.block.clone()));
    // automounted already, or several: just open
    let mounted = volume("sdd1", true, None);
    assert_eq!(offer(&[&mounted]), Offer::Open);
    assert_eq!(offer(&[&stick, &locked]), Offer::Open);
    assert_eq!(offer(&[]), Offer::Open);
  }
}
