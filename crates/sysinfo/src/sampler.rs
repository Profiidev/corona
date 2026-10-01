//! One thread does all the measuring, as reading sysfs and NVML blocks.

use std::{
  env,
  ffi::OsStr,
  fs,
  path::Path,
  thread,
  time::{Duration, Instant, UNIX_EPOCH},
};

use nvml_wrapper::Nvml;
use sysinfo::{
  Components, CpuRefreshKind, Disks, MINIMUM_CPU_UPDATE_INTERVAL, MemoryRefreshKind, Networks,
  System,
};

use crate::{
  gpu::{self, PCI},
  state::{Disk, Gpu, GpuVendor, Sample, SystemInfo},
};

const NIXOS_NVML: &str = "/run/opengl-driver/lib/libnvidia-ml.so.1";

pub(crate) enum Update {
  Info(SystemInfo),
  Sample(Sample),
}

struct Sampler {
  system: System,
  networks: Networks,
  disks: Disks,
  components: Components,
  nvml: Option<Nvml>,
  gpus: Vec<Gpu>,
  last: Instant,
}

pub(crate) fn spawn(intervals: flume::Receiver<Option<Duration>>, updates: flume::Sender<Update>) {
  thread::spawn(move || {
    let mut sampler = Sampler::new();
    if updates.send(Update::Info(sampler.info())).is_err() {
      return;
    }

    let mut interval: Option<Duration> = None;
    loop {
      let next = match interval {
        Some(every) => intervals.recv_timeout(every),
        None => intervals
          .recv()
          .map_err(|_| flume::RecvTimeoutError::Disconnected),
      };
      match next {
        Ok(new) => {
          let resumed = interval.is_none() && new.is_some();
          interval = new.map(|every| every.max(MINIMUM_CPU_UPDATE_INTERVAL));
          if resumed {
            sampler.baseline();
          } else {
            continue;
          }
        }
        Err(flume::RecvTimeoutError::Timeout) => {}
        Err(flume::RecvTimeoutError::Disconnected) => return,
      }
      if updates.send(Update::Sample(sampler.sample())).is_err() {
        return;
      }
    }
  });
}

impl Sampler {
  fn new() -> Self {
    let nvml = Nvml::init()
      .or_else(|_| Nvml::builder().lib_path(OsStr::new(NIXOS_NVML)).init())
      .inspect_err(|e| tracing::debug!("no NVML: {e}"))
      .ok();
    let gpus = gpu::list(Path::new(PCI), nvml.as_ref());
    let nvml = nvml.filter(|_| gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia));
    Self {
      system: System::new(),
      networks: Networks::new_with_refreshed_list(),
      disks: Disks::new_with_refreshed_list(),
      components: Components::new_with_refreshed_list(),
      nvml,
      gpus,
      last: Instant::now(),
    }
  }

  fn info(&mut self) -> SystemInfo {
    self
      .system
      .refresh_cpu_list(CpuRefreshKind::nothing().with_frequency());
    let cpus = self.system.cpus();
    SystemInfo {
      os: pretty_name(&fs::read_to_string("/etc/os-release").unwrap_or_default())
        .or_else(|| Some(format!("{} {}", System::name()?, System::os_version()?)))
        .unwrap_or_else(|| "Linux".into()),
      kernel: System::kernel_version().unwrap_or_default(),
      hostname: System::host_name().unwrap_or_default(),
      cpu_model: cpus
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .unwrap_or_default(),
      cpu_cores: cpus.len(),
      gpus: self.gpus.clone(),
      compositor: env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .filter(|desktop| !desktop.is_empty()),
      installed: fs::metadata("/etc/machine-id")
        .and_then(|m| m.modified())
        .ok(),
      booted: UNIX_EPOCH + Duration::from_secs(System::boot_time()),
    }
  }

  fn baseline(&mut self) {
    self.system.refresh_cpu_usage();
    self.networks.refresh(true);
    self.last = Instant::now();
    thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
  }

  fn sample(&mut self) -> Sample {
    let now = Instant::now();
    let elapsed = now.duration_since(self.last).as_secs_f64().max(0.001);
    self.last = now;

    self
      .system
      .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage().with_frequency());
    self
      .system
      .refresh_memory_specifics(MemoryRefreshKind::everything());
    self.networks.refresh(true);
    self.disks.refresh(true);
    self.components.refresh(true);

    let cpus = self.system.cpus();
    let (rx, tx) = self
      .networks
      .iter()
      .filter(|(name, _)| *name != "lo")
      .fold((0, 0), |(rx, tx), (_, data)| {
        (rx + data.received(), tx + data.transmitted())
      });
    let load = System::load_average();

    Sample {
      time: now,
      cpu: self.system.global_cpu_usage(),
      cpu_cores: cpus.iter().map(|cpu| cpu.cpu_usage()).collect(),
      cpu_frequency: match cpus.len() as u64 {
        0 => 0,
        count => cpus.iter().map(|cpu| cpu.frequency()).sum::<u64>() / count,
      },
      cpu_temperature: cpu_temperature(&self.components),
      memory_used: self.system.used_memory(),
      memory_total: self.system.total_memory(),
      swap_used: self.system.used_swap(),
      swap_total: self.system.total_swap(),
      load: [load.one, load.five, load.fifteen],
      network_rx: rx as f64 / elapsed,
      network_tx: tx as f64 / elapsed,
      disks: disks(&self.disks),
      gpus: self
        .gpus
        .iter()
        .filter(|gpu| gpu.measurable)
        .map(|gpu| gpu::sample(Path::new(PCI), gpu, self.nvml.as_ref()))
        .collect(),
    }
  }
}

fn pretty_name(os_release: &str) -> Option<String> {
  os_release
    .lines()
    .find_map(|line| line.strip_prefix("PRETTY_NAME="))
    .map(|value| value.trim_matches('"').to_string())
    .filter(|name| !name.is_empty())
}

fn disks(disks: &Disks) -> Vec<Disk> {
  let mut all: Vec<Disk> = disks
    .iter()
    .map(|disk| Disk {
      device: disk.name().to_string_lossy().into(),
      mount_point: disk.mount_point().to_string_lossy().into(),
      file_system: disk.file_system().to_string_lossy().into(),
      total: disk.total_space(),
      available: disk.available_space(),
    })
    .collect();
  all.sort_by_key(|disk| disk.mount_point.len());
  let mut seen = std::collections::HashSet::new();
  all.retain(|disk| seen.insert(disk.device.clone()));
  all.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
  all
}

fn cpu_temperature(components: &Components) -> Option<f32> {
  let readings: Vec<(String, f32)> = components
    .iter()
    .filter_map(|c| Some((c.label().to_lowercase(), c.temperature()?)))
    .collect();
  let find = |words: &[&str]| {
    readings
      .iter()
      .filter(|(label, _)| words.iter().any(|word| label.contains(word)))
      .map(|(_, temperature)| *temperature)
      .reduce(f32::max)
  };
  find(&["package id", "tctl", "tdie"]).or_else(|| find(&["coretemp", "k10temp", "cpu"]))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn os_release() {
    let file = "NAME=NixOS\nPRETTY_NAME=\"NixOS 26.11 (Zokor)\"\nVERSION_ID=\"26.11\"\n";
    assert_eq!(pretty_name(file).as_deref(), Some("NixOS 26.11 (Zokor)"));
    assert_eq!(pretty_name("NAME=NixOS\n"), None);
  }

  /// this machine's numbers: `cargo test -p corona_sysinfo -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn live_sample() {
    let mut sampler = Sampler::new();
    println!("{:#?}", sampler.info());
    sampler.baseline();
    thread::sleep(Duration::from_secs(1));
    let sample = sampler.sample();
    println!(
      "cpu {:.1}% {} MHz {:?}°C, memory {}/{} MiB, swap {}/{} MiB, load {:?}",
      sample.cpu,
      sample.cpu_frequency,
      sample.cpu_temperature,
      sample.memory_used >> 20,
      sample.memory_total >> 20,
      sample.swap_used >> 20,
      sample.swap_total >> 20,
      sample.load
    );
    println!(
      "network rx {:.1} kB/s tx {:.1} kB/s",
      sample.network_rx / 1000.,
      sample.network_tx / 1000.
    );
    println!("{:#?}\n{:#?}", sample.disks, sample.gpus);
  }
}
