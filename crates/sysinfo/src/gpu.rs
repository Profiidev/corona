use std::{
  fs,
  path::{Path, PathBuf},
};

use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};

use crate::state::{Gpu, GpuSample, GpuVendor};

pub(crate) const PCI: &str = "/sys/bus/pci/devices";

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
        GpuVendor::Intel | GpuVendor::Other => false,
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

pub(crate) fn sample(pci_dir: &Path, gpu: &Gpu, nvml: Option<&Nvml>) -> GpuSample {
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
    GpuVendor::Intel | GpuVendor::Other => {}
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
    let sample = sample(&root, &gpus[0], None);
    assert_eq!(sample.usage, Some(37.));
    assert_eq!(sample.temperature, Some(52.));
    assert_eq!(sample.vram_total, Some(8589934592));
    fs::remove_dir_all(root).ok();
  }
}
