use super::{Action, Event, Kind, Notification, Urgency, carried, deliver};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, define_class, msg_send, sel};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSError, NSSet, NSString, NSURL};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationAttachment, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationDefaultActionIdentifier, UNNotificationDismissActionIdentifier, UNNotificationInterruptionLevel,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse, UNNotificationSound,
    UNTextInputNotificationAction, UNTextInputNotificationResponse, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};
use pumi_api::{Error, Result};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
const CARRIED: &str = "pumi";
const REPLY: &str = "pumi.reply";
fn bundled() -> bool {
    static BUNDLED: OnceLock<bool> = OnceLock::new();
    *BUNDLED.get_or_init(|| {
        let bundle = NSBundle::mainBundle();
        bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app")
    })
}
define_class!(
    #[unsafe(super(NSObject))]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &block2::DynBlock<dyn Fn()>,
        ) {
            deliver(event(response));
            done.call(());
        }
    }
);
fn event(response: &UNNotificationResponse) -> Event {
    let request = response.notification().request();
    let id = request.identifier().to_string();
    let info = request.content().userInfo();
    let key = NSString::from_str(CARRIED);
    let carried = info
        .objectForKey(AsRef::<AnyObject>::as_ref(&*key))
        .and_then(|value| value.downcast::<NSString>().ok())
        .map(|text| text.to_string())
        .unwrap_or_default();
    let action = response.actionIdentifier();
    let (default, dismiss) = unsafe { (UNNotificationDefaultActionIdentifier, UNNotificationDismissActionIdentifier) };
    let kind = if &*action == default {
        Kind::Clicked
    } else if &*action == dismiss {
        Kind::Dismissed
    } else if action.to_string() == REPLY {
        Kind::Reply
    } else {
        Kind::Action
    };
    let mut event = Event::new(kind, &id, &carried);
    match kind {
        Kind::Action => event.action = Some(action.to_string()),
        Kind::Reply => {
            let any: &AnyObject = response.as_ref();
            event.reply = any.downcast_ref::<UNTextInputNotificationResponse>().map(|r| r.userText().to_string());
        }
        _ => {}
    }
    event
}
pub(super) fn start() {
    if !bundled() {
        return;
    }
    let this = Delegate::alloc().set_ivars(());
    let delegate: Retained<Delegate> = unsafe { msg_send![super(this), init] };
    UNUserNotificationCenter::currentNotificationCenter().setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    std::mem::forget(delegate);
}
static ALLOWED: AtomicU8 = AtomicU8::new(UNKNOWN);
const UNKNOWN: u8 = 0;
const YES: u8 = 1;
const NO: u8 = 2;
pub(super) fn send(id: &str, n: &Notification) -> Result<()> {
    if !bundled() {
        return script(n);
    }
    if ALLOWED.load(Ordering::Relaxed) == NO {
        return Err(Error::new(
            pumi_api::ErrorKind::ProviderUnavailable,
            "notifications are off for pumi: System Settings → Notifications → pumi",
        ));
    }
    let request =
        UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(id), &content(id, n), None);
    let post = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        ALLOWED.store(if granted.as_bool() { YES } else { NO }, Ordering::Relaxed);
        if !granted.as_bool() {
            eprintln!("notify: notifications are off for pumi (System Settings → Notifications)");
            return;
        }
        let done = RcBlock::new(|err: *mut NSError| {
            if let Some(err) = unsafe { err.as_ref() } {
                eprintln!("notify: {}", err.localizedDescription());
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&done));
    });
    UNUserNotificationCenter::currentNotificationCenter().requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
        &post,
    );
    Ok(())
}
fn content(id: &str, n: &Notification) -> Retained<UNMutableNotificationContent> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&n.title));
    content.setBody(&NSString::from_str(n.body.as_deref().unwrap_or_default()));
    if let Some(subtitle) = &n.subtitle {
        content.setSubtitle(&NSString::from_str(subtitle));
    }
    if let Some(group) = &n.group {
        content.setThreadIdentifier(&NSString::from_str(group));
    }
    match n.sound.as_deref() {
        None | Some("") => {}
        Some("default") => content.setSound(Some(&UNNotificationSound::defaultSound())),
        Some(name) => content.setSound(Some(&UNNotificationSound::soundNamed(&NSString::from_str(name)))),
    }
    if content.respondsToSelector(sel!(setInterruptionLevel:)) {
        content.setInterruptionLevel(match n.urgency {
            Urgency::Low => UNNotificationInterruptionLevel::Passive,
            _ => UNNotificationInterruptionLevel::Active,
        });
    }
    content.setCategoryIdentifier(&NSString::from_str(&category(&n.actions, n.reply.as_deref())));
    let (key, value) = (NSString::from_str(CARRIED), NSString::from_str(&carried(n)));
    let info = NSDictionary::<NSString, NSString>::from_slices(&[&*key], &[&*value]);
    unsafe { content.setUserInfo(&Retained::cast_unchecked::<NSDictionary>(info)) };
    if let Some(image) = n.image.as_deref().and_then(|path| attachment(id, path)) {
        content.setAttachments(&NSArray::from_retained_slice(&[image]));
    }
    content
}
fn attachment(id: &str, path: &str) -> Option<Retained<UNNotificationAttachment>> {
    let source = std::path::Path::new(path);
    let name = source.file_name()?.to_string_lossy();
    let copy = std::env::temp_dir().join(format!("pumi-notify-{}-{name}", id.replace('/', "_")));
    if let Err(err) = std::fs::copy(source, &copy) {
        eprintln!("notify: {path}: {err}");
        return None;
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&copy.to_string_lossy()));
    match unsafe {
        UNNotificationAttachment::attachmentWithIdentifier_URL_options_error(&NSString::from_str("image"), &url, None)
    } {
        Ok(attachment) => Some(attachment),
        Err(err) => {
            eprintln!("notify: {path}: {}", err.localizedDescription());
            None
        }
    }
}
type Categories = BTreeMap<String, (Vec<Action>, Option<String>)>;
fn categories() -> &'static Mutex<Categories> {
    static CATEGORIES: OnceLock<Mutex<Categories>> = OnceLock::new();
    CATEGORIES.get_or_init(Default::default)
}
fn category(actions: &[Action], reply: Option<&str>) -> String {
    let id = category_id(actions, reply);
    let mut known = categories().lock().unwrap_or_else(|p| p.into_inner());
    if known.contains_key(&id) {
        return id;
    }
    known.insert(id.clone(), (actions.to_vec(), reply.map(str::to_string)));
    let all: Vec<Retained<UNNotificationCategory>> =
        known.iter().map(|(id, (actions, reply))| make_category(id, actions, reply.as_deref())).collect();
    UNUserNotificationCenter::currentNotificationCenter().setNotificationCategories(&NSSet::from_retained_slice(&all));
    id
}
fn category_id(actions: &[Action], reply: Option<&str>) -> String {
    if actions.is_empty() && reply.is_none() {
        return "pumi".into();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (actions.iter().map(|a| (&a.id, &a.label)).collect::<Vec<_>>(), reply).hash(&mut hasher);
    format!("pumi.{:016x}", hasher.finish())
}
fn make_category(id: &str, actions: &[Action], reply: Option<&str>) -> Retained<UNNotificationCategory> {
    let mut list: Vec<Retained<UNNotificationAction>> = actions
        .iter()
        .map(|a| {
            UNNotificationAction::actionWithIdentifier_title_options(
                &NSString::from_str(&a.id),
                &NSString::from_str(&a.label),
                UNNotificationActionOptions::empty(),
            )
        })
        .collect();
    if let Some(placeholder) = reply {
        let field =
            UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
                &NSString::from_str(REPLY),
                &NSString::from_str("Reply"),
                UNNotificationActionOptions::empty(),
                &NSString::from_str("Send"),
                &NSString::from_str(placeholder),
            );
        list.push(field.into_super());
    }
    UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(id),
        &NSArray::from_retained_slice(&list),
        &NSArray::new(),
        UNNotificationCategoryOptions::CustomDismissAction,
    )
}
pub(super) fn withdraw(id: &str) -> bool {
    if !bundled() {
        return false;
    }
    let ids = NSArray::from_retained_slice(&[NSString::from_str(id)]);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removeDeliveredNotificationsWithIdentifiers(&ids);
    center.removePendingNotificationRequestsWithIdentifiers(&ids);
    true
}
fn script_args(n: &Notification) -> Vec<String> {
    let mut args = vec![n.title.clone(), n.body.clone().unwrap_or_default()];
    let mut line = String::from("display notification (item 2 of argv) with title (item 1 of argv)");
    if let Some(subtitle) = &n.subtitle {
        args.push(subtitle.clone());
        line.push_str(&format!(" subtitle (item {} of argv)", args.len()));
    }
    if let Some(sound) = n.sound.as_deref().filter(|s| !s.is_empty()) {
        args.push(if sound == "default" { "Glass".into() } else { sound.to_string() });
        line.push_str(&format!(" sound name (item {} of argv)", args.len()));
    }
    let mut all = vec!["-e".into(), "on run argv".into(), "-e".into(), line, "-e".into(), "end run".into()];
    all.extend(args);
    all
}
fn script(n: &Notification) -> Result<()> {
    let mut child = std::process::Command::new("osascript")
        .args(script_args(n))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| Error::exception(format!("osascript: {err}")))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_script_takes_its_text_as_arguments() {
        let n = Notification::new("a \"b\"").subtitle("s").body("c").sound("default");
        let args = script_args(&n);
        assert_eq!(
            args[3],
            "display notification (item 2 of argv) with title (item 1 of argv) subtitle (item 3 of argv) sound name (item 4 of argv)"
        );
        assert_eq!(&args[6..], ["a \"b\"", "c", "s", "Glass"]);
    }
    #[test]
    fn buttons_make_a_category_of_their_own() {
        assert_eq!(category_id(&[], None), "pumi");
        let yes = [Action { id: "yes".into(), label: "Yes".into() }];
        assert_eq!(category_id(&yes, None), category_id(&yes, None));
        assert_ne!(category_id(&yes, None), category_id(&yes, Some("Say")));
    }
}
