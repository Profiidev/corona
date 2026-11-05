//! What the panel does with the fds udisks hands out: disk images and benchmarks

use std::{
  fs::File,
  io::{self, BufReader, Read, Seek, SeekFrom, Write},
  os::unix::fs::{FileExt, OpenOptionsExt},
  path::Path,
  sync::atomic::{AtomicBool, AtomicU64, Ordering},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

/// Writes the device to an image file, zstd compressed when asked
pub fn backup(
  mut device: File,
  image: &Path,
  compress: bool,
  progress: &Progress,
) -> io::Result<u64> {
  // written aside under a name of its own and moved in place once whole, so a failure never
  // costs an older image and two backups to one name never share a file
  static PARTS: AtomicU64 = AtomicU64::new(0);
  let part = image.with_added_extension(format!(
    "{}-{}.part",
    std::process::id(),
    PARTS.fetch_add(1, Ordering::Relaxed)
  ));
  let write = || -> io::Result<u64> {
    // the opened fd's own length, the cached size may be stale
    let size = device.seek(SeekFrom::End(0))?;
    device.rewind()?;
    let file = File::options()
      .write(true)
      .create_new(true)
      .mode(0o600)
      .open(&part)?;
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
  let result = write().and_then(|copied| publish(&part, image).map(|()| copied));
  // a half written image is no backup
  if result.is_err() {
    let _ = std::fs::remove_file(&part);
  }
  result
}

/// Moves the finished image in place, failing where a file appeared at the target meanwhile
fn publish(part: &Path, image: &Path) -> io::Result<()> {
  use rustix::{
    fs::{CWD, RenameFlags, renameat_with},
    io::Errno,
  };
  match renameat_with(CWD, part, CWD, image, RenameFlags::NOREPLACE) {
    Ok(()) => Ok(()),
    Err(Errno::EXIST) => Err(io::Error::new(
      io::ErrorKind::AlreadyExists,
      format!("{} appeared while the backup ran", image.display()),
    )),
    // the filesystem cannot rename without replacing, check and rename as the next best
    Err(Errno::INVAL | Errno::NOSYS | Errno::NOTSUP) if !image.exists() => {
      std::fs::rename(part, image)
    }
    Err(Errno::INVAL | Errno::NOSYS | Errno::NOTSUP) => Err(io::ErrorKind::AlreadyExists.into()),
    Err(e) => Err(e.into()),
  }
}

/// Writes an image over the device, a zstd one decompressed on the way
pub fn restore(image: &Path, device: File, progress: &Progress) -> io::Result<u64> {
  let mut file = File::open(image)?;
  let mut magic = [0u8; 4];
  let compressed = file.read_exact(&mut magic).is_ok() && magic == ZSTD_MAGIC;
  file.rewind()?;
  let copied = match compressed {
    true => copy(zstd::stream::Decoder::new(file)?, &device, progress)?,
    false => copy(BufReader::new(file), &device, progress)?,
  };
  device.sync_all()?;
  Ok(copied)
}

/// The image's size as it lands on the device: a zstd one's frame size, None when it does not say
pub fn image_size(image: &Path) -> io::Result<Option<u64>> {
  let len = std::fs::metadata(image)?.len();
  let mut head = vec![0u8; 18];
  let read = File::open(image)?.read(&mut head)?;
  if read >= 4 && head[..4] == ZSTD_MAGIC {
    return Ok(
      zstd::zstd_safe::get_frame_content_size(&head[..read])
        .ok()
        .flatten(),
    );
  }
  Ok(Some(len))
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
  let mut seed = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_nanos() as u64)
    | 1;
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
      let copied = backup(File::open(&device).unwrap(), &image, compress, &progress).unwrap();
      assert_eq!(copied, data.len() as u64);
      assert_eq!(progress.done.load(Ordering::Relaxed), data.len() as u64);
      assert_eq!(image_size(&image).unwrap(), Some(data.len() as u64));
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
    std::fs::write(&image, b"older").unwrap();
    let progress = Progress::default();
    progress.cancel.store(true, Ordering::Relaxed);
    let err = backup(File::open(&device).unwrap(), &image, false, &progress).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    // no half image is left beside it
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    // the image there before stays
    assert_eq!(std::fs::read(&image).unwrap(), b"older");
  }

  #[test]
  fn backup_never_replaces_a_new_image() {
    let dir = tempfile::tempdir().unwrap();
    let device = dir.path().join("device");
    std::fs::write(&device, [1u8; 100]).unwrap();
    let image = dir.path().join("image");
    let part = dir.path().join("image.part");
    std::fs::write(&part, b"new").unwrap();
    std::fs::write(&image, b"appeared").unwrap();
    let err = publish(&part, &image).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&image).unwrap(), b"appeared");
    std::fs::remove_file(&image).unwrap();
    publish(&part, &image).unwrap();
    assert_eq!(std::fs::read(&image).unwrap(), b"new");
    // a backup lands under its own name
    backup(
      File::open(&device).unwrap(),
      &dir.path().join("b"),
      false,
      &Progress::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(dir.path().join("b")).unwrap(), [1u8; 100]);
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
