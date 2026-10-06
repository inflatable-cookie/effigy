//! Private GNU-findutils + native-chown acceptance against a disposable
//! Colima/nerdctl container. Production permission prep never shells out to
//! `colima` here; this module exists so the runtime/container drift guard can
//! allow the fixture without waiving `workspace_permissions.rs`.
//!
//! Marked `#[ignore]` so hosted CI (which has no Colima) does not run it. The
//! `test:workspace:rust-ownership:bulk` selector runs it with `--ignored` and
//! it FAILS, never skips, when the runtime or image is unavailable.

#![cfg(all(test, unix))]

use std::time::Instant;

use super::bulk_chown_argv;

const GNU_ACCEPT_IMAGE: &str = "soundcheck-linux-arm-builder:local";
const GNU_ACCEPT_STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

/// Installs a counting wrapper for `chown` ahead of /usr/bin on PATH. It
/// always ends in the real native chown; marker files in /tmp/ctl add a
/// one-shot intermediate-directory swap or a failure on a deep path.
const GNU_ACCEPT_SETUP: &str = r#"set -eu
mkdir -p /tmp/ctl /tmp/outside
cat > /usr/local/bin/chown <<'WRAP'
#!/bin/sh
echo x >> /tmp/ctl/calls
if [ -e /tmp/ctl/swap ] && [ ! -e /tmp/ctl/swapped ]; then
  : > /tmp/ctl/swapped
  mv /tmp/race/vol/a /tmp/race/vol/a.real
  ln -s /tmp/race/outside /tmp/race/vol/a
fi
if [ -e /tmp/ctl/fail ]; then
  for p in "$@"; do
case "$p" in *let-lambda-and-callables.mdx)
  echo "chown: cannot access '$(pwd)/$p': Permission denied" >&2
  exit 1;;
esac
  done
fi
exec /usr/bin/chown "$@"
WRAP
chmod 755 /usr/local/bin/chown
printf secret > /tmp/outside/secret
/usr/bin/chown 0:12 /tmp/outside/secret
"#;

/// args: root base total deep(0/1) link(0/1)
const GNU_ACCEPT_MAKE: &str = r#"set -eu
root=$1; base=$2; total=$3; deep=$4; link=$5
dirs=40
mkdir -p "$root/$base"
i=0; while [ $i -lt $dirs ]; do mkdir "$root/$base/d$i"; i=$((i+1)); done
if [ "$deep" = 1 ]; then
  mkdir -p "$root/$base/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs"
  printf deep > "$root/$base/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs/let-lambda-and-callables.mdx"
fi
if [ "$base" = debug ]; then printf lock > "$root/debug/.cargo-build-lock"; fi
if [ "$link" = 1 ]; then ln -s /tmp/outside/secret "$root/$base/escape-link"; fi
have=$(find -P "$root" | wc -l)
f=0; need=$((total-have))
while [ $f -lt $need ]; do
  printf 'content-%s-%s' "$root" "$f" > "$root/$base/d$((f%dirs))/f$f"; f=$((f+1))
done
# mixed owners: d0-d9 already 501:20, d10-d19 foreign 1000:1000, rest root
i=0; while [ $i -lt 10 ]; do /usr/bin/chown -R 501:20 "$root/$base/d$i"; i=$((i+1)); done
while [ $i -lt 20 ]; do /usr/bin/chown -R 1000:1000 "$root/$base/d$i"; i=$((i+1)); done
"#;

/// args: root uid gid -> entries / content manifest hash / not-owned count
const GNU_ACCEPT_SNAP: &str = r#"set -eu
root=$1; uid=$2; gid=$3
echo "entries=$(find -P "$root" -xdev | wc -l)"
echo "hash=$( (find -P "$root" -xdev -type f -exec sha256sum {} + ; find -P "$root" -xdev -type l -printf '%p -> %l\n') | sort | sha256sum | cut -d' ' -f1)"
echo "unowned=$(find -P "$root" -xdev ! -type l ! \( -user "$uid" -a -group "$gid" \) | wc -l)"
"#;

/// args: uid gid dir... -> read/write/create-and-remove as the numeric user
const GNU_ACCEPT_ACCESS: &str = r#"u=$1; g=$2; shift 2
exec setpriv --reuid "$u" --regid "$g" --clear-groups sh -c 'for d; do
  test -r "$d" && test -w "$d" || exit 1
  : > "$d/.effigy-write-probe" && rm -f "$d/.effigy-write-probe" || exit 2
done' sh "$@""#;

#[cfg(unix)]
struct AcceptContainer {
    name: String,
}

#[cfg(unix)]
impl Drop for AcceptContainer {
    fn drop(&mut self) {
        let _ = std::process::Command::new("colima")
            .args(["-p", "effigy", "nerdctl", "--", "rm", "-f", &self.name])
            .output();
    }
}

#[cfg(unix)]
fn nerd(args: &[&str]) -> (bool, String, String, std::time::Duration) {
    let dir = tempfile::tempdir().expect("tmp");
    let out_path = dir.path().join("out");
    let err_path = dir.path().join("err");
    let mut child = std::process::Command::new("colima")
        .args(["-p", "effigy", "nerdctl", "--"])
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&out_path).expect("out"))
        .stderr(std::fs::File::create(&err_path).expect("err"))
        .spawn()
        .unwrap_or_else(|error| panic!("BLOCKER: cannot run colima nerdctl: {error}"));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if started.elapsed() > GNU_ACCEPT_STEP_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("acceptance step timed out and its child was reaped: {args:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    (
        status.success(),
        std::fs::read_to_string(&out_path).unwrap_or_default(),
        std::fs::read_to_string(&err_path).unwrap_or_default(),
        started.elapsed(),
    )
}

#[cfg(unix)]
fn kv(text: &str, key: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in {text:?}"))
        .to_owned()
}

#[cfg(unix)]
#[test]
#[ignore = "private container acceptance; run via test:workspace:rust-ownership:bulk"]
fn bulk_gnu_container_full_acceptance() {
    let name = format!(
        "effigy-bulk-accept-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let (ok, _, err, _) = nerd(&[
        "run",
        "-d",
        "--network",
        "none",
        "--name",
        &name,
        GNU_ACCEPT_IMAGE,
        "sleep",
        "3000",
    ]);
    assert!(ok, "BLOCKER: cannot start private fixture container: {err}");
    let _guard = AcceptContainer { name: name.clone() };
    let exec = |argv: &[&str]| {
        let mut args = vec!["exec", name.as_str()];
        args.extend_from_slice(argv);
        nerd(&args)
    };
    let sh = |script: &str, extra: &[&str]| {
        let mut argv = vec!["sh", "-c", script, "sh"];
        argv.extend_from_slice(extra);
        let (ok, out, err, took) = exec(&argv);
        assert!(ok, "script failed: {err}\n{out}");
        (out, took)
    };

    let find_version = sh("find --version | head -1", &[]).0;
    assert!(find_version.contains("GNU findutils"), "{find_version}");
    sh(GNU_ACCEPT_SETUP, &[]);

    let vols: [(&str, &str, usize, &str, &str); 3] = [
        ("/tmp/fx/cargo_registry", "src", 31497, "0", "1"),
        ("/tmp/fx/target", "debug", 9709, "0", "0"),
        ("/tmp/fx/cargo_git", "checkouts", 3133, "1", "0"),
    ];
    let build_started = Instant::now();
    for (root, base, total, deep, link) in vols {
        sh(
            GNU_ACCEPT_MAKE,
            &[root, base, &total.to_string(), deep, link],
        );
    }
    println!("fixture built in {:?}", build_started.elapsed());

    let snap = |root: &str, uid: u32, gid: u32| {
        let out = sh(GNU_ACCEPT_SNAP, &[root, &uid.to_string(), &gid.to_string()]).0;
        (
            kv(&out, "entries").parse::<usize>().expect("entries"),
            kv(&out, "hash"),
            kv(&out, "unowned").parse::<usize>().expect("unowned"),
        )
    };
    let reset_calls = || {
        sh(": > /tmp/ctl/calls", &[]);
    };
    let calls = || -> usize {
        sh("wc -l < /tmp/ctl/calls", &[])
            .0
            .trim()
            .parse()
            .expect("calls")
    };

    let before: Vec<_> = vols.iter().map(|v| snap(v.0, 501, 20)).collect();
    let total_entries: usize = before.iter().map(|b| b.0).sum();
    assert_eq!(total_entries, 44339, "fixture entry count");
    for (vol, b) in vols.iter().zip(&before) {
        assert_eq!(b.0, vol.2);
        assert!(b.2 > 0, "{} must start with unowned entries", vol.0);
    }
    let mixed = sh(
        "find -P /tmp/fx -xdev ! -type l -printf '%U:%G\\n' | sort | uniq -c",
        &[],
    )
    .0;
    println!("owners before:\n{mixed}");
    assert!(mixed.contains("0:0") && mixed.contains("1000:1000") && mixed.contains("501:20"));

    // --- pass A: repair to 501:20, production argv, one exec per volume.
    let run_bulk = |uid: u32, gid: u32| {
        reset_calls();
        let mut elapsed = std::time::Duration::ZERO;
        let mut execs = 0usize;
        for vol in &vols {
            let argv = bulk_chown_argv(vol.0, uid, gid);
            let mut full = vec!["exec", name.as_str()];
            full.extend(argv.iter().map(String::as_str));
            let (ok, out, err, took) = nerd(&full);
            assert!(ok, "bulk find failed on {}: {err}\n{out}", vol.0);
            elapsed += took;
            execs += 1;
        }
        (execs, elapsed, calls())
    };
    let (execs_a, took_a, chowns_a) = run_bulk(501, 20);
    println!(
        "bulk 501:20: runtime execs={execs_a} chown invocations={chowns_a} real elapsed={took_a:?} \
         (legacy model: {} execs ~{}ms @25ms/exec)",
        2 * total_entries,
        2 * total_entries as u64 * 25
    );
    assert_eq!(execs_a, 3);
    assert!(chowns_a < total_entries / 50, "GNU batching: {chowns_a}");
    for (vol, b) in vols.iter().zip(&before) {
        let after = snap(vol.0, 501, 20);
        assert_eq!(after.0, b.0, "entry count preserved {}", vol.0);
        assert_eq!(after.1, b.1, "content manifest preserved {}", vol.0);
        assert_eq!(after.2, 0, "all entries owned 501:20 in {}", vol.0);
    }
    // Numeric 501:20 can read/write/create in the Rust-critical dirs.
    let dirs = [
        "/tmp/fx/target/debug",
        "/tmp/fx/cargo_git/checkouts",
        "/tmp/fx/cargo_git/checkouts/formualizer-f6140fface89ddae/2a8303d/docs-site/content/docs",
        "/tmp/fx/cargo_registry/src",
        "/tmp/fx/cargo_registry/src/d12",
    ];
    let mut access = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "501", "20"];
    access.extend(dirs);
    let (ok, _, err, _) = exec(&access);
    assert!(ok, "501:20 access/create failed: {err}");
    let (ok, _, _, _) = exec(&[
        "setpriv",
        "--reuid",
        "501",
        "--regid",
        "20",
        "--clear-groups",
        "sh",
        "-c",
        ": >> /tmp/fx/target/debug/.cargo-build-lock",
    ]);
    assert!(ok, "501:20 cannot write build lock");
    // Ownership is real, not world-writable: another uid cannot write.
    let mut other = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "1000", "1000"];
    other.push("/tmp/fx/target/debug");
    assert!(!exec(&other).0, "uid 1000 must not gain write via repair");
    // Protected symlink target unchanged.
    let secret = sh(
        "stat -c '%u:%g' /tmp/outside/secret; cat /tmp/outside/secret",
        &[],
    )
    .0;
    assert_eq!(secret, "0:12\nsecret", "symlink escape target untouched");

    // --- idempotence: second run does no chown work at all.
    let (_, _, chowns_idem) = run_bulk(501, 20);
    assert_eq!(chowns_idem, 0, "idempotent run must not invoke chown");

    // --- second numeric identity 1000:1000, mixed starting owners.
    let (_, took_b, chowns_b) = run_bulk(1000, 1000);
    println!("bulk 1000:1000: chown invocations={chowns_b} real elapsed={took_b:?}");
    for (vol, b) in vols.iter().zip(&before) {
        let after = snap(vol.0, 1000, 1000);
        assert_eq!((after.0, &after.1, after.2), (b.0, &b.1, 0));
    }
    let mut access = vec!["sh", "-c", GNU_ACCEPT_ACCESS, "sh", "1000", "1000"];
    access.extend(dirs);
    let (ok, _, err, _) = exec(&access);
    assert!(ok, "1000:1000 access/create failed: {err}");

    // --- deep failed path after partial progress (not-ready, content kept).
    sh("/usr/bin/chown -R 0:0 /tmp/fx/cargo_git", &[]);
    let git_before = snap("/tmp/fx/cargo_git", 501, 20);
    sh(": > /tmp/ctl/fail", &[]);
    let argv = bulk_chown_argv("/tmp/fx/cargo_git", 501, 20);
    let mut full = vec!["exec", name.as_str()];
    full.extend(argv.iter().map(String::as_str));
    let (ok, _, err, _) = nerd(&full);
    assert!(!ok, "deep chown failure must fail the bulk exec");
    assert!(err.contains("let-lambda-and-callables.mdx"), "{err}");
    let git_partial = snap("/tmp/fx/cargo_git", 501, 20);
    assert!(
        git_partial.2 > 0,
        "failing volume must still be unowned/not-ready"
    );
    assert!(
        git_partial.2 < git_before.2,
        "some entries repaired before failure"
    );
    assert_eq!(
        (git_partial.0, &git_partial.1),
        (git_before.0, &git_before.1)
    );
    sh("rm -f /tmp/ctl/fail", &[]);
    let (ok, _, err, _) = nerd(&full);
    assert!(ok, "retry after the failure clears: {err}");
    assert_eq!(snap("/tmp/fx/cargo_git", 501, 20).2, 0);

    // --- intermediate directory swap: production argv vs legacy -exec.
    let race = |argv: &[String]| -> String {
        sh(
            "rm -rf /tmp/race /tmp/ctl/swapped; mkdir -p /tmp/race/vol/a /tmp/race/outside; \
             printf inside > /tmp/race/vol/a/f; printf outside > /tmp/race/outside/f; \
             /usr/bin/chown 0:12 /tmp/race/outside/f; : > /tmp/ctl/swap",
            &[],
        );
        let mut full = vec!["exec", name.as_str()];
        full.extend(argv.iter().map(String::as_str));
        let _ = nerd(&full);
        sh("rm -f /tmp/ctl/swap", &[]);
        sh(
            "stat -c '%u:%g' /tmp/race/outside/f; cat /tmp/race/outside/f",
            &[],
        )
        .0
    };
    let safe = race(&bulk_chown_argv("/tmp/race/vol", 501, 20));
    assert_eq!(
        safe, "0:12\noutside",
        "execdir must leave outside file untouched"
    );
    let mut legacy = bulk_chown_argv("/tmp/race/vol", 501, 20);
    for part in legacy.iter_mut() {
        if part == "-execdir" {
            *part = "-exec".to_owned();
        }
    }
    let escaped = race(&legacy);
    assert_eq!(
        escaped, "501:20\noutside",
        "negative control must reproduce the escape"
    );
}
