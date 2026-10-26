use super::*;
use crate::term::vt::Vt;
use libghostty_vt::kitty::graphics::{ImageFormat, PlacementIterator, PlacementRenderInfo};
use std::sync::{Arc, Mutex};
const SIZE: Size = Size { cols: 40, rows: 10, cell_width: 8, cell_height: 16 };
fn translate(input: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let t = Translator::default().feed(input, SIZE);
    (t.bytes.into_owned(), t.replies)
}
fn translate_bytewise(input: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut translator = Translator::default();
    let (mut bytes, mut replies) = (Vec::new(), Vec::new());
    for byte in input {
        let t = translator.feed(std::slice::from_ref(byte), SIZE);
        bytes.extend_from_slice(&t.bytes);
        replies.extend(t.replies);
    }
    (bytes, replies)
}
fn terminal() -> (Vt, Arc<Mutex<Vec<u8>>>) {
    let mut terminal = Vt::new(SIZE.cols, SIZE.rows, 0).unwrap();
    assert!(terminal.enable_kitty_graphics());
    assert!(terminal.resize(SIZE.cols, SIZE.rows, SIZE.cell_width, SIZE.cell_height));
    let replies = Arc::<Mutex<Vec<u8>>>::default();
    let sink = Arc::clone(&replies);
    assert!(terminal.on_reply(move |bytes| sink.lock().unwrap().extend_from_slice(bytes)));
    (terminal, replies)
}
struct Placement {
    render: PlacementRenderInfo,
    format: ImageFormat,
    image_width: u32,
    image_height: u32,
}
fn placements(terminal: &Vt) -> Vec<Placement> {
    let graphics = terminal.kitty_graphics().unwrap();
    let mut iterator = PlacementIterator::new().unwrap();
    let mut iteration = iterator.update(&graphics).unwrap();
    let mut out = Vec::new();
    while let Some(placement) = iteration.next() {
        let image = graphics.image(placement.image_id().unwrap()).unwrap();
        let render = placement.placement_render_info(&image, terminal).unwrap();
        if render.viewport_visible {
            let (format, image_width, image_height) =
                (image.format().unwrap(), image.width().unwrap(), image.height().unwrap());
            out.push(Placement { render, format, image_width, image_height });
        }
    }
    out
}
fn cursor(terminal: &mut Vt, replies: &Mutex<Vec<u8>>) -> (u16, u16) {
    replies.lock().unwrap().clear();
    terminal.write(b"\x1b[6n");
    let reply = String::from_utf8(std::mem::take(&mut *replies.lock().unwrap())).unwrap();
    let (row, col) = reply.trim_start_matches("\x1b[").trim_end_matches('R').split_once(';').unwrap();
    (row.parse().unwrap(), col.parse().unwrap())
}
fn base64(data: &[u8]) -> String {
    super::base64(data)
}
fn picture(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    let mut file = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).to_rgb8().write_to(&mut file, format).unwrap();
    file.into_inner()
}
fn iterm(args: &str, file: &[u8]) -> Vec<u8> {
    format!("\x1b]1337;File={args}:{}\x07", base64(file)).into_bytes()
}
#[test]
fn other_output_passes_as_it_came() {
    let input: &[u8] =
        b"plain \x1b[31mred\x1b[0m \x1b[?25l\x1b[?1049h\x1b]0;title\x07\x1b]1337;SetUserVar=a=Yg==\x1b\\\
        \x1bP$qm\x1b\\\x1bP+q544e\x1b\\\x1b_Ga=q,i=1;AAAA\x1b\\\x1b7\x1b8 \x1b\x1b[1m done";
    assert!(matches!(Translator::default().feed(b"no escapes here", SIZE).bytes, Cow::Borrowed(_)));
    assert_eq!(translate(input), (input.to_vec(), Vec::new()));
    assert_eq!(translate_bytewise(input), (input.to_vec(), Vec::new()), "split anywhere");
}
#[test]
fn an_iterm_png_is_placed_and_the_cursor_left_right_of_its_last_row() {
    let (mut terminal, replies) = terminal();
    let png = super::encode_png(24, 40, &[255; 24 * 40 * 4]).unwrap();
    terminal.write(b"\x1b[2;5H");
    let (bytes, _) = translate(&iterm("name=eC5wbmc=;inline=1", &png));
    assert!(bytes.starts_with(b"\x1b_Ga=T,f=100,q=2,C=0,m="), "{:?}", String::from_utf8_lossy(&bytes[..40]));
    terminal.write(&bytes);
    let placements = placements(&terminal);
    assert_eq!(placements.len(), 1);
    let r = placements[0].render;
    assert_eq!((r.viewport_col, r.viewport_row, r.grid_cols, r.grid_rows), (4, 1, 3, 3));
    assert_eq!(cursor(&mut terminal, &replies), (4, 8), "last row, right of the image");
}
#[test]
fn other_formats_sizes_and_parts() {
    let (mut terminal, _) = terminal();
    let jpeg = picture(80, 40, image::ImageFormat::Jpeg);
    terminal.write(&translate(&iterm("inline=1;width=5", &jpeg)).0);
    let gif = picture(16, 16, image::ImageFormat::Gif);
    let b64 = base64(&gif);
    let (head, tail) = b64.split_at(b64.len() / 2);
    let parts = format!(
        "\x1b]1337;MultipartFile=inline=1;height=2;doNotMoveCursor=1\x07\x1b]1337;FilePart={head}\x07\x1b]1337;FilePart={tail}\x1b\\\x1b]1337;FileEnd\x07"
    );
    terminal.write(&translate_bytewise(parts.as_bytes()).0);
    terminal.write(&translate(&iterm("name=Zm9v", &jpeg)).0);
    let mut placements = placements(&terminal);
    placements.sort_by_key(|p| p.render.grid_cols);
    let grid: Vec<_> = placements.iter().map(|p| (p.render.grid_cols, p.render.grid_rows)).collect();
    assert_eq!(grid, [(4, 2), (5, 2)], "GIF two rows high (aspect kept), JPEG five columns wide");
    assert!(placements.iter().all(|p| p.format != ImageFormat::Png), "decoded by the terminal");
}
const SIXEL: &[u8] = b"\x1bP0;1;0q\"1;1;8;12#1;2;100;0;0#1!8~-!8~\x1b\\";
#[test]
fn a_sixel_is_placed_and_the_cursor_goes_to_the_line_under_it() {
    let (mut terminal, replies) = terminal();
    terminal.write(b"\x1b[3;6H");
    let (bytes, _) = translate(SIXEL);
    assert_eq!(translate_bytewise(SIXEL).0, bytes, "split anywhere");
    terminal.write(&bytes);
    let placements = placements(&terminal);
    assert_eq!(placements.len(), 1);
    let p = &placements[0];
    assert_eq!((p.image_width, p.image_height), (8, 12));
    assert_eq!((p.render.viewport_col, p.render.viewport_row, p.render.grid_rows), (5, 2, 1));
    assert_eq!(cursor(&mut terminal, &replies), (4, 1), "xterm: the next line");
    terminal.write(b"\x1b[6;6H");
    terminal.write(&translate(&[b"\x1b[?8452h", SIXEL].concat()).0);
    assert_eq!(cursor(&mut terminal, &replies), (6, 7));
}
#[test]
fn a_tall_sixel_at_the_bottom_scrolls_the_screen() {
    let (mut terminal, replies) = terminal();
    terminal.write(b"\x1b[10;1H");
    let tall = b"\x1bPq#0;2;0;100;0!4~-!4~-!4~-!4~-!4~-!4~-!4~-!4~\x1b\\";
    terminal.write(&translate(tall).0);
    let placements = placements(&terminal);
    assert_eq!(placements[0].render.viewport_row, 6, "moved up as the screen scrolled");
    assert_eq!(cursor(&mut terminal, &replies), (10, 1));
}
#[test]
fn tmux_passthrough_is_unwrapped() {
    let kitty = b"\x1b_Ga=q,i=1;AAAA\x1b\\";
    let wrapped = b"\x1bPtmux;\x1b\x1b_Ga=q,i=1;AAAA\x1b\x1b\\\x1b\\";
    assert_eq!(translate(wrapped).0, kitty);
    assert_eq!(translate_bytewise(wrapped).0, kitty);
    let sixel = [
        b"\x1bPtmux;".as_slice(),
        &SIXEL.iter().flat_map(|&b| if b == ESC { vec![ESC, ESC] } else { vec![b] }).collect::<Vec<_>>(),
        b"\x1b\\",
    ]
    .concat();
    assert_eq!(translate(&sixel).0, translate(SIXEL).0);
}
#[test]
fn probes_are_answered() {
    let (bytes, replies) = translate(b"\x1b[?1;1;0S\x1b[?2;1;0S\x1b[?2;4;0S\x1b[?3;1;0S\x1b]1337;ReportCellSize\x1b\\");
    assert!(bytes.is_empty(), "not passed on: {bytes:?}");
    assert_eq!(
        String::from_utf8(replies).unwrap(),
        "\x1b[?1;0;1024S\x1b[?2;0;320;160S\x1b[?2;0;10000;10000S\x1b[?3;1;0S\x1b]1337;ReportCellSize=16.0;8.0;1.0\x1b\\"
    );
}
#[test]
fn the_terminal_says_sixel_in_da1() {
    let (mut terminal, replies) = terminal();
    terminal.write(b"\x1b[c");
    let reply = std::mem::take(&mut *replies.lock().unwrap());
    let reply = pumi_proto::graphics::da1_with_sixel(&reply);
    let reply = String::from_utf8_lossy(&reply);
    assert!(reply.starts_with("\x1b[?") && reply.ends_with('c'), "{reply:?}");
    assert!(reply.trim_end_matches('c').split(';').skip(1).any(|f| f == "4"), "{reply:?}");
}
#[test]
fn a_cancelled_or_oversized_image_shows_nothing_and_output_goes_on() {
    let (bytes, _) = translate(b"\x1bPq#1!8~\x18after");
    assert_eq!(bytes, b"after");
    let (bytes, _) = translate(b"\x1b]1337;File=inline=1:!!!!\x07after");
    assert_eq!(bytes, b"after", "not an image");
}
#[test]
fn base64_is_forgiving() {
    assert_eq!(base64_decode(b"aGVs\nbG8="), b"hello");
    assert_eq!(base64_decode(base64(&[0, 1, 2, 250, 251]).as_bytes()), [0, 1, 2, 250, 251]);
}
