pub const REGISTERS: usize = 1024;
pub const MAX_SIDE: usize = 10_000;
const MAX_PIXELS: usize = 16 << 20;
const VT340: [[u8; 3]; 16] = [
    [0, 0, 0],
    [20, 20, 80],
    [80, 13, 13],
    [20, 80, 20],
    [80, 20, 80],
    [20, 80, 80],
    [80, 80, 20],
    [53, 53, 53],
    [26, 26, 26],
    [33, 33, 60],
    [60, 26, 26],
    [33, 60, 33],
    [60, 33, 60],
    [33, 60, 60],
    [60, 60, 33],
    [80, 80, 80],
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    None,
    Repeat,
    Colour,
    Raster,
}
pub struct Decoder {
    palette: Vec<[u8; 4]>,
    colour: [u8; 4],
    x: usize,
    y: usize,
    pixels: Vec<[u8; 4]>,
    stride: usize,
    rows: usize,
    width: usize,
    height: usize,
    raster: (usize, usize),
    command: Command,
    params: [u32; 5],
    count: usize,
    repeat: usize,
}
impl Default for Decoder {
    fn default() -> Self {
        let mut palette = vec![[0, 0, 0, 255]; REGISTERS];
        for (slot, [r, g, b]) in palette.iter_mut().zip(VT340) {
            *slot = [percent(r.into()), percent(g.into()), percent(b.into()), 255];
        }
        Self {
            colour: palette[0],
            palette,
            x: 0,
            y: 0,
            pixels: Vec::new(),
            stride: 0,
            rows: 0,
            width: 0,
            height: 0,
            raster: (0, 0),
            command: Command::None,
            params: [0; 5],
            count: 0,
            repeat: 1,
        }
    }
}
impl Decoder {
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            if self.command != Command::None {
                match byte {
                    b'0'..=b'9' => {
                        let param = &mut self.params[self.count.min(4)];
                        *param = param.saturating_mul(10).saturating_add(u32::from(byte - b'0'));
                        continue;
                    }
                    b';' => {
                        self.count += 1;
                        continue;
                    }
                    _ => self.end_command(),
                }
            }
            match byte {
                0x3f..=0x7e => {
                    let n = std::mem::replace(&mut self.repeat, 1);
                    self.sixel(byte - 0x3f, n);
                }
                b'!' => self.begin(Command::Repeat),
                b'#' => self.begin(Command::Colour),
                b'"' => self.begin(Command::Raster),
                b'$' => self.x = 0,
                b'-' => {
                    self.x = 0;
                    self.y = (self.y + 6).min(MAX_SIDE);
                }
                _ => {}
            }
        }
    }
    pub fn finish(mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.end_command();
        let width = self.width.max(self.raster.0).min(MAX_SIDE);
        let height = self.height.max(self.raster.1).min(MAX_SIDE);
        if width == 0 || height == 0 || width * height > MAX_PIXELS {
            return None;
        }
        let mut rgba = vec![0u8; width * height * 4];
        for row in 0..height.min(self.rows) {
            let from = &self.pixels[row * self.stride..row * self.stride + width.min(self.stride)];
            let to = &mut rgba[row * width * 4..];
            for (pixel, out) in from.iter().zip(to.chunks_exact_mut(4)) {
                out.copy_from_slice(pixel);
            }
        }
        Some((width as u32, height as u32, rgba))
    }
    fn begin(&mut self, command: Command) {
        self.command = command;
        self.params = [0; 5];
        self.count = 0;
    }
    fn end_command(&mut self) {
        let p = self.params;
        match std::mem::replace(&mut self.command, Command::None) {
            Command::None => {}
            Command::Repeat => self.repeat = (p[0] as usize).clamp(1, MAX_SIDE),
            Command::Raster => {
                if self.count >= 3 {
                    let (w, h) = ((p[2] as usize).min(MAX_SIDE), (p[3] as usize).min(MAX_SIDE));
                    self.raster = (w, h);
                    if w * h <= MAX_PIXELS {
                        self.reserve(w, h);
                    }
                }
            }
            Command::Colour => {
                let register = p[0] as usize % REGISTERS;
                if self.count >= 4 {
                    let colour = match p[1] {
                        1 => hls(p[2], p[3], p[4]),
                        2 => [percent(p[2]), percent(p[3]), percent(p[4])],
                        _ => return,
                    };
                    self.palette[register] = [colour[0], colour[1], colour[2], 255];
                }
                self.colour = self.palette[register];
            }
        }
    }
    fn sixel(&mut self, bits: u8, n: usize) {
        let x = self.x;
        self.x = (x + n).min(MAX_SIDE);
        if bits == 0 || x >= MAX_SIDE || self.y >= MAX_SIDE {
            return;
        }
        let n = n.min(MAX_SIDE - x);
        let top = 8 - bits.leading_zeros() as usize;
        if !self.reserve(x + n, self.y + top) {
            return;
        }
        for bit in 0..6 {
            if bits & (1 << bit) == 0 {
                continue;
            }
            let start = (self.y + bit) * self.stride + x;
            self.pixels[start..start + n].fill(self.colour);
        }
        self.width = self.width.max(x + n);
        self.height = self.height.max(self.y + top);
    }
    fn reserve(&mut self, width: usize, height: usize) -> bool {
        if width <= self.stride && height <= self.rows {
            return true;
        }
        let stride = if width > self.stride { width.max(self.stride * 2).min(MAX_SIDE) } else { self.stride };
        let rows = if height > self.rows { height.max(self.rows * 2).min(MAX_SIDE) } else { self.rows };
        if stride * rows > MAX_PIXELS {
            let (stride, rows) = (width.max(self.stride), height.max(self.rows));
            if stride * rows > MAX_PIXELS {
                return false;
            }
            self.relayout(stride, rows);
            return true;
        }
        self.relayout(stride, rows);
        true
    }
    fn relayout(&mut self, stride: usize, rows: usize) {
        let mut pixels = vec![[0u8; 4]; stride * rows];
        if self.stride > 0 {
            for row in 0..self.rows {
                let from = &self.pixels[row * self.stride..(row + 1) * self.stride];
                pixels[row * stride..row * stride + self.stride].copy_from_slice(from);
            }
        }
        self.pixels = pixels;
        self.stride = stride;
        self.rows = rows;
    }
}
fn percent(p: u32) -> u8 {
    (p.min(100) * 255 / 100) as u8
}
fn hls(hue: u32, lightness: u32, saturation: u32) -> [u8; 3] {
    let h = f64::from((hue + 240) % 360) / 60.0;
    let l = f64::from(lightness.min(100)) / 100.0;
    let s = f64::from(saturation.min(100)) / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let channel = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [channel(r), channel(g), channel(b)]
}
#[cfg(test)]
mod tests {
    use super::*;
    fn decode(data: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut decoder = Decoder::default();
        decoder.feed(data);
        decoder.finish().unwrap()
    }
    fn pixel(image: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.0 + x) * 4) as usize;
        image.2[i..i + 4].try_into().unwrap()
    }
    #[test]
    fn colours_repeats_and_bands() {
        let image = decode(b"#1;2;100;0;0#1!3~-#2;1;0;50;100@");
        assert_eq!((image.0, image.1), (3, 7), "three wide, a band and one row");
        assert_eq!(pixel(&image, 2, 5), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 0, 6), [0, 0, 255, 255]);
        assert_eq!(pixel(&image, 1, 6), [0, 0, 0, 0], "not drawn: transparent");
    }
    #[test]
    fn raster_attributes_size_the_image_and_cr_overdraws() {
        let image = decode(b"\"1;1;4;2#3??$#2A");
        assert_eq!((image.0, image.1), (4, 2));
        assert_eq!(pixel(&image, 0, 1), [204, 33, 33, 255], "register 2 (VT340 red), drawn over");
        assert_eq!(pixel(&image, 3, 0), [0, 0, 0, 0]);
    }
    #[test]
    fn a_split_stream_decodes_the_same() {
        let data = b"\"1;1;10;12#5;2;10;20;30#5!10~-!10N";
        let whole = decode(data);
        let mut decoder = Decoder::default();
        for chunk in data.chunks(3) {
            decoder.feed(chunk);
        }
        assert_eq!(decoder.finish().unwrap(), whole);
    }
    #[test]
    fn nothing_drawn_is_no_image_and_huge_is_clipped() {
        let mut decoder = Decoder::default();
        decoder.feed(b"#1");
        assert!(decoder.finish().is_none());
        let image = decode(b"!99999~");
        assert_eq!((image.0, image.1), (MAX_SIDE as u32, 6));
    }
}
