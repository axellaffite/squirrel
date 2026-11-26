//! Plain ZIP archives, written by hand: store-only (photos are already
//! compressed), no data descriptors and no ZIP64, so that every unzipper
//! (macOS Archive Utility, Windows Explorer, Android, iOS) opens them.
//!
//! Each entry is a local header followed by the file's bytes; the central
//! directory and the end record close the archive. The CRC-32 of a file goes
//! in its local header, so it is computed (one read of the file) before the
//! header is written. Sizes are known up front: [`archive_size`] is exact.

use std::collections::HashSet;
use std::io::{self, Read};

use squirrel_core::NodeName;
use time::{OffsetDateTime, UtcOffset};

/// Archives must stay strictly below this size: beyond it, offsets and sizes
/// no longer fit the plain ZIP format (ZIP64 would be needed).
pub const SIZE_LIMIT: u64 = 0xFFFF_FFFF;

/// Name of an entry whose file name is unusable.
const FALLBACK_NAME: &str = "file";
const LOCAL_HEADER_LEN: u64 = 30;
const CENTRAL_HEADER_LEN: u64 = 46;
const END_RECORD_LEN: u64 = 22;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
/// General-purpose flag bit 11: names are UTF-8.
const UTF8_NAMES: u16 = 1 << 11;
/// ZIP 2.0: enough for stored entries.
const VERSION_NEEDED: u16 = 20;
/// Made on Unix (high byte 3), so the external attributes carry a file mode.
const VERSION_MADE_BY: u16 = (3 << 8) | VERSION_NEEDED;
/// A regular file, `rw-r--r--`, in the high half of the external attributes.
const UNIX_FILE_ATTRIBUTES: u32 = 0o100_644 << 16;
const METHOD_STORED: u16 = 0;
const CRC_BUFFER: usize = 64 * 1024;

/// An MS-DOS date and time, as ZIP headers store them (2-second precision,
/// years 1980 to 2107, no time zone: UTC is written).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosDateTime {
    pub date: u16,
    pub time: u16,
}

impl DosDateTime {
    /// 1980-01-01 00:00:00, the earliest representable moment.
    const MIN: Self = Self {
        date: (1 << 5) | 1,
        time: 0,
    };
    /// 2107-12-31 23:59:58, the latest representable moment.
    const MAX: Self = Self {
        date: (127 << 9) | (12 << 5) | 31,
        time: (23 << 11) | (59 << 5) | 29,
    };

    /// `at` in UTC, clamped to the representable years.
    pub fn from_utc(at: OffsetDateTime) -> Self {
        let at = at.to_offset(UtcOffset::UTC);
        let Ok(years) = u16::try_from(at.year() - 1980) else {
            return Self::MIN;
        };
        if years > 127 {
            return Self::MAX;
        }
        Self {
            date: (years << 9) | (u16::from(u8::from(at.month())) << 5) | u16::from(at.day()),
            time: (u16::from(at.hour()) << 11)
                | (u16::from(at.minute()) << 5)
                | u16::from(at.second() / 2),
        }
    }
}

/// One file of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's path in the archive (a bare file name here).
    pub name: String,
    /// Exact byte length of the file.
    pub size: u64,
    pub modified: DosDateTime,
}

/// Entry names for files named `names`, in order: `/` and `\` replaced, an
/// unusable name becomes `file`, and a name already used (ignoring case) gets
/// ` (2)`, ` (3)`… before its extension.
pub fn entry_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut used = HashSet::new();
    names
        .into_iter()
        .map(|raw| {
            let base = sanitize(raw);
            // `variant(n)` is ` (n)`; the first duplicate is ` (2)`.
            std::iter::once(base.clone())
                .chain((2..=u32::MAX).map(|n| base.variant(n)))
                .find(|name| used.insert(name.as_str().to_lowercase()))
                .map(|name| name.as_str().to_owned())
                .unwrap_or_else(|| FALLBACK_NAME.to_owned())
        })
        .collect()
}

/// `raw` with path separators replaced, or `file` when it is not a valid name.
fn sanitize(raw: &str) -> NodeName {
    let replaced: String = raw
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    NodeName::parse(&replaced)
        .or_else(|_| NodeName::parse(FALLBACK_NAME))
        .expect("the fallback name is valid")
}

/// The exact byte length of the archive holding `entries`.
pub fn archive_size(entries: &[Entry]) -> u64 {
    let entries: u64 = entries
        .iter()
        .map(|entry| {
            let name = entry.name.len() as u64;
            LOCAL_HEADER_LEN + name + entry.size + CENTRAL_HEADER_LEN + name
        })
        .sum();
    entries + END_RECORD_LEN
}

/// CRC-32 and length of everything `reader` yields.
pub fn crc32(reader: impl Read) -> io::Result<(u32, u64)> {
    let mut reader = reader;
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0; CRC_BUFFER];
    let mut total = 0u64;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok((hasher.finalize(), total)),
            Ok(read) => {
                hasher.update(&buffer[..read]);
                total += read as u64;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}

/// Values the caller keeps in range (sizes below [`SIZE_LIMIT`], names of at
/// most 255 bytes, at most 65535 entries) saturate instead of wrapping.
fn u32_of(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn u16_of(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// The fields local and central headers share, from "version needed" to the
/// name length.
fn common_fields(entry: &Entry, crc: u32) -> Vec<u8> {
    let size = u32_of(entry.size);
    [
        &VERSION_NEEDED.to_le_bytes()[..],
        &UTF8_NAMES.to_le_bytes(),
        &METHOD_STORED.to_le_bytes(),
        &entry.modified.time.to_le_bytes(),
        &entry.modified.date.to_le_bytes(),
        &crc.to_le_bytes(),
        &size.to_le_bytes(),
        &size.to_le_bytes(),
        &u16_of(entry.name.len()).to_le_bytes(),
    ]
    .concat()
}

/// Writes the headers of an archive whose entries are emitted in order, each
/// as [`ArchiveWriter::local_header`] then exactly `entry.size` bytes of the
/// file, and closed by [`ArchiveWriter::finish`]. The caller keeps the whole
/// archive below [`SIZE_LIMIT`] (checked with [`archive_size`]).
#[derive(Debug, Default)]
pub struct ArchiveWriter {
    /// Where the next local header starts.
    offset: u64,
    /// The central directory so far.
    central: Vec<u8>,
    count: u16,
}

impl ArchiveWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The local header of `entry`, whose bytes have the CRC-32 `crc`.
    pub fn local_header(&mut self, entry: &Entry, crc: u32) -> Vec<u8> {
        let fields = common_fields(entry, crc);
        let no_extra = 0u16.to_le_bytes();
        let local = [
            &LOCAL_SIGNATURE.to_le_bytes()[..],
            &fields,
            &no_extra,
            entry.name.as_bytes(),
        ]
        .concat();
        let central = [
            &CENTRAL_SIGNATURE.to_le_bytes()[..],
            &VERSION_MADE_BY.to_le_bytes(),
            &fields,
            &no_extra,
            &0u16.to_le_bytes(), // comment length
            &0u16.to_le_bytes(), // disk number
            &0u16.to_le_bytes(), // internal attributes
            &UNIX_FILE_ATTRIBUTES.to_le_bytes(),
            &u32_of(self.offset).to_le_bytes(),
            entry.name.as_bytes(),
        ]
        .concat();
        self.central.extend(central);
        self.count = self.count.saturating_add(1);
        self.offset += local.len() as u64 + entry.size;
        local
    }

    /// The central directory and the end record.
    pub fn finish(self) -> Vec<u8> {
        let end = [
            &END_SIGNATURE.to_le_bytes()[..],
            &0u16.to_le_bytes(), // this disk
            &0u16.to_le_bytes(), // disk of the central directory
            &self.count.to_le_bytes(),
            &self.count.to_le_bytes(),
            &u32_of(self.central.len() as u64).to_le_bytes(),
            &u32_of(self.offset).to_le_bytes(),
            &0u16.to_le_bytes(), // comment length
        ]
        .concat();
        [self.central, end].concat()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use time::macros::datetime;

    use super::*;

    fn le16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn le32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// What the tiny reader found for one entry.
    #[derive(Debug)]
    struct Found {
        name: String,
        data: Vec<u8>,
        modified: DosDateTime,
    }

    /// Walks the local headers, then checks the central directory and the end
    /// record against them. Panics on anything malformed.
    fn read_archive(bytes: &[u8]) -> Vec<Found> {
        let mut found = Vec::new();
        let mut locals = Vec::new();
        let mut at = 0;
        while le32(bytes, at) == LOCAL_SIGNATURE {
            assert_eq!(le16(bytes, at + 4), VERSION_NEEDED, "version needed");
            assert_eq!(
                le16(bytes, at + 6),
                UTF8_NAMES,
                "flags: UTF-8, no data descriptor"
            );
            assert_eq!(le16(bytes, at + 8), METHOD_STORED, "stored");
            let modified = DosDateTime {
                time: le16(bytes, at + 10),
                date: le16(bytes, at + 12),
            };
            let crc = le32(bytes, at + 14);
            let compressed = le32(bytes, at + 18) as usize;
            let size = le32(bytes, at + 22) as usize;
            assert_eq!(compressed, size, "stored entries are not compressed");
            let name_len = le16(bytes, at + 26) as usize;
            assert_eq!(le16(bytes, at + 28), 0, "no extra field");
            let name_start = at + LOCAL_HEADER_LEN as usize;
            let name = std::str::from_utf8(&bytes[name_start..name_start + name_len]).unwrap();
            let data_start = name_start + name_len;
            let data = bytes[data_start..data_start + size].to_vec();
            assert_eq!(crc, crc32fast::hash(&data), "CRC of {name}");
            locals.push((at, name.to_owned(), crc, size, modified));
            found.push(Found {
                name: name.to_owned(),
                data,
                modified,
            });
            at = data_start + size;
        }
        let central_start = at;
        for (offset, name, crc, size, modified) in &locals {
            assert_eq!(
                le32(bytes, at),
                CENTRAL_SIGNATURE,
                "central header of {name}"
            );
            assert_eq!(le16(bytes, at + 4), VERSION_MADE_BY);
            assert_eq!(le16(bytes, at + 6), VERSION_NEEDED);
            assert_eq!(le16(bytes, at + 8), UTF8_NAMES);
            assert_eq!(le16(bytes, at + 10), METHOD_STORED);
            assert_eq!(le16(bytes, at + 12), modified.time);
            assert_eq!(le16(bytes, at + 14), modified.date);
            assert_eq!(le32(bytes, at + 16), *crc);
            assert_eq!(le32(bytes, at + 20) as usize, *size);
            assert_eq!(le32(bytes, at + 24) as usize, *size);
            let name_len = le16(bytes, at + 28) as usize;
            assert_eq!(le16(bytes, at + 30), 0, "no extra field");
            assert_eq!(le16(bytes, at + 32), 0, "no comment");
            assert_eq!(le32(bytes, at + 38), UNIX_FILE_ATTRIBUTES);
            assert_eq!(
                le32(bytes, at + 42) as usize,
                *offset,
                "local header offset"
            );
            let name_start = at + CENTRAL_HEADER_LEN as usize;
            assert_eq!(&bytes[name_start..name_start + name_len], name.as_bytes());
            at = name_start + name_len;
        }
        let end = bytes.len() - END_RECORD_LEN as usize;
        assert_eq!(at, end, "the end record follows the central directory");
        assert_eq!(le32(bytes, end), END_SIGNATURE);
        assert_eq!(
            le16(bytes, end + 8) as usize,
            locals.len(),
            "entries on disk"
        );
        assert_eq!(le16(bytes, end + 10) as usize, locals.len(), "entries");
        assert_eq!(
            le32(bytes, end + 12) as usize,
            end - central_start,
            "directory size"
        );
        assert_eq!(
            le32(bytes, end + 16) as usize,
            central_start,
            "directory offset"
        );
        assert_eq!(le16(bytes, end + 20), 0, "no comment");
        found
    }

    fn build(files: &[(&str, &[u8])]) -> (Vec<Entry>, Vec<u8>) {
        let modified = DosDateTime::from_utc(datetime!(2024-03-15 13:45:31 UTC));
        let entries: Vec<Entry> = files
            .iter()
            .map(|(name, data)| Entry {
                name: (*name).to_owned(),
                size: data.len() as u64,
                modified,
            })
            .collect();
        let mut writer = ArchiveWriter::new();
        let mut bytes = Vec::new();
        for (entry, (_, data)) in entries.iter().zip(files) {
            let (crc, _) = crc32(Cursor::new(data)).unwrap();
            bytes.extend(writer.local_header(entry, crc));
            bytes.extend_from_slice(data);
        }
        bytes.extend(writer.finish());
        (entries, bytes)
    }

    const FILES: [(&str, &[u8]); 4] = [
        ("hello.txt", b"hello squirrel"),
        ("empty.bin", b""),
        ("café ☕.jpg", b"\xff\xd8\xff\xe0 not really a jpeg"),
        ("hello (2).txt", b"a second hello"),
    ];

    #[test]
    fn the_size_formula_matches_the_bytes_written() {
        let (entries, bytes) = build(&FILES);
        assert_eq!(archive_size(&entries), bytes.len() as u64);
        let names: u64 = FILES.iter().map(|(name, _)| name.len() as u64).sum();
        let data: u64 = FILES.iter().map(|(_, data)| data.len() as u64).sum();
        assert_eq!(
            archive_size(&entries),
            4 * (30 + 46) + 2 * names + data + 22
        );
        assert_eq!(archive_size(&[]), 22, "an empty archive is its end record");
    }

    #[test]
    fn a_written_archive_reads_back() {
        let (entries, bytes) = build(&FILES);
        let found = read_archive(&bytes);
        assert_eq!(found.len(), FILES.len());
        for ((found, (name, data)), entry) in found.iter().zip(FILES).zip(&entries) {
            assert_eq!(found.name, name);
            assert_eq!(found.data, data);
            assert_eq!(found.modified, entry.modified);
        }
    }

    #[test]
    fn crc_matches_crc32fast_and_counts_bytes() {
        let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(
            crc32(Cursor::new(&content)).unwrap(),
            (crc32fast::hash(&content), content.len() as u64)
        );
        assert_eq!(crc32(Cursor::new(b"")).unwrap(), (0, 0));
        // The classic check value.
        assert_eq!(crc32(Cursor::new(b"123456789")).unwrap().0, 0xCBF4_3926);
    }

    #[test]
    fn names_are_sanitized_and_deduplicated_ignoring_case() {
        let names = entry_names([
            "a.txt",
            "A.TXT",
            "a.txt",
            "a (2).txt",
            "back\\slash.txt",
            "",
            "   ",
            ".bashrc",
            ".bashrc",
            "notes",
            "NOTES",
            "archive.tar.gz",
            "archive.tar.gz",
        ]);
        assert_eq!(
            names,
            [
                "a.txt",
                "A (2).TXT",
                "a (3).txt",
                "a (2) (2).txt",
                "back_slash.txt",
                "file",
                "file (2)",
                ".bashrc",
                ".bashrc (2)",
                "notes",
                "NOTES (2)",
                "archive.tar.gz",
                "archive.tar (2).gz",
            ]
        );
        assert_eq!(entry_names(["x/y.txt"]), ["x_y.txt"]);
        assert!(entry_names(Vec::<&str>::new()).is_empty());
    }

    #[test]
    fn dos_dates_round_to_two_seconds_and_clamp() {
        let at = DosDateTime::from_utc(datetime!(2024-03-15 13:45:31 UTC));
        assert_eq!(at.date, (44 << 9) | (3 << 5) | 15);
        assert_eq!(at.time, (13 << 11) | (45 << 5) | 15);
        // Converted to UTC first.
        assert_eq!(
            DosDateTime::from_utc(datetime!(2024-03-15 15:45:31 +02:00)),
            at
        );
        assert_eq!(
            DosDateTime::from_utc(datetime!(1970-01-01 00:00:00 UTC)),
            DosDateTime::MIN
        );
        assert_eq!(
            DosDateTime::from_utc(datetime!(1980-01-01 00:00:00 UTC)),
            DosDateTime::MIN
        );
        assert_eq!(
            DosDateTime::from_utc(datetime!(2200-06-01 12:00:00 UTC)),
            DosDateTime::MAX
        );
        assert_eq!(
            DosDateTime::from_utc(datetime!(2107-12-31 23:59:59 UTC)),
            DosDateTime::MAX
        );
    }

    /// The system `unzip`, when installed, agrees the archive is sound.
    #[test]
    fn unzip_tests_the_archive_when_available() {
        let Ok(probe) = std::process::Command::new("unzip").arg("-v").output() else {
            eprintln!("unzip is not installed: skipped");
            return;
        };
        if !probe.status.success() {
            return;
        }
        let (_, bytes) = build(&FILES);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.zip");
        std::fs::write(&path, bytes).unwrap();
        let output = std::process::Command::new("unzip")
            .arg("-t")
            .arg(&path)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "unzip -t failed: {report}");
        assert!(report.contains("No errors detected"), "{report}");
    }
}
