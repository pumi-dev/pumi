//! Repaints the renderer asks for at a time, not every frame: a sprite's next frame, a capped
//! animation's next step. One timer a window, for the soonest time asked.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

thread_local! {
    /// The time each window's pending repaint is for, by window id.
    static SCHEDULED: RefCell<HashMap<u64, Instant>> = RefCell::new(HashMap::new());
    /// The windows a timed repaint woke since their last frame (`take_woken`).
    static WOKEN: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
}

/// Whether the frame `window` is drawing is a timed repaint's: one woke it, or one is due.
pub(crate) fn take_woken(window: &gpui::Window) -> bool {
    let id = window.window_handle().window_id().as_u64();
    let due = SCHEDULED.with(|s| s.borrow().get(&id).is_some_and(|at| *at <= Instant::now()));
    WOKEN.with(|w| w.borrow_mut().remove(&id)) || due
}

/// Repaint `window` at `at` (or now, if that is past). A later request than one pending is
/// already served by it: the repaint then asks again for what it still needs.
pub(crate) fn repaint_at(at: Instant, window: &mut gpui::Window, cx: &mut gpui::App) {
    let id = window.window_handle().window_id().as_u64();
    let now = Instant::now();
    let pending = SCHEDULED.with(|s| s.borrow().get(&id).copied());
    if pending.is_some_and(|pending| pending <= at && pending > now) {
        return;
    }
    SCHEDULED.with(|s| s.borrow_mut().insert(id, at));
    let delay = at.saturating_duration_since(now).max(Duration::from_millis(1));
    let view = window.current_view();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(delay).await;
            let ours = SCHEDULED.with(|s| {
                let mut s = s.borrow_mut();
                let ours = s.get(&id) == Some(&at);
                if ours {
                    s.remove(&id);
                }
                ours
            });
            if ours {
                WOKEN.with(|w| w.borrow_mut().insert(id));
                // The view that asked, not the whole window: a cached subtree's sprite repaints
                // that subtree (`crate::boundary`), the root's motion the root.
                cx.update(|_, cx| cx.notify(view)).ok();
            }
        })
        .detach();
}

/// Repaint `window` in `delay`.
pub(crate) fn repaint_in(delay: Duration, window: &mut gpui::Window, cx: &mut gpui::App) {
    repaint_at(Instant::now() + delay, window, cx);
}
