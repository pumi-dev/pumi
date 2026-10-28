//! Native motion tracks resolved during GPUI rendering, outside React.
//!
//! A track animates any of `MotionStyle`'s properties (sizes, offsets, opacity, radius, a
//! translation, colours) from what is shown to a target, or through keyframes; as a tween
//! (duration, delay, easing) or a spring; once, a number of times, or forever (looping,
//! reversing). One that repeats forever is sampled at a capped rate (`fps`, 30 by default), so
//! an ambient loop does not rebuild the window every display frame, and less often still when the
//! frames it moves in are dear (`renderer::LOOP_SHARE`).

use std::time::Duration;

use serde_json::Value;
use web_time::Instant;

use crate::style::{DimensionValue, StyleDesc};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Number,
    /// Premultiplied RGBA.
    Color,
}

/// The properties a track can move, by their JS names.
const PROPS: [(&str, Kind); 14] = [
    ("width", Kind::Number),
    ("height", Kind::Number),
    ("opacity", Kind::Number),
    ("top", Kind::Number),
    ("right", Kind::Number),
    ("bottom", Kind::Number),
    ("left", Kind::Number),
    ("borderRadius", Kind::Number),
    ("x", Kind::Number),
    ("y", Kind::Number),
    ("rotate", Kind::Number),
    ("backgroundColor", Kind::Color),
    ("color", Kind::Color),
    ("borderColor", Kind::Color),
];

fn prop_index(name: &str) -> Option<usize> {
    PROPS.iter().position(|(prop, _)| *prop == name)
}

type Sample = [f64; 4];

/// What a track shows at one moment: a value for each property it has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct MotionStyle {
    values: [Option<Sample>; PROPS.len()],
}

impl MotionStyle {
    /// A numeric property's value, by its JS name.
    #[cfg(test)]
    pub(crate) fn get(&self, name: &str) -> Option<f64> {
        self.values[prop_index(name)?].map(|v| v[0])
    }

    /// A colour property's value as `#rrggbbaa`.
    #[cfg(test)]
    pub(crate) fn color(&self, name: &str) -> Option<String> {
        self.values[prop_index(name)?].map(color_string)
    }

    pub(crate) fn apply_to(self, style: &mut StyleDesc) {
        let get = |name: &str| prop_index(name).and_then(|i| self.values[i]);
        let num = |name: &str| get(name).map(|v| v[0]);
        if let Some(value) = num("width") {
            style.width = Some(DimensionValue::Pixels(value.max(0.0)));
        }
        if let Some(value) = num("height") {
            style.height = Some(DimensionValue::Pixels(value.max(0.0)));
        }
        if let Some(value) = num("opacity") {
            style.opacity = Some(value.clamp(0.0, 1.0));
        }
        if let Some(value) = num("top") {
            style.top = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = num("right") {
            style.right = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = num("bottom") {
            style.bottom = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = num("left") {
            style.left = Some(DimensionValue::Pixels(value));
        }
        if let Some(value) = num("borderRadius") {
            style.border_radius = Some(value.max(0.0));
        }
        // A translation moves the box from where layout put it: an in-flow box becomes
        // `relative` (it keeps its place in the flow), a placed one moves from its offset.
        let (x, y) = (num("x").unwrap_or(0.0), num("y").unwrap_or(0.0));
        if x != 0.0 || y != 0.0 {
            if style.position.is_none() {
                style.position = Some("relative".into());
            }
            if x != 0.0 {
                match (style.left.clone(), style.right.clone()) {
                    (None, Some(right)) => style.right = Some(moved_by(right, -x)),
                    (left, _) => style.left = Some(moved_by(left.unwrap_or(DimensionValue::Pixels(0.0)), x)),
                }
            }
            if y != 0.0 {
                match (style.top.clone(), style.bottom.clone()) {
                    (None, Some(bottom)) => style.bottom = Some(moved_by(bottom, -y)),
                    (top, _) => style.top = Some(moved_by(top.unwrap_or(DimensionValue::Pixels(0.0)), y)),
                }
            }
        }
        if let Some(value) = num("rotate") {
            style.rotate = Some(value);
        }
        if let Some(value) = get("backgroundColor") {
            style.background_color = Some(color_string(value));
        }
        if let Some(value) = get("color") {
            style.color = Some(color_string(value));
        }
        if let Some(value) = get("borderColor") {
            style.border_color = Some(color_string(value));
        }
    }
}

/// An offset a translation moves: pixels move; a share has no pixel size to move by (the parent's
/// is not known here), so it stays where it is.
fn moved_by(value: DimensionValue, delta: f64) -> DimensionValue {
    match value {
        DimensionValue::Pixels(v) => DimensionValue::Pixels(v + delta),
        other => other,
    }
}

fn color_string([r, g, b, a]: Sample) -> String {
    let a = a.clamp(0.0, 1.0);
    let un = |c: f64| if a > 0.0 { (c / a).clamp(0.0, 1.0) } else { 0.0 };
    let byte = |c: f64| (c * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(un(r)), byte(un(g)), byte(un(b)), byte(a))
}

/// One value given for a property: a number, or a colour string.
fn parse_value(kind: Kind, name: &str, value: &Value) -> Result<Option<Sample>, String> {
    match (kind, value) {
        (_, Value::Null) => Ok(None),
        (Kind::Number, Value::Number(number)) => {
            let v = number.as_f64().unwrap_or(f64::NAN);
            if !v.is_finite() || v.abs() > f32::MAX as f64 {
                return Err(format!("motion {name} must fit a finite 32-bit float"));
            }
            if matches!(name, "width" | "height" | "borderRadius") && v < 0.0 {
                return Err("motion sizes and borderRadius must be non-negative".to_string());
            }
            if name == "opacity" && !(0.0..=1.0).contains(&v) {
                return Err("motion opacity must be between 0 and 1".to_string());
            }
            Ok(Some([v, 0.0, 0.0, 0.0]))
        }
        (Kind::Color, Value::String(text)) => {
            let rgba =
                crate::color::parse_color_rgba(text).ok_or_else(|| format!("motion {name}: not a colour: {text}"))?;
            let a = rgba.a as f64;
            Ok(Some([rgba.r as f64 * a, rgba.g as f64 * a, rgba.b as f64 * a, a]))
        }
        _ => Err(format!("motion {name} has the wrong kind of value")),
    }
}

/// Keyframes for each property of a target: `[value]` for a plain value, the list for an array
/// (a `null` keyframe is the value shown when it starts).
type Frames = Vec<(usize, Vec<Option<Sample>>)>;

fn parse_frames(target: &Value) -> Result<Frames, String> {
    let Some(object) = target.as_object() else {
        return Err("a motion target is an object".to_string());
    };
    let mut frames = Vec::new();
    for (name, value) in object {
        let Some(index) = prop_index(name) else {
            continue;
        };
        let kind = PROPS[index].1;
        let list = match value {
            Value::Array(items) => {
                if items.is_empty() {
                    return Err(format!("motion {name} has no keyframes"));
                }
                items.iter().map(|item| parse_value(kind, name, item)).collect::<Result<Vec<_>, _>>()?
            }
            value => vec![parse_value(kind, name, value)?],
        };
        frames.push((index, list));
    }
    Ok(frames)
}

/// A plain target (`initial`): each property's last keyframe.
fn parse_style(target: &Value) -> Result<MotionStyle, String> {
    let mut style = MotionStyle::default();
    for (index, frames) in parse_frames(target)? {
        style.values[index] = frames.into_iter().rev().flatten().next();
    }
    Ok(style)
}

#[derive(Clone, Debug, PartialEq)]
enum Ease {
    Bezier([f64; 4]),
    Steps(u32, bool),
}

fn parse_ease(value: Option<&Value>) -> Result<Ease, String> {
    let named = |name: &str| -> Result<Ease, String> {
        Ok(Ease::Bezier(match name {
            "linear" => [0.0, 0.0, 1.0, 1.0],
            "easeIn" => [0.42, 0.0, 1.0, 1.0],
            "easeOut" => [0.0, 0.0, 0.58, 1.0],
            "easeInOut" => [0.42, 0.0, 0.58, 1.0],
            "ease" => [0.25, 0.1, 0.25, 1.0],
            "circIn" => [0.55, 0.0, 1.0, 0.45],
            "circOut" => [0.0, 0.55, 0.45, 1.0],
            "circInOut" => [0.85, 0.0, 0.15, 1.0],
            "backIn" => [0.31, 0.01, 0.66, -0.59],
            "backOut" => [0.33, 1.53, 0.69, 0.99],
            "backInOut" => [0.68, -0.6, 0.32, 1.6],
            "stepStart" | "step-start" => return Ok(Ease::Steps(1, true)),
            "stepEnd" | "step-end" => return Ok(Ease::Steps(1, false)),
            other => return Err(format!("unknown motion easing: {other}")),
        }))
    };
    match value {
        None | Some(Value::Null) => named("easeOut"),
        Some(Value::String(name)) => named(name),
        Some(Value::Array(curve)) if curve.len() == 4 => {
            let curve: Vec<f64> = curve.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect();
            let [x1, y1, x2, y2] = [curve[0], curve[1], curve[2], curve[3]];
            if ![x1, y1, x2, y2].iter().all(|value| value.is_finite())
                || !(0.0..=1.0).contains(&x1)
                || !(0.0..=1.0).contains(&x2)
            {
                return Err("motion cubic bezier values must be finite and x values must be 0..1".to_string());
            }
            Ok(Ease::Bezier([x1, y1, x2, y2]))
        }
        Some(Value::Object(steps)) => {
            let count = steps.get("steps").and_then(Value::as_u64).filter(|n| (1..=10_000).contains(n));
            let count = count.ok_or("motion steps must be a count from 1")?;
            Ok(Ease::Steps(count as u32, steps.get("position").and_then(Value::as_str) == Some("start")))
        }
        Some(_) => Err("a motion easing is a name, a cubic bezier or { steps }".to_string()),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Spring {
    stiffness: f64,
    damping: f64,
    mass: f64,
}

impl Spring {
    /// How far (0 to 1, past 1 when it overshoots) a spring released from rest has gone.
    fn progress(&self, t: f64) -> f64 {
        let w0 = (self.stiffness / self.mass).sqrt();
        let zeta = self.damping / (2.0 * (self.stiffness * self.mass).sqrt());
        if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            1.0 - (-zeta * w0 * t).exp() * ((wd * t).cos() + (zeta * w0 / wd) * (wd * t).sin())
        } else if (zeta - 1.0).abs() < 1e-9 {
            1.0 - (-w0 * t).exp() * (1.0 + w0 * t)
        } else {
            let s = w0 * (zeta * zeta - 1.0).sqrt();
            let (r1, r2) = (-zeta * w0 + s, -zeta * w0 - s);
            1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
        }
    }

    /// When it is within a thousandth of rest, capped at ten seconds.
    fn settle_time(&self) -> f64 {
        let mut t = 0.0;
        let step = 1.0 / 120.0;
        let mut still = 0.0;
        while t < 10.0 {
            t += step;
            if (1.0 - self.progress(t)).abs() < 1e-3 {
                still += step;
                if still >= 0.1 {
                    return t - 0.1;
                }
            } else {
                still = 0.0;
            }
        }
        10.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RepeatType {
    Loop,
    Reverse,
    Mirror,
}

#[derive(Clone, Debug, PartialEq)]
struct Transition {
    duration: f64,
    delay: f64,
    ease: Ease,
    /// Plays after the first; `None` forever.
    repeat: Option<u64>,
    repeat_type: RepeatType,
    repeat_delay: f64,
    /// Where each keyframe falls, 0 to 1. Even by default.
    times: Option<Vec<f64>>,
    /// Samples a second, when capped.
    fps: Option<f64>,
    spring: Option<Spring>,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            duration: 0.3,
            delay: 0.0,
            ease: Ease::Bezier([0.0, 0.0, 0.58, 1.0]),
            repeat: Some(0),
            repeat_type: RepeatType::Loop,
            repeat_delay: 0.0,
            times: None,
            fps: None,
            spring: None,
        }
    }
}

fn seconds_of(value: Option<&Value>, name: &str, default: f64) -> Result<f64, String> {
    let value = match value {
        None | Some(Value::Null) => return Ok(default),
        Some(value) => value.as_f64().unwrap_or(f64::NAN),
    };
    if !value.is_finite() || value < 0.0 || Duration::try_from_secs_f64(value).is_err() {
        return Err(format!("motion {name} must be a supported finite non-negative number"));
    }
    Ok(value)
}

fn parse_transition(value: Option<&Value>) -> Result<Transition, String> {
    let empty = serde_json::Map::new();
    let t = match value {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(t)) => t,
        Some(_) => return Err("a motion transition is an object".to_string()),
    };
    let spring = if t.get("type").and_then(Value::as_str) == Some("spring") {
        let positive = |key: &str, default: f64| -> Result<f64, String> {
            let v = t.get(key).and_then(Value::as_f64).unwrap_or(default);
            if !v.is_finite() || v <= 0.0 {
                return Err(format!("motion spring {key} must be positive"));
            }
            Ok(v)
        };
        Some(Spring {
            stiffness: positive("stiffness", 170.0)?,
            damping: positive("damping", 26.0)?,
            mass: positive("mass", 1.0)?,
        })
    } else {
        None
    };
    let repeat = match t.get("repeat") {
        None | Some(Value::Null) => Some(0),
        Some(Value::String(s)) if s == "infinite" || s == "Infinity" => None,
        Some(value) => match value.as_f64() {
            Some(n) if n.is_infinite() => None,
            Some(n) if n >= 0.0 && n.is_finite() => Some(n.floor() as u64),
            _ => return Err("motion repeat is a count or \"infinite\"".to_string()),
        },
    };
    let repeat_type = match t.get("repeatType").and_then(Value::as_str) {
        None | Some("loop") => RepeatType::Loop,
        Some("reverse") => RepeatType::Reverse,
        Some("mirror") => RepeatType::Mirror,
        Some(other) => return Err(format!("unknown motion repeatType: {other}")),
    };
    let times = match t.get("times") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => {
            let times: Vec<f64> = items.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect();
            if times.iter().any(|v| !(0.0..=1.0).contains(v)) || times.windows(2).any(|w| w[1] < w[0]) {
                return Err("motion times run from 0 to 1, in order".to_string());
            }
            Some(times)
        }
        Some(_) => return Err("motion times is a list".to_string()),
    };
    let fps = match t.get("fps").and_then(Value::as_f64) {
        Some(fps) if fps.is_finite() && fps > 0.0 => Some(fps.min(240.0)),
        Some(_) => return Err("motion fps must be positive".to_string()),
        None if repeat.is_none() => Some(30.0),
        None => None,
    };
    let mut duration = seconds_of(t.get("duration"), "duration", 0.3)?;
    if let Some(spring) = &spring {
        duration = spring.settle_time();
    }
    Ok(Transition {
        duration,
        delay: seconds_of(t.get("delay"), "delay", 0.0)?,
        ease: parse_ease(t.get("ease"))?,
        repeat,
        repeat_type,
        repeat_delay: seconds_of(t.get("repeatDelay"), "repeatDelay", 0.0)?,
        times,
        fps,
        spring,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct Description {
    generation: u64,
    is_exit: bool,
    /// `None` when `initial` is false or absent: the track starts at its target.
    initial: Option<MotionStyle>,
    animate: Frames,
    transition: Transition,
}

fn parse_description(source: &Value) -> Result<Description, String> {
    let initial = match source.get("initial") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(Value::Bool(true)) => return Err("motion initial only accepts false or a style object".to_string()),
        Some(style) => Some(parse_style(style)?),
    };
    let animate = parse_frames(source.get("animate").ok_or("motion needs animate")?)?;
    Ok(Description {
        generation: source_generation(source),
        is_exit: source_is_exit(source),
        initial,
        animate,
        transition: parse_transition(source.get("transition"))?,
    })
}

/// The last keyframe of each property: where the track ends up.
fn final_style(frames: &Frames) -> MotionStyle {
    let mut style = MotionStyle::default();
    for (index, list) in frames {
        style.values[*index] = list.iter().rev().flatten().next().copied();
    }
    style
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MotionFrame {
    pub active: bool,
    pub just_settled: bool,
    pub generation: u64,
    /// For a capped track: how long until its next sample. An active track without one wants
    /// every display frame.
    pub wake: Option<Duration>,
    /// For a capped track: the time between its samples.
    pub step: Option<Duration>,
}

pub(crate) struct MotionState {
    source: Value,
    /// What was shown when the track (re)started: properties the target leaves out stay at it.
    from: MotionStyle,
    /// Each moving property's keyframes, resolved.
    tracks: Vec<(usize, Vec<Sample>)>,
    transition: Transition,
    started: Instant,
    valid: bool,
    needs_settle: bool,
    generation: u64,
    /// Where a capped track's samples are counted from: every track of a window counts from
    /// the same moment (`set_grid`), so tracks at one rate move on the same display frames and
    /// the window is drawn once for all of them, not once for each. `None` counts from `started`.
    grid: Option<Instant>,
    /// The sample `moved_since_asked` last found a capped track at.
    last_step: Option<u64>,
}

impl MotionState {
    pub(crate) fn new(source: &Value, now: Instant) -> Result<Self, String> {
        let description = parse_description(source)?;
        let from = description.initial.unwrap_or_else(|| final_style(&description.animate));
        Ok(Self::start(source, from, description, now, false))
    }

    fn start(source: &Value, from: MotionStyle, description: Description, now: Instant, retarget: bool) -> Self {
        let tracks: Vec<(usize, Vec<Sample>)> = description
            .animate
            .iter()
            .filter_map(|(index, list)| {
                let current = from.values[*index];
                let fallback = current.or_else(|| list.iter().flatten().next().copied())?;
                let mut frames: Vec<Sample> = list.iter().map(|v| v.unwrap_or(fallback)).collect();
                if list.len() == 1 {
                    frames.insert(0, current.unwrap_or(frames[0]));
                }
                Some((*index, frames))
            })
            .collect();
        let moving = tracks.iter().any(|(_, frames)| frames.windows(2).any(|w| w[0] != w[1]));
        let _ = retarget;
        Self {
            source: source.clone(),
            from,
            tracks,
            transition: description.transition,
            started: now,
            valid: true,
            needs_settle: description.is_exit || moving,
            generation: description.generation,
            grid: None,
            last_step: None,
        }
    }

    /// Whether an active track shows something other than it did when this was last asked: a
    /// capped track holds each sample for `1 / fps`, so most display frames find it where it
    /// was, and what it is drawn in need not be built again for it. Anything else that is active
    /// moves every frame. Asked once a frame, by the window's root build.
    pub(crate) fn moved_since_asked(&mut self, frame: &MotionFrame, now: Instant) -> bool {
        match self.transition.fps.filter(|_| frame.active) {
            Some(fps) => {
                let sample = self.grid_step(now, fps);
                self.last_step.replace(sample) != Some(sample)
            }
            None => {
                self.last_step = None;
                frame.active
            }
        }
    }

    /// Count a capped track's samples from `grid` (the first track of its window started then),
    /// so every track at one rate moves on the same display frames. A track that started later
    /// is drawn up to one sample (`1 / fps`) behind its own start.
    pub(crate) fn set_grid(&mut self, grid: Instant) {
        self.grid = Some(grid);
    }

    pub(crate) fn invalid(source: &Value, now: Instant) -> Self {
        Self {
            source: source.clone(),
            from: MotionStyle::default(),
            tracks: Vec::new(),
            transition: Transition::default(),
            started: now,
            valid: false,
            needs_settle: source_is_exit(source),
            generation: source_generation(source),
            grid: None,
            last_step: None,
        }
    }

    pub(crate) fn sync(&mut self, source: &Value, now: Instant) -> Result<(), String> {
        if self.source == *source {
            return Ok(());
        }

        let previous_generation = self.generation;
        let description = match parse_description(source) {
            Ok(description) => description,
            Err(error) => {
                self.source = source.clone();
                self.valid = false;
                self.generation = source_generation(source);
                self.needs_settle = source_is_exit(source) || self.generation != previous_generation;
                return Err(error);
            }
        };
        let from = if self.valid {
            self.visible_style(now).unwrap_or_default()
        } else {
            description.initial.unwrap_or_else(|| final_style(&description.animate))
        };
        let changed_generation = description.generation != previous_generation;
        let grid = self.grid;
        *self = Self::start(source, from, description, now, true);
        self.grid = grid;
        self.needs_settle |= changed_generation;
        Ok(())
    }

    pub(crate) fn visible_style(&self, now: Instant) -> Option<MotionStyle> {
        self.valid.then(|| self.sample(now).0)
    }

    /// The elapsed time a capped track is drawn at: whole steps of `1 / fps`, counted from its
    /// grid (`set_grid`), less how long after the grid it started.
    fn elapsed(&self, now: Instant) -> Duration {
        match self.transition.fps {
            Some(fps) => {
                let origin = self.origin();
                let held = self.grid_step(now, fps) as f64 / fps;
                let offset = self.started.saturating_duration_since(origin).as_secs_f64();
                Duration::from_secs_f64((held - offset).max(0.0))
            }
            None => now.saturating_duration_since(self.started),
        }
    }

    /// Where a capped track's samples are counted from.
    fn origin(&self) -> Instant {
        match self.grid {
            Some(grid) if grid <= self.started => grid,
            _ => self.started,
        }
    }

    /// The whole samples of `1 / fps` from the origin to `now`.
    fn grid_step(&self, now: Instant, fps: f64) -> u64 {
        (now.saturating_duration_since(self.origin()).as_secs_f64() * fps).floor() as u64
    }

    /// The style at `now`, whether the track still moves, and how far it has played.
    fn sample(&self, now: Instant) -> (MotionStyle, bool) {
        let t = &self.transition;
        let elapsed = self.elapsed(now).as_secs_f64();
        let moving = self.tracks.iter().any(|(_, frames)| frames.windows(2).any(|w| w[0] != w[1]));

        let after_delay = elapsed - t.delay;
        let (progress, finished) = if after_delay < 0.0 {
            (0.0, false)
        } else if t.duration <= 0.0 {
            (1.0, true)
        } else {
            let cycle = t.duration + t.repeat_delay;
            let index = (after_delay / cycle).floor();
            let cycles = t.repeat.map(|r| r as f64 + 1.0);
            match cycles {
                Some(cycles) if index >= cycles => {
                    // Done: where the last cycle ended.
                    let last = cycles - 1.0;
                    (if self.reversed(last) { 0.0 } else { 1.0 }, true)
                }
                _ => {
                    let within = ((after_delay - index * cycle) / t.duration).min(1.0);
                    (if self.reversed(index) { 1.0 - within } else { within }, false)
                }
            }
        };

        let mut style = self.from;
        for (index, frames) in &self.tracks {
            style.values[*index] = Some(self.interpolate(frames, progress));
        }
        (style, moving && !finished)
    }

    fn reversed(&self, cycle: f64) -> bool {
        self.transition.repeat_type != RepeatType::Loop && (cycle as u64) % 2 == 1
    }

    fn interpolate(&self, frames: &[Sample], progress: f64) -> Sample {
        let t = &self.transition;
        if let Some(spring) = &t.spring {
            // A spring runs first to last, by its own curve (which may overshoot).
            let (a, b) = (frames[0], frames[frames.len() - 1]);
            let k = if progress >= 1.0 { 1.0 } else { spring.progress(progress * t.duration) };
            return [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * k);
        }
        let segments = frames.len() - 1;
        if segments == 0 {
            return frames[0];
        }
        let default_times: Vec<f64>;
        let times = match &t.times {
            Some(times) if times.len() == frames.len() => times.as_slice(),
            _ => {
                default_times = (0..=segments).map(|i| i as f64 / segments as f64).collect();
                &default_times
            }
        };
        let mut segment = segments - 1;
        for i in 0..segments {
            if progress <= times[i + 1] {
                segment = i;
                break;
            }
        }
        let (t0, t1) = (times[segment], times[segment + 1]);
        let local = if t1 > t0 { ((progress - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
        let k = ease(local, &t.ease);
        let (a, b) = (frames[segment], frames[segment + 1]);
        [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * k)
    }

    pub(crate) fn frame(&mut self, now: Instant) -> MotionFrame {
        let (_, active) = if self.valid { self.sample(now) } else { (MotionStyle::default(), false) };
        if active {
            self.needs_settle = true;
        }
        let just_settled = self.needs_settle && !active;
        if just_settled {
            self.needs_settle = false;
        }
        let wake = match (active, self.transition.fps) {
            (true, Some(fps)) => {
                let elapsed = now.saturating_duration_since(self.origin()).as_secs_f64();
                let next = ((elapsed * fps).floor() + 1.0) / fps;
                Some(Duration::from_secs_f64((next - elapsed).max(0.001)))
            }
            _ => None,
        };
        let step = self.transition.fps.filter(|_| active).map(|fps| Duration::from_secs_f64(1.0 / fps));
        MotionFrame { active, just_settled, generation: self.generation, wake, step }
    }
}

fn source_generation(source: &Value) -> u64 {
    source.get("generation").and_then(Value::as_u64).unwrap_or_default()
}

fn source_is_exit(source: &Value) -> bool {
    source.get("isExit").and_then(Value::as_bool).unwrap_or_default()
}

fn ease(progress: f64, ease: &Ease) -> f64 {
    match ease {
        Ease::Bezier([0.0, 0.0, 1.0, 1.0]) => progress,
        Ease::Bezier(curve) => cubic_bezier(progress, *curve),
        Ease::Steps(count, start) => {
            let n = *count as f64;
            let step = if *start { (progress * n).ceil() } else { (progress * n).floor() };
            (step / n).clamp(0.0, 1.0)
        }
    }
}

fn cubic_bezier(x: f64, [x1, y1, x2, y2]: [f64; 4]) -> f64 {
    fn sample(t: f64, a: f64, b: f64) -> f64 {
        let c = 3.0 * a;
        let b = 3.0 * (b - a) - c;
        let a = 1.0 - c - b;
        ((a * t + b) * t + c) * t
    }

    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..20 {
        let middle = (low + high) / 2.0;
        if sample(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    // Not clamped: a curve whose y leaves 0..1 (`backOut`) overshoots, as CSS's does.
    sample((low + high) / 2.0, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An inset in pixels, as motion writes one.
    fn px(value: f64) -> DimensionValue {
        DimensionValue::Pixels(value)
    }

    #[test]
    fn interpolates_and_retargets_from_the_visible_value() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let middle = state.frame(started + Duration::from_millis(500));
        assert_eq!(state.visible_style(started + Duration::from_millis(500)).unwrap().get("width"), Some(50.0));
        assert!(middle.active);
        assert!(!middle.just_settled);
        assert!(middle.wake.is_none(), "a one-off tween takes every display frame");

        let reversed = serde_json::json!({
            "initial": false,
            "animate": { "width": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let reversed_at = started + Duration::from_millis(500);
        state.sync(&reversed, reversed_at).unwrap();
        assert_eq!(state.visible_style(reversed_at).unwrap().get("width"), Some(50.0));
        assert_eq!(state.visible_style(reversed_at + Duration::from_millis(500)).unwrap().get("width"), Some(25.0));
    }

    #[test]
    fn disabled_initial_state_starts_at_the_target() {
        let now = Instant::now();
        let description = serde_json::json!({
            "initial": false,
            "animate": { "width": 260.0 },
            "transition": { "duration": 0.2 }
        });
        let mut state = MotionState::new(&description, now).unwrap();
        let frame = state.frame(now);

        assert_eq!(state.visible_style(now).unwrap().get("width"), Some(260.0));
        assert!(!frame.active);
        assert!(!frame.just_settled);
    }

    #[test]
    fn rejects_unsafe_numbers_and_invalid_initial_booleans() {
        let now = Instant::now();
        for description in [
            serde_json::json!({ "animate": { "width": 1e300 }, "transition": {} }),
            serde_json::json!({ "animate": { "opacity": 2.0 }, "transition": {} }),
            serde_json::json!({ "animate": {}, "transition": { "duration": 1e300 } }),
            serde_json::json!({ "initial": true, "animate": {}, "transition": {} }),
            serde_json::json!({ "animate": { "backgroundColor": "no" } }),
            serde_json::json!({ "animate": { "x": [] } }),
            serde_json::json!({ "animate": {}, "transition": { "ease": "wobble" } }),
            serde_json::json!({ "animate": {}, "transition": { "times": [0.5, 0.2] } }),
        ] {
            assert!(MotionState::new(&description, now).is_err(), "{description}");
        }
    }

    #[test]
    fn finishes_at_the_exact_target() {
        let started = Instant::now();
        let description = serde_json::json!({
            "initial": { "width": 0.0 },
            "animate": { "width": 100.0 },
            "transition": { "duration": 0.2, "ease": "linear" }
        });
        let mut state = MotionState::new(&description, started).unwrap();
        let frame = state.frame(started + Duration::from_millis(200));

        assert_eq!(state.visible_style(started + Duration::from_millis(200)).unwrap().get("width"), Some(100.0));
        assert!(!frame.active);
        assert!(frame.just_settled);
        assert!(!state.frame(started + Duration::from_millis(201)).just_settled);
    }

    #[test]
    fn retarget_with_zero_duration_settles_on_the_next_frame() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "initial": false,
            "animate": { "opacity": 1.0 },
            "transition": { "duration": 0.2, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();
        assert!(!state.frame(started).just_settled);

        let exit = serde_json::json!({
            "initial": false,
            "animate": { "opacity": 0.0 },
            "transition": { "duration": 0.0, "ease": "linear" }
        });
        state.sync(&exit, started).unwrap();
        let frame = state.frame(started);
        assert_eq!(state.visible_style(started).unwrap().get("opacity"), Some(0.0));
        assert!(!frame.active);
        assert!(frame.just_settled);
    }

    #[test]
    fn a_new_generation_settles_when_the_target_already_matches() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "generation": 1,
            "initial": false,
            "animate": { "opacity": 1.0 }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let exit = serde_json::json!({
            "generation": 2,
            "initial": false,
            "animate": { "opacity": 1.0 }
        });
        state.sync(&exit, started).unwrap();

        assert!(state.frame(started).just_settled);
        assert!(!state.frame(started).just_settled);
    }

    #[test]
    fn retarget_keeps_values_omitted_from_the_new_target() {
        let started = Instant::now();
        let initial = serde_json::json!({
            "generation": 1,
            "initial": false,
            "animate": { "width": 100.0, "opacity": 1.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let mut state = MotionState::new(&initial, started).unwrap();

        let exit = serde_json::json!({
            "generation": 2,
            "initial": false,
            "animate": { "opacity": 0.0 },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        state.sync(&exit, started).unwrap();

        state.frame(started + Duration::from_millis(500));
        let style = state.visible_style(started + Duration::from_millis(500)).unwrap();
        assert_eq!(style.get("width"), Some(100.0));
        assert_eq!(style.get("opacity"), Some(0.5));
    }

    #[test]
    fn keyframes_pass_through_each_value_at_its_time() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "opacity": [1.0, 0.2, 1.0] },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let state = MotionState::new(&description, now).unwrap();
        let at = |ms: u64| state.visible_style(now + Duration::from_millis(ms)).unwrap().get("opacity").unwrap();
        assert!((at(0) - 1.0).abs() < 1e-9);
        assert!((at(500) - 0.2).abs() < 1e-9);
        assert!((at(250) - 0.6).abs() < 1e-9);
        assert!((at(1000) - 1.0).abs() < 1e-9);

        let timed = serde_json::json!({
            "animate": { "x": [0, 10, 20] },
            "transition": { "duration": 1.0, "ease": "linear", "times": [0, 0.8, 1] }
        });
        let state = MotionState::new(&timed, now).unwrap();
        assert!((state.visible_style(now + Duration::from_millis(400)).unwrap().get("x").unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_capped_loop_moves_only_when_its_sample_does() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "rotate": [0, 360] },
            "transition": { "duration": 1.0, "ease": "linear", "repeat": "infinite", "fps": 10 }
        });
        let mut state = MotionState::new(&description, now).unwrap();
        let mut moved = |ms: u64| {
            let at = now + Duration::from_millis(ms);
            let frame = state.frame(at);
            state.moved_since_asked(&frame, at)
        };
        // Display frames at 120 Hz: the 10 fps sample holds for 100 ms, so it moves once in it.
        assert!(moved(0), "its first frame is new");
        assert!(!moved(8));
        assert!(!moved(50));
        assert!(!moved(99));
        assert!(moved(100), "the next sample");
        assert!(!moved(108));
        // Frames that skip samples still see the move.
        assert!(moved(450));
    }

    #[test]
    fn capped_loops_on_one_grid_move_on_the_same_frames() {
        let grid = Instant::now();
        let description = serde_json::json!({
            "animate": { "rotate": [0, 360] },
            "transition": { "duration": 1.0, "ease": "linear", "repeat": "infinite", "fps": 10 }
        });
        let mut first = MotionState::new(&description, grid).unwrap();
        first.set_grid(grid);
        // Started 37 ms off the grid: it is drawn from its start pose until the grid's next
        // sample, then on the grid, at most one sample (100 ms) behind its own start.
        let started = grid + Duration::from_millis(37);
        let mut second = MotionState::new(&description, started).unwrap();
        second.set_grid(grid);
        let wake = |state: &mut MotionState, ms: u64| state.frame(grid + Duration::from_millis(ms)).wake.unwrap();
        assert_eq!(wake(&mut first, 37), wake(&mut second, 37), "both are sampled next at 100 ms");
        let rotate = |state: &MotionState, ms: u64| {
            state.visible_style(grid + Duration::from_millis(ms)).unwrap().get("rotate").unwrap()
        };
        assert!(rotate(&second, 37).abs() < 1e-9);
        assert!(rotate(&second, 99).abs() < 1e-9);
        // At 100 ms the grid has moved 100 ms, of which 63 are the second's own.
        assert!((rotate(&second, 100) - 360.0 * 0.063).abs() < 1e-6, "{}", rotate(&second, 100));
        assert!((rotate(&first, 100) - 36.0).abs() < 1e-6);
    }

    #[test]
    fn a_turn_passes_through_every_angle_and_comes_back_round() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "rotate": [0, 360] },
            "transition": { "duration": 1.0, "ease": "linear", "repeat": "infinite", "fps": 1000 }
        });
        let state = MotionState::new(&description, now).unwrap();
        let at = |ms| state.visible_style(now + Duration::from_millis(ms)).unwrap().get("rotate").unwrap();
        assert!((at(250) - 90.0).abs() < 1.0, "{}", at(250));
        assert!((at(1250) - 90.0).abs() < 1.0, "{}", at(1250));
        let mut style = StyleDesc::default();
        state.visible_style(now + Duration::from_millis(500)).unwrap().apply_to(&mut style);
        assert!((style.rotate.unwrap() - 180.0).abs() < 1.0);
    }

    #[test]
    fn a_forever_loop_never_settles_and_samples_at_its_cap() {
        let now = Instant::now();
        let description = serde_json::json!({
            "animate": { "opacity": [1.0, 0.4] },
            "transition": { "duration": 1.0, "ease": "linear", "repeat": "infinite", "repeatType": "reverse" }
        });
        let mut state = MotionState::new(&description, now).unwrap();
        // Default cap: 30 a second, so 1010 ms is drawn as 1000 ms: the end of the first cycle.
        let frame = state.frame(now + Duration::from_millis(1010));
        assert!(frame.active);
        assert!(!frame.just_settled);
        let wake = frame.wake.expect("a capped loop asks for a timed repaint");
        assert!(wake <= Duration::from_millis(34));
        // Reversing: half way through the second cycle is half way back.
        let opacity = state.visible_style(now + Duration::from_millis(1500)).unwrap().get("opacity").unwrap();
        assert!((opacity - 0.7).abs() < 0.03, "{opacity}");
        assert!(state.frame(now + Duration::from_secs(1000)).active);
    }

    #[test]
    fn a_counted_repeat_ends_where_its_last_cycle_does() {
        let now = Instant::now();
        let description = serde_json::json!({
            "initial": { "x": 0 },
            "animate": { "x": 10 },
            "transition": { "duration": 0.1, "ease": "linear", "repeat": 1, "repeatType": "reverse" }
        });
        let mut state = MotionState::new(&description, now).unwrap();
        assert!(state.frame(now + Duration::from_millis(150)).active);
        let done = state.frame(now + Duration::from_millis(250));
        assert!(!done.active);
        assert!(done.just_settled);
        assert_eq!(state.visible_style(now + Duration::from_millis(250)).unwrap().get("x"), Some(0.0));
    }

    #[test]
    fn colours_interpolate_premultiplied() {
        let now = Instant::now();
        let description = serde_json::json!({
            "initial": { "backgroundColor": "#ff000000" },
            "animate": { "backgroundColor": "#ff0000" },
            "transition": { "duration": 1.0, "ease": "linear" }
        });
        let state = MotionState::new(&description, now).unwrap();
        // Half way from transparent red to red is red at half alpha, not a dark red.
        assert_eq!(
            state.visible_style(now + Duration::from_millis(500)).unwrap().color("backgroundColor").unwrap(),
            "#ff000080"
        );
    }

    #[test]
    fn a_spring_overshoots_and_comes_to_rest() {
        let now = Instant::now();
        let description = serde_json::json!({
            "initial": { "x": 0 },
            "animate": { "x": 100 },
            "transition": { "type": "spring", "stiffness": 300, "damping": 10 }
        });
        let mut state = MotionState::new(&description, now).unwrap();
        let peak = (1..100)
            .map(|ms| state.visible_style(now + Duration::from_millis(ms * 10)).unwrap().get("x").unwrap())
            .fold(f64::MIN, f64::max);
        assert!(peak > 100.0, "an underdamped spring passes its target: {peak}");
        assert!(!state.frame(now + Duration::from_secs(11)).active);
        assert_eq!(state.visible_style(now + Duration::from_secs(11)).unwrap().get("x"), Some(100.0));
    }

    #[test]
    fn a_translation_moves_an_in_flow_box_relatively() {
        let now = Instant::now();
        let description = serde_json::json!({ "initial": false, "animate": { "x": 4, "y": -2 } });
        let state = MotionState::new(&description, now).unwrap();
        let mut style = StyleDesc::default();
        state.visible_style(now).unwrap().apply_to(&mut style);
        assert_eq!(style.position.as_deref(), Some("relative"));
        assert_eq!((style.left, style.top), (Some(px(4.0)), Some(px(-2.0))));

        let mut placed = StyleDesc {
            position: Some("absolute".into()),
            right: Some(px(10.0)),
            top: Some(px(5.0)),
            ..Default::default()
        };
        state.visible_style(now).unwrap().apply_to(&mut placed);
        assert_eq!((placed.right, placed.top, placed.left), (Some(px(6.0)), Some(px(3.0)), None));
    }

    #[test]
    fn a_translation_leaves_a_share_where_it_is() {
        let now = Instant::now();
        let description = serde_json::json!({ "initial": false, "animate": { "x": 4, "y": -2 } });
        let state = MotionState::new(&description, now).unwrap();
        // A share has no pixel size to move by (the parent's is not known here): it stays put.
        let mut style = StyleDesc {
            position: Some("absolute".into()),
            left: Some(DimensionValue::Percentage(0.25)),
            ..Default::default()
        };
        state.visible_style(now).unwrap().apply_to(&mut style);
        assert_eq!(style.left, Some(DimensionValue::Percentage(0.25)));
        assert_eq!(style.top, Some(px(-2.0)));
    }

    #[test]
    fn steps_hold_each_value() {
        assert_eq!(ease(0.49, &Ease::Steps(2, false)), 0.0);
        assert_eq!(ease(0.51, &Ease::Steps(2, false)), 0.5);
        assert_eq!(ease(0.01, &Ease::Steps(2, true)), 0.5);
    }
}
