use super::{Event, Kind, Notification, Urgency, body_with_subtitle, carried, deliver};
use pumi_api::{Error, Result};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
const APP: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SHOW: &str = r#"
$ErrorActionPreference = 'Stop'
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
[Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml($env:PUMI_NOTIFY_XML)
$toast = New-Object Windows.UI.Notifications.ToastNotification $xml
$toast.Tag = $env:PUMI_NOTIFY_TAG
$toast.Group = 'pumi'
Register-ObjectEvent -InputObject $toast -EventName Activated -SourceIdentifier activated > $null
Register-ObjectEvent -InputObject $toast -EventName Dismissed -SourceIdentifier dismissed > $null
Register-ObjectEvent -InputObject $toast -EventName Failed -SourceIdentifier failed > $null
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($env:PUMI_NOTIFY_APP).Show($toast)
while ($true) {
  $e = Wait-Event -Timeout 86400
  if ($e -eq $null) { exit 0 }
  Remove-Event -EventIdentifier $e.EventIdentifier
  switch ($e.SourceIdentifier) {
    'activated' {
      $a = [Windows.UI.Notifications.ToastActivatedEventArgs]$e.SourceArgs[1]
      $what = $a.Arguments
      if ($what -eq 'pumi.reply') {
        $text = ''
        try { $text = [string]$a.UserInput['reply'] } catch {}
        [Console]::Out.WriteLine('reply ' + (ConvertTo-Json -Compress $text))
      } elseif ($what.StartsWith('action:')) {
        [Console]::Out.WriteLine('action ' + $what.Substring(7))
      } else {
        [Console]::Out.WriteLine('clicked')
      }
      exit 0
    }
    'dismissed' {
      # Timed out is moved to the Action Center, where it can still be clicked.
      if ([string]$e.SourceArgs[1].Reason -eq 'TimedOut') { continue }
      if ([string]$e.SourceArgs[1].Reason -eq 'UserCanceled') { [Console]::Out.WriteLine('dismissed') }
      exit 0
    }
    'failed' { exit 1 }
  }
}
"#;
const HIDE: &str = r#"
$ErrorActionPreference = 'Stop'
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
[Windows.UI.Notifications.ToastNotificationManager]::History.Remove($env:PUMI_NOTIFY_TAG, 'pumi', $env:PUMI_NOTIFY_APP)
"#;
fn waiting() -> &'static Mutex<HashMap<String, u32>> {
    static WAITING: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    WAITING.get_or_init(Default::default)
}
pub(super) fn start() {}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn tag(id: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
fn xml(n: &Notification) -> String {
    let mut text = format!("<text>{}</text>", escape(&n.title));
    let body = body_with_subtitle(n);
    if !body.is_empty() {
        text.push_str(&format!("<text>{}</text>", escape(&body)));
    }
    if let Some(image) = &n.image {
        text.push_str(&format!("<image placement=\"appLogoOverride\" src=\"{}\"/>", escape(image)));
    }
    let mut actions = String::new();
    if let Some(placeholder) = &n.reply {
        actions.push_str(&format!(
            "<input id=\"reply\" type=\"text\" placeHolderContent=\"{}\"/>\
             <action content=\"Send\" arguments=\"pumi.reply\" hint-inputId=\"reply\" activationType=\"foreground\"/>",
            escape(placeholder)
        ));
    }
    for a in &n.actions {
        actions.push_str(&format!(
            "<action content=\"{}\" arguments=\"action:{}\" activationType=\"foreground\"/>",
            escape(&a.label),
            escape(&a.id)
        ));
    }
    let audio = match n.sound.as_deref() {
        None | Some("") => "<audio silent=\"true\"/>".to_string(),
        _ => "<audio src=\"ms-winsoundevent:Notification.Default\"/>".to_string(),
    };
    let scenario = if n.urgency == Urgency::Critical { " scenario=\"urgent\"" } else { "" };
    format!(
        "<toast activationType=\"foreground\" launch=\"pumi\"{scenario}><visual><binding template=\"ToastGeneric\">{text}</binding></visual>\
         <actions>{actions}</actions>{audio}</toast>"
    )
}
fn powershell(script: &str, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]).creation_flags(CREATE_NO_WINDOW);
    for (k, v) in env {
        command.env(k, v);
    }
    command
}
pub(super) fn send(id: &str, n: &Notification) -> Result<()> {
    let (xml, tag) = (xml(n), tag(id));
    let mut child = powershell(SHOW, &[("PUMI_NOTIFY_XML", &xml), ("PUMI_NOTIFY_TAG", &tag), ("PUMI_NOTIFY_APP", APP)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| Error::new(pumi_api::ErrorKind::ProviderUnavailable, format!("powershell: {err}")))?;
    let pid = child.id();
    if let Some(old) = waiting().lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string(), pid) {
        let _ =
            Command::new("taskkill").args(["/PID", &old.to_string(), "/F"]).creation_flags(CREATE_NO_WINDOW).output();
    }
    let stdout = child.stdout.take();
    let (id, carried) = (id.to_string(), carried(n));
    std::thread::spawn(move || {
        for line in stdout.map(BufReader::new).into_iter().flat_map(|r| r.lines()).map_while(|l| l.ok()) {
            if let Some(event) = event(&id, &carried, line.trim()) {
                deliver(event);
            }
        }
        let _ = child.wait();
        let mut waiting = waiting().lock().unwrap_or_else(|p| p.into_inner());
        if waiting.get(&id) == Some(&pid) {
            waiting.remove(&id);
        }
    });
    Ok(())
}
fn event(id: &str, carried: &str, line: &str) -> Option<Event> {
    let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
    let mut event = Event::new(
        match word {
            "clicked" => Kind::Clicked,
            "action" => Kind::Action,
            "reply" => Kind::Reply,
            "dismissed" => Kind::Dismissed,
            _ => return None,
        },
        id,
        carried,
    );
    match event.kind {
        Kind::Action => event.action = Some(rest.to_string()),
        Kind::Reply => event.reply = Some(serde_json::from_str(rest).unwrap_or_else(|_| rest.to_string())),
        _ => {}
    }
    Some(event)
}
pub(super) fn withdraw(id: &str) -> bool {
    let tag = tag(id);
    powershell(HIDE, &[("PUMI_NOTIFY_TAG", &tag), ("PUMI_NOTIFY_APP", APP)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::Action;
    #[test]
    fn the_text_is_xml_escaped() {
        let shown = xml(&Notification::new("a & b").body("<c>"));
        assert!(shown.contains("<text>a &amp; b</text><text>&lt;c&gt;</text>"), "{shown}");
        assert!(shown.contains("<audio silent=\"true\"/>"));
    }
    #[test]
    fn buttons_and_a_reply_are_actions() {
        let n = Notification {
            actions: vec![Action { id: "yes".into(), label: "Deploy".into() }],
            reply: Some("Say".into()),
            ..Notification::new("t")
        };
        let shown = xml(&n);
        assert!(shown.contains("arguments=\"action:yes\""));
        assert!(shown.contains("hint-inputId=\"reply\""));
        assert_eq!(event("x", "{}", "reply \"hi\\nthere\"").unwrap().reply.as_deref(), Some("hi\nthere"));
        assert_eq!(event("x", "{}", "action yes").unwrap().action.as_deref(), Some("yes"));
        assert!(event("x", "{}", "noise").is_none());
    }
    #[test]
    fn a_tag_is_short() {
        assert_eq!(tag("a"), tag("a"));
        assert!(tag(&"x".repeat(200)).len() <= 16);
    }
}
