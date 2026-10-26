//! `fs.trash`: the `trash` crate, on every OS (the Finder on macOS, so a file can be put back;
//! the FreeDesktop trash on Linux; the Recycle Bin on Windows).
use pumi_api::{Cx, Error, Result, api};
use serde::{Deserialize, Serialize};

/// The paths `fs.trash` moves: at least one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paths(pub Vec<String>);

impl schemars::JsonSchema for Paths {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Paths".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "array", "items": { "type": "string" }, "minItems": 1 })
    }
}

/// Move paths to the Trash.
#[api(name = "fs.trash", runs_on = "server", example(args = r#"[["/tmp/old.log"]]"#, result = "null"))]
pub fn trash(_cx: &mut Cx, paths: Paths) -> Result<()> {
    ::trash::delete_all(&paths.0).map_err(|err| Error::exception(format!("fs.trash: {err}")))
}
