use super::vt::Emulator;
use gpui::RenderImage;
use libghostty_vt::kitty::graphics::{Image, ImageFormat, PlacementIterator, PlacementRenderInfo};
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Clone)]
pub struct Placed {
    pub image: Arc<RenderImage>,
    pub col: i32,
    pub row: i32,
    pub offset: (u32, u32),
    pub size: (u32, u32),
    pub z: i32,
    key: Key,
}
impl PartialEq for Placed {
    fn eq(&self, other: &Self) -> bool {
        (self.key, self.col, self.row, self.offset, self.size, self.z)
            == (other.key, other.col, other.row, other.offset, other.size, other.z)
    }
}
impl std::fmt::Debug for Placed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Placed({} at {},{} {}x{})", self.key.0, self.col, self.row, self.size.0, self.size.1)
    }
}
type Key = (u32, u64, [u32; 4]);
#[derive(Default)]
pub struct Pictures {
    decoded: HashMap<Key, Arc<RenderImage>>,
    generation: Option<u64>,
    placed: Vec<Placed>,
    dropped: Vec<Arc<RenderImage>>,
}
impl Pictures {
    pub fn placed(&mut self, terminal: &Emulator) -> Vec<Placed> {
        let decoded = &self.decoded;
        let known = |image_id: u32, generation: u64| decoded.keys().any(|k| k.0 == image_id && k.1 == generation);
        let placements = Self::found(terminal, known).unwrap_or_default();
        if placements.is_empty() {
            if self.generation.take().is_some() {
                self.dropped.extend(self.decoded.drain().map(|(_, image)| image));
                self.placed.clear();
            }
            return Vec::new();
        }
        let mut kept = HashMap::with_capacity(placements.len());
        let mut placed = Vec::with_capacity(placements.len());
        for found in placements {
            let r = found.render;
            let key =
                (found.image_id, found.image_generation, [r.source_x, r.source_y, r.source_width, r.source_height]);
            let image = match self.decoded.remove(&key).or_else(|| kept.get(&key).cloned()) {
                Some(image) => image,
                None => match found.pixels.as_ref().and_then(|pixels| decode(pixels, &r)) {
                    Some(image) => Arc::new(image),
                    None => continue,
                },
            };
            kept.insert(key, Arc::clone(&image));
            placed.push(Placed {
                image,
                col: r.viewport_col,
                row: r.viewport_row,
                offset: found.offset,
                size: (r.pixel_width, r.pixel_height),
                z: found.z,
                key,
            });
        }
        self.dropped.extend(std::mem::replace(&mut self.decoded, kept).into_values());
        self.generation = terminal.kitty_graphics().and_then(|graphics| graphics.generation()).ok();
        self.placed = placed.clone();
        placed
    }
    fn found(terminal: &Emulator, known: impl Fn(u32, u64) -> bool) -> Option<Vec<Found>> {
        let graphics = terminal.kitty_graphics().ok()?;
        let mut iterator = PlacementIterator::new().ok()?;
        let mut iteration = iterator.update(&graphics).ok()?;
        let mut out = Vec::new();
        while let Some(placement) = iteration.next() {
            let Some(image) = placement.image_id().ok().and_then(|id| graphics.image(id)) else { continue };
            let Ok(render) = placement.placement_render_info(&image, terminal) else { continue };
            if !render.viewport_visible || render.pixel_width == 0 || render.pixel_height == 0 {
                continue;
            }
            let (image_id, image_generation) = (image.id().unwrap_or(0), image.generation().unwrap_or(0));
            out.push(Found {
                image_id,
                image_generation,
                render,
                offset: (placement.x_offset().unwrap_or(0), placement.y_offset().unwrap_or(0)),
                z: placement.z().unwrap_or(0),
                pixels: if known(image_id, image_generation) { None } else { Pixels::of(&image) },
            });
        }
        Some(out)
    }
    pub fn take_dropped(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.dropped)
    }
}
struct Found {
    image_id: u32,
    image_generation: u64,
    render: PlacementRenderInfo,
    offset: (u32, u32),
    z: i32,
    pixels: Option<Pixels>,
}
struct Pixels {
    format: ImageFormat,
    width: usize,
    height: usize,
    data: Vec<u8>,
}
impl Pixels {
    fn of(image: &Image<'_>) -> Option<Self> {
        Some(Self {
            format: image.format().ok()?,
            width: image.width().ok()? as usize,
            height: image.height().ok()? as usize,
            data: image.data().ok().flatten()?.to_vec(),
        })
    }
}
fn decode(pixels: &Pixels, r: &PlacementRenderInfo) -> Option<RenderImage> {
    let channels = match pixels.format {
        ImageFormat::Rgba => 4,
        ImageFormat::Rgb => 3,
        ImageFormat::GrayAlpha => 2,
        ImageFormat::Gray => 1,
        _ => return None,
    };
    let (width, height) = (pixels.width, pixels.height);
    if pixels.data.len() < width * height * channels {
        return None;
    }
    let (x0, y0) = (r.source_x as usize, r.source_y as usize);
    let (w, h) = (
        (r.source_width as usize).min(width.saturating_sub(x0)),
        (r.source_height as usize).min(height.saturating_sub(y0)),
    );
    if w == 0 || h == 0 {
        return None;
    }
    let mut bgra = Vec::with_capacity(w * h * 4);
    for y in y0..y0 + h {
        let row = &pixels.data[(y * width + x0) * channels..(y * width + x0 + w) * channels];
        for pixel in row.chunks_exact(channels) {
            let [r, g, b, a] = match *pixel {
                [r, g, b, a] => [r, g, b, a],
                [r, g, b] => [r, g, b, 255],
                [v, a] => [v, v, v, a],
                [v] => [v, v, v, 255],
                _ => return None,
            };
            bgra.extend_from_slice(&[b, g, r, a]);
        }
    }
    let frame = image::RgbaImage::from_raw(w as u32, h as u32, bgra)?;
    Some(RenderImage::new(vec![image::Frame::new(frame)]))
}
#[cfg(test)]
mod tests {
    use super::*;
    const ICAT: &[u8] = b"\x1b[2;3H\x1b_Ga=T,f=24,s=2,v=1,i=7,q=2;/wAAAAD/\x1b\\";
    #[test]
    fn a_placed_image_is_decoded_once_and_dropped_when_gone() {
        let mut terminal = crate::term::vt::Vt::new(20, 5, 0).unwrap();
        assert!(terminal.enable_kitty_graphics());
        assert!(terminal.resize(20, 5, 8, 16));
        terminal.write(ICAT);
        let mut pictures = Pictures::default();
        let placed = pictures.placed(&terminal);
        assert_eq!(placed.len(), 1);
        assert_eq!((placed[0].col, placed[0].row), (2, 1));
        let first = Arc::clone(&placed[0].image);
        assert_eq!(&first.as_bytes(0).unwrap()[..8], &[0, 0, 255, 255, 255, 0, 0, 255], "BGRA");
        let again = pictures.placed(&terminal);
        assert!(Arc::ptr_eq(&first, &again[0].image), "decoded once");
        terminal.write(b"\x1b_Ga=d,d=A,q=2\x1b\\");
        assert!(pictures.placed(&terminal).is_empty());
        assert_eq!(pictures.take_dropped().len(), 1);
    }
}
