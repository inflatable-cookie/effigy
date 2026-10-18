//! These same live-platform assertions run on macOS and Linux CI through the
//! workspace's default members; no container or external command is required.
use effigy_kernel::ident::*;
use effigy_kernel::observe::*;
#[test]
fn platform_boot_process_and_continuous_clock_are_positive_kernel_facts() {
    let boot = boot_identity()
        .verified()
        .expect("supported platform boot identity")
        .clone();
    assert_eq!(boot_identity().verified(), Some(&boot));
    let pid = std::process::id();
    let process = process_identity(pid)
        .verified()
        .expect("own process identity")
        .clone();
    assert_eq!(process.pid, pid);
    assert_eq!(process.boot, boot);
    assert_eq!(process_identity(pid).verified(), Some(&process));
    #[cfg(target_os = "macos")]
    assert!(matches!(
        process.start,
        ProcessStartIdentity::MacOs { seconds: 1.., .. }
    ));
    #[cfg(target_os = "linux")]
    assert!(matches!(
        process.start,
        ProcessStartIdentity::LinuxTicks(1..)
    ));
    let first = boot_clock().verified().expect("first boot clock").clone();
    let second = boot_clock().verified().expect("second boot clock").clone();
    assert!(second.elapsed_since(&first).is_some());
    assert_eq!(second.boot, boot);
    assert!(matches!(process_identity(0), Observed::Unknown(_)));
    assert!(matches!(process_identity(u32::MAX), Observed::Unknown(_)));
}
