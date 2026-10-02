#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

const DEFAULT_WAIT_SECS: u64 = 30 * 60;

pub(super) fn default_caller_identity() -> String {
    #[cfg(unix)]
    {
        format!("interactive:{}", unsafe { libc::geteuid() })
    }
    #[cfg(not(unix))]
    {
        format!(
            "interactive:{}",
            std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_owned())
        )
    }
}

pub(super) fn requested_reservation_units() -> Result<(u32, u64), String> {
    let cores = std::thread::available_parallelism()
        .map_err(|error| format!("cannot measure host logical CPUs: {error}"))?
        .get() as u32;
    let memory_bytes = physical_memory_bytes()
        .ok_or_else(|| "cannot measure host physical memory; heavy tasks fail closed".to_owned())?;
    let default_cpu = (cores / 2).max(1);
    let default_memory = (memory_bytes / 2) / (1024 * 1024);
    let budget_cpu = env_u32("EFFIGY_ADMISSION_CPU_BUDGET")?.unwrap_or(default_cpu);
    let budget_memory = env_u64("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB")?.unwrap_or(default_memory);
    if budget_cpu == 0 || budget_memory == 0 {
        return Err("admission CPU and memory budgets must both be greater than zero".to_owned());
    }
    let cpu_units = env_u32("EFFIGY_ADMISSION_CPU_UNITS")?.unwrap_or(budget_cpu);
    let memory_mib = env_u64("EFFIGY_ADMISSION_MEMORY_MIB")?.unwrap_or(budget_memory);
    if cpu_units == 0 || memory_mib == 0 {
        return Err("admission task reservation must be greater than zero".to_owned());
    }
    Ok((cpu_units, memory_mib))
}

pub(super) fn capacity_wait_secs() -> Result<u64, String> {
    Ok(env_u64("EFFIGY_ADMISSION_TIMEOUT_SECS")?.unwrap_or(DEFAULT_WAIT_SECS))
}

fn env_u32(key: &str) -> Result<Option<u32>, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| format!("{key} must be a positive integer"))
        })
        .transpose()
}

fn env_u64(key: &str) -> Result<Option<u64>, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{key} must be a positive integer"))
        })
        .transpose()
}

fn physical_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let contents = fs::read_to_string("/proc/meminfo").ok()?;
        let line = contents
            .lines()
            .find(|line| line.starts_with("MemTotal:"))?;
        let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        Some(kib * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
