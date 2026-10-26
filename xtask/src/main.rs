mod budget;
mod generate;
mod platform;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("api") => api(args.iter().any(|a| a == "--check")),
        Some("budget") => budget::run(),
        Some("platform") => platform::run(),
        _ => {
            eprintln!("usage: cargo xtask api [--check] | cargo xtask budget | cargo xtask platform");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask is in the workspace root").to_path_buf()
}
fn current_api() -> pumi_api::ApiInfo {
    pumid::api::link();
    pumi_host::api::link();
    pumi_os::link();
    let mut info = pumi_api::ApiInfo::new(pumi_api::Registry::new().functions());
    for f in &mut info.functions {
        let module = pumi_api::Registry::core(&f.name).map(|c| c.module).unwrap_or_default();
        f.provider = if module.starts_with("pumid") || module.starts_with("pumi_api") { "core" } else { "host" }.into();
    }
    info
}
fn api(check: bool) -> Result<(), String> {
    let info = current_api();
    let failing: Vec<String> = pumi_api::Registry::core_functions()
        .into_iter()
        .filter_map(|function| function.check_examples().err().map(|err| err.message))
        .collect();
    if !failing.is_empty() {
        return Err(failing.join("\n"));
    }
    // The app has no wire-only functions of its own.
    let internal: Vec<pumi_api::FunctionInfo> = Vec::new();
    let files = [
        ("api/pumi-api.json", generate::json(&info)),
        ("api/app-functions.json", generate::app_functions(&info, &internal)),
    ];
    let root = root();
    let mut stale = Vec::new();
    for (path, contents) in &files {
        let full = root.join(path);
        let current = std::fs::read_to_string(&full).unwrap_or_default();
        if &current == contents {
            continue;
        }
        if check {
            stale.push(*path);
        } else {
            std::fs::create_dir_all(full.parent().unwrap()).map_err(|err| format!("{path}: {err}"))?;
            std::fs::write(&full, contents).map_err(|err| format!("{path}: {err}"))?;
            println!("wrote {path}");
        }
    }
    // The SDK's generated files (runtime/app/worker/sdk/*.gen.ts) follow the export: runtime/app/gen
    // writes them, after it.
    let mut sdk = std::process::Command::new("bun");
    sdk.arg(root.join("runtime/app/gen/index.ts")).current_dir(&root);
    if check {
        sdk.arg("--check");
    }
    let status = sdk.status().map_err(|err| format!("bun runtime/app/gen/index.ts: {err} (is Bun installed?)"))?;
    if !status.success() {
        if !check {
            return Err("bun runtime/app/gen/index.ts failed".into());
        }
        stale.push("runtime/app/worker/sdk");
    }
    if !stale.is_empty() {
        return Err(format!("stale, run `cargo xtask api`: {}", stale.join(", ")));
    }
    if check {
        println!("api export is current ({} functions)", info.functions.len());
    }
    Ok(())
}
