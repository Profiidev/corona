//! What the panel does with the fds udisks hands out: disk images and benchmarks

use std::{
  fs::File,
  io::{self, BufReader, Read, Write},
  os::unix::fs::FileExt,
  path::Path,
  sync::atomic::{AtomicBool, AtomicU64, Ordering},
  time::{Duration, Instant},
};

const CHUNK: usize = 4 << 20;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// What a running copy or benchmark reports, shared with its job row
#[derive(Default)]
pub struct Progress {
  pub done: AtomicU64,
  pub cancel: AtomicBool,
}

impl Progress {
  fn cancelled(&self) -> io::Result<()> {
    match self.cancel.load(Ordering::Relaxed) {
      true => Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")),
      false => Ok(()),
    }
  }
}

/// Copies until `from` ends, counting into `progress`
fn copy(mut from: impl Read, mut to: impl Write, progress: &Progress) -> io::Result<u64> {
  let mut buf = vec![0u8; CHUNK];
  let mut total = 0;
  loop {
    progress.cancelled()?;
    let n = match from.read(&mut buf) {
      Ok(0) => break,
      Ok(n) => n,
      Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
      Err(e) => return Err(e),
    };
    to.write_all(&buf[..n])?;
    total += n as u64;
    progress.done.store(total, Ordering::Relaxed);
  }
  to.flush()?;
  Ok(total)
}

/// Writes the device of `size` bytes to an image file, zstd compressed when asked
pub fn backup(
  device: File,
  size: u64,
  image: &Path,
  compress: bool,
  progress: &Progress,
) -> io::Result<u64> {
  let write = || -> io::Result<u64> {
    let file = File::create(image)?;
    match compress {
      true => {
        let mut encoder = zstd::stream::Encoder::new(file, 3)?;
        // the frame says how big the image is unpacked, what restoring checks against
        encoder.include_contentsize(true)?;
        encoder.set_pledged_src_size(Some(size))?;
        let copied = copy(device, &mut encoder, progress)?;
        encoder.finish()?.sync_all()?;
        Ok(copied)
      }
      false => {
        let mut file = file;
        let copied = copy(device, &mut file, progress)?;
        file.sync_all()?;
        Ok(copied)
      }
    }
  };
  let result = write();
  // a half written image is no backup
  if result.is_err() {
    let _ = std::fs::remove_file(image);
  }
  result
}

/// Writes an image over the device, a zstd one decompressed on the way
pub fn restore(image: &Path, device: File, progress: &Progress) -> io::Result<u64> {
  let mut file = File::open(image)?;
  let mut magic = [0u8; 4];
  let compressed = file.read_exact(&mut magic).is_ok() && magic == ZSTD_MAGIC;
  let file = File::open(image)?;
  let copied = match compressed {
    true => copy(zstd::stream::Decoder::new(file)?, &device, progress)?,
    false => copy(BufReader::new(file), &device, progress)?,
  };
  device.sync_all()?;
  Ok(copied)
}

/// The image's size as it lands on the device: a zstd one's frame size when it says
pub fn image_size(image: &Path) -> io::Result<u64> {
  let len = std::fs::metadata(image)?.len();
  let mut head = vec![0u8; 18];
  let read = File::open(image)?.read(&mut head)?;
  if read >= 4 && head[..4] == ZSTD_MAGIC {
    return Ok(
      zstd::zstd_safe::get_frame_content_size(&head[..read])
        .ok()
        .flatten()
        .unwrap_or(len),
    );
  }
  Ok(len)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bench {
  /// Bytes per second
  pub read: f64,
  pub write: Option<f64>,
  pub access: Duration,
}

pub const SAMPLES: u64 = 100;
const SAMPLE_SIZE: u64 = 10 << 20;
const ACCESS_SIZE: u64 = 4096;
const ALIGN: u64 = 4096;

/// `count` offsets of `len` byte samples spread evenly over `size`, aligned for O_DIRECT
pub fn sample_offsets(size: u64, len: u64, count: u64) -> Vec<u64> {
  if size < len || count == 0 {
    return Vec::new();
  }
  let span = size - len;
  (0..count)
    .map(|i| match count {
      1 => 0,
      _ => span * i / (count - 1) / ALIGN * ALIGN,
    })
    .collect()
}

/// A buffer whose start sits on `ALIGN`, which O_DIRECT reads need
struct Aligned {
  raw: Vec<u8>,
  start: usize,
  len: usize,
}

impl Aligned {
  fn new(len: usize) -> Self {
    let raw = vec![0u8; len + ALIGN as usize];
    let start = raw.as_ptr().align_offset(ALIGN as usize);
    Self { raw, start, len }
  }

  fn get(&mut self) -> &mut [u8] {
    &mut self.raw[self.start..self.start + self.len]
  }
}

/// A small xorshift: random offsets without a dependency
fn xorshift(state: &mut u64) -> u64 {
  *state ^= *state << 13;
  *state ^= *state >> 7;
  *state ^= *state << 17;
  *state
}

/// Read rate over 100 samples, average access time over 100 random reads; with `write` each
/// sample is written back where it was read
pub fn benchmark(device: &File, size: u64, write: bool, progress: &Progress) -> io::Result<Bench> {
  let len = SAMPLE_SIZE.min(size / ALIGN * ALIGN);
  if len == 0 {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "device too small",
    ));
  }
  let mut buf = Aligned::new(len as usize);
  let (mut read_time, mut write_time) = (Duration::ZERO, Duration::ZERO);
  let offsets = sample_offsets(size, len, SAMPLES);
  for (i, offset) in offsets.iter().enumerate() {
    progress.cancelled()?;
    let start = Instant::now();
    device.read_exact_at(buf.get(), *offset)?;
    read_time += start.elapsed();
    if write {
      let start = Instant::now();
      device.write_all_at(buf.get(), *offset)?;
      device.sync_data()?;
      write_time += start.elapsed();
    }
    progress.done.store(i as u64 + 1, Ordering::Relaxed);
  }
  let mut small = Aligned::new(ACCESS_SIZE as usize);
  let mut seed = Instant::now().elapsed().as_nanos() as u64 | 1;
  let blocks = (size / ACCESS_SIZE).max(1);
  let mut access = Duration::ZERO;
  for _ in 0..SAMPLES {
    progress.cancelled()?;
    let offset = xorshift(&mut seed) % blocks * ACCESS_SIZE;
    let start = Instant::now();
    device.read_exact_at(small.get(), offset)?;
    access += start.elapsed();
  }
  let bytes = (len * offsets.len() as u64) as f64;
  Ok(Bench {
    read: rate(bytes, read_time),
    write: write.then(|| rate(bytes, write_time)),
    access: access / SAMPLES as u32,
  })
}

fn rate(bytes: f64, time: Duration) -> f64 {
  match time.as_secs_f64() {
    0. => 0.,
    secs => bytes / secs,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn backup_and_restore_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let data: Vec<u8> = (0..(CHUNK * 2 + 77)).map(|i| (i * 7 % 251) as u8).collect();
    for compress in [false, true] {
      let device = dir.path().join("device");
      std::fs::write(&device, &data).unwrap();
      let image = dir.path().join(format!("image-{compress}"));
      let progress = Progress::default();
      let copied = backup(
        File::open(&device).unwrap(),
        data.len() as u64,
        &image,
        compress,
        &progress,
      )
      .unwrap();
      assert_eq!(copied, data.len() as u64);
      assert_eq!(progress.done.load(Ordering::Relaxed), data.len() as u64);
      assert_eq!(image_size(&image).unwrap(), data.len() as u64);
      let compressed = std::fs::metadata(&image).unwrap().len() < data.len() as u64;
      assert_eq!(compressed, compress);

      let target = dir.path().join("target");
      std::fs::write(&target, vec![0u8; data.len()]).unwrap();
      let target_file = File::options().write(true).open(&target).unwrap();
      restore(&image, target_file, &Progress::default()).unwrap();
      assert_eq!(std::fs::read(&target).unwrap(), data);
    }
  }

  #[test]
  fn cancelling_removes_the_half_image() {
    let dir = tempfile::tempdir().unwrap();
    let device = dir.path().join("device");
    std::fs::write(&device, [1u8; 100]).unwrap();
    let image = dir.path().join("image");
    let progress = Progress::default();
    progress.cancel.store(true, Ordering::Relaxed);
    let err = backup(File::open(&device).unwrap(), 100, &image, false, &progress).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    assert!(!image.exists());
  }

  #[test]
  fn samples_spread_and_align() {
    let offsets = sample_offsets(1 << 30, SAMPLE_SIZE, SAMPLES);
    assert_eq!(offsets.len(), 100);
    assert_eq!(offsets[0], 0);
    assert!(offsets.iter().all(|o| o % ALIGN == 0));
    assert!(*offsets.last().unwrap() + SAMPLE_SIZE <= 1 << 30);
    assert!(offsets.windows(2).all(|w| w[0] <= w[1]));
    assert!(sample_offsets(100, SAMPLE_SIZE, SAMPLES).is_empty());
    assert_eq!(sample_offsets(SAMPLE_SIZE, SAMPLE_SIZE, 1), [0]);
  }

  #[test]
  fn benchmark_on_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device");
    let size = 64 << 20;
    let data: Vec<u8> = (0..size).map(|i| (i % 253) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let file = File::options().read(true).write(true).open(&path).unwrap();
    let progress = Progress::default();
    let bench = benchmark(&file, size as u64, true, &progress).unwrap();
    assert!(bench.read > 0. && bench.write.unwrap() > 0.);
    assert_eq!(progress.done.load(Ordering::Relaxed), SAMPLES);
    // the write test puts back what it read
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(rate(1000., Duration::from_secs(2)), 500.);
    assert_eq!(rate(1000., Duration::ZERO), 0.);
  }
}
