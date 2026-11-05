//! The panel's dialogs, drawn over it: one struct, its fields by kind

use std::{
  collections::HashMap,
  fs::{DirBuilder, File},
  os::unix::fs::DirBuilderExt,
  path::{Path, PathBuf},
  rc::Rc,
  sync::{Arc, Mutex, atomic::Ordering},
  time::Duration,
};

use crate::{
  Block, CreatePartitionOptions, DeleteOptions, FormatOptions, LoopSetupOptions, PassphraseOptions,
  UdisksExt, UnlockOptions, is_not_authorized,
};
use anyhow::Context as _;
use corona_components::components::list::kv;
use corona_components::components::modal;
use gpui_kit::{
  AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, PathPromptOptions,
  SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::{
    IndexPath,
    input::InputState,
    slider::{SliderEvent, SliderState},
  },
  component::{
    ActiveTheme, Icon, Sizable,
    button::Button,
    checkbox::Checkbox,
    input::Input,
    select::{Select, SelectState},
    slider::Slider,
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::ui::page::{
  DisksPanel,
  fd::{self, Bench},
  jobs::{self, DisksLog, Start},
  model::{cached_usage, free_spaces},
  types::{FORMATS, default_partition_type, flags_of, fs_name, partition_types, size},
};

const MIB: u64 = 1 << 20;
/// The fields typed in hidden
const MASKED: [&str; 4] = ["passphrase", "current", "new", "confirm"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelTarget {
  Filesystem,
  Swap,
  PartitionName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeTarget {
  Filesystem,
  Encrypted,
  Partition,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DialogKind {
  Format,
  Unlock,
  Label(LabelTarget),
  Resize(ResizeTarget),
  CreatePartition,
  PartitionType,
  Flags,
  ChangePassphrase,
  Backup,
  Restore,
  Benchmark,
  DeletePartition,
  CheckResult {
    repair: bool,
    clean: bool,
    secs: u64,
  },
  LoopSetup,
  Ejected {
    name: String,
    powered_off: bool,
  },
}

impl DialogKind {
  /// A stable id for the dialog's elements
  pub fn id(&self) -> &'static str {
    match self {
      DialogKind::Format => "format",
      DialogKind::Unlock => "unlock",
      DialogKind::Label(_) => "label",
      DialogKind::Resize(_) => "resize",
      DialogKind::CreatePartition => "create-part",
      DialogKind::PartitionType => "partition-type",
      DialogKind::Flags => "flags",
      DialogKind::ChangePassphrase => "change-passphrase",
      DialogKind::Backup => "backup",
      DialogKind::Restore => "restore",
      DialogKind::Benchmark => "benchmark",
      DialogKind::DeletePartition => "delete-part",
      DialogKind::CheckResult { .. } => "check-result",
      DialogKind::LoopSetup => "loop-setup",
      DialogKind::Ejected { .. } => "eject",
    }
  }

  fn danger(&self) -> bool {
    matches!(
      self,
      DialogKind::Format | DialogKind::DeletePartition | DialogKind::Restore
    )
  }
}

type Choice = (Entity<SelectState<Vec<String>>>, Vec<String>);

pub struct Dialog {
  pub kind: DialogKind,
  pub block: Option<OwnedObjectPath>,
  inputs: HashMap<&'static str, Entity<InputState>>,
  checks: HashMap<&'static str, bool>,
  selects: HashMap<&'static str, Choice>,
  /// MiB
  slider: Option<Entity<SliderState>>,
  /// MiB, what the slider shows
  slider_value: u64,
  /// MiB, where the slider started
  slider_start: u64,
  /// MiB, the slider's top, f32 rounds above 16 TiB
  slider_max: u64,
  /// Free gaps for a new partition, (offset, size)
  gaps: Vec<(u64, u64)>,
  path: Option<PathBuf>,
  /// Bytes in the picked restore image, read once when picked
  image_size: Option<u64>,
  pub error: Option<String>,
  busy: bool,
  bench: Option<Bench>,
  /// The panel job the benchmark runs as
  job: Option<u64>,
  _subscriptions: Vec<Subscription>,
}

fn input(window: &mut Window, cx: &mut App, value: &str, masked: bool) -> Entity<InputState> {
  let state = cx.new(|cx| InputState::new(window, cx).masked(masked));
  state.update(cx, |s, cx| s.set_value(value.to_string(), window, cx));
  state
}

fn select(
  items: Vec<(String, String)>,
  selected: usize,
  window: &mut Window,
  cx: &mut App,
) -> Choice {
  let (ids, labels): (Vec<_>, Vec<_>) = items.into_iter().unzip();
  let state =
    cx.new(|cx| SelectState::new(labels, Some(IndexPath::default().row(selected)), window, cx));
  (state, ids)
}

fn home() -> PathBuf {
  std::env::var_os("HOME")
    .map(PathBuf::from)
    .unwrap_or_else(|| "/".into())
}

impl Dialog {
  fn empty(kind: DialogKind, block: Option<OwnedObjectPath>) -> Self {
    Self {
      kind,
      block,
      inputs: HashMap::new(),
      checks: HashMap::new(),
      selects: HashMap::new(),
      slider: None,
      slider_value: 0,
      slider_start: 0,
      slider_max: 0,
      gaps: Vec::new(),
      path: None,
      image_size: None,
      error: None,
      busy: false,
      bench: None,
      job: None,
      _subscriptions: Vec::new(),
    }
  }

  pub fn unlock(block: OwnedObjectPath, window: &mut Window, cx: &mut Context<DisksPanel>) -> Self {
    Self::new(DialogKind::Unlock, block, window, cx)
  }

  pub fn loop_setup(_window: &mut Window, _cx: &mut Context<DisksPanel>) -> Self {
    let mut this = Self::empty(DialogKind::LoopSetup, None);
    this.checks.insert("read_only", true);
    this.checks.insert("autoclear", true);
    this
  }

  pub fn ejected(name: String, powered_off: bool, drive: OwnedObjectPath) -> Self {
    Self::empty(DialogKind::Ejected { name, powered_off }, Some(drive))
  }

  pub fn check_result(block: OwnedObjectPath, repair: bool, clean: bool, took: Duration) -> Self {
    Self::empty(
      DialogKind::CheckResult {
        repair,
        clean,
        secs: took.as_secs(),
      },
      Some(block),
    )
  }

  fn slider_for(&mut self, min: u64, max: u64, value: u64, cx: &mut Context<DisksPanel>) {
    // a filesystem fuller than the room it may get: no shrinking, it still slides up to max
    let min = min.min(max);
    // rounded up, so the slider never goes below the bytes in use
    let min_mib = min.div_ceil(MIB);
    let max_mib = (max / MIB).max(min_mib);
    self.slider_max = max_mib;
    let (min, max) = (min_mib as f32, max_mib as f32);
    let value = (value / MIB) as f32;
    // nothing to choose: no slider, the one size stands
    if min_mib == max_mib {
      self.slider = None;
      self.slider_value = min_mib;
      self.slider_start = min_mib;
      return;
    }
    let slider = cx.new(|_| {
      SliderState::new()
        .max(max)
        .min(min)
        .step(1.)
        .default_value(value.clamp(min, max).min(max_mib as f32))
    });
    self.slider_value = (value.clamp(min, max) as u64).min(max_mib);
    self.slider_start = self.slider_value;
    self
      ._subscriptions
      .push(cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
        if let SliderEvent::Change(value) = event
          && let Some(dialog) = &mut this.dialog
        {
          dialog.slider_value = (value.start().round() as u64).min(dialog.slider_max);
          cx.notify();
        }
      }));
    self.slider = Some(slider);
  }

  pub fn new(
    kind: DialogKind,
    block: OwnedObjectPath,
    window: &mut Window,
    cx: &mut Context<DisksPanel>,
  ) -> Self {
    let mut this = Self::empty(kind.clone(), Some(block.clone()));
    let udisks = cx.udisks().clone();
    let Some(target) = udisks.block(&block, cx).cloned() else {
      return this;
    };
    let table = table_of(&target, cx);
    match kind {
      DialogKind::Format => {
        let items = FORMATS
          .iter()
          .map(|f| (f.to_string(), fs_name(f).to_string()))
          .collect();
        this.selects.insert("fs", select(items, 0, window, cx));
        let erase = vec![
          (
            String::new(),
            t!("app.disks.dialog.erase_quick").to_string(),
          ),
          ("zero".into(), t!("app.disks.dialog.erase_zero").to_string()),
        ];
        this.selects.insert("erase", select(erase, 0, window, cx));
        this
          .inputs
          .insert("label", input(window, cx, &target.id_label, false));
        this
          .inputs
          .insert("passphrase", input(window, cx, "", true));
        this.inputs.insert("confirm", input(window, cx, "", true));
        this.checks.insert("encrypt", false);
        this.checks.insert("no_discard", false);
      }
      DialogKind::Unlock => {
        let pass = input(window, cx, "", true);
        pass.update(cx, |s, cx| s.focus(window, cx));
        this
          ._subscriptions
          .push(corona_components::components::input::on_enter(
            &pass,
            window,
            cx,
            |this: &mut DisksPanel, window, cx| this.confirm(window, cx),
          ));
        this.inputs.insert("passphrase", pass);
      }
      DialogKind::Label(target_kind) => {
        let value = match target_kind {
          LabelTarget::PartitionName => target
            .partition
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
          _ => target.id_label.clone(),
        };
        this
          .inputs
          .insert("label", input(window, cx, &value, false));
      }
      DialogKind::Resize(resize) => {
        let after = free_after(&target, cx);
        let (min, max) = match resize {
          ResizeTarget::Filesystem => {
            let used = target
              .filesystem
              .as_ref()
              .and_then(|f| f.mount_points.first())
              .and_then(|m| cached_usage(m, cx))
              .map_or(MIB, |(used, _)| used);
            (used, target.size + after)
          }
          // neither call looks inside: what the contents need is unknown, it may only grow
          ResizeTarget::Encrypted => {
            let min = target
              .encrypted
              .as_ref()
              .and_then(|e| e.cleartext_device.as_ref())
              .and_then(|c| udisks.block(c, cx))
              .and_then(|c| {
                c.filesystem
                  .as_ref()
                  .map(|f| f.size)
                  .filter(|s| *s > 0)
                  .or(Some(c.size).filter(|s| *s > 0))
              })
              .unwrap_or(target.size);
            (min, target.size)
          }
          ResizeTarget::Partition => {
            let min = target
              .filesystem
              .as_ref()
              .map(|f| f.size)
              .filter(|s| *s > 0)
              .unwrap_or(target.size);
            (min, target.size + after)
          }
        };
        // starts at what the contents take up, which may be less than the partition
        let start = match resize {
          ResizeTarget::Partition => target.size,
          ResizeTarget::Filesystem => target
            .filesystem
            .as_ref()
            .map(|f| f.size)
            .filter(|s| *s > 0)
            .unwrap_or(target.size),
          ResizeTarget::Encrypted => min,
        };
        this.slider_for(min, max, start, cx);
        this.checks.insert(
          "partition_too",
          target.partition.is_some() && resize == ResizeTarget::Filesystem,
        );
        if resize == ResizeTarget::Encrypted {
          this
            .inputs
            .insert("passphrase", input(window, cx, "", true));
        }
      }
      DialogKind::CreatePartition => {
        let taken: Vec<(u64, u64)> = udisks
          .list_blocks(cx)
          .iter()
          .filter_map(|b| b.partition.as_ref())
          .filter(|p| p.table.as_ref() == Some(&target.path))
          .map(|p| (p.offset, p.size))
          .collect();
        this.gaps = free_spaces(target.size, &taken);
        let biggest = this
          .gaps
          .iter()
          .enumerate()
          .max_by_key(|(_, g)| g.1)
          .map_or(0, |(i, _)| i);
        let items = this
          .gaps
          .iter()
          .map(|(offset, len)| {
            (
              offset.to_string(),
              t!(
                "app.disks.dialog.gap",
                size = size(*len),
                offset = size(*offset)
              )
              .to_string(),
            )
          })
          .collect();
        let gaps = select(items, biggest, window, cx);
        this._subscriptions.push(cx.subscribe(
          &gaps.0,
          |this, state, _: &gpui_kit::component::select::SelectEvent<Vec<String>>, cx| {
            let row = state.read(cx).selected_index(cx).map_or(0, |i| i.row);
            if let Some(dialog) = &mut this.dialog
              && let Some((_, len)) = dialog.gaps.get(row).copied()
            {
              dialog.slider = None;
              dialog.slider_for(MIB, len, len, cx);
            }
            cx.notify();
          },
        ));
        this.selects.insert("gap", gaps);
        let len = this.gaps.get(biggest).map_or(MIB, |g| g.1);
        this.slider_for(MIB, len, len, cx);
        let types = partition_types(&table)
          .iter()
          .map(|t| (t.id.to_string(), t.name.to_string()))
          .collect();
        this.selects.insert("type", select(types, 0, window, cx));
        this.inputs.insert("name", input(window, cx, "", false));
        let mut formats: Vec<(String, String)> = FORMATS
          .iter()
          .map(|f| (f.to_string(), fs_name(f).to_string()))
          .collect();
        formats.push(("empty".into(), t!("app.disks.dialog.no_format").to_string()));
        this
          .selects
          .insert("format", select(formats, 0, window, cx));
        this.inputs.insert("fs_label", input(window, cx, "", false));
      }
      DialogKind::PartitionType => {
        let current = target
          .partition
          .as_ref()
          .map(|p| p.kind.clone())
          .unwrap_or_default();
        let mut items: Vec<(String, String)> = partition_types(&table)
          .iter()
          .map(|t| (t.id.to_string(), t.name.to_string()))
          .collect();
        let selected = match items
          .iter()
          .position(|(id, _)| id.eq_ignore_ascii_case(&current))
        {
          Some(i) => i,
          None if !current.is_empty() => {
            items.push((current.clone(), current.clone()));
            items.len() - 1
          }
          None => 0,
        };
        this
          .selects
          .insert("type", select(items, selected, window, cx));
      }
      DialogKind::Flags => {
        let current = target.partition.as_ref().map_or(0, |p| p.flags);
        for flag in flags_of(&table) {
          this.checks.insert(flag.key, current & flag.bit != 0);
        }
      }
      DialogKind::ChangePassphrase => {
        for key in ["current", "new", "confirm"] {
          this.inputs.insert(key, input(window, cx, "", true));
        }
      }
      DialogKind::Backup => {
        // dated, so a default backup never replaces an older one
        let name = format!(
          "{}-{}.img",
          target.device.trim_start_matches("/dev/").replace('/', "-"),
          jiff::Zoned::now().strftime("%Y%m%d-%H%M%S")
        );
        this.path = Some(home().join("Backups").join(name));
        this.checks.insert("compress", true);
      }
      DialogKind::Benchmark => {
        this.checks.insert("write", false);
      }
      _ => {}
    }
    this
  }

  fn text(&self, key: &str, cx: &App) -> String {
    self
      .inputs
      .get(key)
      .map(|i| i.read(cx).value().to_string())
      .unwrap_or_default()
  }

  fn check(&self, key: &str) -> bool {
    self.checks.get(key).copied().unwrap_or(false)
  }

  fn choice(&self, key: &str, cx: &App) -> String {
    self
      .selects
      .get(key)
      .and_then(|(state, ids)| ids.get(state.read(cx).selected_index(cx)?.row).cloned())
      .unwrap_or_default()
  }
}

fn table_of(block: &Block, cx: &App) -> String {
  let udisks = cx.udisks();
  let table = match &block.partition {
    Some(p) => p.table.as_ref().and_then(|t| udisks.block(t, cx)),
    None => Some(block),
  };
  table
    .and_then(|t| t.partition_table.as_ref())
    .map(|t| t.kind.clone())
    .unwrap_or_else(|| "gpt".into())
}

/// Free bytes right after the partition, what growing it may take
fn free_after(block: &Block, cx: &App) -> u64 {
  let Some(part) = &block.partition else {
    return 0;
  };
  let udisks = cx.udisks();
  let Some(disk) = part.table.as_ref().and_then(|t| udisks.block(t, cx)) else {
    return 0;
  };
  let end = part.offset + part.size;
  let next = udisks
    .list_blocks(cx)
    .iter()
    .filter_map(|b| b.partition.as_ref())
    .filter(|p| p.table == part.table && p.offset >= end)
    .map(|p| p.offset)
    .min()
    .unwrap_or(disk.size.saturating_sub(MIB));
  next.saturating_sub(end) / MIB * MIB
}

impl DisksPanel {
  fn close_dialog(&mut self, cx: &mut Context<Self>) {
    self.dialog = None;
    cx.notify();
  }

  fn dialog_error(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
    if let Some(dialog) = &mut self.dialog {
      dialog.error = Some(error.into());
      dialog.busy = false;
    }
    cx.notify();
  }

  /// The row gate ran at render: the target may be mounted or unlocked since.
  /// Shows the error and returns true when the action must not run
  fn refuse_in_use(
    &mut self,
    path: &OwnedObjectPath,
    read_only: bool,
    cx: &mut Context<Self>,
  ) -> bool {
    let refuse = read_only || self.block_in_use(path, cx);
    if refuse {
      self.dialog_error(t!("app.disks.dialog.in_use"), cx);
    }
    refuse
  }

  /// Runs the dialog's action
  pub(super) fn confirm(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
    let Some(dialog) = &self.dialog else {
      return;
    };
    if dialog.busy {
      return;
    }
    let kind = dialog.kind.clone();
    let udisks = cx.udisks().clone();
    let block = dialog.block.clone();
    let target = block.as_ref().and_then(|b| udisks.block(b, cx).cloned());
    let name = target
      .as_ref()
      .map(|b| b.name().to_string())
      .unwrap_or_default();
    let objects: Vec<OwnedObjectPath> = block.iter().cloned().collect();
    let title = |key: &str| format!("{} · {name}", t!(key));
    let path = block.clone().unwrap_or_default();
    match kind {
      DialogKind::Format => {
        if self.refuse_in_use(&path, target.as_ref().is_some_and(|t| t.read_only), cx) {
          return;
        }
        let Some(dialog) = &self.dialog else { return };
        let fs = dialog.choice("fs", cx);
        let encrypt = dialog.check("encrypt");
        let passphrase = dialog.text("passphrase", cx);
        if encrypt && passphrase.is_empty() {
          return self.dialog_error(t!("app.disks.dialog.need_passphrase"), cx);
        }
        if encrypt && passphrase != dialog.text("confirm", cx) {
          return self.dialog_error(t!("app.disks.dialog.passphrases_differ"), cx);
        }
        let options = FormatOptions {
          label: Some(dialog.text("label", cx)).filter(|l| !l.is_empty()),
          encrypt_passphrase: encrypt.then_some(passphrase),
          erase: Some(dialog.choice("erase", cx)).filter(|e| !e.is_empty()),
          no_discard: dialog.check("no_discard"),
          update_partition_type: true,
          tear_down: true,
        };
        let start: Start =
          Rc::new(move |cx| Box::pin(cx.udisks().format(&path, &fs, options.clone())));
        // a retry would keep the passphrase in the history
        match encrypt {
          true => {
            let task = start(cx);
            DisksLog::run_once(cx, title("app.disks.format"), objects, task)
          }
          false => self.act(title("app.disks.format"), objects, start, cx),
        }
        self.close_dialog(cx);
      }
      DialogKind::Unlock => {
        let passphrase = dialog.text("passphrase", cx);
        if let Some(d) = &mut self.dialog {
          d.busy = true;
          d.error = None;
        }
        let unlock = udisks.unlock(&path, &passphrase, UnlockOptions::default());
        cx.spawn(async move |this, cx| {
          let result = unlock.await;
          this
            .update(cx, |this, cx| {
              // only the dialog that started it, not one opened since
              if !matches!(&this.dialog, Some(d)
                if d.kind == DialogKind::Unlock && d.block.as_ref() == Some(&path))
              {
                return;
              }
              match result {
                Ok(_) => this.close_dialog(cx),
                // pressing Unlock again asks polkit again
                Err(e) if is_not_authorized(&e) => {
                  this.dialog_error(t!("app.disks.jobs.not_authorized"), cx)
                }
                Err(e) => this.dialog_error(unlock_error(&e), cx),
              }
            })
            .ok();
        })
        .detach();
        cx.notify();
      }
      DialogKind::Label(target_kind) => {
        let label = dialog.text("label", cx);
        let start: Start = Rc::new(move |cx| {
          let udisks = cx.udisks();
          match target_kind {
            LabelTarget::Filesystem => Box::pin(udisks.set_label(&path, &label)),
            LabelTarget::Swap => Box::pin(udisks.swap_set_label(&path, &label)),
            LabelTarget::PartitionName => Box::pin(udisks.set_partition_name(&path, &label)),
          }
        });
        self.act(title("app.disks.rename"), objects, start, cx);
        self.close_dialog(cx);
      }
      DialogKind::Resize(resize) => {
        let Some(target) = target else { return };
        // unmoved: the floored MiB would resize an unaligned size down
        if dialog.slider_value == dialog.slider_start {
          return self.close_dialog(cx);
        }
        let new = dialog.slider_value * MIB;
        let partition_too = dialog.check("partition_too");
        let passphrase = dialog.text("passphrase", cx);
        let grow = new > target.size;
        // up to the whole block: 0 lets the filesystem take what the block has
        let fill = dialog.slider_value >= target.size / MIB;
        let secret = resize == ResizeTarget::Encrypted && !passphrase.is_empty();
        let start: Start = Rc::new(move |cx| {
          let udisks = cx.udisks().clone();
          let path = path.clone();
          let passphrase = passphrase.clone();
          Box::pin(async move {
            match resize {
              ResizeTarget::Partition => udisks.resize_partition(&path, new).await,
              ResizeTarget::Encrypted => {
                let options = PassphraseOptions {
                  passphrase: Some(passphrase).filter(|p| !p.is_empty()),
                };
                udisks
                  .resize_encrypted(&path, if fill { 0 } else { new }, options)
                  .await
              }
              // growing makes room first, shrinking frees it after
              ResizeTarget::Filesystem => match (partition_too, grow) {
                (true, true) => {
                  udisks.resize_partition(&path, new).await?;
                  udisks.resize_filesystem(&path, 0).await
                }
                _ if fill => udisks.resize_filesystem(&path, 0).await,
                (true, false) => {
                  udisks.resize_filesystem(&path, new).await?;
                  udisks.resize_partition(&path, new).await
                }
                (false, _) => udisks.resize_filesystem(&path, new).await,
              },
            }
          })
        });
        match secret {
          true => {
            let task = start(cx);
            DisksLog::run_once(cx, title("app.disks.resize"), objects, task)
          }
          false => self.act(title("app.disks.resize"), objects, start, cx),
        }
        self.close_dialog(cx);
      }
      DialogKind::CreatePartition => {
        let row = dialog
          .selects
          .get("gap")
          .and_then(|(s, _)| s.read(cx).selected_index(cx))
          .map_or(0, |i| i.row);
        let Some((offset, _)) = dialog.gaps.get(row).copied() else {
          return self.dialog_error(t!("app.disks.dialog.no_space"), cx);
        };
        let len = dialog.slider_value * MIB;
        let table = target.as_ref().map(|t| table_of(t, cx)).unwrap_or_default();
        let kind = Some(dialog.choice("type", cx))
          .filter(|k| !k.is_empty())
          .unwrap_or_else(|| default_partition_type(&table).to_string());
        let part_name = dialog.text("name", cx);
        let format = dialog.choice("format", cx);
        let format_options = FormatOptions {
          label: Some(dialog.text("fs_label", cx)).filter(|l| !l.is_empty()),
          ..Default::default()
        };
        let start: Start = Rc::new(move |cx| {
          let create = cx.udisks().create_partition_and_format(
            &path,
            offset,
            len,
            &kind,
            &part_name,
            CreatePartitionOptions::default(),
            &format,
            format_options.clone(),
          );
          Box::pin(async move { create.await.map(drop) })
        });
        self.act(title("app.disks.create_partition"), objects, start, cx);
        self.close_dialog(cx);
      }
      DialogKind::PartitionType => {
        let kind = dialog.choice("type", cx);
        self.act(
          title("app.disks.change_type"),
          objects,
          Rc::new(move |cx| Box::pin(cx.udisks().set_partition_type(&path, &kind))),
          cx,
        );
        self.close_dialog(cx);
      }
      DialogKind::Flags => {
        let Some(target) = target else { return };
        let table = table_of(&target, cx);
        let current = target.partition.as_ref().map_or(0, |p| p.flags);
        let flags =
          flags_of(&table)
            .iter()
            .fold(current, |flags, flag| match dialog.check(flag.key) {
              true => flags | flag.bit,
              false => flags & !flag.bit,
            });
        self.act(
          title("app.disks.edit_flags"),
          objects,
          Rc::new(move |cx| Box::pin(cx.udisks().set_partition_flags(&path, flags))),
          cx,
        );
        self.close_dialog(cx);
      }
      DialogKind::ChangePassphrase => {
        let (current, new, confirm) = (
          dialog.text("current", cx),
          dialog.text("new", cx),
          dialog.text("confirm", cx),
        );
        if new.is_empty() || new != confirm {
          return self.dialog_error(t!("app.disks.dialog.passphrases_differ"), cx);
        }
        let change = Box::pin(cx.udisks().change_passphrase(&path, &current, &new));
        DisksLog::run_once(cx, title("app.disks.change_passphrase"), objects, change);
        self.close_dialog(cx);
      }
      DialogKind::DeletePartition => {
        if self.refuse_in_use(&path, false, cx) {
          return;
        }
        self.act(
          title("app.disks.delete"),
          objects,
          Rc::new(move |cx| {
            Box::pin(
              cx.udisks()
                .delete_partition(&path, DeleteOptions { tear_down: true }),
            )
          }),
          cx,
        );
        self.close_dialog(cx);
      }
      DialogKind::Backup => {
        let Some(image) = dialog.path.clone() else {
          return;
        };
        let compress = dialog.check("compress");
        let image = match compress && image.extension().is_none_or(|e| e != "zst") {
          true => {
            let mut name = image.into_os_string();
            name.push(".zst");
            PathBuf::from(name)
          }
          false => image,
        };
        // fd::backup renames over its target, and the picker only vetted the name without .zst
        if image.exists() {
          return self.dialog_error(t!("app.disks.dialog.image_exists"), cx);
        }
        let total = target.as_ref().map_or(0, |t| t.size);
        let open = udisks.open_for_backup(&path);
        let job = title("app.disks.backup");
        self.close_dialog(cx);
        cx.spawn(async move |_, cx| {
          let fd = open.await;
          cx.update(|cx| match fd {
            Ok(fd) => {
              DisksLog::spawn(
                cx,
                job,
                objects,
                total,
                move |progress| {
                  if let Some(dir) = image.parent() {
                    // private: the image holds the device's raw data
                    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
                  }
                  fd::backup(File::from(fd), &image, compress, &progress)?;
                  Ok(image.display().to_string())
                },
                |_, _, _| {},
              );
            }
            Err(e) => jobs::log(cx).clone().fail(job, objects, &e, None, cx),
          })
        })
        .detach();
      }
      DialogKind::Restore => {
        if self.refuse_in_use(&path, target.as_ref().is_some_and(|t| t.read_only), cx) {
          return;
        }
        let Some(dialog) = &self.dialog else { return };
        let Some(image) = dialog.path.clone() else {
          return self.dialog_error(t!("app.disks.dialog.pick_image"), cx);
        };
        let Some(image_size) = dialog.image_size else {
          return self.dialog_error(t!("app.disks.dialog.image_size_unknown"), cx);
        };
        let target_size = target.as_ref().map_or(0, |t| t.size);
        if image_size > target_size {
          return self.dialog_error(
            t!(
              "app.disks.dialog.image_too_big",
              image = size(image_size),
              target = size(target_size)
            ),
            cx,
          );
        }
        let open = udisks.open_for_restore(&path);
        let job = title("app.disks.restore");
        self.close_dialog(cx);
        cx.spawn(async move |_, cx| {
          let fd = open.await;
          cx.update(|cx| match fd {
            Ok(fd) => {
              DisksLog::spawn(
                cx,
                job,
                objects,
                image_size,
                move |progress| {
                  fd::restore(&image, File::from(fd), &progress)?;
                  Ok(image.display().to_string())
                },
                |_, _, _| {},
              );
            }
            Err(e) => jobs::log(cx).clone().fail(job, objects, &e, None, cx),
          })
        })
        .detach();
      }
      DialogKind::Benchmark => {
        let write = dialog.check("write") && !self.block_in_use(&path, cx);
        let total = target.as_ref().map_or(0, |t| t.size);
        let open = udisks.open_for_benchmark(&path, write);
        let job = title("app.disks.benchmark");
        if let Some(d) = &mut self.dialog {
          d.busy = true;
          d.bench = None;
          d.error = None;
        }
        let panel = cx.entity().downgrade();
        // the thread leaves the numbers here, the dialog reads them once it is done
        let slot: Arc<Mutex<Option<Bench>>> = Arc::default();
        cx.spawn(async move |_, cx| {
          let fd = open.await;
          cx.update(|cx| match fd {
            Ok(fd) => {
              let id = DisksLog::spawn(
                cx,
                job,
                objects,
                fd::SAMPLES,
                {
                  let slot = slot.clone();
                  move |progress| {
                    let bench = fd::benchmark(&File::from(fd), total, write, &progress)?;
                    let text = bench_text(&bench);
                    *slot.lock().unwrap() = Some(bench);
                    Ok(text)
                  }
                },
                {
                  let panel = panel.clone();
                  move |id, result, cx| {
                    panel
                      .update(cx, |this, cx| {
                        // only the dialog that started it, not one opened since
                        if let Some(d) = &mut this.dialog
                          && d.job == Some(id)
                        {
                          d.busy = false;
                          d.job = None;
                          d.bench = slot.lock().unwrap().take();
                          if let Err(e) = result {
                            d.error = Some(format!("{e:#}"));
                          }
                        }
                        cx.notify();
                      })
                      .ok();
                  }
                },
              );
              panel
                .update(cx, |this, _| {
                  if let Some(d) = &mut this.dialog
                    && d.kind == DialogKind::Benchmark
                    && d.busy
                  {
                    d.job = Some(id);
                  }
                })
                .ok();
            }
            Err(e) => {
              let path = path.clone();
              panel
                .update(cx, |this, cx| {
                  // only the dialog that started it, not one opened since
                  if matches!(&this.dialog, Some(d) if d.kind == DialogKind::Benchmark
                    && d.busy && d.block.as_ref() == Some(&path))
                  {
                    this.dialog_error(format!("{e:#}"), cx)
                  }
                })
                .ok();
              jobs::log(cx).clone().fail(job, objects, &e, None, cx);
            }
          })
        })
        .detach();
        cx.notify();
      }
      DialogKind::LoopSetup => {
        let Some(image) = dialog.path.clone() else {
          return self.dialog_error(t!("app.disks.dialog.pick_image"), cx);
        };
        let read_only = dialog.check("read_only");
        let autoclear = dialog.check("autoclear");
        let job_name = image
          .file_name()
          .map(|f| f.to_string_lossy().into_owned())
          .unwrap_or_default();
        let start: Start = Rc::new(move |cx| {
          let udisks = cx.udisks().clone();
          let image = image.clone();
          // a FIFO or stalled mount would block the UI thread
          let open = cx.background_executor().spawn({
            let image = image.clone();
            async move { File::options().read(true).write(!read_only).open(&image) }
          });
          Box::pin(async move {
            let file = open
              .await
              .with_context(|| format!("opening {}", image.display()))?;
            let options = LoopSetupOptions { read_only };
            let device = udisks.loop_setup(file.into(), options).await?;
            if autoclear {
              // attached already, a retry would attach the image again
              if let Err(e) = udisks.loop_set_autoclear(&device, true).await {
                tracing::warn!("Failed to set autoclear on {device}: {e:#}");
              }
            }
            Ok(())
          })
        });
        self.act(
          format!("{} · {job_name}", t!("app.disks.dialog.loop_setup_title")),
          vec![],
          start,
          cx,
        );
        self.close_dialog(cx);
      }
      DialogKind::CheckResult { .. } | DialogKind::Ejected { .. } => self.close_dialog(cx),
    }
  }

  fn pick(&mut self, new: bool, cx: &mut Context<Self>) {
    let Some(dialog) = &self.dialog else { return };
    let current = dialog.path.clone();
    let picked = match new {
      true => {
        let dir = current
          .as_ref()
          .and_then(|p| p.parent())
          .map(Path::to_path_buf)
          .unwrap_or_else(home);
        let name = current
          .as_ref()
          .and_then(|p| p.file_name())
          .map(|f| f.to_string_lossy().into_owned());
        let receiver = cx.prompt_for_new_path(&dir, name.as_deref());
        Box::pin(async move { receiver.await.ok()?.ok()? })
          as futures::future::LocalBoxFuture<'static, Option<PathBuf>>
      }
      false => {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
          files: true,
          directories: false,
          multiple: false,
          prompt: Some(t!("app.disks.dialog.choose").into()),
        });
        Box::pin(async move { receiver.await.ok()?.ok()??.into_iter().next() })
      }
    };
    cx.spawn(async move |this, cx| {
      let Some(path) = picked.await else { return };
      let probe = path.clone();
      let image_size = cx
        .background_executor()
        .spawn(async move { fd::image_size(&probe).ok().flatten() })
        .await;
      this
        .update(cx, |this, cx| {
          if let Some(dialog) = &mut this.dialog {
            if dialog.kind == DialogKind::Restore {
              dialog.image_size = image_size;
            }
            dialog.path = Some(path);
            dialog.error = None;
          }
          cx.notify();
        })
        .ok();
    })
    .detach();
  }

  pub(super) fn dialog_view(
    &mut self,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<AnyElement> {
    let dialog = self.dialog.as_ref()?;
    let theme = cx.theme().clone();
    let udisks = cx.udisks();
    let target = dialog
      .block
      .as_ref()
      .and_then(|b| udisks.block(b, cx))
      .cloned();
    let device = target
      .as_ref()
      .map(|b| b.device.clone())
      .unwrap_or_default();
    let name = target
      .as_ref()
      .map(|b| b.name().to_string())
      .unwrap_or_default();
    let target_size = target.as_ref().map_or(0, |b| b.size);
    let id = dialog.kind.id();
    // writing back over what is mounted reverts what is written meanwhile
    let system_target = dialog
      .block
      .as_ref()
      .is_some_and(|b| self.block_in_use(b, cx));
    let field_label = |text: SharedString| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(text)
    };
    let mut fields: Vec<AnyElement> = Vec::new();
    let text_field = |key: &str, label: SharedString, fields: &mut Vec<AnyElement>| {
      if let Some(state) = dialog.inputs.get(key) {
        fields.push(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .child(field_label(label))
            .child(
              Input::new(state)
                .small()
                .when(MASKED.contains(&key), |i| i.mask_toggle()),
            )
            .into_any_element(),
        );
      }
    };
    let select_field = |key: &str, label: SharedString, fields: &mut Vec<AnyElement>| {
      if let Some((state, _)) = dialog.selects.get(key) {
        fields.push(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .child(field_label(label))
            .child(Select::new(state).small())
            .into_any_element(),
        );
      }
    };
    let check_field =
      |key: &'static str, label: SharedString, cx: &Context<Self>, fields: &mut Vec<AnyElement>| {
        fields.push(
          Checkbox::new(gpui_kit::ElementId::Name(
            format!("disks-dialog-check-{key}").replace('.', "-").into(),
          ))
          .checked(dialog.check(key))
          .label(label)
          .small()
          .on_click(cx.listener(move |this, on: &bool, _, cx| {
            if let Some(d) = &mut this.dialog {
              d.checks.insert(key, *on);
            }
            cx.notify();
          }))
          .into_any_element(),
        );
      };
    let path_field =
      |label: SharedString, new: bool, cx: &Context<Self>, fields: &mut Vec<AnyElement>| {
        let shown = dialog
          .path
          .as_ref()
          .map(|p| p.display().to_string())
          .unwrap_or_else(|| t!("app.disks.dialog.none_chosen").into());
        fields.push(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .child(field_label(label))
            .child(
              div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w_0().text_sm().truncate().child(shown))
                .child(
                  Button::new("disks-dialog-pick")
                    .label(t!("app.disks.dialog.choose"))
                    .icon(IconName::Folder)
                    .xsmall()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.pick(new, cx))),
                ),
            )
            .into_any_element(),
        );
      };

    let (icon, title, description, warning, ok): (
      IconName,
      String,
      String,
      Option<String>,
      Option<String>,
    ) = match &dialog.kind {
      DialogKind::Format => {
        select_field("fs", t!("app.disks.dialog.filesystem").into(), &mut fields);
        text_field("label", t!("app.disks.label").into(), &mut fields);
        select_field("erase", t!("app.disks.dialog.erase").into(), &mut fields);
        check_field(
          "encrypt",
          t!("app.disks.dialog.encrypt").into(),
          cx,
          &mut fields,
        );
        if dialog.check("encrypt") {
          text_field(
            "passphrase",
            t!("app.disks.dialog.passphrase").into(),
            &mut fields,
          );
          text_field(
            "confirm",
            t!("app.disks.dialog.confirm_passphrase").into(),
            &mut fields,
          );
        }
        check_field(
          "no_discard",
          t!("app.disks.dialog.no_discard").into(),
          cx,
          &mut fields,
        );
        let fs = dialog.choice("fs", cx);
        let supported = self.caps.get(&fs).is_none_or(|c| c.format);
        if !supported {
          fields.push(
            div()
              .text_xs()
              .text_color(cx.theme().colors.warning)
              .child(t!("app.disks.dialog.unsupported", fs = fs_name(&fs)))
              .into_any_element(),
          );
        }
        (
          IconName::HardDrive,
          t!("app.disks.dialog.format_title", name = name.clone()).into(),
          t!("app.disks.dialog.format_desc").into(),
          Some(
            t!(
              "app.disks.dialog.erased",
              device = device.clone(),
              size = size(target_size)
            )
            .into(),
          ),
          Some(t!("app.disks.format").into()),
        )
      }
      DialogKind::Unlock => {
        text_field(
          "passphrase",
          t!("app.disks.dialog.passphrase").into(),
          &mut fields,
        );
        (
          IconName::LockOpen,
          t!("app.disks.dialog.unlock_title", name = name.clone()).into(),
          t!("app.disks.dialog.unlock_desc", device = device.clone()).into(),
          None,
          Some(match dialog.error.is_some() {
            true => t!("app.disks.dialog.try_again").into(),
            false => t!("app.disks.unlock").into(),
          }),
        )
      }
      DialogKind::Label(target_kind) => {
        let (title, label) = match target_kind {
          LabelTarget::PartitionName => (
            t!("app.disks.dialog.name_title"),
            t!("app.disks.partition_name"),
          ),
          _ => (t!("app.disks.dialog.label_title"), t!("app.disks.label")),
        };
        text_field("label", label.into(), &mut fields);
        (
          IconName::Pencil,
          title.into(),
          device.clone(),
          None,
          Some(t!("app.disks.rename").into()),
        )
      }
      DialogKind::Resize(resize) => {
        if let Some(slider) = &dialog.slider {
          let state = slider.read(cx);
          let (min, max) = (
            state.min_value() as u64 * MIB,
            state.max_value() as u64 * MIB,
          );
          fields.push(
            div()
              .flex()
              .flex_col()
              .gap_1()
              .child(field_label(t!("app.disks.dialog.new_size").into()))
              .child(div().text_sm().child(size(dialog.slider_value * MIB)))
              .child(Slider::new(slider))
              .child(
                div()
                  .flex()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .child(
                    div()
                      .flex_1()
                      .child(t!("app.disks.dialog.min", size = size(min))),
                  )
                  .child(t!("app.disks.dialog.max", size = size(max))),
              )
              .into_any_element(),
          );
        }
        if *resize == ResizeTarget::Filesystem
          && target.as_ref().is_some_and(|t| t.partition.is_some())
        {
          check_field(
            "partition_too",
            t!("app.disks.dialog.partition_too").into(),
            cx,
            &mut fields,
          );
        }
        if *resize == ResizeTarget::Encrypted {
          text_field(
            "passphrase",
            t!("app.disks.dialog.passphrase").into(),
            &mut fields,
          );
        }
        (
          IconName::Scaling,
          t!("app.disks.dialog.resize_title", name = name.clone()).into(),
          device.clone(),
          None,
          Some(t!("app.disks.resize").into()),
        )
      }
      DialogKind::CreatePartition => {
        select_field("gap", t!("app.disks.dialog.free_space").into(), &mut fields);
        if let Some(slider) = &dialog.slider {
          fields.push(
            div()
              .flex()
              .flex_col()
              .gap_1()
              .child(field_label(t!("app.disks.size").into()))
              .child(div().text_sm().child(size(dialog.slider_value * MIB)))
              .child(Slider::new(slider))
              .into_any_element(),
          );
        }
        select_field("type", t!("app.disks.partition_type").into(), &mut fields);
        text_field("name", t!("app.disks.partition_name").into(), &mut fields);
        select_field(
          "format",
          t!("app.disks.dialog.format_as").into(),
          &mut fields,
        );
        text_field("fs_label", t!("app.disks.label").into(), &mut fields);
        (
          IconName::Plus,
          t!("app.disks.create_partition").into(),
          device.clone(),
          None,
          Some(t!("app.disks.dialog.create").into()),
        )
      }
      DialogKind::PartitionType => {
        select_field("type", t!("app.disks.partition_type").into(), &mut fields);
        (
          IconName::Tag,
          t!("app.disks.dialog.type_title").into(),
          device.clone(),
          None,
          Some(t!("app.disks.dialog.apply").into()),
        )
      }
      DialogKind::Flags => {
        let table = target.as_ref().map(|t| table_of(t, cx)).unwrap_or_default();
        for flag in flags_of(&table) {
          check_field(flag.key, t!(flag.key).into(), cx, &mut fields);
        }
        (
          IconName::Tag,
          t!("app.disks.dialog.flags_title").into(),
          device.clone(),
          None,
          Some(t!("app.disks.dialog.apply").into()),
        )
      }
      DialogKind::ChangePassphrase => {
        text_field(
          "current",
          t!("app.disks.dialog.current_passphrase").into(),
          &mut fields,
        );
        text_field(
          "new",
          t!("app.disks.dialog.new_passphrase").into(),
          &mut fields,
        );
        text_field(
          "confirm",
          t!("app.disks.dialog.confirm_passphrase").into(),
          &mut fields,
        );
        (
          IconName::KeyRound,
          t!("app.disks.change_passphrase").into(),
          device.clone(),
          None,
          Some(t!("app.disks.dialog.change").into()),
        )
      }
      DialogKind::Backup => {
        path_field(t!("app.disks.dialog.save_to").into(), true, cx, &mut fields);
        fields.push(kv(t!("app.disks.size"), size(target_size), None, cx));
        check_field(
          "compress",
          t!("app.disks.dialog.compress").into(),
          cx,
          &mut fields,
        );
        (
          IconName::Download,
          t!("app.disks.dialog.backup_title").into(),
          t!("app.disks.dialog.backup_desc", device = device.clone()).into(),
          None,
          Some(t!("app.disks.dialog.start_backup").into()),
        )
      }
      DialogKind::Restore => {
        path_field(t!("app.disks.dialog.image").into(), false, cx, &mut fields);
        if dialog.path.is_some() {
          let image_size = dialog.image_size.map_or_else(|| "?".into(), size);
          fields.push(kv(t!("app.disks.dialog.image_size"), image_size, None, cx));
        }
        fields.push(kv(
          t!("app.disks.dialog.target"),
          size(target_size),
          None,
          cx,
        ));
        (
          IconName::Upload,
          t!("app.disks.dialog.restore_title").into(),
          t!("app.disks.dialog.restore_desc", device = device.clone()).into(),
          Some(
            t!(
              "app.disks.dialog.replaced",
              device = device.clone(),
              size = size(target_size)
            )
            .into(),
          ),
          Some(t!("app.disks.restore").into()),
        )
      }
      DialogKind::Benchmark => {
        let running = log_progress(dialog, cx);
        if let Some(bench) = &dialog.bench {
          fields.push(kv(
            t!("app.disks.dialog.read_speed"),
            rate(bench.read),
            Some(cx.theme().colors.success),
            cx,
          ));
          if let Some(write) = bench.write {
            fields.push(kv(
              t!("app.disks.dialog.write_speed"),
              rate(write),
              Some(cx.theme().colors.success),
              cx,
            ));
          }
          fields.push(kv(
            t!("app.disks.dialog.access_time"),
            format!("{:.2} ms", bench.access.as_secs_f64() * 1000.),
            None,
            cx,
          ));
        } else if let Some(running) = running {
          fields.push(kv(t!("app.disks.jobs.running"), running, None, cx));
        }
        if !system_target {
          check_field(
            "write",
            t!("app.disks.dialog.write_test").into(),
            cx,
            &mut fields,
          );
        }
        (
          IconName::Gauge,
          t!("app.disks.dialog.benchmark_title", name = name.clone()).into(),
          t!("app.disks.dialog.benchmark_desc").into(),
          (dialog.check("write") && !system_target)
            .then(|| t!("app.disks.dialog.write_warning", device = device.clone()).into()),
          Some(t!("app.disks.start").into()),
        )
      }
      DialogKind::DeletePartition => {
        fields.push(kv(t!("app.disks.partition"), device.clone(), None, cx));
        fields.push(kv(t!("app.disks.size"), size(target_size), None, cx));
        (
          IconName::Trash,
          t!("app.disks.dialog.delete_title").into(),
          device.clone(),
          Some(
            t!(
              "app.disks.dialog.removed",
              device = device.clone(),
              size = size(target_size)
            )
            .into(),
          ),
          Some(t!("app.disks.delete").into()),
        )
      }
      DialogKind::CheckResult {
        repair,
        clean,
        secs,
      } => {
        let (result, color) = match (repair, clean) {
          (false, true) => (t!("app.disks.dialog.no_errors"), cx.theme().colors.success),
          (false, false) => (
            t!("app.disks.dialog.errors_found"),
            cx.theme().colors.warning,
          ),
          (true, true) => (t!("app.disks.dialog.repaired"), cx.theme().colors.success),
          (true, false) => (
            t!("app.disks.dialog.not_repaired"),
            cx.theme().colors.warning,
          ),
        };
        fields.push(kv(t!("app.disks.dialog.result"), result, Some(color), cx));
        fields.push(kv(
          t!("app.disks.dialog.duration"),
          duration(*secs),
          None,
          cx,
        ));
        let title = match clean {
          true => t!("app.disks.dialog.clean_title"),
          false => t!("app.disks.dialog.dirty_title"),
        };
        (IconName::Check, title.into(), device.clone(), None, None)
      }
      DialogKind::LoopSetup => {
        path_field(t!("app.disks.dialog.image").into(), false, cx, &mut fields);
        check_field(
          "read_only",
          t!("app.disks.read_only").into(),
          cx,
          &mut fields,
        );
        check_field(
          "autoclear",
          t!("app.disks.dialog.autoclear").into(),
          cx,
          &mut fields,
        );
        (
          IconName::Layers,
          t!("app.disks.dialog.loop_setup_title").into(),
          t!("app.disks.dialog.loop_setup_desc").into(),
          None,
          Some(t!("app.disks.dialog.attach").into()),
        )
      }
      DialogKind::Ejected { name, powered_off } => {
        fields.push(kv(
          t!("app.disks.dialog.status"),
          match powered_off {
            true => t!("app.disks.dialog.powered_off"),
            false => t!("app.disks.dialog.media_ejected"),
          },
          Some(cx.theme().colors.success),
          cx,
        ));
        (
          IconName::Eject,
          t!("app.disks.dialog.safe_title").into(),
          t!("app.disks.dialog.safe_desc", name = name.clone()).into(),
          None,
          None,
        )
      }
    };
    let danger = dialog.kind.danger();
    let busy = dialog.busy;
    let error = dialog.error.clone();
    let cancel_label = match ok {
      Some(_) => t!("app.common.cancel"),
      None => t!("app.disks.dialog.close"),
    };
    Some(
      modal::Dialog::new(
        format!("disks-dialog-{id}"),
        title,
        cx.listener(|this, _, _, cx| this.close_dialog(cx)),
      )
      .icon(icon)
      .message(description)
      .when_some(warning, |d, warning| {
        d.child(
          div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .rounded_md()
            .bg(theme.colors.danger.opacity(0.15))
            .text_xs()
            .text_color(theme.colors.danger)
            .child(Icon::new(IconName::TriangleAlert).small())
            .child(warning),
        )
      })
      .children(fields)
      .error(error)
      .cancel_label(cancel_label)
      .when_some(ok, |d, ok| {
        d.confirm(
          ok,
          danger,
          busy,
          cx.listener(|this, _, window, cx| this.confirm(window, cx)),
        )
      })
      .into_any_element(),
    )
  }
}

/// The benchmark job's progress, while it runs
fn log_progress(dialog: &Dialog, cx: &App) -> Option<String> {
  let id = dialog.job?;
  let jobs = jobs::log(cx).running.read(cx);
  let job = jobs.iter().find(|j| j.id == id)?;
  Some(format!(
    "{} / {}",
    job.progress.done.load(Ordering::Relaxed),
    job.total
  ))
}

/// udisks' answer to a wrong passphrase, said shorter
fn unlock_error(e: &anyhow::Error) -> String {
  let text = format!("{e:#}");
  match text.contains("No key available") || text.contains("passphrase") {
    true => t!("app.disks.dialog.wrong_passphrase").into(),
    false => text,
  }
}

fn rate(bytes_per_sec: f64) -> String {
  format!("{}/s", size(bytes_per_sec as u64))
}

fn duration(secs: u64) -> String {
  match secs {
    s if s >= 60 => t!("app.disks.dialog.min_sec", min = s / 60, sec = s % 60).into(),
    s => t!("app.disks.dialog.sec", sec = s).into(),
  }
}

/// The benchmark's numbers as the Finished entry shows them
fn bench_text(bench: &Bench) -> String {
  let mut text = format!("{} {}", t!("app.disks.dialog.read"), rate(bench.read));
  if let Some(write) = bench.write {
    text = format!("{text} · {} {}", t!("app.disks.dialog.write"), rate(write));
  }
  format!("{text} · {:.2} ms", bench.access.as_secs_f64() * 1000.)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rates_and_durations() {
    assert_eq!(rate(3_420_000_000.), "3.4 GB/s");
    assert_eq!(duration(134), "2 min 14 s");
    assert_eq!(duration(9), "9 s");
  }

  #[test]
  fn dialog_ids_and_danger() {
    assert!(DialogKind::Format.danger() && DialogKind::Restore.danger());
    assert!(!DialogKind::Unlock.danger());
    assert_eq!(DialogKind::CreatePartition.id(), "create-part");
  }
}
