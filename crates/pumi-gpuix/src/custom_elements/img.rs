/// Image custom elements for raster images and tintable SVG icons.
///
/// This provides a native `<img>` for GPUIX React apps while keeping the same
/// custom-element prop pipeline (`setCustomProp`/`custom_props`).
///
/// HTTP(S) `src` is a GPUI URI resource. GPUI fetches it through the app
/// `HttpClient` on a background task and paints once decode finishes. A
/// definite `width` and `height` keep the layout box stable during that load.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use base64::Engine as _;

pub struct ImgFactory;

pub struct SvgFactory;

impl CustomElementFactory for SvgFactory {
    fn element_type(&self) -> &str {
        "svg"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(SvgElement::default())
    }
}

impl CustomElementFactory for ImgFactory {
    fn element_type(&self) -> &str {
        "img"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(ImgElement::default())
    }
}

#[derive(Debug, Clone, Default)]
enum ImgObjectFit {
    Fill,
    #[default]
    Contain,
    Cover,
    ScaleDown,
    None,
}

impl ImgObjectFit {
    fn from_str(value: &str) -> Self {
        match value {
            "fill" => Self::Fill,
            "cover" => Self::Cover,
            "scaleDown" => Self::ScaleDown,
            "none" => Self::None,
            _ => Self::Contain,
        }
    }

    fn as_gpui(&self) -> gpui::ObjectFit {
        match self {
            Self::Fill => gpui::ObjectFit::Fill,
            Self::Contain => gpui::ObjectFit::Contain,
            Self::Cover => gpui::ObjectFit::Cover,
            Self::ScaleDown => gpui::ObjectFit::ScaleDown,
            Self::None => gpui::ObjectFit::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
enum ImgSource {
    #[default]
    Empty,
    Path(std::path::PathBuf),
    Data(std::sync::Arc<gpui::Image>),
    Render(std::sync::Arc<gpui::RenderImage>),
    Invalid,
}

#[derive(Debug, Clone, Default)]
pub struct ImgElement {
    source: ImgSource,
    object_fit: ImgObjectFit,
    alt: String,
    dropped: Option<std::sync::Arc<gpui::RenderImage>>,
    /// `src` as given, for the painter (`crate::images`) when this draws its own pixels.
    src: String,
    sprite: Option<SpriteSpec>,
    /// When the sprite started playing: its frame is counted from here.
    started: Option<std::time::Instant>,
}

/// `sprite`: the image is a sheet of equal cells, drawn one at a time.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpriteSpec {
    /// Cells across. Defaults to 1.
    #[serde(default)]
    columns: Option<u32>,
    /// Cells down. Defaults to as many as `count` needs, or 1.
    #[serde(default)]
    rows: Option<u32>,
    /// Cells in use, when the last row is not full.
    #[serde(default)]
    count: Option<u32>,
    /// Frames a second. 0 (the default) shows `frame` and stays.
    #[serde(default)]
    fps: Option<f64>,
    /// The cell shown (or the first one played), from 0.
    #[serde(default)]
    frame: Option<f64>,
    /// Play past the last cell back to the first. Defaults to true.
    #[serde(default, rename = "loop")]
    looping: Option<bool>,
    /// Defaults to true.
    #[serde(default)]
    playing: Option<bool>,
}

impl SpriteSpec {
    fn grid(&self) -> (u32, u32, u32) {
        let columns = self.columns.unwrap_or(1).max(1);
        let rows = self.rows.unwrap_or_else(|| self.count.map(|c| c.div_ceil(columns)).unwrap_or(1)).max(1);
        let count = self.count.unwrap_or(columns * rows).clamp(1, columns * rows);
        (columns, rows, count)
    }

    /// The cell to show `elapsed` after it started, and how long until the next one (`None`
    /// once it stays).
    pub(crate) fn frame_at(&self, elapsed: std::time::Duration) -> (u32, Option<std::time::Duration>) {
        let (_, _, count) = self.grid();
        let first = self.frame.unwrap_or(0.0).max(0.0) as u64;
        let fps = self.fps.unwrap_or(0.0);
        if fps <= 0.0 || !fps.is_finite() || self.playing == Some(false) || count <= 1 {
            return ((first % count as u64) as u32, None);
        }
        let step = elapsed.as_secs_f64() * fps;
        let advanced = step.floor() as u64;
        let next = std::time::Duration::from_secs_f64((((advanced + 1) as f64 - step) / fps).max(0.0));
        if self.looping == Some(false) {
            let last = count as u64 - 1;
            let index = (first + advanced).min(last);
            return (index as u32, (index < last).then_some(next));
        }
        (((first + advanced) % count as u64) as u32, Some(next))
    }
}

impl ImgObjectFit {
    /// Where an image `iw` by `ih` is drawn in `bounds`, as CSS `object-fit` places it.
    fn place(&self, bounds: gpui::Bounds<gpui::Pixels>, iw: f32, ih: f32) -> gpui::Bounds<gpui::Pixels> {
        let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if iw <= 0.0 || ih <= 0.0 {
            return bounds;
        }
        let scale = match self {
            Self::Fill => return bounds,
            Self::Contain => (bw / iw).min(bh / ih),
            Self::Cover => (bw / iw).max(bh / ih),
            Self::ScaleDown => (bw / iw).min(bh / ih).min(1.0),
            Self::None => 1.0,
        };
        let (w, h) = (iw * scale, ih * scale);
        gpui::Bounds::new(
            bounds.origin + gpui::point(gpui::px((bw - w) / 2.0), gpui::px((bh - h) / 2.0)),
            gpui::size(gpui::px(w), gpui::px(h)),
        )
    }
}

impl ImgElement {
    fn load_src(&mut self, src: &str) {
        let src = src.trim();
        self.src = src.to_string();
        self.source = if src.is_empty() {
            ImgSource::Empty
        } else if src.starts_with("data:") {
            // TODO: Replace JSON data URLs with binary mutations to keep base64 decoding off paint.
            decode_image_data_url(src)
                .map(|(format, bytes)| ImgSource::Data(std::sync::Arc::new(gpui::Image::from_bytes(format, bytes))))
                .unwrap_or(ImgSource::Invalid)
        } else if http_image_uri(src).is_some() {
            // Not fetched: the app has no HTTP client, so a page's image is a `data:` URL or a file.
            ImgSource::Invalid
        } else {
            ImgSource::Path(src.into())
        };
    }
}

fn http_image_uri(src: &str) -> Option<gpui::SharedUri> {
    let scheme_end = src.find("://")?;
    let scheme = &src[..scheme_end];
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    (src.len() > scheme_end + 3).then(|| gpui::SharedUri::from(src.to_string()))
}

#[cfg(test)]
mod tests {
    use super::http_image_uri;

    #[test]
    fn a_sprite_counts_its_frames_and_says_when_the_next_is_due() {
        use std::time::Duration;
        let spec: super::SpriteSpec = serde_json::from_str(r#"{"columns":4,"fps":10}"#).unwrap();
        assert_eq!(spec.frame_at(Duration::from_millis(0)).0, 0);
        let (frame, next) = spec.frame_at(Duration::from_millis(250));
        assert_eq!(frame, 2);
        assert!((next.unwrap().as_secs_f64() - 0.05).abs() < 1e-6);
        assert_eq!(spec.frame_at(Duration::from_millis(450)).0, 0, "it loops");

        let once: super::SpriteSpec =
            serde_json::from_str(r#"{"columns":3,"rows":2,"count":5,"fps":10,"loop":false}"#).unwrap();
        let (frame, next) = once.frame_at(Duration::from_secs(5));
        assert_eq!(frame, 4);
        assert!(next.is_none(), "a finished sprite stops asking for frames");

        let still: super::SpriteSpec = serde_json::from_str(r#"{"columns":3,"frame":2}"#).unwrap();
        assert_eq!(still.frame_at(Duration::from_secs(9)), (2, None));
    }

    #[test]
    fn only_http_urls_become_uri_sources() {
        assert!(http_image_uri("https://example.test/a.png").is_some());
        assert!(http_image_uri("HTTP://localhost:9/a.png").is_some());
        assert!(http_image_uri("HTTPS://example.test/a.png").is_some());
        assert!(http_image_uri("/tmp/a.png").is_none());
        assert!(http_image_uri("data:image/png;base64,xx").is_none());
        assert!(http_image_uri("file:///tmp/a.png").is_none());
        assert!(http_image_uri("https://").is_none());
        assert!(http_image_uri("http://").is_none());
    }
}

fn img_fallback(ctx: &CustomRenderContext, alt: &str, message: &str) -> gpui::AnyElement {
    use gpui::prelude::*;

    let mut fallback = super::custom_surface(
        gpui::div()
            .id(gpui::SharedString::from(format!("__gpuix_img_{}", ctx.id)))
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x1f2230ff))
            .border(gpui::px(1.0))
            .border_color(gpui::rgba(0x5d6481ff))
            .text_color(gpui::rgba(0xa4accdff)),
        ctx,
    );
    fallback = crate::accessibility::apply_accessibility(fallback, ctx.props, Some(gpui::Role::Image));
    fallback = crate::accessibility::apply_image_label(fallback, ctx.props, alt);
    fallback.child(ctx.chrome_text(message.to_string(), None)).into_any_element()
}

impl CustomElement for ImgElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        if let Some(image) = self.dropped.take() {
            window.drop_image(image).ok();
        }

        // A sprite sheet, or pixel art, is drawn here rather than by GPUI's `img`: one cell of
        // the sheet, sampled by nearest neighbour when `imageRendering: "pixelated"`.
        let pixelated = ctx.style.and_then(|style| style.image_rendering.as_deref()) == Some("pixelated");
        if (self.sprite.is_some() || pixelated) && !matches!(self.source, ImgSource::Empty | ImgSource::Invalid) {
            return self.render_painted(ctx, pixelated, window, _cx);
        }

        let el = match &self.source {
            ImgSource::Path(path) => gpui::img(path.clone()),
            ImgSource::Data(image) => gpui::img(image.clone()),
            ImgSource::Render(image) => gpui::img(image.clone()),
            ImgSource::Empty => return img_fallback(&ctx, &self.alt, "img: no src"),
            ImgSource::Invalid => return img_fallback(&ctx, &self.alt, "img: load failed"),
        };
        // The id is what makes gpui's `ImgState` persist. Without it `Img` has no
        // `GlobalElementId`, so the animated-GIF frame index and the delayed
        // loading state are rebuilt from scratch on every frame and an animation
        // never advances past frame zero.
        let mut el = el
            .object_fit(self.object_fit.as_gpui())
            .with_fallback(|| {
                gpui::div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x1f2230ff))
                    .border(gpui::px(1.0))
                    .border_color(gpui::rgba(0x5d6481ff))
                    .text_color(gpui::rgba(0xa4accdff))
                    .child("img: load failed")
                    .into_any_element()
            })
            .id(gpui::SharedString::from(format!("__gpuix_img_{}", ctx.id)));

        if let Some(style) = ctx.style {
            el = crate::renderer::apply_interactive_styles(el, style);
            // GPUI fills `aspect_ratio` from the bitmap once it loads. That
            // overrides a definite height and jumps the box. A CSS `<img>` with
            // both width and height keeps that box; `objectFit` paints inside it.
            if let (
                Some(crate::style::DimensionValue::Pixels(width)),
                Some(crate::style::DimensionValue::Pixels(height)),
            ) = (style.width.as_ref(), style.height.as_ref())
            {
                if *width > 0.0 && *height > 0.0 {
                    el = el.aspect_ratio((*width as f32) / (*height as f32));
                }
            }
        }

        let mut el = crate::accessibility::apply_accessibility(el, ctx.props, Some(gpui::Role::Image));
        el = crate::accessibility::apply_image_label(el, ctx.props, &self.alt);
        let el = super::wire_standard_events(el, &ctx);
        crate::automation::track_own_bounds(el, ctx.id).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "src" => {
                if value.is_null() && matches!(self.source, ImgSource::Render(_)) {
                    return;
                }
                if let ImgSource::Render(image) = &self.source {
                    self.dropped = Some(image.clone());
                }
                self.load_src(value.as_str().unwrap_or(""));
            }
            "objectFit" => self.object_fit = value.as_str().map(ImgObjectFit::from_str).unwrap_or_default(),
            "alt" => self.alt = value.as_str().unwrap_or_default().to_string(),
            "sprite" => {
                let next = serde_json::from_value::<SpriteSpec>(value).ok();
                // A new sheet, or a new start frame, plays from its start again.
                if next.as_ref().map(|s| (s.frame, s.fps)) != self.sprite.as_ref().map(|s| (s.frame, s.fps)) {
                    self.started = None;
                }
                self.sprite = next;
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["src", "objectFit", "alt", "sprite"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave", "fileDrop"]
    }

    fn destroy(&mut self) {}

    fn live_image(&self) -> Option<std::sync::Arc<gpui::RenderImage>> {
        match &self.source {
            ImgSource::Render(image) => Some(image.clone()),
            _ => None,
        }
    }

    fn take_dropped_image(&mut self) -> Option<std::sync::Arc<gpui::RenderImage>> {
        self.dropped.take()
    }

    fn replace_live_image(
        &mut self,
        image: std::sync::Arc<gpui::RenderImage>,
    ) -> Option<std::sync::Arc<gpui::RenderImage>> {
        let previous = self.live_image();
        self.source = ImgSource::Render(image);
        previous
    }
}

/// Byte order of a packed `setImagePixels` buffer. Alpha is straight in both.
impl ImgElement {
    /// The image drawn by `crate::decor::draw`: a sprite's cell, or the whole image, placed by
    /// `objectFit` and sampled by nearest neighbour when `pixelated`.
    fn render_painted(
        &mut self,
        ctx: CustomRenderContext,
        pixelated: bool,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let live = match &self.source {
            ImgSource::Render(image) => Some(image.clone()),
            _ => None,
        };
        let src = self.src.clone();
        let sprite = self.sprite.clone();
        let fit = self.object_fit.clone();
        let started = *self.started.get_or_insert_with(std::time::Instant::now);
        let (columns, rows, _) = sprite.as_ref().map(SpriteSpec::grid).unwrap_or((1, 1, 1));
        let load = move |source: &str,
                         live: &Option<std::sync::Arc<gpui::RenderImage>>,
                         window: &mut gpui::Window,
                         cx: &mut gpui::App| {
            match live {
                Some(image) => {
                    let size = image.size(0);
                    Some((image.clone(), size.width.0 as f32, size.height.0 as f32))
                }
                None => {
                    crate::images::load(source, window, cx).map(|loaded| (loaded.image, loaded.width, loaded.height))
                }
            }
        };

        let mut el =
            super::custom_surface(gpui::div().id(gpui::SharedString::from(format!("__gpuix_img_{}", ctx.id))), &ctx);
        // Without a size of its own the box is one cell, as an `<img>` is its image.
        let sized = |dim: Option<&crate::style::DimensionValue>| {
            dim.is_some_and(|d| !matches!(d, crate::style::DimensionValue::Auto))
        };
        let (has_width, has_height) =
            (sized(ctx.style.and_then(|s| s.width.as_ref())), sized(ctx.style.and_then(|s| s.height.as_ref())));
        if !has_width || !has_height {
            if let Some((_, w, h)) = load(&src, &live, window, cx) {
                let (cw, ch) = (w / columns as f32, h / rows as f32);
                if !has_width {
                    el = el.w(gpui::px(cw));
                }
                if !has_height {
                    el = el.h(gpui::px(ch));
                }
            }
        }
        el = crate::accessibility::apply_image_label(el, ctx.props, &self.alt);
        let painter = gpui::canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let Some((image, w, h)) = load(&src, &live, window, cx) else {
                    return;
                };
                let (index, next) = sprite.as_ref().map(|s| s.frame_at(started.elapsed())).unwrap_or((0, None));
                if let Some(next) = next {
                    crate::schedule::repaint_in(next, window, cx);
                }
                let placed = fit.place(bounds, w / columns as f32, h / rows as f32);
                let pixels = image.size(0);
                let (iw, ih) = (pixels.width.0 as f32, pixels.height.0 as f32);
                let (cw, ch) = (iw / columns as f32, ih / rows as f32);
                if cw <= 0.0 || ch <= 0.0 {
                    return;
                }
                let (column, row) = ((index % columns) as f32, (index / columns) as f32);
                let (sx, sy) = (f32::from(placed.size.width) / cw, f32::from(placed.size.height) / ch);
                let image_bounds = gpui::Bounds::new(
                    placed.origin - gpui::point(gpui::px(column * cw * sx), gpui::px(row * ch * sy)),
                    gpui::size(gpui::px(iw * sx), gpui::px(ih * sy)),
                );
                let visible = placed.intersect(&bounds);
                crate::decor::draw(window, visible, image_bounds, gpui::Corners::default(), &image, pixelated);
            },
        )
        .absolute()
        .top(gpui::px(0.0))
        .left(gpui::px(0.0))
        .right(gpui::px(0.0))
        .bottom(gpui::px(0.0));
        el.child(painter).into_any_element()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba,
    /// GPUI's own `RenderImage` order, so the upload skips the swizzle pass.
    Bgra,
}

impl PixelFormat {
    pub fn parse(format: Option<&str>) -> std::result::Result<Self, String> {
        match format {
            None | Some("rgba") => Ok(Self::Rgba),
            Some("bgra") => Ok(Self::Bgra),
            Some(other) => Err(format!("Unknown pixel format {other:?}, expected \"rgba\" or \"bgra\"")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rgba => "RGBA",
            Self::Bgra => "BGRA",
        }
    }
}

pub fn render_image_from_pixels(
    width: u32,
    height: u32,
    mut bytes: Vec<u8>,
    format: PixelFormat,
) -> std::result::Result<std::sync::Arc<gpui::RenderImage>, String> {
    let name = format.name();
    let expected = (width as u64)
        .checked_mul(height as u64)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| format!("{name} pixel buffer {width}x{height} is too large"))?;
    if bytes.len() != expected {
        return Err(format!(
            "{name} pixel buffer length {} does not match {width}x{height} ({expected} bytes)",
            bytes.len()
        ));
    }
    if format == PixelFormat::Rgba {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    gpui::RenderImage::from_bgra(width, height, bytes)
        .map(std::sync::Arc::new)
        .ok_or_else(|| format!("{name} pixel buffer is not a valid image"))
}

pub fn render_image_from_encoded(
    bytes: Vec<u8>,
    svg_renderer: gpui::SvgRenderer,
) -> std::result::Result<std::sync::Arc<gpui::RenderImage>, String> {
    let format = sniff_image_format(&bytes).ok_or_else(|| "unrecognized image format".to_string())?;
    gpui::Image::from_bytes(format, bytes).to_image_data(svg_renderer).map_err(|error| error.to_string())
}

fn sniff_image_format(bytes: &[u8]) -> Option<gpui::ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(gpui::ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(gpui::ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(gpui::ImageFormat::Gif)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(gpui::ImageFormat::Webp)
    } else if bytes.starts_with(b"BM") {
        Some(gpui::ImageFormat::Bmp)
    } else if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        Some(gpui::ImageFormat::Tiff)
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some(gpui::ImageFormat::Ico)
    } else if bytes.starts_with(b"P1")
        || bytes.starts_with(b"P2")
        || bytes.starts_with(b"P3")
        || bytes.starts_with(b"P4")
        || bytes.starts_with(b"P5")
        || bytes.starts_with(b"P6")
    {
        Some(gpui::ImageFormat::Pnm)
    } else if looks_like_svg(bytes) {
        Some(gpui::ImageFormat::Svg)
    } else {
        None
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let start = std::str::from_utf8(bytes).ok().map(|text| text.trim_start()).unwrap_or("");
    start.starts_with("<svg") || start.starts_with("<?xml")
}

#[derive(Debug, Clone, Default)]
pub struct SvgElement {
    src: String,
    bytes: Option<std::sync::Arc<[u8]>>,
    source: String,
}

impl SvgElement {
    fn load_src(&mut self, src: String) {
        self.bytes = svg_bytes(&src).map(std::sync::Arc::from);
        self.src = src;
    }
}

pub(crate) fn svg_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("data:") {
        let (format, bytes) = decode_image_data_url(src)?;
        return (format == gpui::ImageFormat::Svg).then_some(bytes);
    }
    std::fs::read(src).ok()
}

fn decode_image_data_url(src: &str) -> Option<(gpui::ImageFormat, Vec<u8>)> {
    let (metadata, data) = src.strip_prefix("data:")?.split_once(',')?;
    let mut parts = metadata.split(';');
    let mime_type = parts.next()?.to_ascii_lowercase();
    let format = gpui::ImageFormat::from_mime_type(&mime_type)?;
    let is_base64 = parts.any(|part| part.eq_ignore_ascii_case("base64"));
    let bytes =
        if is_base64 { base64::engine::general_purpose::STANDARD.decode(data).ok()? } else { percent_decode(data) };
    Some((format, bytes))
}

fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""), 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

impl CustomElement for SvgElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let bytes = if self.source.trim().is_empty() { self.bytes.as_deref() } else { Some(self.source.as_bytes()) };
        let element_id = gpui::SharedString::from(format!("__gpuix_svg_{}", ctx.id));
        let Some(bytes) = bytes else {
            let empty = super::custom_surface(gpui::div().id(element_id), &ctx);
            return empty.into_any_element();
        };

        // Its own `color`, else the one it inherits (`currentColor`, as an icon in a button wants).
        let tint = ctx.color.unwrap_or_else(|| gpui::rgb(0xe2e2e2));
        let mut icon = gpui::svg().data(bytes).flex_none().text_color(tint);
        if let Some(degrees) = ctx.style.and_then(|style| style.rotate).filter(|d| d.is_finite() && *d != 0.0) {
            icon = icon.with_transformation(gpui::Transformation::rotate(gpui::radians((degrees as f32).to_radians())));
        }
        let mut icon = icon.id(element_id);
        if let Some(style) = ctx.style {
            icon = crate::renderer::apply_interactive_styles(icon, style);
        }
        icon = crate::accessibility::apply_accessibility(icon, ctx.props, None);
        let icon = super::wire_standard_events(icon, &ctx);
        crate::automation::track_own_bounds(icon, ctx.id).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "src" => self.load_src(value.as_str().unwrap_or_default().to_string()),
            "source" => self.source = value.as_str().unwrap_or_default().to_string(),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["src", "source"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave", "fileDrop"]
    }

    fn destroy(&mut self) {}
}
