use pumi_api::{ApiInfo, FunctionInfo};
pub fn json(info: &ApiInfo) -> String {
    let mut text = serde_json::to_string_pretty(info).expect("the export is JSON");
    text.push('\n');
    text
}
/// The functions only the app answers, as the export has them, and its wire-only ones
/// (`internal`): the app provides them from this.
pub fn app_functions(info: &ApiInfo, internal: &[FunctionInfo]) -> String {
    let mut app: Vec<&FunctionInfo> = info.functions.iter().filter(|f| f.provider == "app").collect();
    app.extend(internal);
    app.sort_by(|a, b| a.name.cmp(&b.name));
    let mut text = serde_json::to_string_pretty(&app).expect("the export is JSON");
    text.push('\n');
    text
}

/// The UI protocol (`pumi-ui-proto`): what the window's UI sends the renderer and hears back.
pub fn ui_protocol() -> String {
    let mut text = serde_json::to_string_pretty(&pumi_ui_proto::schema()).expect("the protocol is JSON");
    text.push('\n');
    text
}
