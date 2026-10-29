//! macOS: an NSOpenPanel that greys out every file but the extensions asked for
//! (`allowedContentTypes`), which GPUI's picker can't.
use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSModalResponse, NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{NSArray, NSString};
use objc2_uniform_type_identifiers::UTType;
use std::cell::Cell;
use std::path::PathBuf;

/// Shows the picker `pick` asks for, limited to its extensions, and answers `done` once the user
/// has chosen: the paths, `None` when cancelled. `false` when it shows none (off the main thread,
/// or no extension is asked for), and `done` is dropped.
pub fn pick(pick: &pumi_os::Pick, done: Box<dyn FnOnce(Option<Vec<PathBuf>>) + Send>) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    // An extension no app declares is a dynamic type of its own, which still fits its files.
    let types: Vec<Retained<UTType>> =
        pick.extensions.iter().filter_map(|ext| UTType::typeWithFilenameExtension(&NSString::from_str(ext))).collect();
    if types.is_empty() {
        return false;
    }
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(pick.kind == pumi_os::PickKind::File);
    panel.setCanChooseDirectories(pick.kind == pumi_os::PickKind::Directory);
    panel.setAllowsMultipleSelection(pick.multiple);
    panel.setCanCreateDirectories(true);
    panel.setResolvesAliases(false);
    panel.setAllowedContentTypes(&NSArray::from_retained_slice(&types));
    if let Some(prompt) = &pick.prompt {
        panel.setPrompt(Some(&NSString::from_str(prompt)));
    }
    let done = Cell::new(Some(done));
    let answered = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        let Some(done) = done.take() else { return };
        if response != NSModalResponseOK {
            return done(None);
        }
        done(Some(answered.URLs().iter().filter_map(|url| url.to_file_path()).collect()));
    });
    panel.beginWithCompletionHandler(&handler);
    true
}
