use super::grid::{Grid, Painter};
use super::vt::Vt;
const SCROLLBACK_ROWS: usize = 10_000;
const BRACKETED_PASTE: u16 = 2004;
const ALTERNATE_SCROLL: u16 = 1007;
const FOCUS_EVENTS: u16 = 1004;
const SYNCHRONIZED_OUTPUT: u16 = 2026;
const SYNCHRONIZED_LIMIT: std::time::Duration = std::time::Duration::from_secs(1);
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wheel {
    Send(Vec<u8>),
    /// Rows to move the app's own scrollback by: negative is up.
    Scroll(isize),
}
/// What turning keys, clicks and the wheel into bytes needs of a terminal's modes, copied out of
/// it after each batch of output, so the app's thread encodes input without waiting on the
/// thread that parses. The mouse's report format lives in a `pumi_keys::mouse::Encoder` beside it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Modes {
    pub keys: pumi_keys::Options,
    pub bracketed_paste: bool,
    pub focus_events: bool,
    pub mouse_tracked: bool,
    pub alternate_scroll: bool,
    pub scrolled_back: bool,
    pub screen: (u16, u16),
    pub pointer: Option<String>,
}
impl Modes {
    /// What `steps` of the wheel at cell `at` become: a report to a program that tracks the
    /// mouse, arrows to a full-screen program that asked for them, else a move of the scrollback.
    pub fn wheel(
        &self,
        mouse: Option<&mut pumi_keys::mouse::Encoder>,
        keys: Option<&mut pumi_keys::Encoder>,
        steps: i32,
        lines: u16,
        at: (u16, u16),
        cell_px: (u32, u32),
    ) -> Wheel {
        use pumi_keys::mouse::{Action, Button, Mouse};
        let up = steps > 0;
        let count = steps.unsigned_abs() as usize;
        if self.mouse_tracked {
            let button = if up { Button::WheelUp } else { Button::WheelDown };
            let event = Mouse { action: Action::Press, button: Some(button), column: at.0, row: at.1, mods: 0 };
            let report = mouse.map(|mouse| mouse.encode(cell_px, self.screen, &event)).unwrap_or_default();
            return Wheel::Send(report.repeat(count));
        }
        if self.alternate_scroll {
            let arrow = pumi_keys::parse(if up { "up" } else { "down" });
            let options = pumi_keys::Options { option_as_alt: pumi_keys::OptionAsAlt::Neither, ..self.keys };
            let bytes = arrow.zip(keys).map(|(arrow, keys)| keys.encode(&arrow, &options));
            return Wheel::Send(bytes.unwrap_or_default().repeat(count * usize::from(lines)));
        }
        let delta = (count * usize::from(lines)) as isize;
        Wheel::Scroll(if up { -delta } else { delta })
    }
    /// The report of the tab gaining or losing focus, to a program that asked for one.
    pub fn focus(&self, gained: bool) -> Vec<u8> {
        if !self.focus_events {
            return Vec::new();
        }
        let event = if gained { libghostty_vt::focus::Event::Gained } else { libghostty_vt::focus::Event::Lost };
        let mut buf = [0u8; 8];
        event.encode(&mut buf).map(|len| buf[..len].to_vec()).unwrap_or_default()
    }
}
pub struct Local {
    pub generation: u64,
    pub terminal: Vt,
    painter: Painter,
    pub applied: (u16, u16),
    themed: (u64, [u32; 16]),
    /// The generation of `cursor_defaults` the emulator was last given.
    cursored: u32,
    held: Option<std::time::Instant>,
    osc: pumi_proto::osc::Scanner,
    pub pointer: Option<String>,
    replies: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pictures: super::images::Pictures,
    cursor: Option<(u16, u16)>,
    graphics: super::graphics::Translator,
    cell: (u32, u32),
}
impl Local {
    pub fn new(generation: u64, cols: u16, rows: u16) -> Option<Self> {
        let replies = std::sync::Arc::default();
        let terminal = terminal(cols, rows, &replies)?;
        let painter = Painter::new()?;
        let mut local = Self {
            generation,
            terminal,
            painter,
            applied: (cols, rows),
            themed: (u64::MAX, [0; 16]),
            cursored: u32::MAX,
            held: None,
            osc: Default::default(),
            pointer: None,
            replies,
            pictures: Default::default(),
            cursor: None,
            graphics: Default::default(),
            cell: (0, 0),
        };
        local.follow_theme();
        Some(local)
    }
    pub fn reset(&mut self, cols: u16, rows: u16, cell_px: (u32, u32)) {
        if let Some(terminal) = terminal(cols, rows, &self.replies) {
            self.terminal = terminal;
            self.painter.invalidate();
            self.pointer = None;
            let _ = self.terminal.resize(cols, rows, cell_px.0, cell_px.1);
            self.applied = (cols, rows);
            self.cell = cell_px;
            self.graphics = Default::default();
            self.themed.0 = u64::MAX;
            self.cursored = u32::MAX;
            self.follow_theme();
        }
    }
    pub fn resize(&mut self, cols: u16, rows: u16, cell_px: (u32, u32)) {
        if (cols, rows) != self.applied || cell_px != self.cell {
            self.applied = (cols, rows);
            self.cell = cell_px;
            let _ = self.terminal.resize(cols, rows, cell_px.0, cell_px.1);
        }
    }
    pub fn write(&mut self, bytes: &[u8]) -> Vec<String> {
        let size = pumi_proto::native::Size {
            cols: self.applied.0,
            rows: self.applied.1,
            cell_width: self.cell.0,
            cell_height: self.cell.1,
        };
        let translated = self.graphics.feed(bytes, size);
        if !translated.replies.is_empty() {
            self.replies.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(&translated.replies);
        }
        let bytes = &*translated.bytes;
        let mut heard = Vec::new();
        self.osc.feed(bytes, &mut heard);
        for osc in heard {
            match osc {
                pumi_proto::osc::Osc::Pointer(shape) => self.pointer = Some(shape).filter(|shape| !shape.is_empty()),
                pumi_proto::osc::Osc::Status(pumi_proto::status::Sequence::Query) => {
                    let mut replies = self.replies.lock().unwrap_or_else(|p| p.into_inner());
                    replies.extend_from_slice(pumi_proto::status::QUERY_REPLY);
                }
                _ => {}
            }
        }
        self.terminal.write(bytes);
        self.terminal
            .take_clipboard_writes()
            .into_iter()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .collect()
    }
    fn follow_theme(&mut self) -> [u32; 16] {
        let generation = crate::theme::generation();
        if self.themed.0 == generation {
            return self.themed.1;
        }
        let colours = crate::theme::terminal::ansi();
        let palette = crate::theme::terminal::palette().map(|colour| libghostty_vt::style::RgbColor {
            r: (colour >> 16) as u8,
            g: (colour >> 8) as u8,
            b: colour as u8,
        });
        self.terminal.set_default_palette(palette);
        self.themed = (generation, colours);
        colours
    }
    fn follow_cursor(&mut self) {
        let (generation, shape, blink) = super::cursor_defaults::get();
        if self.cursored != generation && self.terminal.set_default_cursor(shape, blink) {
            self.cursored = generation;
        }
    }
    pub fn screen(&mut self) -> Option<Grid> {
        self.follow_cursor();
        let ansi = self.follow_theme();
        let (cols, rows) = self.applied;
        let mut grid = self.painter.grid(&self.terminal, cols, rows, &ansi)?;
        grid.images = self.pictures.placed(&self.terminal);
        self.cursor = grid.cursor.map(|c| (c.x, c.y));
        Some(grid)
    }
    pub fn take_dropped_images(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        self.pictures.take_dropped()
    }
    pub fn settled_screen(&mut self) -> Option<Grid> {
        if self.terminal.dec_mode(SYNCHRONIZED_OUTPUT) {
            let since = *self.held.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() < SYNCHRONIZED_LIMIT {
                return None;
            }
        }
        self.held = None;
        self.screen()
    }
    /// The terminal's modes as input needs them (see [`Modes`]).
    pub fn modes(&self) -> Modes {
        let terminal = &self.terminal;
        Modes {
            keys: pumi_keys::Options::from_terminal(terminal),
            bracketed_paste: terminal.dec_mode(BRACKETED_PASTE),
            focus_events: terminal.dec_mode(FOCUS_EVENTS),
            mouse_tracked: pumi_keys::mouse::tracked(terminal),
            alternate_scroll: terminal.alternate_screen() && terminal.dec_mode(ALTERNATE_SCROLL),
            scrolled_back: terminal.scrolled_back(),
            screen: self.applied,
            pointer: self.pointer.clone(),
        }
    }
    pub fn take_replies(&mut self) -> Vec<u8> {
        let replies = std::mem::take(&mut *self.replies.lock().unwrap_or_else(|p| p.into_inner()));
        pumi_proto::graphics::da1_with_sixel(&replies).into_owned()
    }
    pub fn hyperlink(&self, col: u16, row: u16) -> Option<String> {
        self.terminal.hyperlink(col, u32::from(row))
    }
}
fn terminal(cols: u16, rows: u16, replies: &std::sync::Arc<std::sync::Mutex<Vec<u8>>>) -> Option<Vt> {
    let mut terminal = Vt::new(cols, rows, SCROLLBACK_ROWS)?;
    terminal.enable_kitty_graphics();
    let replies = std::sync::Arc::clone(replies);
    terminal.on_reply(move |bytes| {
        let bytes = pumi_proto::graphics::da1_with_sixel(bytes);
        replies.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(&bytes);
    });
    Some(terminal)
}
pub fn mods(modifiers: u8) -> u16 {
    [(1, pumi_keys::MOD_SHIFT), (2, pumi_keys::MOD_CTRL), (4, pumi_keys::MOD_ALT)]
        .into_iter()
        .filter(|(bit, _)| modifiers & bit != 0)
        .fold(0, |mods, (_, m)| mods | m)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn the_wheel_follows_the_programs_modes() {
        let mut local = Local::new(1, 20, 4).unwrap();
        let (mut mouse, mut keys) = (pumi_keys::mouse::Encoder::new().unwrap(), pumi_keys::Encoder::new().unwrap());
        let mut wheel = |local: &Local, steps, lines, at| {
            mouse.follow(&local.terminal);
            local.modes().wheel(Some(&mut mouse), Some(&mut keys), steps, lines, at, (8, 16))
        };
        local.write(&b"x\r\n".repeat(30));
        assert_eq!(wheel(&local, 1, 3, (0, 0)), Wheel::Scroll(-3), "plain: the app's scrollback");
        local.write(b"\x1b[?1049h\x1b[?1007h");
        assert_eq!(wheel(&local, 1, 1, (0, 0)), Wheel::Send(b"\x1b[A".to_vec()), "alternate screen: arrows");
        local.write(b"\x1b[?1000h\x1b[?1006h");
        let Wheel::Send(report) = wheel(&local, -1, 1, (2, 1)) else { panic!("tracked: a report") };
        assert!(report.starts_with(b"\x1b[<65;3;2"), "{report:?}");
    }
    #[test]
    fn modes_are_what_the_program_last_set() {
        let mut local = Local::new(1, 20, 4).unwrap();
        assert_eq!(local.modes().focus(true), b"", "no focus reports unless asked");
        local.write(b"\x1b[?2004h\x1b[?1004h\x1b[>1u\x1b]22;pointer\x1b\\");
        let modes = local.modes();
        assert!(modes.bracketed_paste);
        assert_eq!(modes.keys.kitty_flags, 1);
        assert_eq!(modes.focus(true), b"\x1b[I");
        assert_eq!(modes.pointer.as_deref(), Some("pointer"));
        assert_eq!(modes.screen, (20, 4));
    }
    #[test]
    fn dirty_rows_match_a_full_rebuild() {
        let mut local = Local::new(1, 30, 6).unwrap();
        let steps: [&[u8]; 9] = [
            b"hello\r\nworld",
            b"!",
            b"\x1b[31mred\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[9mstruck\x1b[0m",
            &b"line\r\n".repeat(10),
            b"\x1b[?1049hfull screen app\x1b[3;3Hthere",
            b"\x1b[?1049l",
            b"\x1b]4;1;rgb:12/34/56\x1b\\\x1b[31mrepainted\x1b[0m",
            b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ \xe7\x95\x8c wide",
            b"\x1b[2J\x1b[Hcleared",
        ];
        let fresh = |local: &Local| {
            let mut painter = Painter::new().unwrap();
            let (cols, rows) = local.applied;
            painter.grid(&local.terminal, cols, rows, &local.themed.1).unwrap()
        };
        for (i, step) in steps.iter().enumerate() {
            local.write(step);
            assert_eq!(local.screen().unwrap(), fresh(&local), "after step {i}");
        }
        local.terminal.scroll(-2);
        assert_eq!(local.screen().unwrap(), fresh(&local), "scrolled back");
        local.resize(24, 5, (8, 16));
        assert_eq!(local.screen().unwrap(), fresh(&local), "resized");
        let before = local.screen().unwrap();
        let after = local.screen().unwrap();
        assert!(before.rows.iter().zip(&after.rows).all(|(a, b)| Arc::ptr_eq(a, b)), "nothing changed: rows shared");
        local.write(b"x");
        let typed = local.screen().unwrap();
        let shared = after.rows.iter().zip(&typed.rows).filter(|(a, b)| Arc::ptr_eq(a, b)).count();
        assert!(shared >= after.rows.len() - 1, "typing rebuilds one row, kept {shared}");
    }
    #[test]
    fn rows_that_only_moved_are_the_rows_they_were() {
        let mut local = Local::new(1, 20, 6).unwrap();
        let lines: String = (0..30).map(|i| format!("line {i}\r\n")).collect();
        local.write(lines.as_bytes());
        let before = local.screen().unwrap();
        local.terminal.scroll(-1);
        let scrolled = local.screen().unwrap();
        assert_eq!(scrolled, fresh_screen(&local), "still the screen");
        let moved = before.rows[..5].iter().zip(&scrolled.rows[1..]).filter(|(a, b)| Arc::ptr_eq(a, b)).count();
        assert_eq!(moved, 5, "every row that moved down one kept itself");
        local.write(b"\x1b[2J\x1b[H");
        let shown: String = (0..6).map(|i| format!("\x1b[{};1Hline {i}", i + 1)).collect();
        local.write(shown.as_bytes());
        let first = local.screen().unwrap();
        local.write(format!("\x1b[2J{shown}").as_bytes());
        let again = local.screen().unwrap();
        assert!(first.rows.iter().zip(&again.rows).all(|(a, b)| Arc::ptr_eq(a, b)), "a redraw of the same keeps all");
    }
    fn fresh_screen(local: &Local) -> Grid {
        let (cols, rows) = local.applied;
        Painter::new().unwrap().grid(&local.terminal, cols, rows, &local.themed.1).unwrap()
    }
    #[test]
    fn the_cursor_defaults_are_what_a_program_that_asks_nothing_gets() {
        use crate::term::{CursorShape, cursor_defaults};
        cursor_defaults::set(CursorShape::Bar, Some(false));
        let mut local = Local::new(1, 20, 3).unwrap();
        local.write(b"$ ");
        let cursor = local.screen().unwrap().cursor.unwrap();
        assert_eq!((cursor.shape, cursor.blinking), (CursorShape::Bar, false), "the default, at once");
        local.write(b"\x1b[2 q");
        assert_eq!(local.screen().unwrap().cursor.unwrap().shape, CursorShape::Block, "what the program asks");
        local.write(b"\x1b[0 q");
        assert_eq!(local.screen().unwrap().cursor.unwrap().shape, CursorShape::Bar, "reset to the default");
        cursor_defaults::set(CursorShape::Block, None);
    }
    #[test]
    fn a_synchronized_update_holds_the_screen_until_it_ends() {
        let mut local = Local::new(1, 20, 3).unwrap();
        local.write(b"\x1b[?2026hhalf a frame");
        assert!(local.settled_screen().is_none(), "held while the program draws");
        local.write(b"\x1b[?2026l");
        assert!(local.settled_screen().is_some_and(|grid| grid.lines().next().unwrap().starts_with("half a frame")));
    }
    #[test]
    fn probes_are_answered() {
        let mut local = Local::new(1, 80, 24).unwrap();
        local.resize(80, 25, (8, 16));
        local.resize(80, 24, (8, 16));
        let mut ask = |probe: &[u8]| {
            local.write(probe);
            String::from_utf8_lossy(&local.take_replies()).into_owned()
        };
        assert_eq!(ask(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"), "\x1b_Gi=31;OK\x1b\\", "kitty graphics");
        assert_eq!(ask(b"\x1b[16t"), "\x1b[6;16;8t", "cell size");
        assert_eq!(ask(b"\x1b[?u"), "\x1b[?0u", "kitty keyboard");
        assert!(ask(b"\x1b[c").starts_with("\x1b[?62;4;"), "DA1 says sixel (the server translates it)");
        assert!(ask(b"\x1b]7501;?\x1b\\").starts_with("\x1b]7501;?:states=idle,working,done,blocked,error"));
    }
    #[test]
    fn modifier_bits_map_to_ghosttys() {
        assert_eq!(mods(0), 0);
        assert_eq!(mods(1 | 4), pumi_keys::MOD_SHIFT | pumi_keys::MOD_ALT);
    }
}
#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;
    #[test]
    #[ignore = "a measurement"]
    fn screen_cost() {
        let (cols, rows) = (200u16, 60u16);
        let mut local = Local::new(1, cols, rows).unwrap();
        let line: String = (0..cols).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let coloured: String =
            (0..cols / 4).map(|i| format!("\x1b[3{}m{}\x1b[0m", i % 8, &line[..3])).collect::<Vec<_>>().join(" ");
        let cases: [(&str, Vec<u8>); 3] = [
            ("plain full screen", format!("{line}\r\n").repeat(rows as usize).into_bytes()),
            ("coloured full screen", format!("{coloured}\r\n").repeat(rows as usize).into_bytes()),
            ("one line typed", b"x".to_vec()),
        ];
        for (name, bytes) in cases {
            const N: u32 = 200;
            let (mut write, mut screen) = (Duration::ZERO, Duration::ZERO);
            for _ in 0..N {
                let at = Instant::now();
                local.write(&bytes);
                write += at.elapsed();
                let at = Instant::now();
                std::hint::black_box(local.screen());
                screen += at.elapsed();
            }
            eprintln!("{name:24} write {:>8.3} ms  screen {:>8.3} ms", ms(write / N), ms(screen / N));
        }
    }
    use std::time::Duration;
    fn ms(d: Duration) -> f64 {
        d.as_secs_f64() * 1000.0
    }
}
