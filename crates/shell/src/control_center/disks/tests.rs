use std::{cell::Cell, rc::Rc, time::Instant};

use corona_udisks::{Block, Drive, Job, Loop, Udisks};
use corona_utils::test_bus::TestBus;
use futures::executor::block_on;
use gpui_kit::{
  self as gpui, AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions,
  test::TestWindowExt,
};

use super::*;
use crate::control_center::disks::{
  dialogs::{DialogKind, LabelTarget, ResizeTarget},
  jobs::Finished,
  model::tests::{block, path, stick},
};

struct Open {
  _bus: TestBus,
  window: AnyWindowHandle,
  panel: Entity<DisksPanel>,
}

fn open(cx: &mut TestAppContext, drives: Vec<Drive>, blocks: Vec<Block>, jobs: Vec<Job>) -> Open {
  cx.executor().allow_parking();
  let bus = TestBus::new();
  let conn = block_on(bus.conn());
  cx.update(|cx| {
    gpui_kit::init(cx);
    cx.set_global(corona_config::Config::default());
    Udisks::fake(cx, &conn, drives, blocks, jobs);
  });
  let (window, panel) = cx
    .update(|cx| {
      gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        cx.new(|cx| DisksPanel::init(window, cx))
      })
    })
    .unwrap();
  Open {
    _bus: bus,
    window,
    panel,
  }
}

fn present(cx: &mut TestAppContext, window: AnyWindowHandle, ids: &[&str]) -> Vec<bool> {
  cx.update_window(window, |_, window, cx| {
    window.render_frame(cx);
    ids
      .iter()
      .map(|id| {
        window
          .try_find(gpui::ElementId::Name(id.to_string().into()))
          .is_some()
      })
      .collect()
  })
  .unwrap()
}

fn assert_all(cx: &mut TestAppContext, window: AnyWindowHandle, ids: &[&str]) {
  for (id, found) in ids.iter().zip(present(cx, window, ids)) {
    assert!(found, "{id}");
  }
}

fn stick_panel(cx: &mut TestAppContext) -> Open {
  let (drives, blocks) = stick();
  open(cx, drives, blocks, vec![])
}

#[gpui::test]
fn devices_show_drives_partitions_and_their_buttons(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  assert_all(
    cx,
    o.window,
    &[
      "disks-tab-devices",
      "disks-tab-storage",
      "disks-tab-jobs",
      "disks-drive-usb",
      "disks-part-sdb1",
      "disks-primary-sdb1",
      "disks-primary-sdb2",
      "disks-primary-sdb3",
    ],
  );
  // the whole disk is no row of its own
  assert_eq!(present(cx, o.window, &["disks-part-sdb"]), [false]);
}

#[gpui::test]
fn state_tags_follow_the_volume(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  let parts = cx.read(|cx| o.panel.read(cx).drive_views(cx)[0].parts.clone());
  let tags: Vec<_> = parts.iter().map(|p| state_tag(p).0.to_string()).collect();
  assert_eq!(tags, ["Mounted", "Locked", "Inactive"]);
}

#[gpui::test]
fn the_drive_page(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  o.panel.update(cx, |p, cx| {
    p.open_drive(path("/org/freedesktop/UDisks2/drives/usb"), cx)
  });
  assert_all(
    cx,
    o.window,
    &[
      "disks-back",
      "disks-drive-tab-overview",
      "disks-drive-tab-health",
      "disks-create-part",
      "disks-drive-backup",
      "disks-drive-benchmark",
      "disks-drive-rescan",
      "disks-eject",
    ],
  );
  assert_eq!(
    present(cx, o.window, &["disks-tab-devices", "disks-power-off"]),
    [false, false]
  );
  o.panel.update(cx, |p, cx| {
    p.drive_tab = DriveTab::Health;
    cx.notify();
  });
  // no SMART on a usb stick: no refresh
  assert_eq!(present(cx, o.window, &["disks-smart-refresh"]), [false]);
}

#[gpui::test]
fn a_partition_page(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  o.panel.update(cx, |p, cx| {
    p.open_block(path("/org/freedesktop/UDisks2/block_devices/sdb1"), cx)
  });
  assert_all(
    cx,
    o.window,
    &[
      "disks-primary-page-sdb1",
      "disks-open-folder",
      "disks-rename",
      "disks-resize-fs",
      "disks-check",
      "disks-part-type",
      "disks-part-flags",
      "disks-delete-part",
      "disks-format",
      "disks-backup",
      "disks-restore",
      "disks-benchmark",
    ],
  );
  // a LUKS partition has encryption actions, no filesystem ones
  o.panel.update(cx, |p, cx| {
    p.open_block(path("/org/freedesktop/UDisks2/block_devices/sdb2"), cx)
  });
  assert_all(
    cx,
    o.window,
    &["disks-change-passphrase", "disks-resize-luks"],
  );
  assert_eq!(present(cx, o.window, &["disks-rename"]), [false]);
}

#[gpui::test]
fn every_dialog_opens(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  let part = path("/org/freedesktop/UDisks2/block_devices/sdb1");
  let disk = path("/org/freedesktop/UDisks2/block_devices/sdb");
  let kinds = [
    (DialogKind::Format, &part),
    (DialogKind::Unlock, &part),
    (DialogKind::Label(LabelTarget::Filesystem), &part),
    (DialogKind::Label(LabelTarget::PartitionName), &part),
    (DialogKind::Resize(ResizeTarget::Filesystem), &part),
    (DialogKind::Resize(ResizeTarget::Partition), &part),
    (DialogKind::CreatePartition, &disk),
    (DialogKind::PartitionType, &part),
    (DialogKind::Flags, &part),
    (DialogKind::ChangePassphrase, &part),
    (DialogKind::Backup, &part),
    (DialogKind::Restore, &part),
    (DialogKind::Benchmark, &part),
    (DialogKind::DeletePartition, &part),
  ];
  for (kind, block) in kinds {
    let id = kind.id();
    cx.update_window(o.window, |_, window, cx| {
      o.panel.update(cx, |p, cx| {
        p.dialog_for(kind.clone(), block.clone(), window, cx)
      });
    })
    .unwrap();
    let cancel = format!("disks-dialog-{id}-cancel");
    let ok = format!("disks-dialog-{id}-ok");
    assert_eq!(present(cx, o.window, &[&cancel, &ok]), [true, true], "{id}");
  }
  for dialog in [
    dialogs::Dialog::check_result(
      part.clone(),
      false,
      true,
      std::time::Duration::from_secs(90),
    ),
    dialogs::Dialog::ejected("SanDisk".into(), part.clone()),
  ] {
    let id = dialog.kind.id();
    o.panel.update(cx, |p, cx| {
      p.dialog = Some(dialog);
      cx.notify();
    });
    let cancel = format!("disks-dialog-{id}-cancel");
    let ok = format!("disks-dialog-{id}-ok");
    // a result only closes
    assert_eq!(
      present(cx, o.window, &[&cancel, &ok]),
      [true, false],
      "{id}"
    );
  }
}

#[gpui::test]
fn dialogs_check_their_input(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  let part = path("/org/freedesktop/UDisks2/block_devices/sdb2");
  cx.update_window(o.window, |_, window, cx| {
    o.panel.update(cx, |p, cx| {
      p.dialog_for(DialogKind::ChangePassphrase, part.clone(), window, cx);
      p.confirm(window, cx);
    });
  })
  .unwrap();
  let error = cx.read(|cx| {
    o.panel
      .read(cx)
      .dialog
      .as_ref()
      .and_then(|d| d.error.clone())
  });
  assert!(error.is_some(), "empty passphrases are refused");
  // restoring needs an image
  cx.update_window(o.window, |_, window, cx| {
    o.panel.update(cx, |p, cx| {
      p.dialog_for(DialogKind::Restore, part.clone(), window, cx);
      p.confirm(window, cx);
    });
  })
  .unwrap();
  let open = cx.read(|cx| o.panel.read(cx).dialog.as_ref().map(|d| d.error.is_some()));
  assert_eq!(open, Some(true));
}

#[gpui::test]
fn storage_lists_loops_and_swap(cx: &mut TestAppContext) {
  let (drives, mut blocks) = stick();
  let mut image = block("loop0", None);
  image.loop_device = Some(Loop {
    backing_file: "/tmp/nixos.iso".into(),
    autoclear: true,
  });
  // an unused loop device is no row
  let mut unused = block("loop1", None);
  unused.loop_device = Some(Loop::default());
  blocks.extend([image, unused]);
  let o = open(cx, drives, blocks, vec![]);
  o.panel.update(cx, |p, cx| {
    p.tab = Tab::Storage;
    cx.notify();
  });
  assert_all(
    cx,
    o.window,
    &[
      "disks-loop-setup",
      "disks-autoclear-loop0",
      "disks-detach-loop0",
      "disks-swap-toggle-sdb3",
      "disks-swap-label-sdb3",
    ],
  );
  assert_eq!(present(cx, o.window, &["disks-detach-loop1"]), [false]);
}

#[gpui::test]
fn jobs_running_and_finished_with_retry(cx: &mut TestAppContext) {
  let (drives, blocks) = stick();
  let job = Job {
    path: path("/org/freedesktop/UDisks2/jobs/7"),
    operation: "filesystem-check".into(),
    progress: 0.38,
    progress_valid: true,
    rate: 0,
    start_time: 0,
    expected_end_time: 0,
    objects: vec![blocks[1].path.clone()],
    cancelable: true,
    bytes: 0,
  };
  let o = open(cx, drives, blocks, vec![job]);
  let retried = Rc::new(Cell::new(0));
  cx.update(|cx| {
    let retried = retried.clone();
    jobs::log(cx).clone().push(
      Finished {
        title: "Benchmark · sda".into(),
        detail: "not authorized".into(),
        success: false,
        at: "09:05".into(),
        objects: vec![],
        retry: Some(Rc::new(move |_| retried.set(retried.get() + 1))),
        when: Instant::now(),
      },
      cx,
    );
  });
  o.panel.update(cx, |p, cx| {
    p.tab = Tab::Jobs;
    cx.notify();
  });
  assert_all(
    cx,
    o.window,
    &[
      "disks-job-cancel-/org/freedesktop/UDisks2/jobs/7",
      "disks-job-retry-0",
    ],
  );
  cx.update(|cx| {
    let finished = jobs::log(cx).finished.read(cx).clone();
    (finished[0].retry.as_ref().unwrap())(cx);
  });
  assert_eq!(retried.get(), 1);
  // the same failure again within seconds is one entry
  cx.update(|cx| {
    let log = jobs::log(cx).clone();
    let twin = log.finished.read(cx)[0].clone();
    log.push(twin, cx);
    assert_eq!(log.finished.read(cx).len(), 2);
  });
}

#[gpui::test]
fn unreachable_and_empty(cx: &mut TestAppContext) {
  let o = open(cx, vec![], vec![], vec![]);
  assert_eq!(
    present(cx, o.window, &["disks-tab-devices", "disks-retry"]),
    [true, false]
  );
  cx.update(|cx| {
    let running = cx.udisks().running.clone();
    running.update(cx, |r, cx| {
      *r = false;
      cx.notify();
    });
  });
  cx.run_until_parked();
  assert_eq!(
    present(cx, o.window, &["disks-tab-devices", "disks-retry"]),
    [false, true]
  );
}

#[gpui::test]
fn a_notification_opens_the_unlock_dialog(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  let luks = path("/org/freedesktop/UDisks2/block_devices/sdb2");
  cx.update(|cx| {
    cx.set_global(DisksRequest(Some(Request::Unlock(luks.clone()))));
  });
  cx.run_until_parked();
  let (view, dialog) = cx.read(|cx| {
    let p = o.panel.read(cx);
    (p.view.clone(), p.dialog.as_ref().map(|d| d.kind.clone()))
  });
  assert_eq!(view, View::Block(luks));
  assert_eq!(dialog, Some(DialogKind::Unlock));
}

#[gpui::test]
fn a_rows_button_does_not_open_the_row(cx: &mut TestAppContext) {
  let o = stick_panel(cx);
  cx.update_window(o.window, |_, window, cx| {
    window.render_frame(cx);
    window.click(gpui::ElementId::Name("disks-primary-sdb1".into()), cx);
  })
  .unwrap();
  cx.run_until_parked();
  assert_eq!(cx.read(|cx| o.panel.read(cx).view.clone()), View::List);
  // the row itself still opens its page
  cx.update_window(o.window, |_, window, cx| {
    window.click(gpui::ElementId::Name("disks-part-sdb1".into()), cx);
  })
  .unwrap();
  assert_eq!(
    cx.read(|cx| o.panel.read(cx).view.clone()),
    View::Block(path("/org/freedesktop/UDisks2/block_devices/sdb1"))
  );
}

#[gpui::test]
fn the_running_systems_partitions_stay_put(cx: &mut TestAppContext) {
  let (drives, mut blocks) = stick();
  blocks[1].filesystem.as_mut().unwrap().mount_points = vec!["/".into()];
  let o = open(cx, drives, blocks, vec![]);
  cx.update_window(o.window, |_, window, cx| {
    window.render_frame(cx);
    // clicking the greyed out Unmount opens nothing
    window.click(gpui::ElementId::Name("disks-primary-sdb1".into()), cx);
  })
  .unwrap();
  cx.run_until_parked();
  assert_eq!(cx.read(|cx| o.panel.read(cx).view.clone()), View::List);
  // the drive holding it ejects no more, its benchmark writes nothing
  let part = path("/org/freedesktop/UDisks2/block_devices/sdb1");
  let disk = path("/org/freedesktop/UDisks2/block_devices/sdb");
  cx.read(|cx| {
    let panel = o.panel.read(cx);
    assert!(panel.block_is_system(&part, cx) && panel.block_is_system(&disk, cx));
    let luks = path("/org/freedesktop/UDisks2/block_devices/sdb2");
    assert!(!panel.block_is_system(&luks, cx));
  });
  cx.update_window(o.window, |_, window, cx| {
    o.panel.update(cx, |p, cx| {
      p.dialog_for(DialogKind::Benchmark, disk.clone(), window, cx)
    });
  })
  .unwrap();
  assert_eq!(
    present(
      cx,
      o.window,
      &["disks-dialog-benchmark-ok", "disks-dialog-check-write"]
    ),
    [true, false]
  );
}
