//! Legacy container-only rendering until the remote custody cutover (#236).
use effigy_core::shell::shell_quote;

pub fn render_legacy_step(command: &str, timeout_ms: Option<u64>) -> String {
    match timeout_ms {
        None => command.to_owned(),
        Some(ms) => format!(
            "python3 -c 'import subprocess,sys\ntry:\n r=subprocess.run([\"sh\",\"-lc\",sys.argv[2]], timeout=float(sys.argv[1]))\n sys.exit(r.returncode)\nexcept subprocess.TimeoutExpired:\n sys.exit(124)' {} {}",
            (ms as f64) / 1000.0, shell_quote(command)),
    }
}
