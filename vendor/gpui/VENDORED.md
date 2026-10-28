# gpui, patched for pumi

gpuix's zed (`remorses/zed` at `f77e5cfe9dad04288566f00c66ecf3c58d6f84b3`, the commit the root
`Cargo.toml` names): upstream zed through 2026-08-26 plus gpuix's changes (the embedded AppKit
loop, the display-link wake hook, list extents, focus and accessibility fixes). On top of it, one
change of pumi's: a transform for sprites. It is used through
`[patch."https://github.com/remorses/zed.git"]` in the root `Cargo.toml`, so every crate that
draws builds this copy; the other zed crates (`gpui_platform`, `gpui_macos`, …) come from the same
commit by git.

**Why.** A style can `rotate` a node, and pumi-gpuix draws such a node's quads itself. Its
*glyphs* and *SVG icons* could not be turned: gpui 2D-transforms whole elements through
`Element::with_transformation`, but a monochrome sprite (text, an SVG icon) carries one
transform of its own — a glyph `scale_factor` — and nothing composed with it. A rotated label
(win98's vertical bars, the office's tilted signs) came out upright.

**What changed** (every change is marked `pumi:`):

- `src/window.rs`: `Window` gains `sprite_transform`, and two methods —
  `with_sprite_transform(transform, f)` runs `f` with it in effect, composed with any
  already in effect, and `sprite_transform()` reads it. The glyph and SVG paths apply it to
  each sprite's own transform when they build it.
- `src/window.rs`: `Window::invalidate_cached_view(view)` marks a cached view dirty for the
  frame being drawn. pumi-gpuix's `cache` subtrees know what changed under them only when its
  root view renders, which is too late for `notify`.
- `src/elements/div.rs`: `capture_pointer` captures once the press moves (4px) or leaves the
  element, not on the press, so a click on a child of a drag handle (a title bar's button)
  still finds the child hovered on release.
- `src/window.rs`: `reuse_prepaint` rebinds a pointer capture to its element's hitbox in a
  reused cached view, which `insert_hitbox` never sees; without it a drag from a `cache` pane
  lost the pointer at the next frame.
- `src/scene.rs`: `Scene::insert_primitive` culls a transformed sprite by the box around
  where it is drawn (`transformed_bounds`) rather than where it was laid out.
- `src/view.rs`, `src/window.rs`, `src/text_system/line_layout.rs`: a cached view that renders
  again no longer makes the cached views inside it render (upstream sets `refreshing` for its
  render). A nested view that is clean at the same bounds is reused, and when its parent is
  reused as one block instead, the ranges the nested views recorded move with it
  (`PrepaintStateIndex::rebased`, `PaintIndex::rebased`, `LineLayoutIndex::rebased`,
  `Window::update_reused_states`). Each `ViewElementState` says which frame its ranges index
  (`Window::frame_serial`); one from another frame renders rather than being reused. Without it a
  spinner in pumi's side bar laid every plugin's section out again, every frame.
- `src/view.rs`: `Entity::cached_measured(style, measure)` — a cached view whose open sides
  (`auto`) are a measured leaf — and `Entity::recorded()` — the view drawn uncached, laid out
  with everything else, but recorded as a cached render is, so the cached view at the same
  bounds reuses it next frame. pumi-gpuix's compartments (a plugin's view, sized by its
  contents) are built from the two.
- `src/window.rs`: `Window::defer_cached_view(view)` keeps a notified cached view as it was
  drawn for this frame: pumi-gpuix draws a slow compartment on a budget.
- `Cargo.toml`: the workspace's inherited fields and dependencies are written out (the zed
  crates by git at the same commit), `[lints.rust] unexpected_cfgs` allows `cfg(rust_analyzer)`,
  and the examples are dropped with `examples/`, `tests/` and `docs/`.

**gpui_macos** (`vendor/gpui_macos`) is vendored the same way, for one change:

- `src/text_system.rs`: a glyph rasterised with no dilation (a terminal with Settings' Thicken
  text off) is drawn with `set_should_smooth_fonts(false)`. CoreGraphics smooths a bitmap
  context's text unless told not to, so "off" still thickened every stem (about 16% more ink at
  Menlo 12).

**Re-vendoring.** Check out the crate at the new rev, copy `build.rs`, `README.md`, `resources/`
and `src/` over this directory, write its `Cargo.toml` with the workspace's fields resolved, and
apply the `pumi:` source changes again (`git diff` against the pristine crate shows them). The
rev moves in the root `Cargo.toml`'s `[workspace.dependencies]` and `[patch]` at the same time.

**gpui_apple** (`vendor/gpui_apple`, GPUI's Metal renderer) is vendored the same way, with an
empty `[workspace]` of its own in place of an entry in the root's `exclude`, for one change:

- `src/metal_renderer.rs`, `src/shaders.metal`: a see-through window's backgrounds are glass.
  The app sets the window's opacity on its layer (`pumiGlass`, pumi-gpuix's `set_window_glass`)
  and the renderer reads it each frame as the blend colour. Quads, shadows and paths draw
  premultiplied and scaled by it, and every primitive's alpha is drawn over the one under it
  (upstream adds them), so a background lets as much of the desktop through however many are
  stacked, in its own colour, while text, icons, images and video stay opaque. Upstream, a
  translucent part on another made the window opaque there and darker, so a theme had to draw
  each part in a single translucent layer.
