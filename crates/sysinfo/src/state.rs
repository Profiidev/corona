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
    // a reading that is missing this once repeats the last one, so the series stay
    // aligned with the others; one that never came keeps its series empty
    fn push_or_repeat<T: Copy>(queue: &mut VecDeque<T>, value: Option<T>, capacity: usize) {
      if let Some(value) = value.or_else(|| queue.back().copied()) {
        push(queue, value, capacity);
      }
    }
    push(&mut self.cpu, sample.cpu, capacity);
    push_or_repeat(&mut self.cpu_temperature, sample.cpu_temperature, capacity);
    let memory = match sample.memory_total {
      0 => 0.,
      total => sample.memory_used as f32 / total as f32 * 100.,
    };
    push(&mut self.memory, memory, capacity);
    push(&mut self.network_rx, sample.network_rx, capacity);
    push(&mut self.network_tx, sample.network_tx, capacity);
    let gpu = sample.gpus.first();
    push_or_repeat(&mut self.gpu, gpu.and_then(|g| g.usage), capacity);
    let gpu_memory = gpu.and_then(|g| {
      let total = g.vram_total.filter(|t| *t > 0)?;
      Some(g.vram_used? as f32 / total as f32 * 100.)
    });
    push_or_repeat(&mut self.gpu_memory, gpu_memory, capacity);
    push_or_repeat(
      &mut self.gpu_temperature,
      gpu.and_then(|g| g.temperature),
      capacity,
    );
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  pub(crate) fn sample() -> Sample {
    Sample {
      time: Instant::now(),
      cpu: 10.,
      cpu_cores: vec![10.],
      cpu_frequency: 1000,
      cpu_temperature: Some(40.),
      memory_used: 1,
      memory_total: 4,
      swap_used: 0,
      swap_total: 0,
      load: [0.; 3],
      network_rx: 1.,
      network_tx: 2.,
      disks: vec![],
      gpus: vec![GpuSample {
        pci: "a".into(),
        usage: Some(30.),
        temperature: Some(60.),
        vram_used: Some(2),
        vram_total: Some(8),
      }],
    }
  }

  #[test]
  fn history_records_every_series() {
    let mut history = History::default();
    history.push(&sample(), 60);
    assert_eq!(
      history,
      History {
        cpu: [10.].into(),
        cpu_temperature: [40.].into(),
        memory: [25.].into(),
        network_rx: [1.].into(),
        network_tx: [2.].into(),
        gpu: [30.].into(),
        gpu_memory: [25.].into(),
        gpu_temperature: [60.].into(),
      }
    );
  }

  #[test]
  fn history_keeps_the_newest() {
    let mut history = History::default();
    for cpu in 0..10 {
      history.push(
        &Sample {
          cpu: cpu as f32,
          ..sample()
        },
        3,
      );
    }
    assert_eq!(history.cpu, [7., 8., 9.]);
    assert_eq!(history.memory.len(), 3);
    assert_eq!(history.gpu_temperature.len(), 3);
    // capacity 0 keeps nothing
    let mut empty = History::default();
    empty.push(&sample(), 0);
    assert_eq!(empty, History::default());
  }

  #[test]
  fn history_skips_what_is_missing() {
    let mut history = History::default();
    let mut s = sample();
    s.memory_total = 0;
    s.cpu_temperature = None;
    s.gpus[0] = GpuSample {
      pci: "a".into(),
      usage: None,
      temperature: None,
      vram_used: Some(5),
      vram_total: Some(0),
    };
    // only the first GPU counts
    s.gpus.push(GpuSample {
      usage: Some(99.),
      ..sample().gpus[0].clone()
    });
    history.push(&s, 60);
    assert_eq!(history.memory, [0.]);
    assert!(history.cpu_temperature.is_empty());
    assert!(history.gpu.is_empty() && history.gpu_memory.is_empty());
    assert!(history.gpu_temperature.is_empty());

    let mut no_gpu = History::default();
    no_gpu.push(
      &Sample {
        gpus: vec![],
        ..sample()
      },
      60,
    );
    assert!(no_gpu.gpu.is_empty());
    assert_eq!(no_gpu.cpu.len(), 1);
  }

  #[test]
  fn series_stay_aligned() {
    let mut history = History::default();
    history.push(&sample(), 60);
    let gone = Sample {
      cpu_temperature: None,
      gpus: vec![],
      ..sample()
    };
    history.push(&gone, 60);
    history.push(&sample(), 60);
    assert_eq!(history.cpu_temperature, [40., 40., 40.]);
    for series in [&history.gpu, &history.gpu_memory, &history.gpu_temperature] {
      assert_eq!(series.len(), history.cpu.len());
    }
    assert_eq!(history.gpu, [30., 30., 30.]);
  }

  #[test]
  fn history_memory_used_exceeds_total() {
    let mut history = History::default();
    let mut s = sample();
    s.memory_used = 6;
    s.memory_total = 4;
    history.push(&s, 60);
    assert_eq!(history.memory, [150.]);
  }

  #[test]
  fn history_repeats_vram_when_vram_total_is_none_or_zero() {
    let mut history = History::default();
    // first sample with valid vram
    history.push(&sample(), 60);
    assert_eq!(history.gpu_memory, [25.]);

    // second sample with vram_used Some(10) but vram_total None
    let mut s = sample();
    s.gpus[0].vram_used = Some(10);
    s.gpus[0].vram_total = None;
    history.push(&s, 60);
    // repeats previous 25.
    assert_eq!(history.gpu_memory, [25., 25.]);

    // third sample with vram_total Some(0)
    let mut s2 = sample();
    s2.gpus[0].vram_used = Some(10);
    s2.gpus[0].vram_total = Some(0);
    history.push(&s2, 60);
    // repeats previous 25.
    assert_eq!(history.gpu_memory, [25., 25., 25.]);
  }

  #[test]
  fn history_ignores_secondary_gpus() {
    let mut history = History::default();
    let mut s = sample();
    s.gpus = vec![
      GpuSample {
        pci: "gpu0".into(),
        usage: Some(10.),
        temperature: Some(40.),
        vram_used: Some(1),
        vram_total: Some(4),
      },
      GpuSample {
        pci: "gpu1".into(),
        usage: Some(90.),
        temperature: Some(80.),
        vram_used: Some(3),
        vram_total: Some(4),
      },
    ];
    history.push(&s, 60);
    assert_eq!(history.gpu, [10.]);
    assert_eq!(history.gpu_temperature, [40.]);
    assert_eq!(history.gpu_memory, [25.]);
  }
}
