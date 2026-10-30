//! Context-free guest observations (052 §8.11). Mount roots, not cgroup claims,
//! identify the container. Native backend qualification remains a candidate gate.
#![allow(dead_code)] // Joined to the guest entry by the workspace-client owner.

use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};

use super::context::WorkspaceContainerIdentity;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuestSelfObservation {
    pub(crate) container_id: String,
    pub(crate) vm_boot_id: String,
    pub(crate) platform: String,
    pub(crate) libc: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuestIdentityRefusal {
    Unavailable,
    Unproven,
    Ambiguous,
    Malformed,
    TooLarge,
}

pub(crate) trait GuestIdentity {
    fn observe(&mut self) -> Result<GuestSelfObservation, GuestIdentityRefusal>;
}

/// No context, environment variable, hostname, cgroup or Docker socket enters
/// this reader. The two proc files are bounded and the boot is read twice to
/// refuse a mixed observation. Image/ELF evidence is a separate stage check.
#[derive(Debug)]
pub(crate) struct ProcGuestIdentity;

impl ProcGuestIdentity {
    pub(crate) fn process() -> Self {
        Self
    }
}

fn bounded(path: &str, limit: u64) -> Result<Vec<u8>, GuestIdentityRefusal> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| GuestIdentityRefusal::Unavailable)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| GuestIdentityRefusal::Unavailable)?;
    if bytes.len() as u64 > limit {
        return Err(GuestIdentityRefusal::TooLarge);
    }
    Ok(bytes)
}

impl GuestIdentity for ProcGuestIdentity {
    fn observe(&mut self) -> Result<GuestSelfObservation, GuestIdentityRefusal> {
        let boot = bounded("/proc/sys/kernel/random/boot_id", 64)?;
        let mounts = bounded("/proc/self/mountinfo", 1024 * 1024)?;
        let mut observation = observe_proc(&mounts, &boot)?;
        let mut executable =
            std::fs::File::open("/proc/self/exe").map_err(|_| GuestIdentityRefusal::Unavailable)?;
        let (platform, libc) = executable_platform(&mut executable)?;
        observation.platform = platform;
        observation.libc = libc;
        if bounded("/proc/sys/kernel/random/boot_id", 64)? != boot {
            return Err(GuestIdentityRefusal::Ambiguous);
        }
        Ok(observation)
    }
}

fn boot_id(bytes: &[u8]) -> Result<String, GuestIdentityRefusal> {
    let text = std::str::from_utf8(bytes).map_err(|_| GuestIdentityRefusal::Malformed)?;
    let value = text.strip_suffix('\n').unwrap_or(text);
    if value.len() != 36
        || !value.bytes().enumerate().all(|(i, byte)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
        || value == "00000000-0000-0000-0000-000000000000"
    {
        return Err(GuestIdentityRefusal::Malformed);
    }
    Ok(value.to_owned())
}

/// Parse only mountinfo's root column. A source, mountpoint or optional field
/// that happens to contain a container id is not independent root evidence.
fn observe_proc(mounts: &[u8], boot: &[u8]) -> Result<GuestSelfObservation, GuestIdentityRefusal> {
    if mounts.len() > 1024 * 1024 || boot.len() > 64 {
        return Err(GuestIdentityRefusal::TooLarge);
    }
    let vm_boot_id = boot_id(boot)?;
    let mounts = std::str::from_utf8(mounts).map_err(|_| GuestIdentityRefusal::Malformed)?;
    let mut ids = BTreeSet::new();
    for line in mounts.lines() {
        let (head, tail) = line
            .split_once(" - ")
            .ok_or(GuestIdentityRefusal::Malformed)?;
        let fields = head.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 6 || tail.split_whitespace().count() < 3 {
            return Err(GuestIdentityRefusal::Malformed);
        }
        let root = fields.get(3).ok_or(GuestIdentityRefusal::Malformed)?;
        let parts = root.split('/').collect::<Vec<_>>();
        for pair in parts.windows(2) {
            if pair.first() != Some(&"containers") {
                continue;
            }
            let id = pair.get(1).ok_or(GuestIdentityRefusal::Malformed)?;
            if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(GuestIdentityRefusal::Unproven);
            }
            ids.insert(id.to_ascii_lowercase());
        }
    }
    if ids.len() > 1 {
        return Err(GuestIdentityRefusal::Ambiguous);
    }
    let container_id = ids
        .into_iter()
        .next()
        .ok_or(GuestIdentityRefusal::Unproven)?;
    Ok(GuestSelfObservation {
        container_id,
        vm_boot_id,
        platform: String::new(),
        libc: String::new(),
    })
}

impl GuestSelfObservation {
    /// The context is only the expected host claim. All compared facts come
    /// from this independent observation; incomplete image evidence refuses.
    pub(crate) fn compare(
        &self,
        expected: &WorkspaceContainerIdentity,
    ) -> Result<(), GuestIdentityRefusal> {
        if self.platform.is_empty()
            || self.libc.is_empty()
            || !expected
                .container_id
                .eq_ignore_ascii_case(&self.container_id)
            || expected.vm_boot_id != self.vm_boot_id
            || expected.platform != self.platform
            || expected.libc != self.libc
        {
            return Err(GuestIdentityRefusal::Unproven);
        }
        Ok(())
    }
}

fn number<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], GuestIdentityRefusal> {
    bytes
        .get(offset..offset + N)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(GuestIdentityRefusal::Malformed)
}

/// ELF machine and interpreter verify the executable already selected by the
/// host's image observation. They never select an artifact or infer the image
/// from the host architecture. Static/unknown and conflicting loaders refuse.
pub(crate) fn executable_platform(
    reader: &mut (impl Read + Seek),
) -> Result<(String, String), GuestIdentityRefusal> {
    let mut header = [0u8; 64];
    reader
        .read_exact(&mut header)
        .map_err(|_| GuestIdentityRefusal::Unavailable)?;
    if header.get(..7) != Some(b"\x7fELF\x02\x01\x01".as_slice()) {
        return Err(GuestIdentityRefusal::Malformed);
    }
    let platform = match u16::from_le_bytes(number(&header, 18)?) {
        62 => "x86_64",
        183 => "aarch64",
        _ => return Err(GuestIdentityRefusal::Unproven),
    };
    let start = u64::from_le_bytes(number(&header, 32)?);
    let width = u16::from_le_bytes(number(&header, 54)?);
    let count = u16::from_le_bytes(number(&header, 56)?);
    if width != 56 || count == 0 || count > 128 {
        return Err(GuestIdentityRefusal::Malformed);
    }
    let mut loader = None;
    for index in 0..count {
        let offset = start
            .checked_add(u64::from(index) * u64::from(width))
            .ok_or(GuestIdentityRefusal::Malformed)?;
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|_| GuestIdentityRefusal::Unavailable)?;
        let mut program = [0u8; 56];
        reader
            .read_exact(&mut program)
            .map_err(|_| GuestIdentityRefusal::Unavailable)?;
        if u32::from_le_bytes(number(&program, 0)?) != 3 {
            continue;
        }
        if loader.is_some() {
            return Err(GuestIdentityRefusal::Ambiguous);
        }
        let offset = u64::from_le_bytes(number(&program, 8)?);
        let size = u64::from_le_bytes(number(&program, 32)?);
        if !(2..=256).contains(&size) {
            return Err(GuestIdentityRefusal::Malformed);
        }
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|_| GuestIdentityRefusal::Unavailable)?;
        let mut path = vec![0u8; size as usize];
        reader
            .read_exact(&mut path)
            .map_err(|_| GuestIdentityRefusal::Unavailable)?;
        if path.pop() != Some(0) || path.contains(&0) {
            return Err(GuestIdentityRefusal::Malformed);
        }
        let path = std::str::from_utf8(&path).map_err(|_| GuestIdentityRefusal::Malformed)?;
        loader = Some(match (platform, path) {
            ("x86_64", "/lib64/ld-linux-x86-64.so.2")
            | ("aarch64", "/lib/ld-linux-aarch64.so.1") => "gnu",
            ("x86_64", "/lib/ld-musl-x86_64.so.1") | ("aarch64", "/lib/ld-musl-aarch64.so.1") => {
                "musl"
            }
            _ => return Err(GuestIdentityRefusal::Unproven),
        });
    }
    Ok((
        platform.to_owned(),
        loader.ok_or(GuestIdentityRefusal::Unproven)?.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT: &[u8] = b"3a7c86df-0045-472e-b065-c1295816ab22\n";

    fn mount(root: &str) -> String {
        format!("100 99 0:1 {root} /etc/hostname rw - ext4 /dev/vda rw\n")
    }

    #[test]
    fn workspace_identity_backend_shaped_mount_roots_and_retries() {
        let id = "a1".repeat(32);
        for prefix in [
            "/var/lib/docker",
            "/mnt/docker-desktop-disk/data/docker",
            "/docker",
        ] {
            let mounts = mount(&format!("{prefix}/containers/{id}/hostname"));
            let first = observe_proc(mounts.as_bytes(), BOOT).expect("full mount root");
            assert_eq!(first.container_id, id);
            assert_eq!(observe_proc(mounts.as_bytes(), BOOT), Ok(first));
        }
    }

    #[test]
    fn workspace_identity_cgroup_context_hostname_and_prefix_never_prove_placement() {
        let id = "a1".repeat(32);
        // A copied context/cgroup id in a source or mountpoint grants nothing.
        for mounts in [
            mount("/"),
            format!("100 99 0:1 / /containers/{id} rw - ext4 /containers/{id} rw\n"),
            mount("/docker/containers/a1/hostname"),
        ] {
            assert_eq!(
                observe_proc(mounts.as_bytes(), BOOT),
                Err(GuestIdentityRefusal::Unproven)
            );
        }
    }

    #[test]
    fn workspace_identity_refuses_conflicts_unknown_boot_and_malformed_records() {
        let mounts = mount(&format!("/docker/containers/{}/hostname", "a1".repeat(32)));
        let other = mount(&format!("/docker/containers/{}/hostname", "b2".repeat(32)));
        assert_eq!(
            observe_proc(format!("{mounts}{other}").as_bytes(), BOOT),
            Err(GuestIdentityRefusal::Ambiguous)
        );
        for boot in [
            b"".as_slice(),
            b"copied-boot",
            b"00000000-0000-0000-0000-000000000000",
        ] {
            assert_eq!(
                observe_proc(mounts.as_bytes(), boot),
                Err(GuestIdentityRefusal::Malformed)
            );
        }
        assert_eq!(
            observe_proc(b"unreadable mount record", BOOT),
            Err(GuestIdentityRefusal::Malformed)
        );
        assert_eq!(
            observe_proc(&vec![b'x'; 1024 * 1024 + 1], BOOT),
            Err(GuestIdentityRefusal::TooLarge)
        );
    }
}

#[cfg(test)]
mod elf_tests {
    use super::*;

    fn elf(machine: u16, interpreter: &str) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&3u32.to_le_bytes());
        bytes[72..80].copy_from_slice(&128u64.to_le_bytes());
        bytes[96..104].copy_from_slice(&((interpreter.len() + 1) as u64).to_le_bytes());
        bytes[128..128 + interpreter.len()].copy_from_slice(interpreter.as_bytes());
        bytes
    }

    #[test]
    fn workspace_identity_observes_elf_machine_and_single_loader_without_host_uname() {
        for (machine, architecture, path, libc) in [
            (62, "x86_64", "/lib64/ld-linux-x86-64.so.2", "gnu"),
            (183, "aarch64", "/lib/ld-linux-aarch64.so.1", "gnu"),
            (62, "x86_64", "/lib/ld-musl-x86_64.so.1", "musl"),
            (183, "aarch64", "/lib/ld-musl-aarch64.so.1", "musl"),
        ] {
            assert_eq!(
                executable_platform(&mut std::io::Cursor::new(elf(machine, path))),
                Ok((architecture.into(), libc.into()))
            );
        }
        assert!(executable_platform(&mut std::io::Cursor::new(elf(62, "/lib/unknown"))).is_err());
        assert!(executable_platform(&mut std::io::Cursor::new(elf(
            183,
            "/lib64/ld-linux-x86-64.so.2"
        )))
        .is_err());
        assert!(executable_platform(&mut std::io::Cursor::new(b"not an ELF".to_vec())).is_err());
    }
}
