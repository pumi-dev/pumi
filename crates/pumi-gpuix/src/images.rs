//! Images a style paints (`backgroundImage`, `borderImage`, a sprite sheet) and the rasters the
//! decoration painter makes (gradients, tiled and faded images), behind one cache.
//!
//! A path or `http(s)` source loads through GPUI's asset system, off the main thread, and the
//! frame it lands in repaints. A `data:` URI decodes here, once. An SVG is parsed once and
//! rasterised at the size it is drawn, so it stays sharp at any scale. Everything this module
//! made itself is kept by recency and released from the GPU atlas when it falls out.
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use base64::Engine as _;

/// An image ready to paint, with the size it has on screen at 1x (an SVG's own size, a raster's
/// pixels).
#[derive(Clone)]
pub(crate) struct Loaded {
    pub image: Arc<gpui::RenderImage>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    /// A decoded `data:` raster.
    Data(u64),
    /// An SVG at its own size (for measuring) or at a drawn size in device pixels.
    Svg(u64, u32, u32),
    /// A rasterised gradient: its spec and the raster's size.
    Gradient(u64, u32, u32),
    /// An image repeated `nx` by `ny` times, at an opacity (in 1/255ths).
    Tile(usize, u32, u32, u8),
}

struct Entry {
    image: Option<Arc<gpui::RenderImage>>,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    /// Parsed SVG documents by source, with their own size.
    svgs: HashMap<u64, Option<(Arc<gpui::ParsedSvg>, f32, f32)>>,
    tick: u64,
}

/// How many rasters this module keeps before it releases the least recently drawn.
const CAPACITY: usize = 384;

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(crate) fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The cached raster under `key`, or `make`'s, kept from now on. `None` when `make` failed (and
/// that is remembered too, so a bad source is not decoded every frame).
fn cached(
    key: Key,
    window: &mut gpui::Window,
    make: impl FnOnce() -> Option<Arc<gpui::RenderImage>>,
) -> Option<Arc<gpui::RenderImage>> {
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.tick += 1;
        let tick = cache.tick;
        cache.entries.get_mut(&key).map(|entry| {
            entry.used = tick;
            entry.image.clone()
        })
    });
    if let Some(hit) = hit {
        return hit;
    }
    let image = make();
    let evicted = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let tick = cache.tick;
        cache.entries.insert(key, Entry { image: image.clone(), used: tick });
        let mut evicted = Vec::new();
        while cache.entries.len() > CAPACITY {
            let Some(oldest) = cache.entries.iter().min_by_key(|(_, entry)| entry.used).map(|(key, _)| *key) else {
                break;
            };
            if let Some(Entry { image: Some(image), .. }) = cache.entries.remove(&oldest) {
                evicted.push(image);
            }
        }
        evicted
    });
    for image in evicted {
        window.drop_image(image).ok();
    }
    image
}

fn is_svg(src: &str) -> bool {
    if let Some(rest) = src.strip_prefix("data:") {
        return rest.to_ascii_lowercase().starts_with("image/svg");
    }
    let path = src.split(['?', '#']).next().unwrap_or(src);
    path.to_ascii_lowercase().ends_with(".svg")
}

fn local_path(src: &str) -> &str {
    src.strip_prefix("file://").unwrap_or(src)
}

fn is_http(src: &str) -> bool {
    let lower = src.get(..8).unwrap_or(src).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// A `data:` URI's format and bytes.
pub(crate) fn decode_data_url(src: &str) -> Option<(Option<gpui::ImageFormat>, Vec<u8>)> {
    let (metadata, data) = src.strip_prefix("data:")?.split_once(',')?;
    let mut parts = metadata.split(';');
    let mime_type = parts.next()?.to_ascii_lowercase();
    let format = gpui::ImageFormat::from_mime_type(&mime_type);
    let is_base64 = parts.any(|part| part.eq_ignore_ascii_case("base64"));
    let bytes = if is_base64 {
        base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?
    } else {
        percent_encoding::percent_decode_str(data).collect()
    };
    Some((format, bytes))
}

fn svg_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("data:") {
        return decode_data_url(src).map(|(_, bytes)| bytes);
    }
    std::fs::read(local_path(src)).ok()
}

/// The parsed SVG for `src` and its own size, parsed once.
fn parsed_svg(src: &str, cx: &gpui::App) -> Option<(Arc<gpui::ParsedSvg>, f32, f32)> {
    let id = hash_of(&src);
    if let Some(found) = CACHE.with(|cache| cache.borrow().svgs.get(&id).cloned()) {
        return found;
    }
    let renderer = cx.svg_renderer();
    let parsed = svg_bytes(src).and_then(|bytes| renderer.parse_svg(&bytes).ok()).and_then(|parsed| {
        // A render at scale 1 is GPUI's 2x raster: its own size is half that.
        let probe = renderer.render_parsed(&parsed, 1.0).ok()?;
        let size = probe.size(0);
        let scale = gpui::SMOOTH_SVG_SCALE_FACTOR;
        Some((Arc::new(parsed), size.width.0 as f32 / scale, size.height.0 as f32 / scale))
    });
    if parsed.is_none() {
        log::warn!("gpuix: could not load the SVG {}", short(src));
    }
    CACHE.with(|cache| cache.borrow_mut().svgs.insert(id, parsed.clone()));
    parsed
}

fn short(src: &str) -> &str {
    if src.starts_with("data:") {
        src.get(..40).unwrap_or(src)
    } else {
        src
    }
}

/// `src` at its own size. `None` while it loads, or when it cannot.
pub(crate) fn load(src: &str, window: &mut gpui::Window, cx: &mut gpui::App) -> Option<Loaded> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    if is_svg(src) {
        let (_, width, height) = parsed_svg(src, cx)?;
        let image = load_sized(src, width, height, window, cx)?;
        return Some(Loaded { image, width, height });
    }
    let image = if src.starts_with("data:") {
        let key = Key::Data(hash_of(&src));
        let svg_renderer = cx.svg_renderer();
        cached(key, window, || {
            let (format, bytes) = decode_data_url(src)?;
            let decoded = match format {
                Some(format) => {
                    gpui::Image::from_bytes(format, bytes).to_image_data(svg_renderer).map_err(|e| e.to_string())
                }
                None => crate::custom_elements::img::render_image_from_encoded(bytes, svg_renderer),
            };
            decoded.map_err(|error| log::warn!("gpuix: could not decode {}: {error}", short(src))).ok()
        })?
    } else {
        let resource = if is_http(src) {
            gpui::Resource::Uri(gpui::SharedUri::from(src.to_string()))
        } else {
            gpui::Resource::Path(Arc::from(std::path::Path::new(local_path(src))))
        };
        match window.use_asset::<gpui::ImgResourceLoader>(&resource, cx)? {
            Ok(image) => image,
            Err(_) => return None,
        }
    };
    let size = image.size(0);
    Some(Loaded { width: size.width.0 as f32, height: size.height.0 as f32, image })
}

/// `src` to be drawn `width` by `height` (logical pixels): an SVG rasterised at that size in
/// device pixels, any other image as it is.
pub(crate) fn load_sized(
    src: &str,
    width: f32,
    height: f32,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> Option<Arc<gpui::RenderImage>> {
    if !is_svg(src) {
        return load(src, window, cx).map(|loaded| loaded.image);
    }
    let (parsed, _, _) = parsed_svg(src, cx)?;
    let scale = window.scale_factor();
    // Whole device pixels, and never more than a 4k texture side.
    let w = ((width * scale).ceil() as u32).clamp(1, 4096);
    let h = ((height * scale).ceil() as u32).clamp(1, 4096);
    let renderer = cx.svg_renderer();
    cached(Key::Svg(hash_of(&src), w, h), window, || {
        renderer
            .render_parsed(
                &parsed,
                gpui::SvgSize::ExactSize(gpui::size(gpui::DevicePixels(w as i32), gpui::DevicePixels(h as i32))),
            )
            .ok()
    })
}

/// A gradient raster `w` by `h` pixels (BGRA), made by `make` once per spec and size.
pub(crate) fn gradient(
    spec: u64,
    w: u32,
    h: u32,
    window: &mut gpui::Window,
    make: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Arc<gpui::RenderImage>> {
    cached(Key::Gradient(spec, w, h), window, || {
        make().and_then(|bytes| gpui::RenderImage::from_bgra(w, h, bytes)).map(Arc::new)
    })
}

/// `image` repeated `nx` by `ny` times, at `opacity`: fewer sprites for a small tile over a big
/// floor, and a layer's own opacity (GPUI's images take only the element's).
pub(crate) fn tiled(
    image: &Arc<gpui::RenderImage>,
    nx: u32,
    ny: u32,
    opacity: f32,
    window: &mut gpui::Window,
) -> Option<Arc<gpui::RenderImage>> {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    if nx <= 1 && ny <= 1 && alpha == 255 {
        return Some(image.clone());
    }
    let key = Key::Tile(image.id.0, nx, ny, alpha);
    cached(key, window, || {
        let size = image.size(0);
        let (w, h) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
        let bytes = image.as_bytes(0)?;
        let out = repeat_pixels(bytes, w, h, nx.max(1), ny.max(1), alpha)?;
        gpui::RenderImage::from_bgra(w * nx.max(1), h * ny.max(1), out).map(Arc::new)
    })
}

/// `bytes` (`w` by `h`, 4 bytes a pixel) repeated `nx` by `ny` times, alpha scaled by `alpha`/255.
pub(crate) fn repeat_pixels(bytes: &[u8], w: u32, h: u32, nx: u32, ny: u32, alpha: u8) -> Option<Vec<u8>> {
    let row = (w as usize).checked_mul(4)?;
    if w == 0 || h == 0 || bytes.len() < row * h as usize {
        return None;
    }
    let out_row = row.checked_mul(nx as usize)?;
    let mut out = Vec::with_capacity(out_row.checked_mul((h * ny) as usize)?);
    for _ in 0..ny {
        for y in 0..h as usize {
            let line = &bytes[y * row..(y + 1) * row];
            for _ in 0..nx {
                out.extend_from_slice(line);
            }
        }
    }
    if alpha != 255 {
        // GPUI's rasters are straight alpha: only the alpha channel scales.
        for pixel in out.chunks_exact_mut(4) {
            pixel[3] = ((pixel[3] as u16 * alpha as u16 + 127) / 255) as u8;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_repeats_across_and_down() {
        // 1x2: a red pixel over a blue one (BGRA).
        let bytes = [0, 0, 255, 255, 255, 0, 0, 255];
        let out = repeat_pixels(&bytes, 1, 2, 2, 2, 255).unwrap();
        assert_eq!(out.len(), 2 * 4 * 4);
        let px = |x: usize, y: usize| &out[(y * 2 + x) * 4..(y * 2 + x) * 4 + 4];
        assert_eq!(px(1, 0), &[0, 0, 255, 255]);
        assert_eq!(px(0, 1), &[255, 0, 0, 255]);
        assert_eq!(px(1, 3), &[255, 0, 0, 255]);
    }

    #[test]
    fn an_opacity_scales_only_alpha() {
        let out = repeat_pixels(&[10, 20, 30, 200], 1, 1, 1, 1, 128).unwrap();
        assert_eq!(out, vec![10, 20, 30, 100]);
    }

    #[test]
    fn a_short_buffer_is_refused() {
        assert!(repeat_pixels(&[0, 0, 0], 1, 1, 2, 2, 255).is_none());
    }

    #[test]
    fn data_urls_decode_base64_and_percent_forms() {
        let (format, bytes) = decode_data_url("data:image/png;base64,iVBORw==").unwrap();
        assert_eq!(format, Some(gpui::ImageFormat::Png));
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        let (format, bytes) = decode_data_url("data:image/svg+xml,%3Csvg%3E").unwrap();
        assert_eq!(format, Some(gpui::ImageFormat::Svg));
        assert_eq!(bytes, b"<svg>");
    }

    #[test]
    fn svg_sources_are_recognised() {
        assert!(is_svg("/a/b/floor.svg"));
        assert!(is_svg("data:image/svg+xml;base64,AAAA"));
        assert!(!is_svg("/a/b/floor.png"));
        assert!(is_svg("file:///a/b.SVG?x=1"));
    }
}
