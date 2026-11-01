use super::{Annotation, Document, Point, Tool};
use crate::app::App;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const POINTER_TRAIL_LIFETIME: Duration = Duration::from_millis(900);

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
    pub pointer_trail: VecDeque<PointerTrailPoint>,
    pointer_stroke: u64,
    erase_points: Vec<Point>,
    move_origin: Option<Point>,
    move_last: Option<Point>,
}
impl Session {
    pub fn begin(&mut self, app: &App, p: Point) {
        match app.tool {
            Tool::Eraser => self.erase_points.push(p),
            Tool::Select => {
                let v = &mut *self;
                v.selected = v.document.objects.iter().rposition(|a| a.hit(p, 5.));
                v.move_origin = Some(p);
                v.move_last = Some(p);
            }
            Tool::Pointer => {
                self.pointer_stroke = self.pointer_stroke.wrapping_add(1);
                self.push_pointer_point(p, Instant::now());
            }
            _ => self.draft = Some(app.annotation(p)),
        }
    }
    pub fn motion(&mut self, app: &App, p: Point) {
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
            if let (Some(selected), Some(last)) = (v.selected, v.move_last) {
                v.document
                    .translate(selected, Point::new(p.x - last.x, p.y - last.y));
            }
            v.move_last = Some(p);
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
        if let (Some(selected), Some(origin), Some(last)) =
            (v.selected, v.move_origin.take(), v.move_last.take())
        {
            v.document
                .record_move(selected, Point::new(last.x - origin.x, last.y - origin.y));
        }
        if !v.erase_points.is_empty() {
            v.document.erase(&v.erase_points, app.width * 2.);
            v.erase_points.clear();
        }
        if v.draft.as_ref().is_some_and(|a| a.tool != Tool::Text)
            && let Some(a) = v.draft.take()
        {
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
