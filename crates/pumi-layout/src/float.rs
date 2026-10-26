use crate::Rect;
use pumi_api::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
pub const CELL_PX: (f64, f64) = (8.4, 18.);
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    Cells(i64),
    Pixels(f64),
    Share(f64),
}
impl Default for Dim {
    fn default() -> Self {
        Dim::Cells(0)
    }
}
impl Dim {
    pub fn parse(value: &Value) -> Result<Dim> {
        let bad =
            || Error::invalid_params(format!("a float's length is cells (20), \"360px\" or \"60%\": got {value}"));
        let number = |text: &str| text.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        match value {
            Value::Number(n) => match (n.as_i64(), n.as_f64()) {
                (Some(cells), _) => Ok(Dim::Cells(cells)),
                (_, Some(share)) if share > 0. && share <= 1. => Ok(Dim::Share(share)),
                (_, Some(cells)) if cells.is_finite() => Ok(Dim::Cells(cells.round() as i64)),
                _ => Err(bad()),
            },
            Value::String(text) => {
                if let Some(percent) = text.strip_suffix('%') {
                    let percent = number(percent).filter(|p| (0. ..=100.).contains(p)).ok_or_else(bad)?;
                    Ok(Dim::Share(percent / 100.))
                } else if let Some(pixels) = text.strip_suffix("px") {
                    number(pixels).map(Dim::Pixels).ok_or_else(bad)
                } else {
                    number(text).map(|cells| Dim::Cells(cells.round() as i64)).ok_or_else(bad)
                }
            }
            _ => Err(bad()),
        }
    }
    pub fn to_value(self) -> Value {
        let short = |n: f64| {
            let text = format!("{:.3}", n);
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        match self {
            Dim::Cells(cells) => json!(cells),
            Dim::Pixels(pixels) => json!(format!("{}px", short(pixels))),
            Dim::Share(share) => json!(format!("{}%", short(share * 100.))),
        }
    }
    pub fn cells(self, total: u32, cell: f64) -> i64 {
        match self {
            Dim::Cells(cells) => cells,
            Dim::Pixels(pixels) => (pixels / cell.max(1.)).round() as i64,
            Dim::Share(share) => (total as f64 * share).round() as i64,
        }
    }
    pub fn pixels(self, total: f32, cell: f32) -> f32 {
        match self {
            Dim::Cells(cells) => cells as f32 * cell,
            Dim::Pixels(pixels) => pixels as f32,
            Dim::Share(share) => total * share as f32,
        }
    }
}
impl Serialize for Dim {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.to_value().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Dim {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        Dim::parse(&value).map_err(|err| serde::de::Error::custom(err.message))
    }
}
impl schemars::JsonSchema for Dim {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Dim".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Cells (`20`), pixels (`\"360px\"`) or a share of what it floats over (`\"60%\"`).",
            "type": ["integer", "number", "string"]
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub x: Align,
    pub y: Align,
}
impl Anchor {
    pub const NAMES: [&str; 9] = ["NW", "N", "NE", "W", "center", "E", "SW", "S", "SE"];
    pub fn parse(name: &str) -> Option<Anchor> {
        let name = name.trim();
        if name.eq_ignore_ascii_case("center") || name.eq_ignore_ascii_case("c") {
            return Some(Anchor { x: Align::Center, y: Align::Center });
        }
        let name = name.to_ascii_uppercase();
        let y = match name.chars().next()? {
            'N' => Align::Start,
            'S' => Align::End,
            _ => Align::Center,
        };
        let x = match name.chars().last()? {
            'W' => Align::Start,
            'E' => Align::End,
            _ => Align::Center,
        };
        let known = matches!(name.as_str(), "NW" | "N" | "NE" | "W" | "E" | "SW" | "S" | "SE");
        known.then_some(Anchor { x, y })
    }
    pub fn place(align: Align, total: f64, size: f64, offset: f64) -> f64 {
        let at = match align {
            Align::Start => offset,
            Align::Center => (total - size) / 2. + offset,
            Align::End => total - size - offset,
        };
        at.clamp(0., (total - size).max(0.))
    }
}
pub fn default_anchor(row: bool, col: bool) -> &'static str {
    match (row, col) {
        (true, true) => "NW",
        (true, false) => "N",
        (false, true) => "W",
        (false, false) => "center",
    }
}
pub fn rect(area: Rect, anchor: &str, row: Dim, col: Dim, width: Dim, height: Dim) -> Rect {
    let anchor = Anchor::parse(anchor).unwrap_or(Anchor { x: Align::Start, y: Align::Start });
    let w = width.cells(area.width, CELL_PX.0).clamp(1, area.width.max(1) as i64) as f64;
    let h = height.cells(area.height, CELL_PX.1).clamp(1, area.height.max(1) as i64) as f64;
    let x = Anchor::place(anchor.x, area.width as f64, w, col.cells(area.width, CELL_PX.0) as f64);
    let y = Anchor::place(anchor.y, area.height as f64, h, row.cells(area.height, CELL_PX.1) as f64);
    Rect { x: area.x + x.round() as u32, y: area.y + y.round() as u32, width: w as u32, height: h as u32 }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lengths_are_cells_pixels_or_shares() {
        assert_eq!(Dim::parse(&json!(20)).unwrap(), Dim::Cells(20));
        assert_eq!(Dim::parse(&json!(-2)).unwrap(), Dim::Cells(-2));
        assert_eq!(Dim::parse(&json!("20")).unwrap(), Dim::Cells(20));
        assert_eq!(Dim::parse(&json!("360px")).unwrap(), Dim::Pixels(360.));
        assert_eq!(Dim::parse(&json!("60%")).unwrap(), Dim::Share(0.6));
        assert_eq!(Dim::parse(&json!(0.5)).unwrap(), Dim::Share(0.5));
        assert!(Dim::parse(&json!("120%")).is_err());
        assert!(Dim::parse(&json!("wide")).is_err());
        assert!(Dim::parse(&json!(null)).is_err());
    }
    #[test]
    fn a_length_is_written_back_as_given() {
        for value in [json!(20), json!("360px"), json!("60%"), json!("12.5%")] {
            assert_eq!(Dim::parse(&value).unwrap().to_value(), value);
        }
        let dim: Dim = serde_json::from_value(json!("40%")).unwrap();
        assert_eq!(serde_json::to_value(dim).unwrap(), json!("40%"));
    }
    #[test]
    fn anchors_are_corners_edges_and_the_middle() {
        for name in Anchor::NAMES {
            assert!(Anchor::parse(name).is_some(), "{name}");
        }
        assert_eq!(Anchor::parse("ne"), Some(Anchor { x: Align::End, y: Align::Start }));
        assert_eq!(Anchor::parse("S"), Some(Anchor { x: Align::Center, y: Align::End }));
        assert_eq!(Anchor::parse("NNE"), None);
        assert_eq!(Anchor::parse("middle"), None);
    }
    #[test]
    fn a_float_is_pinned_in_its_area() {
        let area = Rect { x: 0, y: 0, width: 100, height: 40 };
        let centred = rect(area, "center", Dim::Cells(0), Dim::Cells(0), Dim::Share(0.5), Dim::Cells(10));
        assert_eq!(centred, Rect { x: 25, y: 15, width: 50, height: 10 });
        let corner = rect(area, "SE", Dim::Cells(1), Dim::Cells(2), Dim::Cells(20), Dim::Cells(5));
        assert_eq!(corner, Rect { x: 78, y: 34, width: 20, height: 5 });
        let top = rect(area, "N", Dim::Cells(1), Dim::Cells(0), Dim::Pixels(168.), Dim::Cells(5));
        assert_eq!(top, Rect { x: 40, y: 1, width: 20, height: 5 });
        let huge = rect(area, "NW", Dim::Cells(90), Dim::Cells(90), Dim::Cells(500), Dim::Cells(500));
        assert_eq!(huge, Rect { x: 0, y: 0, width: 100, height: 40 }, "kept inside");
        assert_eq!(default_anchor(true, false), "N");
    }
}
