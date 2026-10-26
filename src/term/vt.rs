use libghostty_vt::Terminal;
use libghostty_vt::ffi;
use libghostty_vt::screen::Screen;
use libghostty_vt::style::{Palette, RgbColor};
use libghostty_vt::terminal::{ClipboardLocation, ClipboardWriteError, Mode, ModeKind, Point, PointCoordinate};
use libghostty_vt::terminal::{ScrollViewport, SizeReportSize};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Arc, Mutex, Once};
const TERM: &str = "xterm-256color";
const KITTY_STORAGE_BYTES: u64 = 64 * 1024 * 1024;
const APC_MAX_BYTES: usize = 16 * 1024 * 1024;
const CLIPBOARD_MAX_BYTES: usize = 8 * 1024 * 1024;
pub type Emulator = Terminal<'static, 'static>;
#[derive(Default)]
struct Effects {
    size: Option<SizeReportSize>,
    clipboard: Vec<Vec<u8>>,
}
pub struct Vt {
    terminal: Emulator,
    effects: Arc<Mutex<Effects>>,
}
unsafe impl Send for Vt {}
impl Deref for Vt {
    type Target = Emulator;
    fn deref(&self) -> &Emulator {
        &self.terminal
    }
}
impl DerefMut for Vt {
    fn deref_mut(&mut self) -> &mut Emulator {
        &mut self.terminal
    }
}
impl Vt {
    pub fn new(cols: u16, rows: u16, scrollback: usize) -> Option<Self> {
        let mut terminal = Terminal::new(cols, rows).ok()?;
        let effects = Arc::new(Mutex::new(Effects::default()));
        terminal.set_scrollback_max_lines(Some(scrollback)).ok()?;
        let _ = terminal.set_terminfo_name(TERM);
        let _ = terminal.set_default_mode(Mode::GRAPHEME_CLUSTER, true);
        let _ = terminal.set_mode(Mode::GRAPHEME_CLUSTER, true);
        let _ = terminal.set_clipboard_write_max_bytes(Some(CLIPBOARD_MAX_BYTES));
        let sizes = Arc::clone(&effects);
        terminal.on_size(move |_| lock(&sizes).size).ok()?;
        let copies = Arc::clone(&effects);
        terminal
            .on_clipboard_write(move |_, write| {
                if write.location() != ClipboardLocation::Standard {
                    return write.reply(Err(ClipboardWriteError::Unsupported), false);
                }
                let text = write.contents().find(|content| is_text(content.mime)).map(|content| content.data.to_vec());
                match text {
                    Some(text) if !text.is_empty() => {
                        lock(&copies).clipboard.push(text);
                        write.reply(Ok(()), false);
                    }
                    _ => write.reply(Err(ClipboardWriteError::Unsupported), false),
                }
            })
            .ok()?;
        Some(Self { terminal, effects })
    }
    pub fn on_reply(&mut self, mut reply: impl FnMut(&[u8]) + Send + 'static) -> bool {
        self.terminal.on_pty_write(move |_, bytes| reply(bytes)).is_ok()
    }
    pub fn enable_kitty_graphics(&mut self) -> bool {
        install_png_decoder();
        let temp = std::env::temp_dir();
        let t = &mut self.terminal;
        t.set_kitty_image_storage_limit(KITTY_STORAGE_BYTES).is_ok()
            && t.set_kitty_image_from_file_allowed(true).is_ok()
            && t.set_kitty_image_temp_file_dir(Some(Path::new(&temp))).is_ok()
            && t.set_kitty_image_from_shared_mem_allowed(true).is_ok()
            && t.set_apc_max_bytes(Some(APC_MAX_BYTES)).is_ok()
            && t.set_apc_max_bytes_kitty(Some(APC_MAX_BYTES)).is_ok()
    }
    pub fn write(&mut self, bytes: &[u8]) {
        self.terminal.vt_write(bytes);
    }
    pub fn resize(&mut self, cols: u16, rows: u16, cell_width: u32, cell_height: u32) -> bool {
        let (cell_width, cell_height) = (cell_width.max(1), cell_height.max(1));
        lock(&self.effects).size = Some(SizeReportSize { rows, columns: cols, cell_width, cell_height });
        self.terminal.resize(cols, rows, cell_width, cell_height).is_ok()
    }
    pub fn take_clipboard_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut lock(&self.effects).clipboard)
    }
    pub fn dec_mode(&self, mode: u16) -> bool {
        self.terminal.mode(Mode::new(mode, ModeKind::Dec)).unwrap_or(false)
    }
    pub fn alternate_screen(&self) -> bool {
        matches!(self.terminal.active_screen(), Ok(Screen::Alternate))
    }
    pub fn scrolled_back(&self) -> bool {
        self.terminal.scrollbar().is_ok_and(|bar| bar.offset + bar.len < bar.total)
    }
    pub fn scroll_to_bottom(&mut self) {
        self.terminal.scroll_viewport(ScrollViewport::Bottom);
    }
    pub fn scroll(&mut self, delta: isize) {
        self.terminal.scroll_viewport(ScrollViewport::Delta(delta));
    }
    pub fn hyperlink(&self, col: u16, row: u32) -> Option<String> {
        let at = self.terminal.grid_ref(Point::Viewport(PointCoordinate { x: col, y: row })).ok()?;
        let mut buf = vec![0u8; 256];
        let len = match at.hyperlink_uri(&mut buf) {
            Ok(len) => len,
            Err(libghostty_vt::Error::OutOfSpace { required }) => {
                buf.resize(required, 0);
                at.hyperlink_uri(&mut buf).ok()?
            }
            Err(_) => return None,
        };
        buf.truncate(len);
        (!buf.is_empty()).then(|| String::from_utf8_lossy(&buf).into_owned())
    }
    pub fn set_default_palette(&mut self, palette: [RgbColor; 256]) -> bool {
        self.terminal.set_default_color_palette(Some(Palette(palette))).is_ok()
    }
    /// The cursor's shape and blink while the program hasn't set them (DECSCUSR 0 goes back to
    /// them); `None` blink is the emulator's own.
    pub fn set_default_cursor(&mut self, shape: super::CursorShape, blink: Option<bool>) -> bool {
        use libghostty_vt::terminal::CursorStyle;
        let style = match shape {
            super::CursorShape::Block => CursorStyle::Block,
            super::CursorShape::Bar => CursorStyle::Bar,
            super::CursorShape::Underline => CursorStyle::Underline,
            super::CursorShape::Hollow => CursorStyle::BlockHollow,
        };
        let styled = self.terminal.set_default_cursor_style(Some(style)).is_ok();
        styled && self.terminal.set_default_cursor_blink(blink).is_ok()
    }
}
fn is_text(mime: &str) -> bool {
    mime.split(';').next().is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/plain"))
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn install_png_decoder() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let decode: unsafe extern "C" fn(_, _, _, _, _) -> bool = decode_png;
        let _ = unsafe { ffi::ghostty_sys_set(ffi::SysOption::DECODE_PNG, decode as *const std::ffi::c_void) };
    });
}
unsafe extern "C" fn decode_png(
    _userdata: *mut std::ffi::c_void,
    allocator: *const ffi::Allocator,
    data: *const u8,
    len: usize,
    out: *mut ffi::SysImage,
) -> bool {
    if data.is_null() || out.is_null() {
        return false;
    }
    let png = unsafe { std::slice::from_raw_parts(data, len) };
    let decoded = std::panic::catch_unwind(|| {
        image::load_from_memory_with_format(png, image::ImageFormat::Png).ok().map(|image| image.to_rgba8())
    });
    let Ok(Some(rgba)) = decoded else { return false };
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    let buffer = unsafe { ffi::ghostty_alloc(allocator, pixels.len()) };
    if buffer.is_null() {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), buffer, pixels.len());
        *out = ffi::SysImage { width, height, data: buffer, data_len: pixels.len() };
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    fn replies(vt: &mut Vt) -> Arc<Mutex<Vec<u8>>> {
        let replies = Arc::<Mutex<Vec<u8>>>::default();
        let sink = Arc::clone(&replies);
        assert!(vt.on_reply(move |bytes| lock(&sink).extend_from_slice(bytes)));
        replies
    }
    #[test]
    fn copies_links_sizes_and_modes() {
        let mut vt = Vt::new(20, 5, 100).unwrap();
        let replies = replies(&mut vt);
        assert!(vt.resize(20, 5, 8, 16));
        vt.write(b"\x1b]52;c;aGk=\x07");
        assert_eq!(vt.take_clipboard_writes(), vec![b"hi".to_vec()]);
        assert!(vt.take_clipboard_writes().is_empty());
        vt.write(b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\");
        assert_eq!(vt.hyperlink(1, 0).as_deref(), Some("https://example.com"));
        assert_eq!(vt.hyperlink(6, 0), None);
        vt.write(b"\x1b[18t\x1b[14t");
        assert_eq!(&*lock(&replies), b"\x1b[8;5;20t\x1b[4;80;160t");
        assert!(!vt.dec_mode(2004));
        vt.write(b"\x1b[?2004h\x1b[?1049h");
        assert!(vt.dec_mode(2004) && vt.alternate_screen());
        assert!(vt.dec_mode(2027), "grapheme clustering");
    }
    #[test]
    fn scrollback_scrolls() {
        let mut vt = Vt::new(10, 3, 100).unwrap();
        vt.write(b"1\r\n2\r\n3\r\n4\r\n5");
        assert!(!vt.scrolled_back());
        vt.scroll(-1);
        assert!(vt.scrolled_back());
        vt.scroll_to_bottom();
        assert!(!vt.scrolled_back());
    }
}
