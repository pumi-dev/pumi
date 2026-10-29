//! Elsewhere: GPUI's picker, which shows every file; `fs.pick` checks what was chosen.
use std::path::PathBuf;

/// Shows no picker of its own: `false`, and `done` is dropped.
pub fn pick(_pick: &pumi_os::Pick, _done: Box<dyn FnOnce(Option<Vec<PathBuf>>) + Send>) -> bool {
    false
}
