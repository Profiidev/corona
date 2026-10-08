use std::time::{SystemTime, UNIX_EPOCH};

use corona_sysinfo::{self as si, SystemMonitorExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::Module,
  module::{Subscribe, Subscriptions, read},
};

fn unix_seconds(time: SystemTime) -> f64 {
  time
    .duration_since(UNIX_EPOCH)
    .map_or(0., |d| d.as_secs_f64())
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum GpuVendor {
  Nvidia,
  Amd,
  Intel,
  Other,
}

#[derive(Serialize, TS)]
struct Gpu {
  /// PCI address, matches `GpuSample.pci`.
  pci: String,
  name: String,
  vendor: GpuVendor,
  driver: Option<String>,
  /// Whether samples carry its usage.
  measurable: bool,
}

#[derive(Serialize, TS)]
struct SystemInfo {
  os: String,
  kernel: String,
  hostname: String,
  cpu_model: String,
  cpu_cores: usize,
  gpus: Vec<Gpu>,
  compositor: Option<String>,
  /// Unix time in seconds.
  installed: Option<f64>,
  /// Unix time in seconds.
  booted: f64,
}

impl From<&si::SystemInfo> for SystemInfo {
  fn from(i: &si::SystemInfo) -> Self {
    Self {
      os: i.os.clone(),
      kernel: i.kernel.clone(),
      hostname: i.hostname.clone(),
      cpu_model: i.cpu_model.clone(),
      cpu_cores: i.cpu_cores,
      gpus: i
        .gpus
        .iter()
        .map(|g| Gpu {
          pci: g.pci.clone(),
          name: g.name.clone(),
          vendor: match g.vendor {
            si::GpuVendor::Nvidia => GpuVendor::Nvidia,
            si::GpuVendor::Amd => GpuVendor::Amd,
            si::GpuVendor::Intel => GpuVendor::Intel,
            si::GpuVendor::Other => GpuVendor::Other,
          },
          driver: g.driver.clone(),
          measurable: g.measurable,
        })
        .collect(),
      compositor: i.compositor.clone(),
      installed: i.installed.map(unix_seconds),
      booted: unix_seconds(i.booted),
    }
  }
}

#[derive(Serialize, TS)]
struct Disk {
  device: String,
  mount_point: String,
  file_system: String,
  /// In bytes.
  total: u64,
  /// In bytes.
  available: u64,
}

#[derive(Serialize, TS)]
struct GpuSample {
  pci: String,
  /// In percent.
  usage: Option<f32>,
  /// In °C.
  temperature: Option<f32>,
  /// In bytes.
  vram_used: Option<u64>,
  /// In bytes.
  vram_total: Option<u64>,
}

#[derive(Serialize, TS)]
struct Sample {
  /// In percent, all cores.
  cpu: f32,
  /// In percent, per core.
  cpu_cores: Vec<f32>,
  /// In MHz.
  cpu_frequency: u64,
  /// In °C.
  cpu_temperature: Option<f32>,
  /// In bytes.
  memory_used: u64,
  memory_total: u64,
  swap_used: u64,
  swap_total: u64,
  /// Over 1, 5 and 15 minutes.
  load: [f64; 3],
  /// In bytes per second.
  network_rx: f64,
  network_tx: f64,
  disks: Vec<Disk>,
  gpus: Vec<GpuSample>,
}

impl From<&si::Sample> for Sample {
  fn from(s: &si::Sample) -> Self {
    Self {
      cpu: s.cpu,
      cpu_cores: s.cpu_cores.clone(),
      cpu_frequency: s.cpu_frequency,
      cpu_temperature: s.cpu_temperature,
      memory_used: s.memory_used,
      memory_total: s.memory_total,
      swap_used: s.swap_used,
      swap_total: s.swap_total,
      load: s.load,
      network_rx: s.network_rx,
      network_tx: s.network_tx,
      disks: s
        .disks
        .iter()
        .map(|d| Disk {
          device: d.device.clone(),
          mount_point: d.mount_point.clone(),
          file_system: d.file_system.clone(),
          total: d.total,
          available: d.available,
        })
        .collect(),
      gpus: s
        .gpus
        .iter()
        .map(|g| GpuSample {
          pci: g.pci.clone(),
          usage: g.usage,
          temperature: g.temperature,
          vram_used: g.vram_used,
          vram_total: g.vram_total,
        })
        .collect(),
    }
  }
}

/// Recent samples, oldest first.
#[derive(Serialize, TS)]
struct History {
  /// In percent.
  cpu: Vec<f32>,
  /// In °C.
  cpu_temperature: Vec<f32>,
  /// In percent.
  memory: Vec<f32>,
  /// In bytes per second.
  network_rx: Vec<f64>,
  network_tx: Vec<f64>,
  /// In percent, the first GPU.
  gpu: Vec<f32>,
  gpu_memory: Vec<f32>,
  /// In °C.
  gpu_temperature: Vec<f32>,
}

impl From<&si::History> for History {
  fn from(h: &si::History) -> Self {
    Self {
      cpu: h.cpu.iter().copied().collect(),
      cpu_temperature: h.cpu_temperature.iter().copied().collect(),
      memory: h.memory.iter().copied().collect(),
      network_rx: h.network_rx.iter().copied().collect(),
      network_tx: h.network_tx.iter().copied().collect(),
      gpu: h.gpu.iter().copied().collect(),
      gpu_memory: h.gpu_memory.iter().copied().collect(),
      gpu_temperature: h.gpu_temperature.iter().copied().collect(),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Info,
  Sample,
  History,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Sysinfo(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let monitor = cx.system_monitor();

  Module::new("corona/sysinfo")
    .func(read(
      reads,
      subs,
      "info",
      Updates::Info,
      monitor.info.clone(),
      |cx| cx.system_monitor().info(cx).map(SystemInfo::from),
    ))
    .func(read(
      reads,
      subs,
      "sample",
      Updates::Sample,
      monitor.sample.clone(),
      // null until the first sample
      |cx| cx.system_monitor().sample(cx).map(Sample::from),
    ))
    .func(read(
      reads,
      subs,
      "history",
      Updates::History,
      monitor.history.clone(),
      |cx| History::from(cx.system_monitor().history(cx)),
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use std::{
    collections::VecDeque,
    time::{Duration, Instant, UNIX_EPOCH},
  };

  use corona_sysinfo as si;

  use super::{History, Sample, SystemInfo};

  fn gpu(vendor: si::GpuVendor) -> si::Gpu {
    si::Gpu {
      pci: "0000:01:00.0".into(),
      name: "GPU".into(),
      vendor,
      driver: None,
      measurable: true,
    }
  }

  #[test]
  fn converts_info() {
    let mut info = si::SystemInfo {
      os: "NixOS".into(),
      kernel: "7.2".into(),
      hostname: "host".into(),
      cpu_model: "CPU".into(),
      cpu_cores: 16,
      gpus: [
        si::GpuVendor::Nvidia,
        si::GpuVendor::Amd,
        si::GpuVendor::Intel,
        si::GpuVendor::Other,
      ]
      .map(gpu)
      .into(),
      compositor: Some("Hyprland".into()),
      installed: Some(UNIX_EPOCH + Duration::from_secs(5)),
      booted: UNIX_EPOCH + Duration::from_millis(1500),
    };
    let json = serde_json::to_value(SystemInfo::from(&info)).unwrap();
    let vendors: Vec<_> = (0..4).map(|i| json["gpus"][i]["vendor"].clone()).collect();
    assert_eq!(vendors, ["nvidia", "amd", "intel", "other"]);
    assert_eq!(json["gpus"][0]["measurable"], true);
    assert_eq!(json["cpu_cores"], 16);
    assert_eq!(json["installed"], 5.);
    assert_eq!(json["booted"], 1.5);

    info.installed = None;
    info.booted = UNIX_EPOCH - Duration::from_secs(1);
    let json = serde_json::to_value(SystemInfo::from(&info)).unwrap();
    assert!(json["installed"].is_null());
    assert_eq!(json["booted"], 0.);
  }

  #[test]
  fn converts_samples() {
    let sample = si::Sample {
      time: Instant::now(),
      cpu: 12.5,
      cpu_cores: vec![10., 15.],
      cpu_frequency: 3200,
      cpu_temperature: Some(48.),
      memory_used: 4 << 30,
      memory_total: 16 << 30,
      swap_used: 0,
      swap_total: 8 << 30,
      load: [0.5, 0.4, 0.3],
      network_rx: 1500.,
      network_tx: 200.,
      disks: vec![si::Disk {
        device: "/dev/nvme0n1p2".into(),
        mount_point: "/".into(),
        file_system: "btrfs".into(),
        total: 500 << 30,
        available: 200 << 30,
      }],
      gpus: vec![si::GpuSample {
        pci: "0000:01:00.0".into(),
        usage: Some(30.),
        temperature: None,
        vram_used: Some(1 << 30),
        vram_total: Some(8 << 30),
      }],
    };
    let json = serde_json::to_value(Sample::from(&sample)).unwrap();
    assert_eq!(json["gpus"][0]["usage"], 30.);
    assert!(json["gpus"][0]["temperature"].is_null());
    assert_eq!(json["gpus"][0]["vram_total"], 8u64 << 30);
    assert_eq!(json["memory_total"], 16u64 << 30);
    assert_eq!(json["load"][2], 0.3);
    assert_eq!(json["disks"][0]["mount_point"], "/");
  }

  #[test]
  fn history_keeps_order() {
    let history = si::History {
      cpu: VecDeque::from([1., 2., 3.]),
      cpu_temperature: VecDeque::from([40.]),
      memory: VecDeque::from([50.]),
      network_rx: VecDeque::from([1.]),
      network_tx: VecDeque::from([2.]),
      gpu: VecDeque::from([3.]),
      gpu_memory: VecDeque::from([4.]),
      gpu_temperature: VecDeque::from([5.]),
    };
    let converted = History::from(&history);
    assert_eq!(converted.cpu, [1., 2., 3.]);
    assert_eq!(converted.cpu_temperature, [40.]);
    assert_eq!(converted.memory, [50.]);
    assert_eq!(converted.network_rx, [1.]);
    assert_eq!(converted.network_tx, [2.]);
    assert_eq!(converted.gpu, [3.]);
    assert_eq!(converted.gpu_memory, [4.]);
    assert_eq!(converted.gpu_temperature, [5.]);
    assert!(History::from(&si::History::default()).cpu.is_empty());
  }
}
