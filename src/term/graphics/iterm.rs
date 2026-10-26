#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub inline: bool,
    pub width: Dim,
    pub height: Dim,
    pub preserve_aspect: bool,
    pub stay: bool,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dim {
    Auto,
    Cells(f64),
    Pixels(f64),
    Percent(f64),
}
impl Args {
    pub fn parse(args: &[u8]) -> Self {
        let mut parsed =
            Args { inline: false, width: Dim::Auto, height: Dim::Auto, preserve_aspect: true, stay: false };
        for pair in String::from_utf8_lossy(args).split(';') {
            let Some((key, value)) = pair.split_once('=') else { continue };
            match key {
                "inline" => parsed.inline = value == "1",
                "width" => parsed.width = Dim::parse(value),
                "height" => parsed.height = Dim::parse(value),
                "preserveAspectRatio" => parsed.preserve_aspect = value != "0",
                "doNotMoveCursor" => parsed.stay = value == "1",
                _ => {}
            }
        }
        parsed
    }
}
impl Dim {
    fn parse(value: &str) -> Self {
        let number = |s: &str| s.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0);
        if let Some(px) = value.strip_suffix("px") {
            number(px).map_or(Dim::Auto, Dim::Pixels)
        } else if let Some(pc) = value.strip_suffix('%') {
            number(pc).map_or(Dim::Auto, Dim::Percent)
        } else {
            number(value).map_or(Dim::Auto, Dim::Cells)
        }
    }
    fn pixels(self, cell: f64, screen: f64) -> Option<f64> {
        match self {
            Dim::Auto => None,
            Dim::Cells(n) => Some(n * cell),
            Dim::Pixels(n) => Some(n),
            Dim::Percent(p) => Some(p / 100.0 * screen),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    pub cols: Option<u32>,
    pub rows: Option<u32>,
}
pub fn fit(args: &Args, image: (u32, u32), cell: (u32, u32), screen: (u16, u16)) -> Fit {
    let (iw, ih) = (f64::from(image.0.max(1)), f64::from(image.1.max(1)));
    let (cw, ch) = (f64::from(cell.0.max(1)), f64::from(cell.1.max(1)));
    let (sw, sh) = (f64::from(screen.0) * cw, f64::from(screen.1) * ch);
    let cols = |w: f64| Some((w / cw).round().max(1.0) as u32);
    let rows = |h: f64| Some((h / ch).round().max(1.0) as u32);
    let natural = Fit { cols: None, rows: None };
    match (args.width.pixels(cw, sw), args.height.pixels(ch, sh)) {
        (None, None) if iw > sw && sw > 0.0 => Fit { cols: cols(sw), rows: None },
        (None, None) => natural,
        (Some(w), None) if args.preserve_aspect => Fit { cols: cols(w), rows: None },
        (Some(w), None) => Fit { cols: cols(w), rows: rows(ih) },
        (None, Some(h)) if args.preserve_aspect => Fit { cols: None, rows: rows(h) },
        (None, Some(h)) => Fit { cols: cols(iw), rows: rows(h) },
        (Some(w), Some(h)) if args.preserve_aspect => {
            if w / iw <= h / ih {
                Fit { cols: cols(w), rows: None }
            } else {
                Fit { cols: None, rows: rows(h) }
            }
        }
        (Some(w), Some(h)) => Fit { cols: cols(w), rows: rows(h) },
    }
}
pub fn png(file: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if let Some(size) = png_size(file) {
        return Some((file.to_vec(), size.0, size.1));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(super::sixel::MAX_SIDE as u32);
    limits.max_image_height = Some(super::sixel::MAX_SIDE as u32);
    limits.max_alloc = Some(256 << 20);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(file)).with_guessed_format().ok()?;
    reader.limits(limits);
    let rgba = reader.decode().ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some((super::encode_png(width, height, rgba.as_raw())?, width, height))
}
fn png_size(file: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if file.len() < 24 || !file.starts_with(SIGNATURE) || &file[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(file[at..at + 4].try_into().unwrap());
    Some((side(16), side(20))).filter(|(w, h)| *w > 0 && *h > 0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arguments() {
        let args = Args::parse(b"name=Zm9v;size=3;inline=1;width=50%;height=10px;preserveAspectRatio=0");
        assert_eq!(
            args,
            Args {
                inline: true,
                width: Dim::Percent(50.0),
                height: Dim::Pixels(10.0),
                preserve_aspect: false,
                stay: false
            }
        );
        assert_eq!(Args::parse(b"width=auto;height=3;doNotMoveCursor=1").height, Dim::Cells(3.0));
        assert!(!Args::parse(b"").inline, "inline is 0 unless said");
    }
    #[test]
    fn sizes() {
        let args = |s: &[u8]| Args::parse(s);
        let at = |a: &Args, image| fit(a, image, (8, 16), (80, 24));
        let none = Fit { cols: None, rows: None };
        assert_eq!(at(&args(b"inline=1"), (100, 50)), none, "auto: its own size");
        assert_eq!(at(&args(b"inline=1"), (1280, 50)), Fit { cols: Some(80), rows: None }, "narrowed to the screen");
        assert_eq!(at(&args(b"width=10"), (100, 50)), Fit { cols: Some(10), rows: None });
        assert_eq!(at(&args(b"height=50%"), (100, 50)), Fit { cols: None, rows: Some(12) });
        assert_eq!(at(&args(b"width=40px;height=4"), (100, 50)), Fit { cols: Some(5), rows: None }, "width binds");
        assert_eq!(at(&args(b"width=40;height=4"), (100, 50)), Fit { cols: None, rows: Some(4) }, "height binds");
        assert_eq!(
            at(&args(b"width=40;height=4;preserveAspectRatio=0"), (100, 50)),
            Fit { cols: Some(40), rows: Some(4) },
            "stretched"
        );
    }
    #[test]
    fn png_header() {
        let png = super::super::encode_png(3, 2, &[255; 24]).unwrap();
        assert_eq!(png_size(&png), Some((3, 2)));
        assert_eq!(super::png(&png).map(|(_, w, h)| (w, h)), Some((3, 2)));
        assert_eq!(png_size(b"GIF89a"), None);
    }
}
