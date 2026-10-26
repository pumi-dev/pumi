use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, FontStyle, FontWeight, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Size, Style, TextAlign, TextRun, TruncateFrom, Window, px,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
const ELLIPSIS: &str = "…";
pub struct Text {
    text: SharedString,
    spans: Vec<Span>,
    measured: Rc<RefCell<Option<Measured>>>,
}
#[derive(Debug, Clone, Default)]
pub struct Span {
    pub range: Range<usize>,
    pub color: Option<Hsla>,
    pub weight: Option<FontWeight>,
    pub italic: Option<bool>,
}
struct Measured {
    width: Option<Pixels>,
    line: ShapedLine,
    size: Size<Pixels>,
    line_height: Pixels,
}
impl Text {
    pub fn new(text: impl Into<SharedString>) -> Self {
        let text: SharedString = text.into();
        let text = match text.as_ref().split_once('\n') {
            Some((first, _)) => SharedString::from(first.to_string()),
            None => text,
        };
        Self { text, spans: Vec::new(), measured: Rc::new(RefCell::new(None)) }
    }
    pub fn span(mut self, span: Span) -> Self {
        self.spans.push(span);
        self
    }
}
impl IntoElement for Text {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}
impl Element for Text {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height.to_pixels(font_size.into(), window.rem_size());
        let (text, spans, measured) = (self.text.clone(), self.spans.clone(), Rc::clone(&self.measured));
        let mut layout = Style::default();
        layout.min_size.width = px(0.).into();
        let layout_id = window.request_measured_layout(layout, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                _ => None,
            });
            if let Some(measured) = measured.borrow().as_ref()
                && measured.width == width
            {
                return measured.size;
            }
            let runs = runs(&style, &text, &spans);
            let natural = window.text_system().shape_line(text.clone(), font_size, &runs, None).width;
            let (text, runs) = match width {
                Some(width) if natural > width => {
                    let font = runs.first().map(|run| run.font.clone()).unwrap_or_else(|| style.font());
                    let mut wrapper = cx.text_system().line_wrapper(font, font_size);
                    let (text, runs) = wrapper.truncate_line(text.clone(), width, ELLIPSIS, &runs, TruncateFrom::End);
                    (text, runs.into_owned())
                }
                _ => (text.clone(), runs),
            };
            let line = window.text_system().shape_line(text, font_size, &runs, None);
            let measured_size = Size { width: line.width, height: line_height };
            measured.borrow_mut().replace(Measured { width, line, size: measured_size, line_height });
            measured_size
        });
        (layout_id, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let measured = self.measured.borrow();
        let Some(measured) = measured.as_ref() else { return };
        let _ = measured.line.paint(bounds.origin, measured.line_height, TextAlign::Left, None, window, cx);
    }
}
fn runs(style: &gpui::TextStyle, text: &str, spans: &[Span]) -> Vec<TextRun> {
    if spans.is_empty() {
        return vec![style.to_run(text.len())];
    }
    let mut bounds: Vec<usize> = vec![0, text.len()];
    bounds.extend(spans.iter().flat_map(|span| [span.range.start, span.range.end]));
    bounds.retain(|at| *at <= text.len() && text.is_char_boundary(*at));
    bounds.sort_unstable();
    bounds.dedup();
    let mut runs = Vec::new();
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end {
            continue;
        }
        let mut run = style.to_run(end - start);
        if let Some(span) = spans.iter().find(|span| span.range.start <= start && span.range.end >= end) {
            run.color = span.color.unwrap_or(run.color);
            if let Some(weight) = span.weight {
                run.font.weight = weight;
            }
            if span.italic == Some(true) {
                run.font.style = FontStyle::Italic;
            }
        }
        runs.push(run);
    }
    runs
}
pub fn text(text: impl Into<SharedString>) -> Text {
    Text::new(text)
}
