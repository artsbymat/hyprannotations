use super::{Annotation, Document, Point, Tool};
use crate::app::App;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const POINTER_TRAIL_LIFETIME: Duration = Duration::from_millis(900);
const RESIZE_HANDLE_RADIUS: f32 = 9.0;
const MIN_RESIZE_SIZE: f32 = 8.0;

fn crop_preview(before: &Annotation, current: tiny_skia::Rect) -> Option<tiny_skia::Pixmap> {
    let original = before.bounds()?;
    let source = before.image.as_ref()?;
    let x = (((current.left() - original.left()) / original.width()) * source.width() as f32)
        .round()
        .clamp(0.0, source.width().saturating_sub(1) as f32) as u32;
    let y = (((current.top() - original.top()) / original.height()) * source.height() as f32)
        .round()
        .clamp(0.0, source.height().saturating_sub(1) as f32) as u32;
    let width = ((current.width() / original.width()) * source.width() as f32)
        .round()
        .max(1.0) as u32;
    let height = ((current.height() / original.height()) * source.height() as f32)
        .round()
        .max(1.0) as u32;
    let rect = tiny_skia::IntRect::from_xywh(
        x as i32,
        y as i32,
        width.min(source.width() - x),
        height.min(source.height() - y),
    )?;
    source.clone_rect(rect)
}

#[derive(Clone, Copy, Debug)]
enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

struct ResizeGesture {
    index: usize,
    corner: ResizeCorner,
    before: Annotation,
    original_width: f32,
    original_height: f32,
    crop: bool,
    last_crop_preview: Instant,
}

#[derive(Clone, Copy, Debug)]
pub struct PointerTrailPoint {
    pub point: Point,
    pub created: Instant,
    pub stroke: u64,
}

impl PointerTrailPoint {
    pub fn opacity(self, now: Instant) -> f32 {
        1.0 - now.saturating_duration_since(self.created).as_secs_f32()
            / POINTER_TRAIL_LIFETIME.as_secs_f32()
    }
}

#[derive(Default)]
pub struct Session {
    pub document: Document,
    pub draft: Option<Annotation>,
    pub selected: Option<usize>,
    selected_extra: std::collections::BTreeSet<usize>,
    pub pointer_trail: VecDeque<PointerTrailPoint>,
    pointer_stroke: u64,
    erase_points: Vec<Point>,
    move_origin: Option<Point>,
    move_last: Option<Point>,
    resize: Option<ResizeGesture>,
    pub crop_mode: bool,
}
impl Session {
    pub fn selected_indices(&self) -> Vec<usize> {
        let mut indices: std::collections::BTreeSet<_> = self.selected_extra.clone();
        if let Some(selected) = self.selected {
            indices.insert(selected);
        }
        indices.into_iter().collect()
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_extra.clear();
        self.crop_mode = false;
    }

    pub fn set_selection(&mut self, indices: Vec<usize>) {
        self.clear_selection();
        let mut indices = indices.into_iter();
        self.selected = indices.next_back();
        self.selected_extra.extend(indices);
    }

    pub fn delete_selected(&mut self) {
        let indices = self.selected_indices();
        self.document.delete_many(&indices);
        self.clear_selection();
    }

    pub fn duplicate_selected(&mut self) {
        let indices = self.selected_indices();
        let copies = self.document.duplicate(&indices);
        self.set_selection(copies);
    }

    pub fn reorder_selected(&mut self, forward: bool) {
        let indices = self.selected_indices();
        let reordered = self.document.reorder(&indices, forward);
        self.set_selection(reordered);
    }

    pub fn rotate_selected_images(&mut self) {
        let indices = self.selected_indices();
        self.document.rotate_images(&indices);
    }

    pub fn toggle_crop_mode(&mut self) -> Option<bool> {
        let is_image = self
            .selected
            .and_then(|index| self.document.objects.get(index))
            .is_some_and(|annotation| annotation.tool == Tool::Image);
        if is_image {
            self.crop_mode = !self.crop_mode;
            Some(self.crop_mode)
        } else {
            self.crop_mode = false;
            None
        }
    }

    fn resize_handle_at(&self, point: Point) -> Option<(usize, ResizeCorner)> {
        let index = self.selected?;
        let annotation = self.document.objects.get(index)?;
        if !matches!(
            annotation.tool,
            Tool::Rectangle | Tool::Ellipse | Tool::Image
        ) {
            return None;
        }
        let bounds = annotation.bounds()?;
        [
            (
                Point::new(bounds.left(), bounds.top()),
                ResizeCorner::TopLeft,
            ),
            (
                Point::new(bounds.right(), bounds.top()),
                ResizeCorner::TopRight,
            ),
            (
                Point::new(bounds.left(), bounds.bottom()),
                ResizeCorner::BottomLeft,
            ),
            (
                Point::new(bounds.right(), bounds.bottom()),
                ResizeCorner::BottomRight,
            ),
        ]
        .into_iter()
        .find(|(handle, _)| handle.distance(point) <= RESIZE_HANDLE_RADIUS)
        .map(|(_, corner)| (index, corner))
    }

    pub fn over_resize_handle(&self, point: Point) -> bool {
        self.resize_handle_at(point).is_some()
    }

    #[cfg(test)]
    pub fn begin(&mut self, app: &App, p: Point) {
        self.begin_additive(app, p, false);
    }

    pub fn begin_additive(&mut self, app: &App, p: Point, additive: bool) {
        match app.tool {
            Tool::Eraser => self.erase_points.push(p),
            Tool::Select => {
                if !additive && let Some((index, corner)) = self.resize_handle_at(p) {
                    let annotation = &self.document.objects[index];
                    let bounds = annotation.bounds().expect("resizable object has bounds");
                    self.resize = Some(ResizeGesture {
                        index,
                        corner,
                        before: annotation.clone(),
                        original_width: bounds.width(),
                        original_height: bounds.height(),
                        crop: self.crop_mode && annotation.tool == Tool::Image,
                        last_crop_preview: Instant::now() - Duration::from_millis(16),
                    });
                    self.move_origin = None;
                    self.move_last = None;
                    return;
                }
                let hit = self.document.objects.iter().rposition(|a| a.hit(p, 5.));
                if additive {
                    if let Some(hit) = hit {
                        if self.selected == Some(hit) {
                            self.selected = self.selected_extra.pop_last();
                        } else if !self.selected_extra.remove(&hit)
                            && let Some(previous) = self.selected.replace(hit)
                        {
                            self.selected_extra.insert(previous);
                        }
                    }
                    self.move_origin = None;
                    self.move_last = None;
                } else {
                    let already_selected = hit.is_some_and(|index| {
                        self.selected == Some(index) || self.selected_extra.contains(&index)
                    });
                    if !already_selected {
                        self.clear_selection();
                        self.selected = hit;
                    }
                    self.move_origin = hit.map(|_| p);
                    self.move_last = hit.map(|_| p);
                }
            }
            Tool::Pointer => {
                self.pointer_stroke = self.pointer_stroke.wrapping_add(1);
                self.push_pointer_point(p, Instant::now());
            }
            _ => self.draft = Some(app.annotation(p)),
        }
    }
    #[cfg(test)]
    pub fn motion(&mut self, app: &App, p: Point) {
        self.motion_free(app, p, false);
    }

    pub fn motion_free(&mut self, app: &App, p: Point, free_image_resize: bool) {
        let v = &mut *self;
        if app.tool == Tool::Pointer {
            let now = Instant::now();
            let start = self.pointer_trail.back().map_or(p, |point| point.point);
            let steps = (start.distance(p) / 3.0).ceil().max(1.0) as usize;
            for step in 1..=steps {
                let ratio = step as f32 / steps as f32;
                self.push_pointer_point(
                    Point::new(
                        start.x + (p.x - start.x) * ratio,
                        start.y + (p.y - start.y) * ratio,
                    ),
                    now,
                );
            }
        } else if app.tool == Tool::Select {
            if let Some(resize) = &mut v.resize {
                let Some(annotation) = v.document.objects.get_mut(resize.index) else {
                    return;
                };
                let Some(original) = resize.before.bounds() else {
                    return;
                };
                let (opposite, sign_x, sign_y) = match resize.corner {
                    ResizeCorner::TopLeft => {
                        (Point::new(original.right(), original.bottom()), -1.0, -1.0)
                    }
                    ResizeCorner::TopRight => {
                        (Point::new(original.left(), original.bottom()), 1.0, -1.0)
                    }
                    ResizeCorner::BottomLeft => {
                        (Point::new(original.right(), original.top()), -1.0, 1.0)
                    }
                    ResizeCorner::BottomRight => {
                        (Point::new(original.left(), original.top()), 1.0, 1.0)
                    }
                };
                let (width, height) = if resize.crop {
                    let width =
                        ((p.x - opposite.x) * sign_x).clamp(MIN_RESIZE_SIZE, original.width());
                    let height =
                        ((p.y - opposite.y) * sign_y).clamp(MIN_RESIZE_SIZE, original.height());
                    (width, height)
                } else if annotation.tool == Tool::Image && !free_image_resize {
                    let base_x = sign_x * resize.original_width;
                    let base_y = sign_y * resize.original_height;
                    let projected = ((p.x - opposite.x) * base_x + (p.y - opposite.y) * base_y)
                        / (base_x * base_x + base_y * base_y);
                    let minimum = (MIN_RESIZE_SIZE / resize.original_width)
                        .max(MIN_RESIZE_SIZE / resize.original_height);
                    let scale = projected.max(minimum);
                    (
                        resize.original_width * scale,
                        resize.original_height * scale,
                    )
                } else {
                    (
                        ((p.x - opposite.x) * sign_x).max(MIN_RESIZE_SIZE),
                        ((p.y - opposite.y) * sign_y).max(MIN_RESIZE_SIZE),
                    )
                };
                let target = Point::new(opposite.x + sign_x * width, opposite.y + sign_y * height);
                annotation.points = vec![
                    Point::new(opposite.x.min(target.x), opposite.y.min(target.y)),
                    Point::new(opposite.x.max(target.x), opposite.y.max(target.y)),
                ];
                if resize.crop
                    && resize.last_crop_preview.elapsed() >= Duration::from_millis(16)
                    && let Some(bounds) = annotation.bounds()
                    && let Some(cropped) = crop_preview(&resize.before, bounds)
                {
                    annotation.image = Some(std::sync::Arc::new(cropped));
                    resize.last_crop_preview = Instant::now();
                }
            } else if let Some(last) = v.move_last {
                let delta = Point::new(p.x - last.x, p.y - last.y);
                for selected in v.selected_indices() {
                    v.document.translate(selected, delta);
                }
                v.move_last = Some(p);
            }
        } else if app.tool == Tool::Eraser {
            let start = v.erase_points.last().copied().unwrap_or(p);
            let n = (start.distance(p) / (app.width.max(2.))).ceil() as usize;
            for step in 1..=n {
                let t = step as f32 / n as f32;
                v.erase_points.push(Point::new(
                    start.x + (p.x - start.x) * t,
                    start.y + (p.y - start.y) * t,
                ));
            }
        } else if let Some(a) = &mut v.draft {
            match a.tool {
                Tool::Pen | Tool::Highlighter => {
                    if a.points.last().is_none_or(|last| last.distance(p) > 0.5) {
                        a.points.push(p);
                    }
                }
                Tool::Text => {}
                _ => {
                    a.points.truncate(1);
                    a.points.push(p);
                }
            }
        }
    }

    fn push_pointer_point(&mut self, point: Point, created: Instant) {
        self.pointer_trail.push_back(PointerTrailPoint {
            point,
            created,
            stroke: self.pointer_stroke,
        });
        while self.pointer_trail.len() > 512 {
            self.pointer_trail.pop_front();
        }
    }

    pub fn animate_pointer(&mut self, now: Instant) -> bool {
        let was_active = !self.pointer_trail.is_empty();
        while self.pointer_trail.front().is_some_and(|point| {
            now.saturating_duration_since(point.created) >= POINTER_TRAIL_LIFETIME
        }) {
            self.pointer_trail.pop_front();
        }
        was_active
    }

    pub fn pointer_active(&self) -> bool {
        !self.pointer_trail.is_empty()
    }
    pub fn finish(&mut self, app: &App) {
        let v = &mut *self;
        if let Some(resize) = v.resize.take() {
            if resize.crop
                && let Some(bounds) = v.document.objects[resize.index].bounds()
                && let Some(cropped) = crop_preview(&resize.before, bounds)
            {
                v.document.objects[resize.index].image = Some(std::sync::Arc::new(cropped));
            }
            v.document.record_transform(resize.index, resize.before);
        }
        if let (Some(origin), Some(last)) = (v.move_origin.take(), v.move_last.take()) {
            v.document.record_moves(
                v.selected_indices(),
                Point::new(last.x - origin.x, last.y - origin.y),
            );
        }
        if !v.erase_points.is_empty() {
            v.document.erase(&v.erase_points, app.width * 2.);
            v.erase_points.clear();
        }
        if v.draft.as_ref().is_some_and(|a| a.tool != Tool::Text)
            && let Some(mut a) = v.draft.take()
        {
            if app.auto_number {
                a.number = Some(v.document.next_number());
            }
            v.document.add(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    #[test]
    fn moving_selection_is_one_undoable_command() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Line;
        let mut session = Session::default();
        session.begin(&app, Point::new(10., 10.));
        session.motion(&app, Point::new(100., 10.));
        session.finish(&app);
        app.tool = Tool::Select;
        session.begin(&app, Point::new(50., 10.));
        session.motion(&app, Point::new(60., 30.));
        session.motion(&app, Point::new(70., 40.));
        session.finish(&app);
        assert_eq!(session.document.objects[0].points[0], Point::new(30., 40.));
        session.document.undo();
        assert_eq!(session.document.objects[0].points[0], Point::new(10., 10.));
        session.document.redo();
        assert_eq!(session.document.objects[0].points[0], Point::new(30., 40.));
        session.document.delete(0);
        assert!(session.document.objects.is_empty());
        session.document.undo();
        assert_eq!(session.document.objects.len(), 1);
    }

    #[test]
    fn rectangle_resize_is_one_undoable_command() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Rectangle;
        let mut session = Session::default();
        session.begin(&app, Point::new(10., 10.));
        session.motion(&app, Point::new(50., 40.));
        session.finish(&app);

        app.tool = Tool::Select;
        session.begin(&app, Point::new(10., 10.));
        session.finish(&app);
        session.begin(&app, Point::new(50., 40.));
        session.motion(&app, Point::new(90., 70.));
        session.finish(&app);

        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (80., 60.));
        session.document.undo();
        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (40., 30.));
        session.document.redo();
        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (80., 60.));
    }

    #[test]
    fn image_resize_preserves_aspect_ratio() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Select;
        let mut session = Session::default();
        session.document.add(Annotation {
            tool: Tool::Image,
            points: vec![Point::new(10., 10.), Point::new(110., 60.)],
            color: [255; 4],
            width: 1.,
            filled: false,
            text: String::new(),
            font_size: 12.,
            image: Some(std::sync::Arc::new(tiny_skia::Pixmap::new(2, 1).unwrap())),
            number: None,
        });
        session.selected = Some(0);
        session.begin(&app, Point::new(110., 60.));
        session.motion(&app, Point::new(210., 130.));
        session.finish(&app);

        let bounds = session.document.objects[0].bounds().unwrap();
        assert!((bounds.width() / bounds.height() - 2.).abs() < 0.001);
        session.document.undo();
        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (100., 50.));
    }

    fn image_annotation(width: u32, height: u32, right: f32, bottom: f32) -> Annotation {
        Annotation {
            tool: Tool::Image,
            points: vec![Point::new(0., 0.), Point::new(right, bottom)],
            color: [255; 4],
            width: 1.,
            filled: false,
            text: String::new(),
            font_size: 12.,
            image: Some(std::sync::Arc::new(
                tiny_skia::Pixmap::new(width, height).unwrap(),
            )),
            number: None,
        }
    }

    #[test]
    fn shift_allows_free_image_resize() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Select;
        let mut session = Session::default();
        session.document.add(image_annotation(2, 1, 100., 50.));
        session.selected = Some(0);
        session.begin(&app, Point::new(100., 50.));
        session.motion_free(&app, Point::new(120., 120.), true);
        session.finish(&app);
        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (120., 120.));
    }

    #[test]
    fn crop_and_rotate_image_are_undoable() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Select;
        let mut session = Session::default();
        session.document.add(image_annotation(4, 4, 100., 100.));
        session.selected = Some(0);
        assert_eq!(session.toggle_crop_mode(), Some(true));
        session.begin(&app, Point::new(100., 100.));
        session.motion(&app, Point::new(50., 50.));
        let preview = session.document.objects[0].image.as_ref().unwrap();
        assert_eq!((preview.width(), preview.height()), (2, 2));
        session.finish(&app);
        let cropped = session.document.objects[0].image.as_ref().unwrap();
        assert_eq!((cropped.width(), cropped.height()), (2, 2));
        session.document.undo();
        assert_eq!(
            session.document.objects[0].image.as_ref().unwrap().width(),
            4
        );

        session.document.objects[0] = image_annotation(4, 2, 100., 50.);
        session.rotate_selected_images();
        let bounds = session.document.objects[0].bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (50., 100.));
        let rotated = session.document.objects[0].image.as_ref().unwrap();
        assert_eq!((rotated.width(), rotated.height()), (2, 4));
        session.document.undo();
        assert_eq!(
            session.document.objects[0].image.as_ref().unwrap().width(),
            4
        );
    }

    #[test]
    fn multi_selection_moves_and_duplicates_as_one_group() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Line;
        let mut session = Session::default();
        for y in [10., 40.] {
            session.begin(&app, Point::new(10., y));
            session.motion(&app, Point::new(50., y));
            session.finish(&app);
        }
        app.tool = Tool::Select;
        session.begin(&app, Point::new(30., 10.));
        session.finish(&app);
        session.begin_additive(&app, Point::new(30., 40.), true);
        assert_eq!(session.selected_indices(), vec![0, 1]);
        session.begin(&app, Point::new(30., 10.));
        session.motion(&app, Point::new(40., 20.));
        session.finish(&app);
        assert_eq!(session.document.objects[0].points[0], Point::new(20., 20.));
        assert_eq!(session.document.objects[1].points[0], Point::new(20., 50.));
        session.document.undo();
        assert_eq!(session.document.objects[0].points[0], Point::new(10., 10.));
        assert_eq!(session.document.objects[1].points[0], Point::new(10., 40.));

        session.duplicate_selected();
        assert_eq!(session.document.objects.len(), 4);
        assert_eq!(session.selected_indices(), vec![2, 3]);
        session.document.undo();
        assert_eq!(session.document.objects.len(), 2);
    }
    #[test]
    fn fast_eraser_motion_hits_objects_between_pointer_events() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Line;
        let mut session = Session::default();
        session.begin(&app, Point::new(50., 0.));
        session.motion(&app, Point::new(50., 100.));
        session.finish(&app);
        app.tool = Tool::Eraser;
        session.begin(&app, Point::new(0., 50.));
        session.motion(&app, Point::new(100., 50.));
        session.finish(&app);
        assert!(session.document.objects.is_empty());
        session.document.undo();
        assert_eq!(session.document.objects.len(), 1);
    }
    #[test]
    fn text_stays_editable_after_pointer_release() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Text;
        let mut session = Session::default();
        session.begin(&app, Point::new(10., 10.));
        session.finish(&app);
        assert!(session.draft.is_some());
        assert!(session.document.objects.is_empty());
    }
    #[test]
    fn highlighter_applies_configured_opacity() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Highlighter;
        let a = app.annotation(Point::new(1., 1.));
        assert_eq!(a.color[3], 89);
        assert_eq!(a.width, 16.);
    }

    #[test]
    fn automatic_numbering_skips_text_and_restarts_after_clear() {
        let mut app = App::new(Settings::default());
        app.auto_number = true;
        let mut session = Session::default();

        app.tool = Tool::Pen;
        session.begin(&app, Point::new(20., 20.));
        session.finish(&app);
        assert_eq!(session.document.objects[0].number, Some(1));

        app.tool = Tool::Text;
        session.begin(&app, Point::new(30., 30.));
        session.finish(&app);
        assert_eq!(session.draft.as_ref().unwrap().number, None);
        session.draft = None;

        app.tool = Tool::Rectangle;
        session.begin(&app, Point::new(40., 40.));
        session.motion(&app, Point::new(80., 80.));
        session.finish(&app);
        assert_eq!(session.document.objects[1].number, Some(2));

        session.document.clear();
        session.begin(&app, Point::new(50., 50.));
        session.motion(&app, Point::new(90., 90.));
        session.finish(&app);
        assert_eq!(session.document.objects[0].number, Some(1));
    }

    #[test]
    fn pointer_trail_is_temporary_and_not_added_to_history() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Pointer;
        let mut session = Session::default();
        let started = Instant::now();
        session.begin(&app, Point::new(10., 10.));
        session.motion(&app, Point::new(50., 10.));
        session.finish(&app);
        assert!(session.pointer_trail.len() > 2);
        assert!(session.document.objects.is_empty());
        assert!(
            session.animate_pointer(started + POINTER_TRAIL_LIFETIME + Duration::from_millis(20))
        );
        assert!(!session.pointer_active());
    }

    #[test]
    fn select_drag_and_delete_multiline_text_on_second_line() {
        let mut app = App::new(Settings::default());
        let mut session = Session::default();
        session.document.add(Annotation {
            tool: Tool::Text,
            points: vec![Point::new(10., 10.)],
            color: [255, 255, 255, 255],
            width: 2.,
            filled: false,
            text: "Baris 1\nBaris 2".into(),
            font_size: 20.,
            image: None,
            number: None,
        });
        // Select tool clicking on line 2 (y in [38, 66])
        app.tool = Tool::Select;
        session.begin(&app, Point::new(20., 50.));
        assert_eq!(session.selected, Some(0));

        // Drag the text by moving the mouse
        session.motion(&app, Point::new(30., 60.));
        session.finish(&app);
        assert_eq!(session.document.objects[0].points[0], Point::new(20., 20.));

        // Delete the selected text
        if let Some(selected) = session.selected.take() {
            session.document.delete(selected);
        }
        assert!(session.document.objects.is_empty());
    }
}
