use std::{fs::File, io::Write, os::fd::OwnedFd, sync::Arc};

use anyhow::Result;
use gpui_kit::{Dmabuf, DmabufPlane};
use rustix::fs::{MemfdFlags, memfd_create};

/// Opaque BGRA `pixels` in a memfd, for [`crate::FrameView`] to paint
pub fn shm_dmabuf(width: u32, height: u32, pixels: &[u8]) -> Result<Arc<Dmabuf>> {
  anyhow::ensure!(
    pixels.len() == width as usize * height as usize * 4,
    "{} bytes is not {width}x{height} BGRA",
    pixels.len()
  );
  let mut file = File::from(memfd_create("corona-image", MemfdFlags::CLOEXEC)?);
  file.write_all(pixels)?;
  Ok(Arc::new(Dmabuf {
    width,
    height,
    planes: vec![DmabufPlane {
      fd: OwnedFd::from(file),
      offset: 0,
      stride: width * 4,
    }],
    modifier: None,
    opaque: true,
  }))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn size_must_match() {
    assert!(shm_dmabuf(2, 2, &[0; 16]).is_ok());
    assert!(shm_dmabuf(2, 2, &[0; 15]).is_err());
  }
}
