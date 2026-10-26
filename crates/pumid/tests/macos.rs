#![cfg(target_os = "macos")]
//! A tab's first process on macOS, spawned disclaimed through pumid's `--tty-exec` trampoline.
use pumi_sys::pty::{Signal, Spawn, spawn, use_trampoline, wait};
use std::ffi::c_char;
use std::io::Read;

fn run(spec: &Spawn) -> (u32, String) {
    let (pty, mut reader) = spawn(spec).unwrap();
    let pid = pty.pid();
    let drain = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        out
    });
    assert_eq!(wait(pid), Some(0));
    drop(pty);
    (pid, String::from_utf8_lossy(&drain.join().unwrap()).into_owned())
}

fn spec(script: &str, disclaim: bool) -> Spawn {
    Spawn {
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
        argv0: Some("my-sh".into()),
        cwd: "/usr".into(),
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        cols: 77,
        rows: 13,
        disclaim,
    }
}

/// zsh opens its tty with `O_NOCTTY`: the controlling terminal is the trampoline's doing, and
/// without one Ctrl+C is a byte, not SIGINT.
#[test]
fn a_disclaimed_zsh_has_a_controlling_terminal() {
    use_trampoline(env!("CARGO_BIN_EXE_pumid").into());
    let mut zsh = spec("", true);
    zsh.program = "/bin/zsh".into();
    zsh.args = vec!["-f".into(), "-c".into(), "ps -o tpgid= -p $$; echo ok > /dev/tty && echo ctty".into()];
    let (pid, out) = run(&zsh);
    assert!(out.contains("ctty"), "{out:?}");
    assert_eq!(out.split_whitespace().next(), Some(pid.to_string().as_str()), "the foreground group: {out:?}");
}

#[test]
fn a_disclaimed_child_is_responsible_for_itself() {
    unsafe extern "C" {
        fn responsibility_get_responsible_for_pid(
            pid: i32,
            responsible: *mut i32,
            unique: *mut u64,
            len: *mut usize,
            path: *mut c_char,
        ) -> i32;
    }
    // The responsible process is a path: the app whose prompt it is.
    fn responsible(pid: u32) -> String {
        let (mut rpid, mut unique, mut path) = (0, 0u64, [0 as c_char; 4096]);
        let mut len = path.len();
        let code = unsafe {
            responsibility_get_responsible_for_pid(pid as i32, &mut rpid, &mut unique, &mut len, path.as_mut_ptr())
        };
        assert_eq!(code, 0);
        unsafe { std::ffi::CStr::from_ptr(path.as_ptr()) }.to_string_lossy().into_owned()
    }
    let ours = responsible(std::process::id());
    for disclaim in [false, true] {
        let (pty, _reader) = spawn(&spec("sleep 1", disclaim)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let theirs = responsible(pty.pid());
        assert_eq!(theirs != ours, disclaim, "disclaim {disclaim}: {theirs} vs {ours}");
        pty.signal(Signal::Kill);
        wait(pty.pid());
    }
}
