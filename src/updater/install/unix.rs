//! Unix: the swap is a `/bin/sh` script in a session of its own.
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

/// The swap script; its inputs are environment variables (`super::swap`, and `PUMI_OS`).
const SWAP: &str = r#"
set -u
launch() {
  if [ "$PUMI_OS" = macos ]; then
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$PUMI_APP" >/dev/null 2>&1
    open "$PUMI_APP"
  else
    nohup "$PUMI_APP/bin/pumi" >/dev/null 2>&1 &
  fi
}
i=0
while kill -0 "$PUMI_PID" 2>/dev/null; do
  sleep 0.2; i=$((i + 1))
  [ "$i" -gt 600 ] && exit 1
done
rm -rf "$PUMI_OLD"
mv "$PUMI_APP" "$PUMI_OLD" || { [ "$PUMI_RELAUNCH" = 1 ] && launch; exit 1; }
if ! mv "$PUMI_NEW" "$PUMI_APP"; then
  mv "$PUMI_OLD" "$PUMI_APP"
  [ "$PUMI_RELAUNCH" = 1 ] && launch
  exit 1
fi
[ "$PUMI_OS" = macos ] && xattr -dr com.apple.quarantine "$PUMI_APP" 2>/dev/null
rm -f "$PUMI_MARKER"
[ "$PUMI_RELAUNCH" = 1 ] || exit 0
launch
i=0
while [ "$i" -lt "$PUMI_ROLLBACK_AFTER" ]; do
  [ -f "$PUMI_MARKER" ] && { rm -rf "$PUMI_OLD"; exit 0; }
  sleep 1; i=$((i + 1))
done
# The new version never said it works: put the old one back. Only the app is ended, never
# the server beside it (`pumid`), which holds every terminal.
pkill -f "^$PUMI_EXE\$" 2>/dev/null
sleep 1
rm -rf "$PUMI_APP.failed"
mv "$PUMI_APP" "$PUMI_APP.failed" && mv "$PUMI_OLD" "$PUMI_APP" && launch
"#;

/// Starts the swap script with `env`, plus `PUMI_OS` (`macos`, `linux`).
pub fn swap(env: &[(&str, OsString)]) -> std::io::Result<()> {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SWAP)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .env("PUMI_OS", std::env::consts::OS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Its own session, so nothing that ends the app (a terminal's hangup, a process group's
    // signal) ends the script with it.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The script against fake installs: a swap, then a relaunch that never reports healthy
    /// and is rolled back.
    #[test]
    fn swap_script_swaps_and_rolls_back() {
        let root = std::env::temp_dir().join(format!("pumi-swap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (app, new, old) = (root.join("pumi.app"), root.join("new/pumi.app"), root.join(".pumi.app.old"));
        let script = |relaunch: &str, marker: &Path| {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(SWAP)
                .env("PUMI_PID", "999999")
                .env("PUMI_OS", "test")
                .env("PUMI_APP", &app)
                .env("PUMI_NEW", &new)
                .env("PUMI_EXE", app.join("bin/pumi"))
                .env("PUMI_OLD", &old)
                .env("PUMI_MARKER", marker)
                .env("PUMI_RELAUNCH", relaunch)
                .env("PUMI_ROLLBACK_AFTER", "1")
                .status()
                .unwrap()
        };
        let fresh = |dir: &Path, version: &str| {
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            std::fs::write(dir.join("VERSION"), version).unwrap();
        };
        let version = |dir: &Path| std::fs::read_to_string(dir.join("VERSION")).unwrap();

        fresh(&app, "1");
        fresh(&new, "2");
        assert!(script("0", &root.join("marker")).success());
        assert_eq!(version(&app), "2");
        assert_eq!(version(&old), "1", "the old copy waits for the new one to work");

        fresh(&new, "3");
        let _ = script("1", &root.join("never-written"));
        assert_eq!(version(&app), "2", "rolled back");
        assert_eq!(version(&root.join("pumi.app.failed")), "3");
        let _ = std::fs::remove_dir_all(&root);
    }
}
