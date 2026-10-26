//! macOS: NSHapticFeedbackManager, on the main thread.
use objc2_app_kit::{NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer};

pub fn perform(pattern: &str) {
    if objc2::MainThreadMarker::new().is_none() {
        return;
    }
    let pattern = match pattern {
        "alignment" => NSHapticFeedbackPattern::Alignment,
        "levelChange" | "level_change" => NSHapticFeedbackPattern::LevelChange,
        _ => NSHapticFeedbackPattern::Generic,
    };
    NSHapticFeedbackManager::defaultPerformer()
        .performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
}
