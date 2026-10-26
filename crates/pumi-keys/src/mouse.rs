use libghostty_vt::Terminal;
use libghostty_vt::key::Mods;
use libghostty_vt::mouse::{self, EncoderSize, Position};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    Motion,
}
pub const ACTIONS: &[(&str, Action)] =
    &[("down", Action::Press), ("up", Action::Release), ("drag", Action::Motion), ("move", Action::Motion)];
pub const BUTTONS: &[(&str, Button)] = &[("left", Button::Left), ("middle", Button::Middle), ("right", Button::Right)];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    pub action: Action,
    pub button: Option<Button>,
    pub column: u16,
    pub row: u16,
    pub mods: u16,
}
fn raw_button(button: Button) -> mouse::Button {
    match button {
        Button::Left => mouse::Button::Left,
        Button::Middle => mouse::Button::Middle,
        Button::Right => mouse::Button::Right,
        Button::WheelUp => mouse::Button::Four,
        Button::WheelDown => mouse::Button::Five,
    }
}
fn raw_action(action: Action) -> mouse::Action {
    match action {
        Action::Press => mouse::Action::Press,
        Action::Release => mouse::Action::Release,
        Action::Motion => mouse::Action::Motion,
    }
}
pub fn tracked(terminal: &Terminal<'_, '_>) -> bool {
    terminal.is_mouse_tracking().unwrap_or(false)
}
pub fn encode(terminal: &Terminal<'_, '_>, cell: (u32, u32), screen: (u16, u16), mouse: &Mouse) -> Vec<u8> {
    if !tracked(terminal) {
        return Vec::new();
    }
    let Some(mut encoder) = Encoder::new() else { return Vec::new() };
    encoder.follow(terminal);
    encoder.encode(cell, screen, mouse)
}
/// A mouse encoder that holds a terminal's tracking mode and report format, so it encodes without
/// the terminal at hand: whoever owns the terminal tells it the modes (`follow`), and whoever has
/// the pointer encodes with it.
pub struct Encoder {
    encoder: mouse::Encoder<'static>,
    event: mouse::Event<'static>,
}
// SAFETY: the encoder and event are plain allocations of libghostty's, owned here and touched by
// one thread at a time (callers hold `&mut`).
unsafe impl Send for Encoder {}
impl Encoder {
    pub fn new() -> Option<Self> {
        Some(Self { encoder: mouse::Encoder::new().ok()?, event: mouse::Event::new().ok()? })
    }
    /// Takes the terminal's tracking mode and report format.
    pub fn follow(&mut self, terminal: &Terminal<'_, '_>) {
        self.encoder.set_options_from_terminal(terminal);
    }
    /// The bytes a program that tracks the mouse is told of `mouse`, in the modes last followed.
    pub fn encode(&mut self, cell: (u32, u32), screen: (u16, u16), mouse: &Mouse) -> Vec<u8> {
        let (cell_w, cell_h) = (cell.0.max(1), cell.1.max(1));
        self.encoder.set_size(EncoderSize {
            screen_width: u32::from(screen.0) * cell_w,
            screen_height: u32::from(screen.1) * cell_h,
            cell_width: cell_w,
            cell_height: cell_h,
            padding_top: 0,
            padding_bottom: 0,
            padding_right: 0,
            padding_left: 0,
        });
        let x = f32::from(mouse.column) * cell_w as f32 + cell_w as f32 / 2.0;
        let y = f32::from(mouse.row) * cell_h as f32 + cell_h as f32 / 2.0;
        self.event
            .set_action(raw_action(mouse.action))
            .set_button(mouse.button.map(raw_button))
            .set_mods(Mods::from_bits_retain(mouse.mods))
            .set_position(Position { x, y });
        let mut out = Vec::new();
        match self.encoder.encode_to_vec(&self.event, &mut out) {
            Ok(()) => out,
            Err(_) => Vec::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn click(program: &[u8]) -> Vec<u8> {
        let mut terminal = Terminal::new(80, 24).unwrap();
        terminal.vt_write(program);
        let press = Mouse { action: Action::Press, button: Some(Button::Left), column: 2, row: 1, mods: 0 };
        encode(&terminal, (8, 16), (80, 24), &press)
    }
    #[test]
    fn only_a_tracking_program_gets_the_mouse() {
        assert!(click(b"").is_empty());
        assert_eq!(click(b"\x1b[?1000h\x1b[?1006h"), b"\x1b[<0;3;2M", "SGR, 1-based cells");
    }
    #[test]
    fn an_encoder_keeps_the_modes_it_followed_without_the_terminal() {
        let mut terminal = Terminal::new(80, 24).unwrap();
        terminal.vt_write(b"\x1b[?1000h\x1b[?1006h");
        let mut encoder = Encoder::new().unwrap();
        encoder.follow(&terminal);
        drop(terminal);
        let press = Mouse { action: Action::Press, button: Some(Button::Left), column: 2, row: 1, mods: 0 };
        assert_eq!(encoder.encode((8, 16), (80, 24), &press), b"\x1b[<0;3;2M");
        let wheel = Mouse { action: Action::Press, button: Some(Button::WheelUp), column: 0, row: 0, mods: 0 };
        assert_eq!(encoder.encode((8, 16), (80, 24), &wheel), b"\x1b[<64;1;1M", "the same encoder, again");
    }
}
