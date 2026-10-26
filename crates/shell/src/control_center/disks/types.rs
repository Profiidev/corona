//! Names and tables the panel shows: sizes, filesystems, partition types and flags

/// Decimal like the drives are sold: `512 MB`, `1.2 TB`
pub fn size(bytes: u64) -> String {
  const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
  let mut value = bytes as f64;
  let mut unit = 0;
  while value >= 1000. && unit < UNITS.len() - 1 {
    value /= 1000.;
    unit += 1;
  }
  match value < 10. && unit > 0 && value.fract() >= 0.05 {
    true => format!("{value:.1} {}", UNITS[unit]),
    false => format!("{value:.0} {}", UNITS[unit]),
  }
}

/// A filesystem's display name, like `FAT32` for `vfat`
pub fn fs_name(id_type: &str) -> &str {
  match id_type {
    "vfat" => "FAT32",
    "ext2" => "ext2",
    "ext3" => "ext3",
    "ext4" => "ext4",
    "btrfs" => "Btrfs",
    "xfs" => "XFS",
    "ntfs" => "NTFS",
    "exfat" => "exFAT",
    "f2fs" => "F2FS",
    "swap" => "Swap",
    "crypto_LUKS" => "LUKS",
    "" => "—",
    other => other,
  }
}

/// What `Format…` offers, the ones udisksd cannot make are greyed out
pub const FORMATS: [&str; 8] = [
  "ext4", "btrfs", "xfs", "vfat", "exfat", "ntfs", "f2fs", "swap",
];

pub struct PartitionType {
  /// A GPT GUID or a dos code like `0x83`, as udisks names it
  pub id: &'static str,
  pub name: &'static str,
}

const fn t(id: &'static str, name: &'static str) -> PartitionType {
  PartitionType { id, name }
}

pub const GPT_TYPES: [PartitionType; 12] = [
  t("0fc63daf-8483-4772-8e79-3d69d8477de4", "Linux filesystem"),
  t("0657fd6d-a4ab-43c4-84e5-0933c84b4f4f", "Linux swap"),
  t("ca7d7ccb-63ed-4c53-861c-1742536059cc", "Linux LUKS"),
  t("e6d6d379-f507-44c2-a23c-238f2a3df928", "Linux LVM"),
  t("a19d880f-05fc-4d3b-a006-743f0f84911e", "Linux RAID"),
  t(
    "4f68bce3-e8cd-4db1-96e7-fbcaf984b709",
    "Linux root (x86-64)",
  ),
  t("933ac7e1-2eb4-4f13-b844-0e14e2aef915", "Linux home"),
  t(
    "bc13c2ff-59e6-4262-a352-b275fd6f7172",
    "Linux extended boot",
  ),
  t("c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "EFI System"),
  t("21686148-6449-6e6f-744e-656564454649", "BIOS boot"),
  t(
    "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7",
    "Microsoft basic data",
  ),
  t("e3c9e316-0b5c-4db8-817d-f92df00215ae", "Microsoft reserved"),
];

pub const DOS_TYPES: [PartitionType; 8] = [
  t("0x83", "Linux"),
  t("0x82", "Linux swap"),
  t("0x8e", "Linux LVM"),
  t("0xfd", "Linux RAID"),
  t("0x07", "NTFS / exFAT"),
  t("0x0c", "W95 FAT32 (LBA)"),
  t("0xef", "EFI System"),
  t("0x05", "Extended"),
];

/// The types of a `gpt` or `dos` table
pub fn partition_types(table: &str) -> &'static [PartitionType] {
  match table {
    "dos" => &DOS_TYPES,
    _ => &GPT_TYPES,
  }
}

/// Its name, the raw id for one not in the list
pub fn partition_type_name(table: &str, id: &str) -> String {
  partition_types(table)
    .iter()
    .find(|t| t.id.eq_ignore_ascii_case(id))
    .map(|t| t.name.to_string())
    .unwrap_or_else(|| match id.is_empty() {
      true => "—".into(),
      false => id.to_string(),
    })
}

/// The default type of a new partition
pub fn default_partition_type(table: &str) -> &'static str {
  partition_types(table)[0].id
}

/// One settable flag bit, with its label key
pub struct Flag {
  pub bit: u64,
  pub key: &'static str,
}

pub const GPT_FLAGS: [Flag; 3] = [
  Flag {
    bit: 1 << 0,
    key: "app.disks.flags.required",
  },
  Flag {
    bit: 1 << 1,
    key: "app.disks.flags.no_block_io",
  },
  Flag {
    bit: 1 << 2,
    key: "app.disks.flags.legacy_boot",
  },
];

pub const DOS_FLAGS: [Flag; 1] = [Flag {
  bit: 0x80,
  key: "app.disks.flags.bootable",
}];

pub fn flags_of(table: &str) -> &'static [Flag] {
  match table {
    "dos" => &DOS_FLAGS,
    _ => &GPT_FLAGS,
  }
}

/// `gpt` → `GPT`, `dos` → `MBR`
pub fn table_name(table: &str) -> &str {
  match table {
    "gpt" => "GPT",
    "dos" => "MBR",
    "" => "—",
    other => other,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sizes_read_like_the_box() {
    assert_eq!(size(512), "512 B");
    assert_eq!(size(512_000_000), "512 MB");
    assert_eq!(size(1_000_204_886_016), "1 TB");
    assert_eq!(size(1_200_000_000_000), "1.2 TB");
    assert_eq!(size(32_000_000_000), "32 GB");
    assert_eq!(size(0), "0 B");
  }

  #[test]
  fn partition_types_and_flags() {
    // udisks hands GPT GUIDs back in lower case, any case matches
    assert_eq!(
      partition_type_name("gpt", "C12A7328-F81F-11D2-BA4B-00A0C93EC93B"),
      "EFI System"
    );
    assert_eq!(partition_type_name("dos", "0x83"), "Linux");
    assert_eq!(partition_type_name("gpt", "deadbeef"), "deadbeef");
    assert_eq!(partition_type_name("gpt", ""), "—");
    assert_eq!(default_partition_type("dos"), "0x83");
    assert_eq!(flags_of("dos")[0].bit, 0x80);
    assert_eq!(flags_of("gpt").len(), 3);
    assert_eq!((table_name("dos"), fs_name("vfat")), ("MBR", "FAT32"));
  }
}
