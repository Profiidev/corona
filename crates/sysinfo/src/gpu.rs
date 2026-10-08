use std::{
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
};

use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};

use crate::state::{Gpu, GpuSample, GpuVendor};

pub(crate) const PCI: &str = "/sys/bus/pci/devices";
pub(crate) const PROC: &str = "/proc";

/// busy and total time per (device, DRM client, engine): cycles on xe, ns on i915
pub(crate) type Engines = HashMap<(String, String, String), (u64, u64)>;

fn read(path: impl AsRef<Path>) -> Option<String> {
  Some(fs::read_to_string(path).ok()?.trim().to_string())
}

/// display controllers, PCI class 0x03
pub(crate) fn list(pci_dir: &Path, nvml: Option<&Nvml>) -> Vec<Gpu> {
  let mut gpus: Vec<Gpu> = fs::read_dir(pci_dir)
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|device| {
      let path = device.path();
      read(path.join("class"))?
        .starts_with("0x03")
        .then_some(())?;
      let pci = device.file_name().into_string().ok()?;
      let vendor = match read(path.join("vendor"))?.as_str() {
        "0x10de" => GpuVendor::Nvidia,
        "0x1002" => GpuVendor::Amd,
        "0x8086" => GpuVendor::Intel,
        _ => GpuVendor::Other,
      };
      let driver = fs::read_link(path.join("driver"))
        .ok()
        .and_then(|driver| Some(driver.file_name()?.to_str()?.to_string()));
      let model = match vendor {
        GpuVendor::Nvidia => nvml.and_then(|nvml| nvidia_device(nvml, &pci)?.name().ok()),
        // amdgpu names some cards, most only have the generic id
        GpuVendor::Amd => read(path.join("product_name")).filter(|name| !name.is_empty()),
        _ => None,
      };
      let name = model.unwrap_or_else(|| {
        match vendor {
          GpuVendor::Nvidia => "NVIDIA GPU",
          GpuVendor::Amd => "AMD GPU",
          GpuVendor::Intel => "Intel GPU",
          GpuVendor::Other => "GPU",
        }
        .into()
      });
      let measurable = match vendor {
        GpuVendor::Nvidia => nvml.is_some_and(|nvml| nvidia_device(nvml, &pci).is_some()),
        GpuVendor::Amd => path.join("gpu_busy_percent").exists(),
        GpuVendor::Intel => matches!(driver.as_deref(), Some("xe" | "i915")),
        GpuVendor::Other => false,
      };
      Some(Gpu {
        pci,
        name,
        vendor,
        driver,
        measurable,
      })
    })
    .collect();
  gpus.sort_by(|a, b| a.pci.cmp(&b.pci));
  gpus
}

/// NVML spells the domain with 8 digits, sysfs with 4: compare from the bus on
fn same_pci(nvml: &str, sysfs: &str) -> bool {
  let tail = |address: &str| {
    address.rsplit_once(':').map(|(head, function)| {
      let bus = head.rsplit_once(':').map_or(head, |(_, bus)| bus);
      format!("{bus}:{function}").to_lowercase()
    })
  };
  tail(nvml).is_some() && tail(nvml) == tail(sysfs)
}

fn nvidia_device<'n>(nvml: &'n Nvml, pci: &str) -> Option<nvml_wrapper::Device<'n>> {
  (0..nvml.device_count().ok()?)
    .filter_map(|index| nvml.device_by_index(index).ok())
    .find(|device| {
      device
        .pci_info()
        .is_ok_and(|info| same_pci(&info.bus_id, pci))
    })
}

/// asking a runtime suspended GPU for anything wakes it up, and keeps it awake while sampled
fn suspended(device: &Path) -> bool {
  read(device.join("power/runtime_status")).is_some_and(|status| status == "suspended")
}

pub(crate) fn engines(proc_dir: &Path, now: u64) -> Engines {
  let mut engines = Engines::new();
  let fdinfos = fs::read_dir(proc_dir)
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|process| fs::read_dir(process.path().join("fdinfo")).ok())
    .flatten()
    .flatten();
  for fdinfo in fdinfos {
    let Some(text) = read(fdinfo.path()) else {
      continue;
    };
    let fields: HashMap<&str, &str> = text
      .lines()
      .filter_map(|line| line.split_once(':'))
      .map(|(key, value)| (key, value.trim()))
      .collect();
    let (Some(pdev), Some(client)) = (fields.get("drm-pdev"), fields.get("drm-client-id")) else {
      continue;
    };
    let number = |key: &str| fields.get(key)?.split(' ').next()?.parse::<u64>().ok();
    for key in fields.keys() {
      let (engine, busy, total) = if let Some(engine) = key.strip_prefix("drm-cycles-") {
        (
          engine,
          number(key),
          number(&format!("drm-total-cycles-{engine}")),
        )
      } else if let Some(engine) = key
        .strip_prefix("drm-engine-")
        .filter(|engine| !engine.starts_with("capacity-"))
      {
        (engine, number(key), Some(now))
      } else {
        continue;
      };
      let (Some(busy), Some(total)) = (busy, total) else {
        continue;
      };
      let capacity = number(&format!("drm-engine-capacity-{engine}")).unwrap_or(1);
      engines.insert(
        (pdev.to_string(), client.to_string(), engine.to_string()),
        (busy, total * capacity),
      );
    }
  }
  engines
}

fn drm_usage(pci: &str, previous: &Engines, current: &Engines) -> f32 {
  let mut per_engine: HashMap<&str, (u64, u64)> = HashMap::new();
  for (key @ (device, _, engine), (busy, total)) in current {
    let Some((previous_busy, previous_total)) = previous.get(key).filter(|_| device == pci) else {
      continue;
    };
    let entry = per_engine.entry(engine).or_default();
    entry.0 += busy.saturating_sub(*previous_busy);
    entry.1 = entry.1.max(total.saturating_sub(*previous_total));
  }
  per_engine
    .values()
    .filter(|(_, total)| *total > 0)
    .map(|(busy, total)| (*busy as f32 / *total as f32 * 100.).min(100.))
    .fold(0., f32::max)
}

pub(crate) fn sample(
  pci_dir: &Path,
  gpu: &Gpu,
  nvml: Option<&Nvml>,
  (previous, current): (&Engines, &Engines),
) -> GpuSample {
  let path = pci_dir.join(&gpu.pci);
  let mut sample = GpuSample {
    pci: gpu.pci.clone(),
    usage: None,
    temperature: None,
    vram_used: None,
    vram_total: None,
  };
  if suspended(&path) {
    return sample;
  }
  match gpu.vendor {
    GpuVendor::Nvidia => {
      if let Some(device) = nvml.and_then(|nvml| nvidia_device(nvml, &gpu.pci)) {
        sample.usage = device.utilization_rates().ok().map(|u| u.gpu as f32);
        sample.temperature = device
          .temperature(TemperatureSensor::Gpu)
          .ok()
          .map(|t| t as f32);
        if let Ok(memory) = device.memory_info() {
          sample.vram_used = Some(memory.used);
          sample.vram_total = Some(memory.total);
        }
      }
    }
    GpuVendor::Amd => {
      let number = |file: &str| read(path.join(file))?.parse::<u64>().ok();
      sample.usage = number("gpu_busy_percent").map(|u| u as f32);
      sample.vram_used = number("mem_info_vram_used");
      sample.vram_total = number("mem_info_vram_total");
      sample.temperature = hwmon_temperature(&path.join("hwmon"));
    }
    GpuVendor::Intel => sample.usage = Some(drm_usage(&gpu.pci, previous, current)),
    GpuVendor::Other => {}
  }
  sample
}

/// `temp1_input` of the device's hwmon, in millidegrees
fn hwmon_temperature(hwmon: &PathBuf) -> Option<f32> {
  fs::read_dir(hwmon)
    .ok()?
    .flatten()
    .find_map(|dir| read(dir.path().join("temp1_input"))?.parse::<f32>().ok())
    .map(|millidegrees| millidegrees / 1000.)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pci_addresses() {
    assert!(same_pci("00000000:01:00.0", "0000:01:00.0"));
    assert!(!same_pci("00000000:02:00.0", "0000:01:00.0"));
  }

  #[test]
  fn amd() {
    let root = std::env::temp_dir().join(format!("corona-gpu-{}", std::process::id()));
    fs::remove_dir_all(&root).ok();
    let device = root.join("0000:03:00.0");
    fs::create_dir_all(device.join("hwmon/hwmon7")).unwrap();
    for (file, value) in [
      ("class", "0x030000"),
      ("vendor", "0x1002"),
      ("gpu_busy_percent", "37"),
      ("mem_info_vram_used", "1073741824"),
      ("mem_info_vram_total", "8589934592"),
      ("hwmon/hwmon7/temp1_input", "52000"),
    ] {
      fs::write(device.join(file), value).unwrap();
    }
    // not a GPU
    fs::create_dir_all(root.join("0000:00:14.0")).unwrap();
    fs::write(root.join("0000:00:14.0/class"), "0x0c0330").unwrap();

    let gpus = list(&root, None);
    assert_eq!(gpus.len(), 1);
    assert_eq!(
      (gpus[0].name.as_str(), gpus[0].vendor),
      ("AMD GPU", GpuVendor::Amd)
    );
    assert!(gpus[0].measurable);
    let sample = sample(&root, &gpus[0], None, (&Engines::new(), &Engines::new()));
    assert_eq!(sample.usage, Some(37.));
    assert_eq!(sample.temperature, Some(52.));
    assert_eq!(sample.vram_total, Some(8589934592));
    fs::remove_dir_all(root).ok();
  }

  #[test]
  fn intel() {
    let root = std::env::temp_dir().join(format!("corona-drm-{}", std::process::id()));
    fs::remove_dir_all(&root).ok();
    let fdinfo = |pid: &str, fd: &str, text: &str| {
      fs::create_dir_all(root.join(pid).join("fdinfo")).unwrap();
      fs::write(root.join(pid).join("fdinfo").join(fd), text).unwrap();
    };
    let xe = |cycles: u64, total: u64| {
      format!(
        "pos:\t0\ndrm-driver:\txe\ndrm-client-id:\t219\ndrm-pdev:\t0000:00:02.0\n\
         drm-total-system:\t45420 KiB\ndrm-cycles-rcs:\t{cycles}\ndrm-total-cycles-rcs:\t{total}\n\
         drm-cycles-vcs:\t0\ndrm-total-cycles-vcs:\t{total}\n"
      )
    };
    let i915 = |ns: u64| {
      format!(
        "drm-driver:\ti915\ndrm-client-id:\t7\ndrm-pdev:\t0000:00:02.0\n\
         drm-engine-video:\t{ns} ns\ndrm-engine-capacity-video:\t2\n"
      )
    };
    // the same client open twice, and a non DRM fd
    fdinfo("100", "62", &xe(1000, 10_000));
    fdinfo("100", "63", &xe(1000, 10_000));
    fdinfo("100", "0", "pos:\t0\nflags:\t02\n");
    fdinfo("200", "5", &i915(0));
    let previous = engines(&root, 1_000);

    fdinfo("100", "62", &xe(3500, 20_000));
    fdinfo("100", "63", &xe(3500, 20_000));
    fdinfo("200", "5", &i915(1_500));
    let current = engines(&root, 2_000);
    // rcs: 2500 of 10000 cycles, video: 1500 ns of 2 × 1000 ns
    assert_eq!(drm_usage("0000:00:02.0", &previous, &current), 75.);
    assert_eq!(drm_usage("0000:03:00.0", &previous, &current), 0.);
    fs::remove_dir_all(root).ok();
  }

  fn write(root: &Path, files: &[(&str, &str)]) {
    for (file, value) in files {
      let path = root.join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, value).unwrap();
    }
  }

  fn gpu(pci: &str, vendor: GpuVendor) -> Gpu {
    Gpu {
      pci: pci.into(),
      name: String::new(),
      vendor,
      driver: None,
      measurable: true,
    }
  }

  fn engine(entries: &[(&str, &str, &str, u64, u64)]) -> Engines {
    entries
      .iter()
      .map(|(pdev, client, engine, busy, total)| {
        (
          (pdev.to_string(), client.to_string(), engine.to_string()),
          (*busy, *total),
        )
      })
      .collect()
  }

  #[test]
  fn pci_address_edges() {
    assert!(same_pci("00000000:01:00.0", "0000:01:00.0"));
    assert!(same_pci("00000000:0A:00.1", "0000:0a:00.1"));
    assert!(!same_pci("00000000:01:00.1", "0000:01:00.0"));
    // only bus and function count, the domain does not
    assert!(same_pci("00000001:01:00.0", "0000:01:00.0"));
    assert!(same_pci("01:00.0", "0000:01:00.0"));
    assert!(!same_pci("x", "x"));
    assert!(!same_pci("", ""));
    assert!(!same_pci("0000:01:00.0", "garbage"));
  }

  #[test]
  fn vendors_and_drivers() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let gpu_at = |pci: &str, vendor: &str, extra: &[(&str, &str)]| {
      let mut files = vec![("class", "0x030000"), ("vendor", vendor)];
      files.extend_from_slice(extra);
      write(&root.join(pci), &files);
    };
    gpu_at("0000:04:00.0", "0x10de", &[]);
    gpu_at("0000:00:02.0", "0x8086", &[]);
    gpu_at("0000:00:03.0", "0x8086", &[]);
    gpu_at("0000:05:00.0", "0x1af4", &[]);
    gpu_at(
      "0000:03:00.0",
      "0x1002",
      &[
        ("product_name", "Radeon RX 7600"),
        ("gpu_busy_percent", "0"),
      ],
    );
    gpu_at("0000:06:00.0", "0x1002", &[("product_name", "")]);
    // 3D controllers count too, other classes and unreadable devices do not
    write(
      &root.join("0000:07:00.0"),
      &[("class", "0x030200"), ("vendor", "0x10de")],
    );
    write(
      &root.join("0000:08:00.0"),
      &[("class", "0x020000"), ("vendor", "0x8086")],
    );
    write(&root.join("0000:09:00.0"), &[("class", "0x030000")]);
    fs::create_dir_all(root.join("0000:0a:00.0")).unwrap();
    let drivers = root.join("drivers");
    fs::create_dir_all(drivers.join("xe")).unwrap();
    fs::create_dir_all(drivers.join("vfio-pci")).unwrap();
    std::os::unix::fs::symlink(drivers.join("xe"), root.join("0000:00:02.0/driver")).unwrap();
    std::os::unix::fs::symlink(drivers.join("vfio-pci"), root.join("0000:00:03.0/driver")).unwrap();

    let gpus = list(root, None);
    let summary: Vec<_> = gpus
      .iter()
      .map(|g| {
        (
          g.pci.as_str(),
          g.name.as_str(),
          g.vendor,
          g.driver.as_deref(),
          g.measurable,
        )
      })
      .collect();
    assert_eq!(
      summary,
      [
        (
          "0000:00:02.0",
          "Intel GPU",
          GpuVendor::Intel,
          Some("xe"),
          true
        ),
        (
          "0000:00:03.0",
          "Intel GPU",
          GpuVendor::Intel,
          Some("vfio-pci"),
          false
        ),
        ("0000:03:00.0", "Radeon RX 7600", GpuVendor::Amd, None, true),
        ("0000:04:00.0", "NVIDIA GPU", GpuVendor::Nvidia, None, false),
        ("0000:05:00.0", "GPU", GpuVendor::Other, None, false),
        ("0000:06:00.0", "AMD GPU", GpuVendor::Amd, None, false),
        ("0000:07:00.0", "NVIDIA GPU", GpuVendor::Nvidia, None, false),
      ]
    );
    assert!(list(&root.join("missing"), None).is_empty());
  }

  #[test]
  fn i915_is_measurable() {
    let root = tempfile::tempdir().unwrap();
    let device = root.path().join("0000:00:02.0");
    write(&device, &[("class", "0x030000"), ("vendor", "0x8086")]);
    fs::create_dir_all(root.path().join("i915")).unwrap();
    std::os::unix::fs::symlink(root.path().join("i915"), device.join("driver")).unwrap();
    assert!(list(root.path(), None)[0].measurable);
  }

  #[test]
  fn suspended_gpus_are_left_alone() {
    let root = tempfile::tempdir().unwrap();
    write(
      &root.path().join("0000:03:00.0"),
      &[
        ("power/runtime_status", "suspended\n"),
        ("gpu_busy_percent", "50"),
      ],
    );
    let none = (&Engines::new(), &Engines::new());
    let sample = sample(
      root.path(),
      &gpu("0000:03:00.0", GpuVendor::Amd),
      None,
      none,
    );
    assert_eq!(
      sample,
      GpuSample {
        pci: "0000:03:00.0".into(),
        usage: None,
        temperature: None,
        vram_used: None,
        vram_total: None,
      }
    );
    write(
      &root.path().join("0000:03:00.0"),
      &[("power/runtime_status", "active")],
    );
    let awake = super::sample(
      root.path(),
      &gpu("0000:03:00.0", GpuVendor::Amd),
      None,
      none,
    );
    assert_eq!(awake.usage, Some(50.));
  }

  #[test]
  fn amd_partial_readings() {
    let root = tempfile::tempdir().unwrap();
    write(
      &root.path().join("0000:03:00.0"),
      &[
        ("gpu_busy_percent", "x"),
        ("mem_info_vram_used", " 12 \n"),
        ("hwmon/hwmon1/name", "amdgpu"),
        ("hwmon/hwmon1/temp1_input", "not a number"),
      ],
    );
    let none = (&Engines::new(), &Engines::new());
    let sample = sample(
      root.path(),
      &gpu("0000:03:00.0", GpuVendor::Amd),
      None,
      none,
    );
    assert_eq!(sample.usage, None);
    assert_eq!((sample.vram_used, sample.vram_total), (Some(12), None));
    assert_eq!(sample.temperature, None);
  }

  #[test]
  fn other_and_nvidia_without_nvml_sample_nothing() {
    let root = tempfile::tempdir().unwrap();
    let none = (&Engines::new(), &Engines::new());
    for vendor in [GpuVendor::Other, GpuVendor::Nvidia] {
      let sample = sample(root.path(), &gpu("0000:01:00.0", vendor), None, none);
      assert_eq!(
        (sample.usage, sample.temperature, sample.vram_used),
        (None, None, None)
      );
    }
    // intel without any DRM client reads idle
    let intel = sample(
      root.path(),
      &gpu("0000:00:02.0", GpuVendor::Intel),
      None,
      none,
    );
    assert_eq!(intel.usage, Some(0.));
  }

  #[test]
  fn fdinfo_parsing() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(
      root,
      &[
        // engine without its total, a capacity without an engine, junk values
        (
          "1/fdinfo/3",
          "drm-pdev:\tA\ndrm-client-id:\t1\ndrm-cycles-rcs:\t5\ndrm-cycles-bcs:\tx\n\
           drm-total-cycles-bcs:\t9\ndrm-engine-capacity-vcs:\t2\nno colon here\n",
        ),
        // no client id: skipped
        ("2/fdinfo/3", "drm-pdev:\tA\ndrm-engine-render:\t5 ns\n"),
        // no pdev: skipped
        (
          "3/fdinfo/3",
          "drm-client-id:\t3\ndrm-engine-render:\t5 ns\n",
        ),
        // i915 style with capacity
        (
          "4/fdinfo/3",
          "drm-pdev:\tB\ndrm-client-id:\t4\ndrm-engine-render:\t7 ns\ndrm-engine-capacity-render:\t3\n",
        ),
        ("not-a-pid", ""),
      ],
    );
    fs::create_dir_all(root.join("5/fdinfo/7")).unwrap();
    assert_eq!(engines(root, 100), engine(&[("B", "4", "render", 7, 300)]));
    assert!(engines(&root.join("missing"), 1).is_empty());
  }

  #[test]
  fn drm_usage_edges() {
    let pci = "0000:00:02.0";
    // counters reset: no negative busy time
    let previous = engine(&[(pci, "1", "rcs", 500, 1000)]);
    let current = engine(&[(pci, "1", "rcs", 100, 2000)]);
    assert_eq!(drm_usage(pci, &previous, &current), 0.);
    // no time passed: nothing to divide by
    let current = engine(&[(pci, "1", "rcs", 600, 1000)]);
    assert_eq!(drm_usage(pci, &previous, &current), 0.);
    // more busy than total is capped
    let current = engine(&[(pci, "1", "rcs", 5000, 2000)]);
    assert_eq!(drm_usage(pci, &previous, &current), 100.);
    // a client that just appeared has no baseline yet
    let current = engine(&[(pci, "2", "rcs", 900, 2000)]);
    assert_eq!(drm_usage(pci, &previous, &current), 0.);
    // clients add up per engine, the busiest engine wins
    let previous = engine(&[
      (pci, "1", "rcs", 0, 0),
      (pci, "2", "rcs", 0, 0),
      (pci, "1", "vcs", 0, 0),
    ]);
    let current = engine(&[
      (pci, "1", "rcs", 100, 1000),
      (pci, "2", "rcs", 200, 1000),
      (pci, "1", "vcs", 500, 1000),
    ]);
    assert_eq!(drm_usage(pci, &previous, &current), 50.);
    assert_eq!(drm_usage("other", &previous, &current), 0.);
    assert_eq!(drm_usage(pci, &Engines::new(), &Engines::new()), 0.);
  }

  #[test]
  fn fdinfo_capacity_multiplication() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(
      root,
      &[
        (
          "1/fdinfo/3",
          "drm-pdev:\tB\ndrm-client-id:\t1\ndrm-engine-render:\t10 ns\ndrm-engine-capacity-render:\t5\n",
        ),
      ],
    );
    // 100 ns now, capacity 5 -> total = 100 * 5 = 500
    assert_eq!(engines(root, 100), engine(&[("B", "1", "render", 10, 500)]));
  }

  #[test]
  fn hwmon_multiple_subdirectories() {
    let root = tempfile::tempdir().unwrap();
    let hwmon = root.path().join("hwmon");
    write(
      &hwmon,
      &[
        ("hwmon0/temp1_input", "50000"),
        ("hwmon1/temp1_input", "70000"),
      ],
    );
    let temp = hwmon_temperature(&hwmon).unwrap();
    assert!(temp == 50. || temp == 70.);
  }
}
