use std::{
  collections::VecDeque,
  time::{Instant, SystemTime},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuVendor {
  Nvidia,
  Amd,
  Intel,
  Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gpu {
  pub pci: String,
  pub name: String,
  pub vendor: GpuVendor,
  pub driver: Option<String>,
  pub measurable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemInfo {
  pub os: String,
  pub kernel: String,
  pub hostname: String,
  pub cpu_model: String,
  pub cpu_cores: usize,
  pub gpus: Vec<Gpu>,
  pub compositor: Option<String>,
  pub installed: Option<SystemTime>,
  pub booted: SystemTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Disk {
  pub device: String,
  pub mount_point: String,
  pub file_system: String,
  pub total: u64,
  pub available: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuSample {
  pub pci: String,
  pub usage: Option<f32>,
  pub temperature: Option<f32>,
  pub vram_used: Option<u64>,
  pub vram_total: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
  pub time: Instant,
  pub cpu: f32,
  pub cpu_cores: Vec<f32>,
  pub cpu_frequency: u64,
  pub cpu_temperature: Option<f32>,
  pub memory_used: u64,
  pub memory_total: u64,
  pub swap_used: u64,
  pub swap_total: u64,
  pub load: [f64; 3],
  pub network_rx: f64,
  pub network_tx: f64,
  pub disks: Vec<Disk>,
  pub gpus: Vec<GpuSample>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
  pub cpu: VecDeque<f32>,
  pub cpu_temperature: VecDeque<f32>,
  pub memory: VecDeque<f32>,
  pub network_rx: VecDeque<f64>,
  pub network_tx: VecDeque<f64>,
  pub gpu: VecDeque<f32>,
  pub gpu_memory: VecDeque<f32>,
  pub gpu_temperature: VecDeque<f32>,
}

impl History {
  pub(crate) fn push(&mut self, sample: &Sample, capacity: usize) {
    fn push<T>(queue: &mut VecDeque<T>, value: T, capacity: usize) {
      queue.push_back(value);
      while queue.len() > capacity {
        queue.pop_front();
      }
    }
    push(&mut self.cpu, sample.cpu, capacity);
    if let Some(temperature) = sample.cpu_temperature {
      push(&mut self.cpu_temperature, temperature, capacity);
    }
    let memory = match sample.memory_total {
      0 => 0.,
      total => sample.memory_used as f32 / total as f32 * 100.,
    };
    push(&mut self.memory, memory, capacity);
    push(&mut self.network_rx, sample.network_rx, capacity);
    push(&mut self.network_tx, sample.network_tx, capacity);
    if let Some(gpu) = sample.gpus.first() {
      if let Some(usage) = gpu.usage {
        push(&mut self.gpu, usage, capacity);
      }
      if let (Some(used), Some(total)) = (gpu.vram_used, gpu.vram_total.filter(|t| *t > 0)) {
        push(
          &mut self.gpu_memory,
          used as f32 / total as f32 * 100.,
          capacity,
        );
      }
      if let Some(temperature) = gpu.temperature {
        push(&mut self.gpu_temperature, temperature, capacity);
      }
    }
  }
}
