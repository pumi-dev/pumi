//! What GPUI's quad cannot paint, painted under an element's children: gradients past two linear
//! stops (radial, conic, many stops), layered backgrounds, image backgrounds (sized, positioned,
//! tiled), nine-slice frames (`borderImage`) and a colour per border side.
//!
//! The painter is the element's first child: a canvas laid over its padding box (an absolute
//! child's box), which it grows by the border widths to reach the border box. GPUI paints the
//! element's own quad (shadow, fill, plain border) before it and the element's children after.
use std::sync::Arc;

use gpui::{Bounds, Corners, Pixels, Point, Size};

use crate::style::{
    BackgroundExt, BackgroundImageLayer, BackgroundLayerValue, BackgroundPositionValue, BackgroundSizeValue,
    BackgroundValue, BorderImageValue, GradientValue, LengthValue, LinearGradientStopValue, RadiusValue, StyleDesc,
    StyleExt,
};

/// A raster gradient's longest side. It is stretched over the box with bilinear sampling, which a
/// smooth gradient survives; a hard stop softens by at most a pixel of this raster.
const GRADIENT_RASTER: f32 = 512.0;
/// A tiled image whose tiles would be more sprites than this is repeated into a bigger tile first.
const TILE_SPRITES: usize = 96;

/// Everything one element paints here, owned so the canvas can keep it past the build.
#[derive(Clone)]
pub(crate) struct Decor {
    /// The background's layers, bottom first.
    layers: Vec<BackgroundLayerValue>,
    /// Image layers, bottom first.
    images: Vec<BackgroundImageLayer>,
    size: Option<BackgroundSizeValue>,
    repeat: Option<String>,
    position: Option<BackgroundPositionValue>,
    frame: Option<BorderImageValue>,
    /// `[top, right, bottom, left]`, when a side has its own colour.
    sides: Option<[gpui::Hsla; 4]>,
    widths: [f32; 4],
    radii: [f32; 4],
    nearest: bool,
}

impl Decor {
    pub(crate) fn of(style: &StyleDesc) -> Option<Self> {
        if !style.has_decor() {
            return None;
        }
        let layers = match &style.background {
            Some(background @ BackgroundValue::Gradient(gradient)) if background.is_painted() => {
                vec![BackgroundLayerValue::Gradient(gradient.clone())]
            }
            Some(BackgroundValue::Layers(layers)) => layers.iter().rev().cloned().collect(),
            _ => Vec::new(),
        };
        let images =
            style.background_image.as_ref().map(|v| v.layers().into_iter().rev().collect()).unwrap_or_default();
        let sides = style.has_side_colors().then(|| {
            let all = style.border_color.as_deref();
            let pick = |side: &Option<String>| {
                side.as_deref()
                    .or(all)
                    .and_then(crate::color::parse_color_rgba)
                    .map(gpui::Hsla::from)
                    .unwrap_or_else(gpui::transparent_black)
            };
            [
                pick(&style.border_top_color),
                pick(&style.border_right_color),
                pick(&style.border_bottom_color),
                pick(&style.border_left_color),
            ]
        });
        Some(Self {
            layers,
            images,
            size: style.background_size.clone(),
            repeat: style.background_repeat.clone(),
            position: style.background_position.clone(),
            frame: style.border_image.clone(),
            sides,
            widths: style.border_widths().map(|w| w as f32),
            radii: style.corner_radii().map(|r| r as f32),
            nearest: style.image_rendering.as_deref() == Some("pixelated"),
        })
    }

    fn corners(&self) -> Corners<Pixels> {
        let [tl, tr, br, bl] = self.radii.map(gpui::px);
        Corners { top_left: tl, top_right: tr, bottom_right: br, bottom_left: bl }
    }

    fn rounded(&self) -> bool {
        self.radii.iter().any(|r| *r > 0.0)
    }

    /// The border box around `padding_box`.
    fn border_box(&self, padding_box: Bounds<Pixels>) -> Bounds<Pixels> {
        let [t, r, b, l] = self.widths;
        Bounds {
            origin: padding_box.origin - Point::new(gpui::px(l), gpui::px(t)),
            size: Size {
                width: padding_box.size.width + gpui::px(l + r),
                height: padding_box.size.height + gpui::px(t + b),
            },
        }
    }

    pub(crate) fn paint(&self, padding_box: Bounds<Pixels>, window: &mut gpui::Window, cx: &mut gpui::App) {
        let border_box = self.border_box(padding_box);
        for layer in &self.layers {
            self.paint_layer(layer, border_box, window);
        }
        if !self.images.is_empty() {
            window.with_content_mask(Some(gpui::ContentMask { bounds: border_box }), |window| {
                for layer in &self.images {
                    self.paint_image_layer(layer, padding_box, border_box, window, cx);
                }
            });
        }
        if let Some(frame) = &self.frame {
            self.paint_frame(frame, border_box, window, cx);
        }
        if let Some(sides) = self.sides {
            self.paint_sides(sides, border_box, window);
        }
    }

    fn paint_layer(&self, layer: &BackgroundLayerValue, bounds: Bounds<Pixels>, window: &mut gpui::Window) {
        match layer {
            BackgroundLayerValue::Color(color) => {
                if let Some(color) = crate::color::parse_color_rgba(color) {
                    window.paint_quad(gpui::fill(bounds, color).corner_radii(self.corners()));
                }
            }
            BackgroundLayerValue::Gradient(gradient) => {
                let native = BackgroundValue::Gradient(gradient.clone());
                if !native.is_painted() {
                    if let Some(background) = native_background(gradient) {
                        window.paint_quad(gpui::fill(bounds, background).corner_radii(self.corners()));
                    }
                    return;
                }
                paint_gradient(gradient, bounds, self.corners(), window);
            }
        }
    }

    fn paint_image_layer(
        &self,
        layer: &BackgroundImageLayer,
        area: Bounds<Pixels>,
        clip: Bounds<Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let Some(natural) = crate::images::load(&layer.src, window, cx) else {
            return;
        };
        if natural.width <= 0.0 || natural.height <= 0.0 {
            return;
        }
        let (aw, ah) = (f32::from(area.size.width), f32::from(area.size.height));
        let size = layer.size.as_ref().or(self.size.as_ref());
        let (tw, th) = tile_size(size, aw, ah, natural.width, natural.height);
        if tw <= 0.0 || th <= 0.0 || !tw.is_finite() || !th.is_finite() {
            return;
        }
        let position = layer.position.as_ref().or(self.position.as_ref());
        let (ox, oy) = tile_offset(position, aw, ah, tw, th);
        let repeat = layer.repeat.as_deref().or(self.repeat.as_deref()).unwrap_or("repeat");
        let (rx, ry) = match repeat {
            "no-repeat" => (false, false),
            "repeat-x" => (true, false),
            "repeat-y" => (false, true),
            _ => (true, true),
        };
        let Some(image) = crate::images::load_sized(&layer.src, tw, th, window, cx) else {
            return;
        };
        let origin = (f32::from(area.origin.x) + ox, f32::from(area.origin.y) + oy);
        let span = |start: f32, tile: f32, from: f32, to: f32, repeats: bool| -> (f32, usize) {
            if !repeats {
                return (start, 1);
            }
            let first = start - ((start - from) / tile).ceil() * tile;
            let count = ((to - first) / tile).ceil().max(1.0) as usize;
            (first, count)
        };
        let (cx0, cy0) = (f32::from(clip.origin.x), f32::from(clip.origin.y));
        let (cx1, cy1) = (cx0 + f32::from(clip.size.width), cy0 + f32::from(clip.size.height));
        let (mut x0, mut nx) = span(origin.0, tw, cx0, cx1, rx);
        let (mut y0, mut ny) = span(origin.1, th, cy0, cy1, ry);
        let opacity = layer.opacity.unwrap_or(1.0) as f32;

        // Many small tiles: repeat the tile in memory into one about 256 px a side, and paint
        // those. Only whole multiples, so the pattern's phase is the same.
        let (mut image, mut tw, mut th) = (image, tw, th);
        if nx * ny > TILE_SPRITES || opacity < 1.0 {
            let src = image.size(0);
            let kx = if rx { (256 / src.width.0.max(1) as u32).clamp(1, 32).min(nx as u32) } else { 1 };
            let ky = if ry { (256 / src.height.0.max(1) as u32).clamp(1, 32).min(ny as u32) } else { 1 };
            if let Some(bigger) = crate::images::tiled(&image, kx, ky, opacity, window) {
                image = bigger;
                tw *= kx as f32;
                th *= ky as f32;
                (x0, nx) = span(origin.0, tw, cx0, cx1, rx);
                (y0, ny) = span(origin.1, th, cy0, cy1, ry);
            }
        }

        let covers = |x: f32, y: f32| x <= cx0 && y <= cy0 && x + tw >= cx1 && y + th >= cy1;
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (x0 + i as f32 * tw, y0 + j as f32 * th);
                let tile = Bounds::new(gpui::point(gpui::px(x), gpui::px(y)), gpui::size(gpui::px(tw), gpui::px(th)));
                // One tile over the whole box takes the box's corners; tiles of a pattern are
                // clipped square.
                let (bounds, corners) = if nx * ny == 1 && covers(x, y) && self.rounded() {
                    (clip, self.corners())
                } else {
                    (tile, Corners::default())
                };
                draw(window, bounds, tile, corners, &image, self.nearest);
            }
        }
    }

    fn paint_frame(
        &self,
        frame: &BorderImageValue,
        border_box: Bounds<Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let Some(natural) = crate::images::load(&frame.src, window, cx) else {
            return;
        };
        let outset = frame.outset.unwrap_or(0.0) as f32;
        let bb = border_box.dilate(gpui::px(outset));
        let pixels = natural.image.size(0);
        // Slices are in the image's own pixels; an SVG's raster is GPUI's 2x of its size.
        let (iw, ih) = (pixels.width.0 as f32, pixels.height.0 as f32);
        let (sx, sy) = (iw / natural.width.max(1.0), ih / natural.height.max(1.0));
        let [st, sr, sb, sl] = frame.slice.sides().map(|v| v.max(0.0) as f32);
        let (st, sb, sl, sr) = (st * sy, sb * sy, sl * sx, sr * sx);
        let widths = match &frame.width {
            Some(width) => width.sides().map(|v| v.max(0.0) as f32),
            None if self.widths.iter().any(|w| *w > 0.0) => self.widths,
            None => [st / sy, sr / sx, sb / sy, sl / sx],
        };
        let [wt, wr, wb, wl] = widths;
        let (x0, y0) = (f32::from(bb.origin.x), f32::from(bb.origin.y));
        let (x3, y3) = (x0 + f32::from(bb.size.width), y0 + f32::from(bb.size.height));
        let (x1, x2) = (x0 + wl, (x3 - wr).max(x0 + wl));
        let (y1, y2) = (y0 + wt, (y3 - wb).max(y0 + wt));
        let (u1, u2) = (sl, (iw - sr).max(sl));
        let (v1, v2) = (st, (ih - sb).max(st));
        let mode = frame.repeat.as_deref().unwrap_or("stretch");
        let image = natural.image.clone();
        let nearest = self.nearest;
        // [source rect in image pixels] -> [destination], with how each axis repeats.
        let piece = |window: &mut gpui::Window, src: [f32; 4], dst: [f32; 4], repeat_x: bool, repeat_y: bool| {
            nine_piece(
                window,
                &image,
                (iw, ih),
                src,
                dst,
                if repeat_x { mode } else { "stretch" },
                if repeat_y { mode } else { "stretch" },
                nearest,
            );
        };
        // Corners.
        piece(window, [0.0, 0.0, u1, v1], [x0, y0, x1, y1], false, false);
        piece(window, [u2, 0.0, iw, v1], [x2, y0, x3, y1], false, false);
        piece(window, [0.0, v2, u1, ih], [x0, y2, x1, y3], false, false);
        piece(window, [u2, v2, iw, ih], [x2, y2, x3, y3], false, false);
        // Edges.
        piece(window, [u1, 0.0, u2, v1], [x1, y0, x2, y1], true, false);
        piece(window, [u1, v2, u2, ih], [x1, y2, x2, y3], true, false);
        piece(window, [0.0, v1, u1, v2], [x0, y1, x1, y2], false, true);
        piece(window, [u2, v1, iw, v2], [x2, y1, x3, y2], false, true);
        if frame.fill.unwrap_or(true) {
            piece(window, [u1, v1, u2, v2], [x1, y1, x2, y2], true, true);
        }
    }

    fn paint_sides(&self, sides: [gpui::Hsla; 4], bb: Bounds<Pixels>, window: &mut gpui::Window) {
        let [t, r, b, l] = self.widths;
        if self.rounded() {
            // One quad a side: GPUI's quad tapers each side into its rounded corners.
            let corners = self.corners();
            for (side, color) in sides.iter().enumerate() {
                let mut edges = gpui::Edges::<Pixels>::default();
                match side {
                    0 => edges.top = gpui::px(t),
                    1 => edges.right = gpui::px(r),
                    2 => edges.bottom = gpui::px(b),
                    _ => edges.left = gpui::px(l),
                }
                window.paint_quad(gpui::quad(
                    bb,
                    corners,
                    gpui::transparent_black(),
                    edges,
                    *color,
                    gpui::BorderStyle::Solid,
                ));
            }
            return;
        }
        // Square corners: four mitred trapezoids, as CSS draws them.
        let (x0, y0) = (f32::from(bb.origin.x), f32::from(bb.origin.y));
        let (x1, y1) = (x0 + f32::from(bb.size.width), y0 + f32::from(bb.size.height));
        let quads: [[[f32; 2]; 4]; 4] = [
            // top, right, bottom, left: outer edge, then inner.
            [[x0, y0], [x1, y0], [x1 - r, y0 + t], [x0 + l, y0 + t]],
            [[x1, y0], [x1, y1], [x1 - r, y1 - b], [x1 - r, y0 + t]],
            [[x1, y1], [x0, y1], [x0 + l, y1 - b], [x1 - r, y1 - b]],
            [[x0, y1], [x0, y0], [x0 + l, y0 + t], [x0 + l, y1 - b]],
        ];
        for (index, [a, b2, c, d]) in quads.into_iter().enumerate() {
            if self.widths[index] <= 0.0 || sides[index].a <= 0.0 {
                continue;
            }
            let mut path = gpui::PathBuilder::fill();
            path.move_to(gpui::point(gpui::px(a[0]), gpui::px(a[1])));
            path.line_to(gpui::point(gpui::px(b2[0]), gpui::px(b2[1])));
            path.line_to(gpui::point(gpui::px(c[0]), gpui::px(c[1])));
            path.line_to(gpui::point(gpui::px(d[0]), gpui::px(d[1])));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, sides[index]);
            }
        }
    }
}

fn native_background(gradient: &GradientValue) -> Option<gpui::Background> {
    let style =
        StyleDesc { background: Some(BackgroundValue::Gradient(Box::new(gradient.clone()))), ..Default::default() };
    style.resolved_background()
}

/// Paints `image` so its full extent is `image_bounds`, showing only `bounds`.
pub(crate) fn draw(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    image_bounds: Bounds<Pixels>,
    corners: Corners<Pixels>,
    image: &Arc<gpui::RenderImage>,
    nearest: bool,
) {
    let result = if nearest {
        window.paint_image_nearest(bounds, image_bounds, corners, image.clone(), 0, false)
    } else {
        window.paint_image(bounds, image_bounds, corners, image.clone(), 0, false)
    };
    if let Err(error) = result {
        log::debug!("gpuix: an image did not paint: {error}");
    }
}

/// One of a nine-slice's pieces: `src` (`[x0, y0, x1, y1]` in the image's pixels) drawn into
/// `dst` (window pixels), stretched or repeated along each axis.
#[allow(clippy::too_many_arguments)]
fn nine_piece(
    window: &mut gpui::Window,
    image: &Arc<gpui::RenderImage>,
    (iw, ih): (f32, f32),
    src: [f32; 4],
    dst: [f32; 4],
    mode_x: &str,
    mode_y: &str,
    nearest: bool,
) {
    let (sw, sh) = (src[2] - src[0], src[3] - src[1]);
    let (dw, dh) = (dst[2] - dst[0], dst[3] - dst[1]);
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return;
    }
    // A repeating axis keeps the piece's aspect at the other axis's scale.
    let tile = |mode: &str, length: f32, natural: f32| -> f32 {
        match mode {
            "repeat" => natural,
            "round" => length / (length / natural).round().max(1.0),
            _ => length,
        }
    };
    let tw = tile(mode_x, dw, sw * (dh / sh));
    let th = tile(mode_y, dh, sh * (dw / sw));
    let (tw, th) = (if mode_x == "stretch" { dw } else { tw }, if mode_y == "stretch" { dh } else { th });
    if tw <= 0.0 || th <= 0.0 {
        return;
    }
    let area = Bounds::new(gpui::point(gpui::px(dst[0]), gpui::px(dst[1])), gpui::size(gpui::px(dw), gpui::px(dh)));
    let nx = (dw / tw).ceil().max(1.0) as usize;
    let ny = (dh / th).ceil().max(1.0) as usize;
    window.with_content_mask(Some(gpui::ContentMask { bounds: area }), |window| {
        for j in 0..ny.min(512) {
            for i in 0..nx.min(512) {
                let (x, y) = (dst[0] + i as f32 * tw, dst[1] + j as f32 * th);
                let (sx, sy) = (tw / sw, th / sh);
                let image_bounds = Bounds::new(
                    gpui::point(gpui::px(x - src[0] * sx), gpui::px(y - src[1] * sy)),
                    gpui::size(gpui::px(iw * sx), gpui::px(ih * sy)),
                );
                let bounds = Bounds::new(gpui::point(gpui::px(x), gpui::px(y)), gpui::size(gpui::px(tw), gpui::px(th)));
                draw(window, bounds, image_bounds, Corners::default(), image, nearest);
            }
        }
    });
}

fn length(value: &LengthValue, basis: f32) -> Option<f32> {
    match value {
        LengthValue::Px(px) => Some(*px as f32),
        LengthValue::Str(s) => {
            let s = s.trim();
            if let Some(pct) = s.strip_suffix('%') {
                return pct.trim().parse::<f32>().ok().map(|p| basis * p / 100.0);
            }
            s.strip_suffix("px").unwrap_or(s).trim().parse::<f32>().ok()
        }
    }
}

/// A tile's size for `backgroundSize` in an `aw` by `ah` area, from an image `iw` by `ih`.
pub(crate) fn tile_size(size: Option<&BackgroundSizeValue>, aw: f32, ah: f32, iw: f32, ih: f32) -> (f32, f32) {
    let ratio = iw / ih;
    match size {
        None => (iw, ih),
        Some(BackgroundSizeValue::Width(w)) => (*w as f32, *w as f32 / ratio),
        Some(BackgroundSizeValue::Keyword(k)) => match k.as_str() {
            "cover" => {
                let s = (aw / iw).max(ah / ih);
                (iw * s, ih * s)
            }
            "contain" => {
                let s = (aw / iw).min(ah / ih);
                (iw * s, ih * s)
            }
            "100%" | "fill" | "stretch" => (aw, ah),
            _ => {
                // "auto", or one length in a string ("50%", "32px").
                match length(&LengthValue::Str(k.clone()), aw) {
                    Some(w) if k != "auto" => (w, w / ratio),
                    _ => (iw, ih),
                }
            }
        },
        Some(BackgroundSizeValue::Pair([w, h])) => {
            let w = length(w, aw);
            let h = length(h, ah);
            match (w, h) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (w, w / ratio),
                (None, Some(h)) => (h * ratio, h),
                (None, None) => (iw, ih),
            }
        }
    }
}

/// Where a tile sits in its area, like CSS `background-position`: a percentage puts the same
/// point of the tile on that point of the area.
pub(crate) fn tile_offset(
    position: Option<&BackgroundPositionValue>,
    aw: f32,
    ah: f32,
    tw: f32,
    th: f32,
) -> (f32, f32) {
    let free = (aw - tw, ah - th);
    let along = |value: &LengthValue, free: f32| -> f32 {
        match value {
            LengthValue::Px(px) => *px as f32,
            LengthValue::Str(s) => match s.trim() {
                "left" | "top" => 0.0,
                "center" => free / 2.0,
                "right" | "bottom" => free,
                other => match other.strip_suffix('%') {
                    Some(pct) => pct.trim().parse::<f32>().map(|p| free * p / 100.0).unwrap_or(0.0),
                    None => other.strip_suffix("px").unwrap_or(other).trim().parse::<f32>().unwrap_or(0.0),
                },
            },
        }
    };
    match position {
        None => (0.0, 0.0),
        Some(BackgroundPositionValue::Pair([x, y])) => (along(x, free.0), along(y, free.1)),
        Some(BackgroundPositionValue::Keyword(keyword)) => {
            let (mut x, mut y) = (None, None);
            let mut centers = 0;
            for word in keyword.split_whitespace() {
                match word {
                    "left" => x = Some(0.0),
                    "right" => x = Some(free.0),
                    "top" => y = Some(0.0),
                    "bottom" => y = Some(free.1),
                    "center" => centers += 1,
                    other => {
                        let value = LengthValue::Str(other.to_string());
                        if x.is_none() {
                            x = Some(along(&value, free.0));
                        } else {
                            y = Some(along(&value, free.1));
                        }
                    }
                }
            }
            let _ = centers;
            (x.unwrap_or(free.0 / 2.0), y.unwrap_or(free.1 / 2.0))
        }
    }
}

// ── Gradients ────────────────────────────────────────────────────────

/// Stops with every position filled in (CSS's rules: the ends at 0 and 1, the rest spread evenly
/// between known ones, and never before the one ahead), colours premultiplied.
pub(crate) fn resolve_stops(stops: &[LinearGradientStopValue]) -> Option<Vec<(f32, [f32; 4])>> {
    if stops.is_empty() {
        return None;
    }
    let mut positions: Vec<Option<f32>> = stops.iter().map(|s| s.position.map(|p| p as f32)).collect();
    let last = positions.len() - 1;
    if positions[0].is_none() {
        positions[0] = Some(0.0);
    }
    if positions[last].is_none() {
        positions[last] = Some(if last == 0 { 0.0 } else { 1.0 });
    }
    let mut index = 0;
    while index < positions.len() {
        if positions[index].is_some() {
            index += 1;
            continue;
        }
        let start = index - 1;
        let mut end = index;
        while positions[end].is_none() {
            end += 1;
        }
        let (a, b) = (positions[start].unwrap(), positions[end].unwrap());
        for (k, slot) in positions.iter_mut().enumerate().take(end).skip(index) {
            *slot = Some(a + (b - a) * (k - start) as f32 / (end - start) as f32);
        }
        index = end;
    }
    let mut out = Vec::with_capacity(stops.len());
    let mut floor = f32::MIN;
    for (stop, position) in stops.iter().zip(positions) {
        let rgba = crate::color::parse_color_rgba(&stop.color)?;
        let p = position.unwrap().max(floor);
        floor = p;
        out.push((p, [rgba.r * rgba.a, rgba.g * rgba.a, rgba.b * rgba.a, rgba.a]));
    }
    Some(out)
}

fn sample(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let first = stops[0];
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if t <= b {
            if b - a <= f32::EPSILON {
                return cb;
            }
            let k = (t - a) / (b - a);
            return [0, 1, 2, 3].map(|i| ca[i] + (cb[i] - ca[i]) * k);
        }
    }
    stops[stops.len() - 1].1
}

/// Where in the gradient (0 at the first stop's end) the point `(x, y)` of a `w` by `h` box is.
fn gradient_t(gradient: &GradientValue, x: f32, y: f32, w: f32, h: f32) -> f32 {
    match gradient {
        GradientValue::LinearGradient { angle, .. } => {
            let a = (*angle as f32).to_radians();
            let (dx, dy) = (a.sin(), -a.cos());
            let length = (w * dx).abs() + (h * dy).abs();
            if length <= f32::EPSILON {
                return 0.0;
            }
            ((x - w / 2.0) * dx + (y - h / 2.0) * dy) / length + 0.5
        }
        GradientValue::RadialGradient { shape, center, radius, .. } => {
            let [fx, fy] = center.unwrap_or([0.5, 0.5]).map(|v| v as f32);
            let (cx, cy) = (fx * w, fy * h);
            let (far_x, far_y) = (cx.max(w - cx), cy.max(h - cy));
            let circle = shape.as_deref() == Some("circle");
            let (rx, ry) = match radius {
                Some(RadiusValue::One(r)) => (*r as f32, *r as f32),
                Some(RadiusValue::Pair([x, y])) => (*x as f32, *y as f32),
                None if circle => {
                    let r = (far_x * far_x + far_y * far_y).sqrt();
                    (r, r)
                }
                // The farthest corner, keeping the box's proportions.
                None => (far_x * std::f32::consts::SQRT_2, far_y * std::f32::consts::SQRT_2),
            };
            let (dx, dy) = ((x - cx) / rx.max(f32::EPSILON), (y - cy) / ry.max(f32::EPSILON));
            (dx * dx + dy * dy).sqrt()
        }
        GradientValue::ConicGradient { angle, center, .. } => {
            let [fx, fy] = center.unwrap_or([0.5, 0.5]).map(|v| v as f32);
            let (dx, dy) = (x - fx * w, y - fy * h);
            let degrees = dx.atan2(-dy).to_degrees() - *angle as f32;
            (degrees / 360.0).rem_euclid(1.0)
        }
    }
}

fn stops_of(gradient: &GradientValue) -> &[LinearGradientStopValue] {
    match gradient {
        GradientValue::LinearGradient { stops, .. }
        | GradientValue::RadialGradient { stops, .. }
        | GradientValue::ConicGradient { stops, .. } => stops,
    }
}

/// `gradient` over a `w` by `h` box, rasterised `rw` by `rh`, straight-alpha BGRA.
pub(crate) fn rasterize(gradient: &GradientValue, w: f32, h: f32, rw: u32, rh: u32) -> Option<Vec<u8>> {
    let stops = resolve_stops(stops_of(gradient))?;
    let mut out = Vec::with_capacity((rw * rh * 4) as usize);
    for j in 0..rh {
        let y = (j as f32 + 0.5) / rh as f32 * h;
        for i in 0..rw {
            let x = (i as f32 + 0.5) / rw as f32 * w;
            let [r, g, b, a] = sample(&stops, gradient_t(gradient, x, y, w, h));
            let un = |c: f32| if a > 0.0 { (c / a).clamp(0.0, 1.0) } else { 0.0 };
            let byte = |c: f32| (c * 255.0).round() as u8;
            out.extend_from_slice(&[byte(un(b)), byte(un(g)), byte(un(r)), byte(a.clamp(0.0, 1.0))]);
        }
    }
    Some(out)
}

fn paint_gradient(
    gradient: &GradientValue,
    bounds: Bounds<Pixels>,
    corners: Corners<Pixels>,
    window: &mut gpui::Window,
) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let scale = window.scale_factor();
    // Each axis at device resolution up to the cap, in steps of 8 so a live resize reuses rasters.
    let side = |logical: f32| -> u32 {
        let device = (logical * scale).clamp(1.0, GRADIENT_RASTER);
        ((device / 8.0).ceil() * 8.0).min(GRADIENT_RASTER) as u32
    };
    let (rw, rh) = (side(w), side(h));
    // A conic or radial gradient's shape depends on the box's proportions, a linear one's angle
    // too: the box's size is part of the spec.
    let spec = crate::images::hash_of(&(serde_json::to_string(gradient).unwrap_or_default(), w.to_bits(), h.to_bits()));
    let Some(image) = crate::images::gradient(spec, rw, rh, window, || rasterize(gradient, w, h, rw, rh)) else {
        return;
    };
    draw(window, bounds, bounds, corners, &image, false);
}

/// The decoration painter for `style`, if it has anything to paint. A `hover` that changes a
/// decoration repaints it while the mouse is over the box.
pub(crate) fn element(style: &StyleDesc) -> Option<gpui::AnyElement> {
    use gpui::prelude::*;

    let base = Decor::of(style);
    let hover =
        style.hover.as_deref().filter(|hover| hover.has_decor() || (base.is_some() && touches_decor(hover))).and_then(
            |hover| {
                let mut merged = style.clone();
                merge_decor(&mut merged, hover);
                Decor::of(&merged)
            },
        );
    if base.is_none() && hover.is_none() {
        return None;
    }
    let painter = gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let decor = match (&hover, &base) {
                (Some(hover), _) if hover.border_box(bounds).contains(&window.mouse_position()) => Some(hover),
                (_, base) => base.as_ref(),
            };
            if let Some(decor) = decor {
                decor.paint(bounds, window, cx);
            }
        },
    )
    .absolute()
    .top(gpui::px(0.0))
    .left(gpui::px(0.0))
    .right(gpui::px(0.0))
    .bottom(gpui::px(0.0));
    Some(painter.into_any_element())
}

/// Whether a state style changes something the painter reads.
fn touches_decor(state: &StyleDesc) -> bool {
    state.has_decor()
        || state.border_color.is_some()
        || state.border_width.is_some()
        || state.background_size.is_some()
        || state.background_position.is_some()
        || state.background_repeat.is_some()
}

fn merge_decor(into: &mut StyleDesc, state: &StyleDesc) {
    macro_rules! over {
        ($($field:ident),*) => { $( if state.$field.is_some() { into.$field = state.$field.clone(); } )* };
    }
    over!(
        background,
        background_image,
        background_size,
        background_repeat,
        background_position,
        border_image,
        border_color,
        border_top_color,
        border_right_color,
        border_bottom_color,
        border_left_color,
        border_width,
        border_top_width,
        border_right_width,
        border_bottom_width,
        border_left_width,
        image_rendering
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(color: &str, position: Option<f64>) -> LinearGradientStopValue {
        LinearGradientStopValue { color: color.into(), position }
    }

    #[test]
    fn missing_stop_positions_spread_evenly() {
        let stops =
            resolve_stops(&[stop("#000", None), stop("#000", None), stop("#000", Some(0.8)), stop("#000", None)])
                .unwrap();
        let positions: Vec<f32> = stops.iter().map(|s| s.0).collect();
        assert_eq!(positions[0], 0.0);
        assert!((positions[1] - 0.4).abs() < 1e-6);
        assert_eq!(positions[2], 0.8);
        assert_eq!(positions[3], 1.0);
    }

    #[test]
    fn a_stop_before_the_last_is_pushed_forward() {
        let stops = resolve_stops(&[stop("#000", Some(0.6)), stop("#fff", Some(0.2))]).unwrap();
        assert_eq!(stops[1].0, 0.6);
    }

    #[test]
    fn a_three_stop_linear_gradient_passes_through_its_middle() {
        let gradient = GradientValue::LinearGradient {
            angle: 90.0,
            stops: vec![stop("#ff0000", None), stop("#00ff00", None), stop("#0000ff", None)],
            color_space: None,
        };
        let bytes = rasterize(&gradient, 100.0, 10.0, 3, 1).unwrap();
        // BGRA: left red, middle green, right blue (pixel centres at 1/6, 1/2, 5/6).
        assert!(bytes[2] > 100 && bytes[1] > 50, "the left is red going to green");
        assert_eq!(&bytes[4..8], &[0, 255, 0, 255]);
        assert!(bytes[8] > 100, "the right is mostly blue");
    }

    #[test]
    fn a_radial_gradient_is_its_first_stop_at_the_centre() {
        let gradient = GradientValue::RadialGradient {
            shape: Some("circle".into()),
            center: None,
            radius: Some(RadiusValue::One(10.0)),
            stops: vec![stop("#ffffff", None), stop("#000000", None)],
        };
        assert!(gradient_t(&gradient, 10.0, 10.0, 20.0, 20.0) < 1e-6);
        assert!((gradient_t(&gradient, 20.0, 10.0, 20.0, 20.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_conic_gradient_sweeps_clockwise_from_the_top() {
        let gradient = GradientValue::ConicGradient {
            angle: 0.0,
            center: None,
            stops: vec![stop("#000", None), stop("#fff", None)],
        };
        // Right of the centre is a quarter turn, below it half.
        assert!((gradient_t(&gradient, 20.0, 10.0, 20.0, 20.0) - 0.25).abs() < 1e-4);
        assert!((gradient_t(&gradient, 10.0, 20.0, 20.0, 20.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_gradient_fades_without_a_dark_fringe() {
        // Premultiplied interpolation: red to transparent stays red, only alpha falls.
        let gradient = GradientValue::LinearGradient {
            angle: 90.0,
            stops: vec![stop("#ff0000", None), stop("transparent", None)],
            color_space: None,
        };
        let bytes = rasterize(&gradient, 2.0, 1.0, 2, 1).unwrap();
        assert_eq!(bytes[2], 255, "red at full strength");
        assert!(bytes[3] > 0 && bytes[3] < 255);
    }

    #[test]
    fn sizes_cover_contain_and_pairs() {
        assert_eq!(tile_size(None, 100.0, 50.0, 10.0, 20.0), (10.0, 20.0));
        let cover = BackgroundSizeValue::Keyword("cover".into());
        assert_eq!(tile_size(Some(&cover), 100.0, 50.0, 10.0, 10.0), (100.0, 100.0));
        let contain = BackgroundSizeValue::Keyword("contain".into());
        assert_eq!(tile_size(Some(&contain), 100.0, 50.0, 10.0, 10.0), (50.0, 50.0));
        let pair = BackgroundSizeValue::Pair([LengthValue::Str("50%".into()), LengthValue::Str("auto".into())]);
        assert_eq!(tile_size(Some(&pair), 100.0, 50.0, 10.0, 20.0), (50.0, 100.0));
        assert_eq!(tile_size(Some(&BackgroundSizeValue::Width(32.0)), 100.0, 50.0, 16.0, 8.0), (32.0, 16.0));
    }

    #[test]
    fn positions_put_the_same_point_of_the_tile_on_the_area() {
        let center = BackgroundPositionValue::Keyword("center".into());
        assert_eq!(tile_offset(Some(&center), 100.0, 100.0, 20.0, 40.0), (40.0, 30.0));
        let corner = BackgroundPositionValue::Keyword("bottom right".into());
        assert_eq!(tile_offset(Some(&corner), 100.0, 100.0, 20.0, 40.0), (80.0, 60.0));
        let pair = BackgroundPositionValue::Pair([LengthValue::Px(5.0), LengthValue::Str("100%".into())]);
        assert_eq!(tile_offset(Some(&pair), 100.0, 100.0, 20.0, 40.0), (5.0, 60.0));
    }

    #[test]
    fn only_painted_styles_get_a_painter() {
        let plain: StyleDesc =
            serde_json::from_str(r##"{"backgroundColor":"#fff","borderColor":"#000","borderWidth":1}"##).unwrap();
        assert!(Decor::of(&plain).is_none());
        let bevel: StyleDesc =
            serde_json::from_str(r##"{"borderWidth":2,"borderTopColor":"#fff","borderBottomColor":"#000"}"##).unwrap();
        let decor = Decor::of(&bevel).unwrap();
        assert_eq!(decor.widths, [2.0; 4]);
        assert!(decor.sides.is_some());
        let layered: StyleDesc = serde_json::from_str(
            r##"{"background":[{"type":"radial-gradient","stops":[{"color":"#fff"},{"color":"#0000"}]},"#123"]}"##,
        )
        .unwrap();
        let decor = Decor::of(&layered).unwrap();
        assert!(matches!(decor.layers[0], BackgroundLayerValue::Color(_)), "the last layer paints first");
    }

    #[test]
    fn styles_take_every_new_shape() {
        let style: StyleDesc = serde_json::from_str(
            r##"{
                "backgroundImage": [{"src":"/a.png","size":"cover","repeat":"no-repeat","position":"center"},"/b.png"],
                "backgroundSize": [16, "auto"],
                "backgroundPosition": ["50%", 4],
                "borderImage": {"src":"/frame.png","slice":[8,8,8,8],"repeat":"round"},
                "imageRendering": "pixelated",
                "zIndex": 3,
                "fontStyle": "italic",
                "background": {"type":"conic-gradient","angle":45,"stops":[{"color":"red"},{"color":"blue","position":0.5},{"color":"red"}]}
            }"##,
        )
        .unwrap();
        assert_eq!(style.background_image.unwrap().layers().len(), 2);
        assert_eq!(style.border_image.unwrap().slice.sides(), [8.0; 4]);
        assert_eq!(style.z_index, Some(3.0));
    }
}
