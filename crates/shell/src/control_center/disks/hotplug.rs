//! A notification when a drive is plugged in, with Mount, Unlock… and Open Disks

use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  rc::Rc,
};

use corona_notifications::{Action, Notifications, Notify, Urgency};
use corona_surface::panel::PanelState;
use corona_udisks::{MountOptions, Udisks, UdisksExt, Volume};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::control_center::disks::{DisksRequest, Request, types::size};

const MOUNT: &str = "mount";
const UNLOCK: &str = "unlock";
const OPEN: &str = "open";
/// Clicking the notification itself
const DEFAULT: &str = "default";

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
  let _ = PanelState::show("disks", false, cx).log_err();
}

pub fn init(cx: &mut App) {
  let (Some(udisks), Some(notifications)) = (
    cx.try_global::<Udisks>().cloned(),
    cx.try_global::<Notifications>().cloned(),
  ) else {
    return;
  };
  let sent: Rc<RefCell<HashMap<u32, Sent>>> = Rc::default();
  // None until the first drives arrive: those were there already
  let seen: Rc<RefCell<Option<HashSet<OwnedObjectPath>>>> = Rc::default();
  {
    let sent = sent.clone();
    cx.observe(&udisks.volumes, move |_, cx| {
      let udisks = cx.udisks();
      let drives: HashSet<OwnedObjectPath> = udisks
        .list_drives(cx)
        .iter()
        .map(|d| d.path.clone())
        .collect();
      let mut seen = seen.borrow_mut();
      let Some(known) = seen.as_mut() else {
        if udisks.running(cx) && !drives.is_empty() {
          *seen = Some(drives);
        }
        return;
      };
      if !udisks.running(cx) {
        // udisksd went away, its drives come back on restart
        *seen = None;
        return;
      }
      let new: Vec<OwnedObjectPath> = drives
        .iter()
        .filter(|d| !known.contains(*d))
        .cloned()
        .collect();
      *known = drives;
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
        (OPEN | DEFAULT, _) | (_, None) => open(Request::Drive(target.drive), cx),
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
  use corona_udisks::Lock;

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
