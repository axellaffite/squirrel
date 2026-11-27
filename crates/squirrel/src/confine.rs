//! Confines a child that parses untrusted bytes (`media-exec`, `ml-exec`), on
//! top of its resource limits: a decoder taken over by a crafted file must not
//! reach the other files, the network or the processes around it.
//!
//! Linux only, in two layers, both applied before the first decoder or model
//! is loaded (and so before any other thread exists: both are inherited by
//! the threads started afterwards):
//!
//! - Landlock: the file system is denied except for the paths of a
//!   [`Confinement`], TCP is denied, and signals stop at the child (where the
//!   kernel knows how: Landlock ABI 4 and 6). `/proc` is not readable, so
//!   neither is the environment of the worker that started the child.
//! - seccomp: the system calls of [`DENIED_SYSCALLS`] fail with `EPERM`: no
//!   socket of any kind, no other program, no look into another process.
//!
//! A kernel without Landlock (before 5.13, or with the module off) still gets
//! the seccomp layer; the child says so on stderr and [`filesystem_isolation`]
//! lets the server and the worker warn at start. Elsewhere this is a no-op.

use std::path::PathBuf;

/// System directories every child may read: its shared libraries (libvips'
/// and libheif's plugins are loaded on first use) and their configuration.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const SYSTEM_READ: [&str; 9] = [
    "/usr",
    "/lib",
    "/lib64",
    "/etc",
    "/opt/squirrel",
    "/dev/urandom",
    "/dev/null",
    // What ONNX Runtime sizes its thread pools from.
    "/proc/cpuinfo",
    "/sys/devices/system/cpu",
];

/// What the child may touch besides [`SYSTEM_READ`]. Paths that do not exist
/// are skipped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Confinement {
    /// Files, or directories with everything below them, to read (and load
    /// libraries from).
    pub read: Vec<PathBuf>,
    /// Directories to read and write in.
    pub write: Vec<PathBuf>,
}

impl Confinement {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn read_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        SYSTEM_READ
            .iter()
            .map(PathBuf::from)
            .chain(self.read.iter().cloned())
    }
}

/// Whether the kernel can deny the file system to a child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Each platform builds some of the variants only.
#[allow(dead_code)]
pub enum FilesystemIsolation {
    Enforced,
    /// Linux without Landlock: the children keep the file access of their user.
    Unavailable,
    /// Not Linux (development machines).
    Unsupported,
}

pub use imp::{apply, filesystem_isolation};

/// Says once, at start, when the children's file system cannot be confined here.
pub fn warn_if_unavailable() {
    if filesystem_isolation() == FilesystemIsolation::Unavailable {
        tracing::warn!(
            "this kernel has no Landlock (Linux 5.13 or later, with the landlock LSM on): \
             the media and ML children are limited to no network and no other program, \
             but can read the files of their user"
        );
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::BTreeMap;

    use anyhow::Context;
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus, Scope, path_beneath_rules,
    };
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};

    use super::{Confinement, FilesystemIsolation};

    /// The newest Landlock ABI asked for; an older kernel enforces what it knows.
    const LANDLOCK_ABI: ABI = ABI::V6;

    /// Refused with `EPERM`. The children compute: they open the files they
    /// were given and write their answer, nothing else.
    const DENIED_SYSCALLS: [i64; 19] = [
        // No network and no local sockets (the database, the tailnet).
        libc::SYS_socket,
        libc::SYS_socketpair,
        // io_uring opens sockets and files without the calls filtered here.
        libc::SYS_io_uring_setup,
        // No other program.
        libc::SYS_execve,
        libc::SYS_execveat,
        // No look into (or write to) another process of the same user.
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_pidfd_open,
        libc::SYS_kcmp,
        // No new namespaces (a user namespace hands out capabilities) or mounts.
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        // Kernel attack surface a decoder has no use for.
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
    ];

    pub fn filesystem_isolation() -> FilesystemIsolation {
        let supported = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::Execute)
            .and_then(Ruleset::create)
            .is_ok();
        if supported {
            FilesystemIsolation::Enforced
        } else {
            FilesystemIsolation::Unavailable
        }
    }

    /// Confines this process for good. Fails (and the child refuses to run)
    /// when a layer the kernel supports cannot be applied.
    pub fn apply(confinement: &Confinement) -> anyhow::Result<FilesystemIsolation> {
        let isolation = landlock(confinement).context("applying the Landlock rules")?;
        if isolation == FilesystemIsolation::Unavailable {
            eprintln!("no Landlock in this kernel: the file system is not confined");
        }
        seccomp().context("installing the seccomp filter")?;
        Ok(isolation)
    }

    fn landlock(confinement: &Confinement) -> anyhow::Result<FilesystemIsolation> {
        let read = AccessFs::from_read(LANDLOCK_ABI);
        let status = Ruleset::default()
            .handle_access(AccessFs::from_all(LANDLOCK_ABI))?
            // No rule allows a port: every TCP bind and connect is denied.
            .handle_access(AccessNet::from_all(LANDLOCK_ABI))?
            .scope(Scope::from_all(LANDLOCK_ABI))?
            .create()?
            .add_rules(path_beneath_rules(confinement.read_paths(), read))?
            .add_rules(path_beneath_rules(
                &confinement.write,
                AccessFs::from_all(LANDLOCK_ABI),
            ))?
            .restrict_self()?;
        Ok(match status.ruleset {
            RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => {
                FilesystemIsolation::Enforced
            }
            RulesetStatus::NotEnforced => FilesystemIsolation::Unavailable,
        })
    }

    fn seccomp() -> anyhow::Result<()> {
        // No condition on the arguments: the call is refused whatever they are.
        let rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> = DENIED_SYSCALLS
            .iter()
            .map(|&call| (call, Vec::new()))
            .collect();
        let arch = std::env::consts::ARCH
            .try_into()
            .map_err(|err| anyhow::anyhow!("no seccomp filter for this architecture: {err}"))?;
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM.unsigned_abs()),
            arch,
        )?;
        let program: BpfProgram = filter.try_into()?;
        seccompiler::apply_filter(&program)?;
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{Confinement, FilesystemIsolation};

    pub fn filesystem_isolation() -> FilesystemIsolation {
        FilesystemIsolation::Unsupported
    }

    pub fn apply(_confinement: &Confinement) -> anyhow::Result<FilesystemIsolation> {
        Ok(FilesystemIsolation::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_libraries_are_always_readable() {
        let confinement = Confinement {
            read: vec!["/data/blobs/ab/cd".into()],
            write: vec!["/data/tmp/x".into()],
        };
        let read: Vec<PathBuf> = confinement.read_paths().collect();
        assert!(read.contains(&PathBuf::from("/usr")));
        assert!(read.contains(&PathBuf::from("/data/blobs/ab/cd")));
        // Of /proc, the processor description only: never a process's files.
        let proc: Vec<&PathBuf> = read.iter().filter(|p| p.starts_with("/proc")).collect();
        assert_eq!(proc, [&PathBuf::from("/proc/cpuinfo")]);
        assert!(!read.contains(&PathBuf::from("/data/tmp/x")));
    }
}

/// The confinement itself, on a thread of its own: Landlock and seccomp bind
/// the thread that asks (and what it starts), not the rest of the test binary.
#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use std::io::ErrorKind;

    use super::*;

    fn confined<T: Send + 'static>(
        confinement: Confinement,
        probe: impl FnOnce(FilesystemIsolation) -> T + Send + 'static,
    ) -> T {
        std::thread::spawn(move || probe(apply(&confinement).expect("confined")))
            .join()
            .expect("the probe ran")
    }

    #[test]
    fn only_the_listed_files_can_be_opened() {
        let dir = tempfile::tempdir().unwrap();
        let (input, other) = (dir.path().join("input"), dir.path().join("other"));
        std::fs::write(&input, b"mine").unwrap();
        std::fs::write(&other, b"someone else's").unwrap();
        let out = tempfile::tempdir().unwrap();
        let confinement = Confinement {
            read: vec![input.clone()],
            write: vec![out.path().to_owned()],
        };
        let written = out.path().join("thumb.webp");
        let report = confined(confinement, move |isolation| {
            (
                isolation,
                std::fs::read(&input).map_err(|err| err.kind()),
                std::fs::read(&other).map_err(|err| err.kind()),
                std::fs::write(&input, b"changed").map_err(|err| err.kind()),
                std::fs::write(&written, b"out").map_err(|err| err.kind()),
                // The parent's environment (the database URL) is behind /proc.
                std::fs::read("/proc/self/environ").map_err(|err| err.kind()),
                std::fs::read_dir("/usr")
                    .map(|_| ())
                    .map_err(|err| err.kind()),
            )
        });
        let (isolation, own, other, overwrite, output, environ, system) = report;
        if isolation == FilesystemIsolation::Unavailable {
            assert!(
                std::env::var_os("CI").is_none(),
                "the CI kernel is expected to have Landlock"
            );
            eprintln!("skipped: no Landlock in this kernel");
            return;
        }
        assert_eq!(own.as_deref(), Ok(&b"mine"[..]));
        assert_eq!(other, Err(ErrorKind::PermissionDenied));
        assert_eq!(overwrite, Err(ErrorKind::PermissionDenied));
        assert_eq!(output, Ok(()));
        assert_eq!(environ, Err(ErrorKind::PermissionDenied));
        assert_eq!(system, Ok(()));
    }

    #[test]
    fn no_socket_and_no_other_program() {
        let report = confined(Confinement::default(), |_| {
            (
                std::net::TcpStream::connect("127.0.0.1:9").map_err(|err| err.kind()),
                std::net::UdpSocket::bind("127.0.0.1:0").map_err(|err| err.kind()),
                std::os::unix::net::UnixDatagram::unbound().map_err(|err| err.kind()),
                std::process::Command::new("/usr/bin/env")
                    .status()
                    .map_err(|err| err.kind()),
            )
        });
        let (tcp, udp, unix, program) = report;
        assert_eq!(tcp.map(|_| ()), Err(ErrorKind::PermissionDenied));
        assert_eq!(udp.map(|_| ()), Err(ErrorKind::PermissionDenied));
        assert_eq!(unix.map(|_| ()), Err(ErrorKind::PermissionDenied));
        assert_eq!(program.map(|_| ()), Err(ErrorKind::PermissionDenied));
    }

    #[test]
    fn the_kernel_is_asked_whether_it_has_landlock() {
        assert_ne!(filesystem_isolation(), FilesystemIsolation::Unsupported);
    }
}
