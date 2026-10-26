//! The plugins that draw files: which ones could draw a `file` tab, by the globs their
//! manifests' `renderers.file.match` give (`registry.list("viewer", {path})`). Which one does is
//! the app's choice; it never opens a file.

/// The core's block for a file.
const FILE: &str = "file";

/// Every enabled plugin that draws `file` tabs, with how well it fits `path`; `None` for a fit
/// is one whose `match` does not.
pub(crate) fn renderers(path: &str) -> Vec<(String, String, Option<usize>)> {
    crate::contributions::plugins()
        .into_iter()
        .filter_map(|p| {
            let renderer = p.manifest.contributes.renderers.get(FILE)?;
            Some((p.id.clone(), renderer.export().to_string(), renderer.fits(path)))
        })
        .collect()
}
