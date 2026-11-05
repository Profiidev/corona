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
use crate::ui::page::jobs::DisksLog;
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

/// Drives to announce now, and `known` pruned to the drives still there with media: the ones
/// without a volume stay known (a format or repartition is no plug-in), a swapped card is new again
fn announce(
  known: &mut HashSet<OwnedObjectPath>,
  with_volume: &HashSet<OwnedObjectPath>,
  present: &HashSet<OwnedObjectPath>,
) -> Vec<OwnedObjectPath> {
  let new: Vec<_> = with_volume.difference(known).cloned().collect();
  known.extend(new.iter().cloned());
  known.retain(|d| present.contains(d));
  new
}

/// The drives that are there with media
fn present(cx: &App) -> HashSet<OwnedObjectPath> {
  cx.udisks()
    .list_drives(cx)
    .iter()
    .filter(|d| d.media_available)
    .map(|d| d.path.clone())
    .collect()
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
  // the drives showing a volume: a card reader without a card is not there yet
  let drives_now = |cx: &App| -> HashSet<OwnedObjectPath> {
    cx.udisks()
      .list_volumes(cx)
      .iter()
      .filter_map(|v| v.drive.clone())
      .collect()
  };
  let baseline = move |cx: &App| {
    let mut known = drives_now(cx);
    known.retain(|d| present(cx).contains(d));
    known
  };
  if udisks.running(cx) {
    *seen.borrow_mut() = Some(baseline(cx));
  }
  {
    // running is published after the drives and volumes, so this is the baseline
    let seen = seen.clone();
    cx.observe(&udisks.running, move |_, cx| {
      *seen.borrow_mut() = cx.udisks().running(cx).then(|| baseline(cx));
    })
    .detach();
  }
  {
    let sent = sent.clone();
    cx.observe(&udisks.volumes, move |_, cx| {
      let drives = drives_now(cx);
      let mut seen = seen.borrow_mut();
      let Some(known) = seen.as_mut() else {
        return;
      };
      // a drive is new once it shows a volume, again after its media was swapped out
      let new = announce(known, &drives, &present(cx));
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
        (MOUNT, Some(volume)) => DisksLog::run(
          cx,
          t!("app.disks.mount").to_string(),
          vec![volume.clone()],
          Rc::new(move |cx| {
            let mount = cx.udisks().mount(&volume, MountOptions::default());
            Box::pin(async move { mount.await.map(drop) })
          }),
        ),
        (UNLOCK, Some(volume)) => open(Request::Unlock(volume), cx),
        // Open, or the notification itself clicked
        _ => open(Request::Drive(target.drive), cx),
      });
    }
  })
  .detach();
}

/// Names come off the medium and daemons parse markup in notifications
fn escape(text: &str) -> String {
  text
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
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
    .map(|v| escape(&v.label))
    .collect::<Vec<_>>()
    .join(", ");
  let send = cx.global::<Notifications>().send_notify(Notify {
    app_name: corona_config::APP_NAME.into(),
    app_icon: "drive-removable-media".into(),
    summary: t!(
      "app.disks.hotplug.connected",
      name = escape(&d.name()),
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

  #[test]
  fn markup_in_names_is_escaped() {
    assert_eq!(
      escape(r#"R&D <a href="x">go</a>"#),
      r#"R&amp;D &lt;a href="x"&gt;go&lt;/a&gt;"#
    );
  }

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

  #[test]
  fn announces_once_per_drive_and_media() {
    let set = |d: &[&str]| {
      d.iter()
        .map(|d| OwnedObjectPath::try_from(*d).unwrap())
        .collect()
    };
    let mut known = HashSet::new();
    let (a, b): (HashSet<_>, HashSet<_>) = (set(&["/a"]), set(&["/b"]));
    assert_eq!(
      announce(&mut known, &a, &a),
      [a.iter().next().unwrap().clone()]
    );
    // format: volumes vanish, the drive stays with media, the next volume is no plug-in
    assert!(announce(&mut known, &set(&[]), &a).is_empty());
    assert!(announce(&mut known, &a, &a).is_empty());
    // media pulled from a card reader, a new card is announced again
    assert!(announce(&mut known, &set(&[]), &set(&[])).is_empty());
    assert_eq!(announce(&mut known, &a, &a).len(), 1);
    // unplugged
    assert!(announce(&mut known, &set(&[]), &b).is_empty());
    assert_eq!(announce(&mut known, &a, &a).len(), 1);
  }
}
