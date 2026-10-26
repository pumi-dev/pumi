//! Elsewhere: no blur behind the window yet (Windows' acrylic and the compositors' are for later).

pub fn set_blur(_window: &gpui::Window, _radius: f64) -> Result<(), String> {
    Ok(())
}
