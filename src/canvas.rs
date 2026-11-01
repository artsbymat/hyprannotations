mod session;
use serde::{Deserialize, Serialize};
pub use session::Session;
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Stroke, Transform,
};

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let radius = radius.min(w / 2.).min(h / 2.);
    let mut p = PathBuilder::new();
    p.move_to(x + radius, y);
    p.line_to(x + w - radius, y);
    p.quad_to(x + w, y, x + w, y + radius);
    p.line_to(x + w, y + h - radius);
    p.quad_to(x + w, y + h, x + w - radius, y + h);
    p.line_to(x + radius, y + h);
    p.quad_to(x, y + h, x, y + h - radius);
    p.line_to(x, y + radius);
    p.quad_to(x, y, x + radius, y);
    p.close();
    p.finish()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Pen,
    Highlighter,
    Eraser,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Text,
    Pointer,
    Select,
    Image,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}
#[derive(Clone, Debug)]
pub struct Annotation {
    pub tool: Tool,
    pub points: Vec<Point>,
    pub color: [u8; 4],
    pub width: f32,
    pub filled: bool,
    pub text: String,
    pub font_size: f32,
    pub image: Option<std::sync::Arc<Pixmap>>,
}
impl Annotation {
    /// Black or white foreground that stays legible on top of this annotation's
    /// fill color. Only used for text backgrounds.
    pub fn contrasting_text_color(&self) -> [u8; 4] {
        let [r, g, b, _] = self.color;
        let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        if luminance > 150. {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    }

    pub fn path(&self) -> Option<Path> {
        let first = *self.points.first()?;
        let last = *self.points.last()?;
        let mut p = PathBuilder::new();
        match self.tool {
            Tool::Rectangle | Tool::Ellipse => {
                let r = tiny_skia::Rect::from_xywh(
                    first.x.min(last.x),
                    first.y.min(last.y),
                    (last.x - first.x).abs().max(0.1),
                    (last.y - first.y).abs().max(0.1),
                )?;
                if self.tool == Tool::Rectangle {
                    p.push_rect(r);
                } else {
                    p.push_oval(r);
                }
            }
            Tool::Line | Tool::Arrow => {
                p.move_to(first.x, first.y);
                p.line_to(last.x, last.y);
                if self.tool == Tool::Arrow {
                    let angle = (last.y - first.y).atan2(last.x - first.x);
                    let size = (self.width * 4.0).max(12.0).min(first.distance(last) * 0.6);
                    for offset in [-0.5_f32, 0.5] {
                        p.move_to(last.x, last.y);
                        p.line_to(
                            last.x - size * (angle + offset).cos(),
                            last.y - size * (angle + offset).sin(),
                        );
                    }
                }
            }
            Tool::Pen | Tool::Highlighter => {
                p.move_to(first.x, first.y);
                if self.points.len() == 1 {
                    p.line_to(first.x + 0.01, first.y);
                }
                for pair in self.points.windows(2) {
                    let mid =
                        Point::new((pair[0].x + pair[1].x) / 2.0, (pair[0].y + pair[1].y) / 2.0);
                    p.quad_to(pair[0].x, pair[0].y, mid.x, mid.y);
                }
                p.line_to(last.x, last.y);
            }
            _ => return None,
        }
        p.finish()
    }
    pub fn bounds(&self) -> Option<tiny_skia::Rect> {
        if self.tool == Tool::Image {
            let first = *self.points.first()?;
            let last = *self.points.last()?;
            tiny_skia::Rect::from_xywh(
                first.x.min(last.x),
                first.y.min(last.y),
                (last.x - first.x).abs().max(0.1),
                (last.y - first.y).abs().max(0.1),
            )
        } else if self.tool == Tool::Text {
            let p = self.points.first()?;
            let line_height = self.font_size * 1.4;
            let lines: Vec<&str> = self.text.split('\n').collect();
            let max_chars = lines
                .iter()
                .map(|l| l.trim_end_matches('\r').chars().count())
                .max()
                .unwrap_or(0);
            let width = (max_chars as f32 * self.font_size * 0.75).max(self.font_size * 0.5);
            let height = (lines.len().max(1) as f32 * line_height).max(line_height);
            if self.filled {
                let padding = (self.font_size * 0.2).max(3.);
                tiny_skia::Rect::from_xywh(
                    p.x - padding,
                    p.y - padding,
                    width + padding * 2.,
                    height + padding * 2.,
                )
            } else {
                tiny_skia::Rect::from_xywh(p.x, p.y, width, height)
            }
        } else {
            self.path().map(|p| p.bounds())
        }
    }
    pub fn hit(&self, point: Point, radius: f32) -> bool {
        if self.tool == Tool::Image {
            return self.bounds().is_some_and(|bounds| {
                point.x >= bounds.left() - radius
                    && point.x <= bounds.right() + radius
                    && point.y >= bounds.top() - radius
                    && point.y <= bounds.bottom() + radius
            });
        }
        if self.tool == Tool::Text {
            if self.filled
                && let Some(bounds) = self.bounds()
                && point.x >= bounds.left() - radius
                && point.x <= bounds.right() + radius
                && point.y >= bounds.top() - radius
                && point.y <= bounds.bottom() + radius
            {
                return true;
            }
            let Some(p) = self.points.first() else {
                return false;
            };
            let line_height = self.font_size * 1.4;
            return self.text.split('\n').enumerate().any(|(i, line)| {
                let line = line.trim_end_matches('\r');
                let line_top = p.y + i as f32 * line_height;
                let line_bottom = line_top + line_height;
                let line_width =
                    (line.chars().count() as f32 * self.font_size * 0.75).max(self.font_size * 0.5);
                point.x >= p.x - radius
                    && point.x <= p.x + line_width + radius
                    && point.y >= line_top - radius
                    && point.y <= line_bottom + radius
            });
        }
        // Rasterize a small region for accurate hit testing of curves and filled shapes.
        let size = (radius * 2.0).ceil().max(2.0) as u32;
        let Some(mut pix) = Pixmap::new(size, size) else {
            return false;
        };
        self.render_shape(
            &mut pix,
            Transform::from_translate(radius - point.x, radius - point.y),
        );
        pix.pixels().iter().any(|p| p.alpha() != 0)
    }
    pub fn render_shape(&self, pix: &mut Pixmap, transform: Transform) {
        if self.tool == Tool::Image {
            let (Some(image), Some(bounds)) = (&self.image, self.bounds()) else {
                return;
            };
            let image_transform = Transform::from_scale(
                bounds.width() / image.width() as f32,
                bounds.height() / image.height() as f32,
            )
            .post_translate(bounds.left(), bounds.top())
            .post_concat(transform);
            pix.draw_pixmap(
                0,
                0,
                image.as_ref().as_ref(),
                &PixmapPaint {
                    quality: FilterQuality::Bicubic,
                    ..PixmapPaint::default()
                },
                image_transform,
                None,
            );
            return;
        }
        let Some(path) = self.path() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color[0], self.color[1], self.color[2], self.color[3]);
        if self.filled && matches!(self.tool, Tool::Rectangle | Tool::Ellipse) {
            pix.fill_path(&path, &paint, FillRule::Winding, transform, None);
        } else {
            pix.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: self.width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                },
                transform,
                None,
            );
        }
    }
}
#[derive(Debug)]
enum Command {
    Add(Annotation),
    Remove(Vec<(usize, Annotation)>),
    Clear(Vec<Annotation>),
    Move(usize, Point),
}
#[derive(Default)]
pub struct Document {
    pub objects: Vec<Annotation>,
    undo: Vec<Command>,
    redo: Vec<Command>,
}
impl Document {
    pub fn add(&mut self, annotation: Annotation) {
        self.objects.push(annotation.clone());
        self.undo.push(Command::Add(annotation));
        self.redo.clear();
    }
    pub fn erase(&mut self, points: &[Point], radius: f32) {
        let mut indices = std::collections::BTreeSet::new();
        for point in points {
            // Only the frontmost annotation under each sampled point is erased.
            // Lower layers stay until the next eraser pass.
            if let Some(index) = self.objects.iter().rposition(|a| a.hit(*point, radius)) {
                indices.insert(index);
            }
        }
        if indices.is_empty() {
            return;
        }
        let removed: Vec<_> = indices
            .into_iter()
            .map(|index| (index, self.objects[index].clone()))
            .collect();
        for (i, _) in removed.iter().rev() {
            self.objects.remove(*i);
        }
        self.undo.push(Command::Remove(removed));
        self.redo.clear();
    }
    pub fn delete(&mut self, index: usize) {
        if index >= self.objects.len() {
            return;
        }
        let a = self.objects.remove(index);
        self.undo.push(Command::Remove(vec![(index, a)]));
        self.redo.clear();
    }
    pub fn translate(&mut self, index: usize, delta: Point) {
        if let Some(a) = self.objects.get_mut(index) {
            for p in &mut a.points {
                p.x += delta.x;
                p.y += delta.y;
            }
        }
    }
    pub fn record_move(&mut self, index: usize, delta: Point) {
        if delta.x != 0. || delta.y != 0. {
            self.undo.push(Command::Move(index, delta));
            self.redo.clear();
        }
    }
    pub fn clear(&mut self) {
        if !self.objects.is_empty() {
            self.undo
                .push(Command::Clear(std::mem::take(&mut self.objects)));
            self.redo.clear();
        }
    }
    pub fn undo(&mut self) {
        let Some(command) = self.undo.pop() else {
            return;
        };
        match &command {
            Command::Add(_) => {
                self.objects.pop();
            }
            Command::Remove(items) => {
                for (i, a) in items {
                    self.objects.insert(*i, a.clone());
                }
            }
            Command::Clear(items) => self.objects = items.clone(),
            Command::Move(i, d) => self.translate(*i, Point::new(-d.x, -d.y)),
        }
        self.redo.push(command);
    }
    pub fn redo(&mut self) {
        let Some(command) = self.redo.pop() else {
            return;
        };
        match &command {
            Command::Add(a) => self.objects.push(a.clone()),
            Command::Remove(items) => {
                for (i, _) in items.iter().rev() {
                    self.objects.remove(*i);
                }
            }
            Command::Clear(_) => self.objects.clear(),
            Command::Move(i, d) => self.translate(*i, *d),
        }
        self.undo.push(command);
    }
}

pub struct Renderer {
    fonts: cosmic_text::FontSystem,
    cache: cosmic_text::SwashCache,
}
impl Renderer {
    pub fn new() -> Self {
        Self {
            fonts: cosmic_text::FontSystem::new(),
            cache: cosmic_text::SwashCache::new(),
        }
    }
    pub fn draw(&mut self, pix: &mut Pixmap, annotation: &Annotation, scale: f32) {
        self.draw_internal(pix, annotation, scale, false);
    }

    pub fn draw_editing(&mut self, pix: &mut Pixmap, annotation: &Annotation, scale: f32) {
        self.draw_internal(pix, annotation, scale, true);
    }

    fn draw_internal(
        &mut self,
        pix: &mut Pixmap,
        annotation: &Annotation,
        scale: f32,
        caret: bool,
    ) {
        if annotation.tool != Tool::Text {
            annotation.render_shape(pix, Transform::from_scale(scale, scale));
            return;
        }
        let Some(origin) = annotation.points.first() else {
            return;
        };
        use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};
        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(
                annotation.font_size * scale,
                annotation.font_size * scale * 1.4,
            ),
        );
        buffer.set_text(
            &mut self.fonts,
            &annotation.text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let c = if annotation.filled {
            annotation.contrasting_text_color()
        } else {
            annotation.color
        };
        let last_run = buffer.layout_runs().last();
        let fallback_height = annotation.font_size * scale * 1.4;
        let newlines = annotation
            .text
            .chars()
            .filter(|character| *character == '\n')
            .count();
        let trailing_newlines = annotation.text.ends_with('\n');
        let (caret_x, caret_y, caret_height) = if trailing_newlines {
            last_run.map_or(
                (0.0, fallback_height * newlines as f32, fallback_height),
                |run| {
                    // `last_run` may already represent an empty line created
                    // by the trailing newlines, so only advance the remaining
                    // lines instead of adding every trailing newline again.
                    let remaining_lines = newlines.saturating_sub(run.line_i);
                    (
                        0.0,
                        run.line_top + run.line_height * remaining_lines as f32,
                        run.line_height,
                    )
                },
            )
        } else {
            last_run.map_or((0.0, 0.0, fallback_height), |run| {
                (
                    if run.rtl { 0.0 } else { run.line_w },
                    run.line_top,
                    run.line_height,
                )
            })
        };
        if annotation.filled {
            let padding = (annotation.font_size * scale * 0.2).max(3.);
            let mut text_width: f32 = 0.;
            let mut text_height: f32 = 0.;
            for run in buffer.layout_runs() {
                text_width = text_width.max(run.line_w);
                text_height = text_height.max(run.line_top + run.line_height);
            }
            // The caret can be on an empty line that has no layout run yet.
            text_height = text_height.max(caret_y + caret_height);
            let min_width = annotation.font_size * scale * 0.6;
            let x = origin.x * scale - padding;
            let y = origin.y * scale - padding;
            let width = text_width.max(min_width) + padding * 2.;
            let height = text_height.max(fallback_height) + padding * 2.;
            if let Some(path) = rounded_rect(x, y, width, height, padding) {
                let mut background = Paint::default();
                background.set_color_rgba8(
                    annotation.color[0],
                    annotation.color[1],
                    annotation.color[2],
                    annotation.color[3],
                );
                pix.fill_path(
                    &path,
                    &background,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        let caret_geometry = if caret {
            Some((caret_x, caret_y, caret_height))
        } else {
            None
        };
        buffer.draw(
            &mut self.fonts,
            &mut self.cache,
            cosmic_text::Color::rgba(c[0], c[1], c[2], c[3]),
            |x, y, w, h, color| {
                let Some(rect) = tiny_skia::Rect::from_xywh(
                    origin.x * scale + x as f32,
                    origin.y * scale + y as f32,
                    w as f32,
                    h as f32,
                ) else {
                    return;
                };
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba8(
                    color.r(),
                    color.g(),
                    color.b(),
                    color.a(),
                ));
                pix.fill_rect(rect, &paint, Transform::identity(), None);
            },
        );
        if let Some((x, y, height)) = caret_geometry
            && let Some(rect) = tiny_skia::Rect::from_xywh(
                (origin.x * scale + x).round(),
                (origin.y * scale + y).round(),
                (1.5 * scale).ceil().max(1.0),
                height.ceil().max(1.0),
            )
        {
            let mut caret_paint = Paint::default();
            caret_paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
            pix.fill_rect(rect, &caret_paint, Transform::identity(), None);
        }
    }

    pub fn draw_pointer_trail(
        &mut self,
        pix: &mut Pixmap,
        points: &std::collections::VecDeque<session::PointerTrailPoint>,
        scale: f32,
        now: std::time::Instant,
    ) {
        const COLOR: [u8; 3] = [255, 48, 82];
        for (a, b) in points.iter().zip(points.iter().skip(1)) {
            if a.stroke != b.stroke {
                continue;
            }
            let alpha = b.opacity(now);
            if alpha <= 0.0 {
                continue;
            }
            let mut path = PathBuilder::new();
            path.move_to(a.point.x, a.point.y);
            path.line_to(b.point.x, b.point.y);
            let Some(path) = path.finish() else { continue };
            for (width, opacity) in [(14.0, alpha * 0.18), (5.0, alpha * 0.95)] {
                let mut paint = Paint::default();
                paint.set_color_rgba8(COLOR[0], COLOR[1], COLOR[2], (opacity * 255.) as u8);
                pix.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    },
                    Transform::from_scale(scale, scale),
                    None,
                );
            }
        }

        if let Some(head) = points.back() {
            let alpha = head.opacity(now);
            for (radius, opacity) in [(11.0, alpha * 0.18), (5.0, alpha)] {
                let Some(circle) = PathBuilder::from_circle(head.point.x, head.point.y, radius)
                else {
                    continue;
                };
                let mut paint = Paint::default();
                paint.set_color_rgba8(COLOR[0], COLOR[1], COLOR[2], (opacity * 255.) as u8);
                pix.fill_path(
                    &circle,
                    &paint,
                    FillRule::Winding,
                    Transform::from_scale(scale, scale),
                    None,
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn line() -> Annotation {
        Annotation {
            tool: Tool::Line,
            points: vec![Point::new(10., 10.), Point::new(50., 10.)],
            color: [255; 4],
            width: 4.,
            filled: false,
            text: String::new(),
            font_size: 28.,
            image: None,
        }
    }
    #[test]
    fn history_branch_and_erase() {
        let mut d = Document::default();
        d.add(line());
        d.erase(&[Point::new(30., 10.)], 3.);
        assert!(d.objects.is_empty());
        d.undo();
        assert_eq!(d.objects.len(), 1);
        d.redo();
        assert!(d.objects.is_empty());
        d.undo();
        d.clear();
        d.undo();
        assert_eq!(d.objects.len(), 1);
        d.undo();
        d.add(line());
        d.redo();
        assert_eq!(d.objects.len(), 1);
    }
    #[test]
    fn hit_geometry() {
        let a = line();
        assert!(a.hit(Point::new(30., 11.), 2.));
        assert!(!a.hit(Point::new(30., 30.), 2.));
    }

    #[test]
    fn pasted_image_renders_and_participates_in_history() {
        let mut source = Pixmap::new(2, 2).unwrap();
        source.fill(Color::from_rgba8(255, 0, 0, 255));
        let image = Annotation {
            tool: Tool::Image,
            points: vec![Point::new(10., 10.), Point::new(30., 30.)],
            color: [255; 4],
            width: 1.,
            filled: false,
            text: String::new(),
            font_size: 12.,
            image: Some(std::sync::Arc::new(source)),
        };
        assert!(image.hit(Point::new(20., 20.), 0.));
        assert!(!image.hit(Point::new(40., 40.), 0.));

        let mut output = Pixmap::new(50, 50).unwrap();
        Renderer::new().draw(&mut output, &image, 1.);
        assert_ne!(output.pixel(20, 20).unwrap().alpha(), 0);

        let mut document = Document::default();
        document.add(image);
        document.translate(0, Point::new(5., 7.));
        document.record_move(0, Point::new(5., 7.));
        assert_eq!(document.objects[0].bounds().unwrap().left(), 15.);
        document.undo();
        assert_eq!(document.objects[0].bounds().unwrap().left(), 10.);
        document.undo();
        assert!(document.objects.is_empty());
        document.redo();
        assert_eq!(document.objects.len(), 1);
    }

    #[test]
    fn erase_removes_overlapping_annotations_one_layer_at_a_time() {
        let mut d = Document::default();
        d.add(line());
        d.add(line());
        assert_eq!(d.objects.len(), 2);

        d.erase(&[Point::new(30., 10.)], 3.);
        assert_eq!(d.objects.len(), 1, "only the frontmost layer is erased");

        d.erase(&[Point::new(30., 10.)], 3.);
        assert!(d.objects.is_empty(), "the next pass erases the lower layer");

        d.undo();
        assert_eq!(d.objects.len(), 1);
        d.undo();
        assert_eq!(d.objects.len(), 2);
    }

    #[test]
    fn arrow_has_head() {
        let mut a = line();
        a.tool = Tool::Arrow;
        assert!(a.path().unwrap().bounds().height() > 5.);
    }

    #[test]
    fn empty_text_draft_draws_a_caret() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(4., 4.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: false,
            text: String::new(),
            font_size: 20.,
            image: None,
        };
        let mut renderer = Renderer::new();
        let mut hidden = Pixmap::new(64, 64).unwrap();
        renderer.draw(&mut hidden, &annotation, 1.);
        assert!(hidden.pixels().iter().all(|pixel| pixel.alpha() == 0));

        let mut visible = Pixmap::new(64, 64).unwrap();
        renderer.draw_editing(&mut visible, &annotation, 1.);
        assert!(visible.pixels().iter().any(|pixel| pixel.alpha() != 0));
    }

    #[test]
    fn text_ending_in_newline_draws_caret_on_next_line() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(4., 4.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: false,
            text: "abc\n".into(),
            font_size: 20.,
            image: None,
        };
        let mut pixmap = Pixmap::new(100, 80).unwrap();
        Renderer::new().draw_editing(&mut pixmap, &annotation, 1.);

        let caret_pixel = pixmap.pixel(4, 36).unwrap();
        assert_ne!(
            caret_pixel.alpha(),
            0,
            "caret should start at the left edge of the second line"
        );
    }

    #[test]
    fn consecutive_trailing_newlines_keep_caret_on_the_last_line() {
        for newlines in 1..=4 {
            let annotation = Annotation {
                tool: Tool::Text,
                points: vec![Point::new(4., 4.)],
                color: [255, 255, 255, 255],
                width: 2.,
                filled: false,
                text: format!("abc{}", "\n".repeat(newlines)),
                font_size: 20.,
                image: None,
            };
            let mut pixmap = Pixmap::new(100, 200).unwrap();
            Renderer::new().draw_editing(&mut pixmap, &annotation, 1.);

            // The caret belongs on the line directly after the last newline.
            let caret_y = 4 + newlines as u32 * 28;
            assert_ne!(
                pixmap.pixel(4, caret_y).unwrap().alpha(),
                0,
                "caret missing for {newlines} trailing newline(s)"
            );
            if newlines > 1 {
                assert_eq!(
                    pixmap.pixel(4, 4 + 28).unwrap().alpha(),
                    0,
                    "first empty line must stay empty for {newlines} newlines"
                );
            }
        }
    }

    #[test]
    fn text_background_uses_contrasting_foreground() {
        let mut annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(10., 10.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: true,
            text: "hello".into(),
            font_size: 20.,
            image: None,
        };
        assert_eq!(annotation.contrasting_text_color(), [0, 0, 0, 255]);
        annotation.color = [0, 0, 0, 255];
        assert_eq!(annotation.contrasting_text_color(), [255, 255, 255, 255]);
    }

    #[test]
    fn filled_text_draws_a_background_and_keeps_text_readable() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(20., 20.)],
            color: [255, 255, 0, 255],
            width: 2.,
            filled: true,
            text: "hello".into(),
            font_size: 20.,
            image: None,
        };
        let mut pixmap = Pixmap::new(160, 100).unwrap();
        Renderer::new().draw(&mut pixmap, &annotation, 1.);

        // Background padding is painted even before the glyphs begin.
        assert!(pixmap.pixel(17, 22).unwrap().alpha() != 0);
        // Dark text sits on the bright yellow background.
        let dark_pixels = (20..120)
            .flat_map(|x| (10..70).map(move |y| (x, y)))
            .filter(|(x, y)| {
                pixmap.pixel(*x, *y).is_some_and(|p| {
                    p.alpha() != 0 && p.red() < 80 && p.green() < 80 && p.blue() < 80
                })
            })
            .count();
        assert!(dark_pixels > 0);
    }

    #[test]
    fn filled_text_background_has_padding_and_rounded_corners() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(30., 30.)],
            color: [255, 255, 0, 255],
            width: 2.,
            filled: true,
            text: "hi".into(),
            font_size: 20.,
            image: None,
        };
        let mut pixmap = Pixmap::new(140, 100).unwrap();
        Renderer::new().draw(&mut pixmap, &annotation, 1.);

        // Padding extends outside the text origin.
        assert_ne!(pixmap.pixel(27, 32).unwrap().alpha(), 0);
        // The rounded corner is transparent while the padding edge is filled.
        assert_eq!(pixmap.pixel(26, 26).unwrap().alpha(), 0);
        assert_ne!(pixmap.pixel(31, 27).unwrap().alpha(), 0);
    }

    #[test]
    fn filled_text_background_covers_the_empty_line_after_shift_enter() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(30., 30.)],
            color: [255, 255, 0, 255],
            width: 2.,
            filled: true,
            text: "hi\n".into(),
            font_size: 20.,
            image: None,
        };
        let mut pixmap = Pixmap::new(140, 140).unwrap();
        Renderer::new().draw_editing(&mut pixmap, &annotation, 1.);

        // The second (empty) line is inside the background and shows the caret.
        assert_ne!(pixmap.pixel(30, 30 + 28 + 4).unwrap().alpha(), 0);
        // Background padding reaches below that empty line.
        assert_ne!(pixmap.pixel(30, 30 + 2 * 28 + 2).unwrap().alpha(), 0);
    }

    #[test]
    fn multiline_text_bounds_covers_all_lines() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(10., 20.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: false,
            text: "Baris 1\nBaris 2 panjang".into(),
            font_size: 20.,
            image: None,
        };
        let bounds = annotation.bounds().expect("text should have bounds");
        assert_eq!(bounds.left(), 10.);
        assert_eq!(bounds.top(), 20.);
        assert_eq!(bounds.height(), 56.);
        assert!(bounds.width() >= 15. * 20. * 0.75);
    }

    #[test]
    fn multiline_text_hit_testing_works_on_all_lines() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(10., 20.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: false,
            text: "Baris 1\nBaris 2".into(),
            font_size: 20.,
            image: None,
        };
        // Line 1: y in [20, 48]. Test at y = 30.
        assert!(annotation.hit(Point::new(20., 30.), 5.));
        // Line 2: y in [48, 76]. Test at y = 60.
        assert!(annotation.hit(Point::new(20., 60.), 5.));
        // Below line 2: y = 90. Should NOT hit.
        assert!(!annotation.hit(Point::new(20., 90.), 5.));
        // Above line 1: y = 5. Should NOT hit.
        assert!(!annotation.hit(Point::new(20., 5.), 5.));
    }

    #[test]
    fn filled_text_hit_testing_covers_entire_background() {
        let annotation = Annotation {
            tool: Tool::Text,
            points: vec![Point::new(10., 20.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: true,
            text: "Baris 1\nBaris 2 panjang".into(),
            font_size: 20.,
            image: None,
        };
        let bounds = annotation.bounds().unwrap();
        // Point in the padding / blank area of the first short line
        assert!(annotation.hit(Point::new(bounds.left() + 5., bounds.top() + 5.), 1.));
        assert!(annotation.hit(Point::new(bounds.right() - 5., bounds.top() + 10.), 1.));
        // Outside the bounding box
        assert!(!annotation.hit(Point::new(bounds.right() + 10., bounds.top() + 10.), 1.));
    }
}
