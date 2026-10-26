//! Drives, partitions, loop devices, swap and disk jobs through udisks

use std::{collections::HashMap, rc::Rc};

use corona_launcher::Tone;
use corona_udisks::{Block, SmartAttributes, UdisksExt};
use gpui_kit::{
  AnyElement, App, Context, Global, InteractiveElement, IntoElement, ParentElement, Render,
  SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
  assets::IconName,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::control_center::{
  ControlCenterPanel,
  disks::{
    dialogs::Dialog,
    jobs::{DisksLog, Start, log},
    model::{DriveView, Health, PartView, drives, health},
    ui::section,
  },
  variants::ControlCenterType,
};

mod devices;
mod dialogs;
mod drive;
pub(crate) mod fd;
pub(crate) mod hotplug;
pub(crate) mod jobs;
mod model;
mod partition;
mod storage;
mod types;
mod ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
  Devices,
  Storage,
  Jobs,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DriveTab {
  Overview,
  Health,
}

#[derive(Clone, PartialEq, Debug)]
pub enum View {
  List,
  Drive(OwnedObjectPath),
  /// Any block: a partition, a whole-disk filesystem or a cleartext
  Block(OwnedObjectPath),
}

/// Where the panel opens next, asked for by a notification
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
  Drive(OwnedObjectPath),
  Unlock(OwnedObjectPath),
}

#[derive(Default)]
pub struct DisksRequest(pub Option<Request>);

impl Global for DisksRequest {}

/// What udisksd can do with a filesystem type
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Caps {
  pub format: bool,
  pub resize: bool,
  pub check: bool,
  pub repair: bool,
}

pub struct DisksPanel {
  tab: Tab,
  drive_tab: DriveTab,
  view: View,
  dialog: Option<Dialog>,
  /// The drive page's SMART attributes, loaded on its Health tab
  smart: Option<(OwnedObjectPath, Option<SmartAttributes>)>,
  refreshing: bool,
  /// By filesystem type, asked once
  caps: HashMap<String, Caps>,
  _subscriptions: Vec<Subscription>,
}

impl ControlCenterPanel for DisksPanel {
  const TYPE: ControlCenterType = ControlCenterType::Disks;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let udisks = cx.udisks().clone();
    if cx.try_global::<DisksLog>().is_none() {
      jobs::init(cx);
    }
    let log = log(cx).clone();
    let mut subscriptions = vec![
      cx.observe(&udisks.running, |_, _, cx| cx.notify()),
      cx.observe(&udisks.drives, |_, _, cx| cx.notify()),
      cx.observe(&udisks.jobs, |_, _, cx| cx.notify()),
      cx.observe(&log.running, |_, _, cx| cx.notify()),
      cx.observe(&log.finished, |_, _, cx| cx.notify()),
      cx.observe_global_in::<DisksRequest>(window, |this, window, cx| {
        this.take_request(window, cx)
      }),
    ];
    // a page whose block went away goes back to the list
    subscriptions.push(cx.observe(&udisks.blocks, |this, _, cx| {
      let gone = match &this.view {
        View::Block(path) => cx.udisks().block(path, cx).is_none(),
        View::Drive(path) => cx.udisks().drive(path, cx).is_none(),
        View::List => false,
      };
      if gone {
        this.view = View::List;
        this.dialog = None;
      }
      cx.notify();
    }));
    let mut this = Self {
      tab: Tab::Devices,
      drive_tab: DriveTab::Overview,
      view: View::List,
      dialog: None,
      smart: None,
      refreshing: false,
      caps: HashMap::new(),
      _subscriptions: subscriptions,
    };
    this.take_request(window, cx);
    this
  }

  fn buttons(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    if self.view != View::List || !cx.udisks().running(cx) {
      return Vec::new();
    }
    vec![
      Button::new("disks-rescan")
        .icon(IconName::RefreshCw)
        .small()
        .ghost()
        .cursor_pointer()
        .tooltip(t!("app.disks.rescan"))
        .on_click(cx.listener(|this, _, _, cx| this.rescan_all(cx)))
        .into_any_element(),
    ]
  }
}

impl DisksPanel {
  fn take_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    // reading first: taking marks the global changed, which would run this again
    if cx
      .try_global::<DisksRequest>()
      .is_none_or(|r| r.0.is_none())
    {
      return;
    }
    let Some(request) = cx.global_mut::<DisksRequest>().0.take() else {
      return;
    };
    match request {
      Request::Drive(path) => self.open_drive(path, cx),
      Request::Unlock(block) => {
        self.view = View::Block(block.clone());
        self.dialog = Some(Dialog::unlock(block, window, cx));
      }
    }
    cx.notify();
  }

  fn open_drive(&mut self, path: OwnedObjectPath, cx: &mut Context<Self>) {
    self.view = View::Drive(path);
    self.drive_tab = DriveTab::Overview;
    self.smart = None;
    cx.notify();
  }

  fn open_block(&mut self, path: OwnedObjectPath, cx: &mut Context<Self>) {
    if let Some(block) = cx.udisks().block(&path, cx) {
      let kind = block.id_type.clone();
      self.load_caps(&kind, cx);
    }
    self.view = View::Block(path);
    cx.notify();
  }

  /// Runs a udisks call, a failure shows on the Jobs tab with a retry
  pub(super) fn act(
    &mut self,
    title: String,
    objects: Vec<OwnedObjectPath>,
    start: Start,
    cx: &mut Context<Self>,
  ) {
    DisksLog::run(cx, title, objects, start);
  }

  fn rescan_all(&mut self, cx: &mut Context<Self>) {
    let disks: Vec<OwnedObjectPath> = self
      .drive_views(cx)
      .into_iter()
      .filter_map(|d| d.disk.map(|b| b.path))
      .collect();
    for disk in disks {
      let path = disk.clone();
      self.act(
        t!("app.disks.rescan").into(),
        vec![disk],
        Rc::new(move |cx| Box::pin(cx.udisks().rescan(&path))),
        cx,
      );
    }
  }

  /// Asks what udisksd can do with `kind`, once
  pub(super) fn load_caps(&mut self, kind: &str, cx: &mut Context<Self>) {
    if kind.is_empty() || self.caps.contains_key(kind) {
      return;
    }
    self.caps.insert(kind.to_string(), Caps::default());
    let udisks = cx.udisks().clone();
    let (format, resize, check, repair) = (
      udisks.can_format(kind),
      udisks.can_resize(kind),
      udisks.can_check(kind),
      udisks.can_repair(kind),
    );
    let kind = kind.to_string();
    cx.spawn(async move |this, cx| {
      let caps = Caps {
        format: format.await.is_ok_and(|(ok, _)| ok),
        resize: resize.await.is_ok_and(|(ok, _, _)| ok),
        check: check.await.is_ok_and(|(ok, _)| ok),
        repair: repair.await.is_ok_and(|(ok, _)| ok),
      };
      this
        .update(cx, |this, cx| {
          this.caps.insert(kind, caps);
          cx.notify();
        })
        .ok();
    })
    .detach();
  }

  pub(super) fn caps(&self, kind: &str) -> Caps {
    self.caps.get(kind).copied().unwrap_or_default()
  }

  pub(super) fn drive_views(&self, cx: &App) -> Vec<DriveView> {
    let udisks = cx.udisks();
    drives(udisks.list_drives(cx), udisks.list_blocks(cx))
  }

  /// A block or drive disk the running system uses: whole disks by their partitions
  pub(super) fn block_is_system(&self, path: &OwnedObjectPath, cx: &App) -> bool {
    if let Some(view) = self
      .drive_views(cx)
      .into_iter()
      .find(|v| v.disk.as_ref().is_some_and(|d| &d.path == path))
    {
      return view.is_system();
    }
    self.find_part(path, cx).is_some_and(|(_, p)| p.is_system())
  }

  /// The partition view of a block, with its drive
  pub(super) fn find_part(
    &self,
    path: &OwnedObjectPath,
    cx: &App,
  ) -> Option<(Option<DriveView>, PartView)> {
    let views = self.drive_views(cx);
    for view in &views {
      if let Some(part) = view.parts.iter().find(|p| &p.block.path == path) {
        return Some((Some(view.clone()), part.clone()));
      }
    }
    let block: Block = cx.udisks().block(path, cx)?.clone();
    let drive = views.into_iter().find(|v| {
      v.parts
        .iter()
        .any(|p| p.cleartext.as_ref().is_some_and(|c| c.path == block.path))
    });
    let cleartext = cx
      .udisks()
      .list_blocks(cx)
      .iter()
      .find(|b| b.crypto_backing_device.as_ref() == Some(&block.path))
      .cloned();
    Some((
      drive,
      PartView {
        block,
        cleartext,
        hue: 250.,
      },
    ))
  }

  /// A drive's or block's page title with a back button; the list has none
  fn header(&self, cx: &Context<'_, Self>) -> Option<AnyElement> {
    let theme = cx.theme();
    if !cx.udisks().running(cx) {
      return None;
    }
    // the drive's partition map, or how full a mounted partition is, under the title
    let (title, status, tags, bar): (String, String, Vec<AnyElement>, Option<AnyElement>) =
      match &self.view {
        View::List => return None,
        View::Drive(path) => {
          let view = self
            .drive_views(cx)
            .into_iter()
            .find(|v| &v.drive.path == path)?;
          let tag = match health(&view.drive) {
            Health::Ok => Some((t!("app.disks.healthy"), Tone::Ok)),
            Health::Warn => Some((t!("app.disks.warning"), Tone::Warn)),
            Health::Err => Some((t!("app.disks.failing"), Tone::Err)),
            Health::Unknown => None,
          };
          let tags = tag
            .map(|(text, tone)| ui::tag(text, tone, cx).into_any_element())
            .into_iter()
            .collect();
          let map = (!view.parts.is_empty())
            .then(|| ui::partition_map(&devices::segments(&view), cx).into_any_element());
          (view.drive.name(), devices::drive_line(&view), tags, map)
        }
        View::Block(path) => {
          let (_, part) = self.find_part(path, cx)?;
          let block = &part.block;
          let status = format!(
            "{} · {} · {}",
            block.device.trim_start_matches("/dev/"),
            types::fs_name(&block.id_type),
            types::size(block.size)
          );
          let (state, tone) = state_tag(&part);
          let mut tags = Vec::new();
          if part.is_system() {
            tags.push(ui::tag(t!("app.disks.system"), Tone::Warn, cx).into_any_element());
          }
          tags.push(ui::tag(state, tone, cx).into_any_element());
          let mount = block
            .filesystem
            .as_ref()
            .and_then(|f| f.mount_points.first().cloned());
          let bar = mount
            .as_deref()
            .and_then(model::usage)
            .zip(mount.clone())
            .map(|((used, total), mount)| {
              let used_percent = model::percent(used, total);
              ui::usage_bar(
                used_percent,
                t!("app.disks.used", percent = format!("{used_percent:.0}")),
                mount,
                cx,
              )
              .into_any_element()
            });
          (block.name().to_string(), status, tags, bar)
        }
      };
    let top = div()
      .flex()
      .items_center()
      .gap_2()
      .child(
        Button::new("disks-back")
          .icon(IconName::ChevronLeft)
          .small()
          .ghost()
          .cursor_pointer()
          .on_click(cx.listener(|this, _, _, cx| {
            this.view = match &this.view {
              // a cleartext or partition goes back to its drive
              View::Block(path) => match this.find_part(path, cx).and_then(|(d, _)| d) {
                Some(drive) => View::Drive(drive.drive.path),
                None => View::List,
              },
              _ => View::List,
            };
            this.dialog = None;
            cx.notify();
          })),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(
            div()
              .text_sm()
              .font_weight(gpui_kit::FontWeight::BOLD)
              .truncate()
              .child(title),
          )
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(status),
          ),
      )
      .children(tags);
    Some(
      div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_2()
        .child(top)
        .children(bar)
        .into_any_element(),
    )
  }

  fn tabs(&self, cx: &Context<'_, Self>) -> AnyElement {
    let running = log(cx).running_count(cx);
    div()
      .flex()
      .flex_shrink_0()
      .gap_1()
      .children(
        [
          (
            Tab::Devices,
            "disks-tab-devices",
            t!("app.disks.tab.devices"),
          ),
          (
            Tab::Storage,
            "disks-tab-storage",
            t!("app.disks.tab.storage"),
          ),
          (Tab::Jobs, "disks-tab-jobs", t!("app.disks.tab.jobs")),
        ]
        .map(|(tab, id, label)| {
          let label: SharedString = match (tab, running) {
            (Tab::Jobs, n) if n > 0 => format!("{label} · {n}").into(),
            _ => label.into(),
          };
          Button::new(id)
            .label(label)
            .small()
            .flex_1()
            .cursor_pointer()
            .when(tab == self.tab, |b| b.primary())
            .on_click(cx.listener(move |this, _, _, cx| {
              this.tab = tab;
              cx.notify();
            }))
        }),
      )
      .into_any_element()
  }

  fn unreachable(&self, cx: &Context<'_, Self>) -> AnyElement {
    let theme = cx.theme();
    section(
      "",
      "",
      None,
      [div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .p_4()
        .child(Icon::new(IconName::TriangleAlert).text_color(theme.colors.danger))
        .child(
          div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
            .child(t!("app.disks.unreachable_title")),
        )
        .child(
          div()
            .text_xs()
            .text_center()
            .text_color(theme.colors.muted_foreground)
            .child(t!("app.disks.unreachable_body")),
        )
        .child(
          Button::new("disks-retry")
            .label(t!("app.disks.jobs.retry"))
            .small()
            .primary()
            .cursor_pointer()
            // asking starts udisksd through D-Bus activation
            .on_click(cx.listener(|_, _, _, cx| {
              let ask = cx.udisks().get_block_devices();
              cx.spawn(async move |_, _| drop(ask.await)).detach();
            })),
        )
        .into_any_element()],
      cx,
    )
    .into_any_element()
  }

  /// An action line: icon, label, greyed out when not possible
  #[allow(clippy::too_many_arguments)]
  pub(super) fn action_row(
    &self,
    id: impl Into<SharedString>,
    icon: IconName,
    label: impl Into<SharedString>,
    enabled: bool,
    danger: bool,
    on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    cx: &Context<'_, Self>,
  ) -> AnyElement {
    let theme = cx.theme();
    let color = match danger {
      true => theme.colors.danger,
      false => theme.colors.foreground,
    };
    div()
      .id(gpui_kit::ElementId::Name(id.into()))
      .test_support()
      .flex()
      .items_center()
      .gap_2()
      .px_1()
      .py_1()
      .rounded_md()
      .text_sm()
      .text_color(color)
      .when(!enabled, |d| d.opacity(0.45))
      .when(enabled, |d| {
        d.cursor_pointer()
          .hover(|s| s.bg(theme.colors.muted))
          .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
      })
      .child(Icon::new(icon).small())
      .child(label.into())
      .into_any_element()
  }
}

/// The tag of a partition's state
pub(super) fn state_tag(part: &PartView) -> (SharedString, Tone) {
  let on = part.is_on();
  let key = match (&part.block.encrypted, &part.block.swapspace, on) {
    (Some(_), _, true) => "app.disks.state.unlocked",
    (Some(_), _, false) => "app.disks.state.locked",
    (_, Some(_), true) => "app.disks.state.active",
    (_, Some(_), false) => "app.disks.state.inactive",
    (_, _, true) => "app.disks.state.mounted",
    (_, _, false) => "app.disks.state.unmounted",
  };
  (t!(key).into(), if on { Tone::Ok } else { Tone::Off })
}

impl Render for DisksPanel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let running = cx.udisks().running(cx);
    let body: Vec<AnyElement> = match (running, &self.view) {
      (false, _) => vec![self.unreachable(cx)],
      (true, View::List) => match self.tab {
        Tab::Devices => self.devices_tab(cx),
        Tab::Storage => self.storage_tab(cx),
        Tab::Jobs => self.jobs_tab(cx),
      },
      (true, View::Drive(path)) => self.drive_page(&path.clone(), cx),
      (true, View::Block(path)) => self.block_page(&path.clone(), cx),
    };
    let dialog = self.dialog_view(window, cx);
    div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .children(self.header(cx))
      .when(running && self.view == View::List, |d| {
        d.child(self.tabs(cx))
      })
      .child(
        div()
          .id("disks-body")
          .flex()
          .flex_col()
          .flex_1()
          .min_h_0()
          .gap_2()
          .overflow_y_scrollbar()
          .children(body),
      )
      .children(dialog)
  }
}

#[cfg(test)]
mod tests;

/// The locale keys the panel builds at runtime
#[cfg(test)]
pub(crate) fn runtime_keys() -> Vec<String> {
  let operations = model::OPERATIONS
    .iter()
    .map(|o| format!("app.disks.operation.{}", o.replace('-', "_")));
  let selftests = drive::SELFTEST
    .iter()
    .map(|s| format!("app.disks.selftest_status.{s}"));
  operations.chain(selftests).collect()
}

#[cfg(test)]
pub(crate) fn test_drive(removable: bool) -> corona_udisks::Drive {
  model::tests::drive("usb", removable)
}
