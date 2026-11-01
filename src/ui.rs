use crate::{
    app::App,
    canvas::{Annotation, Point, Tool},
};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const WIDTH: f32 = 682.0;
pub const HEIGHT: f32 = 88.0;
pub const PICKER_X: f32 = 19.0;
pub const PICKER_Y: f32 = 94.0;
pub const PICKER_COLS: usize = 24;
pub const PICKER_ROWS: usize = 6;
pub const PICKER_CELL: f32 = 14.0;
pub const PICKER_WIDTH: f32 = PICKER_COLS as f32 * PICKER_CELL;
pub const PICKER_HEIGHT: f32 = PICKER_Y + PICKER_ROWS as f32 * PICKER_CELL;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    Color(usize),
    Picker,
    Width,
    Fill,
    Undo,
    Redo,
    Clear,
    Delete,
    Desktop,
    Exit,
}
#[derive(Clone, Copy)]
struct Button {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    action: Action,
    icon: &'static str,
}
fn buttons() -> Vec<Button> {
    let mut buttons = vec![Button {
        x: 16.,
        y: 16.,
        w: 56.,
        h: 56.,
        action: Action::Picker,
        icon: "",
    }];
    for i in 0..6 {
        buttons.push(Button {
            x: 80. + (i % 3) as f32 * 30.,
            y: 16. + (i / 3) as f32 * 30.,
            w: 24.,
            h: 26.,
            action: Action::Color(i),
            icon: "",
        });
    }
    buttons.push(Button {
        x: 180.,
        y: 16.,
        w: 48.,
        h: 56.,
        action: Action::Width,
        icon: "width",
    });
    for (i, (action, icon)) in [
        (Action::Tool(Tool::Pen), "pen"),
        (Action::Tool(Tool::Highlighter), "highlighter"),
        (Action::Tool(Tool::Line), "line"),
        (Action::Tool(Tool::Arrow), "arrow"),
        (Action::Tool(Tool::Rectangle), "rect"),
        (Action::Tool(Tool::Ellipse), "ellipse"),
        (Action::Tool(Tool::Text), "text"),
        (Action::Tool(Tool::Eraser), "eraser"),
        (Action::Tool(Tool::Pointer), "pointer"),
    ]
    .into_iter()
    .enumerate()
    {
        buttons.push(Button {
            x: 244. + (i % 5) as f32 * 34.,
            y: 16. + (i / 5) as f32 * 30.,
            w: 28.,
            h: 26.,
            action,
            icon,
        });
    }
    for (i, (action, icon)) in [
        (Action::Tool(Tool::Select), "select"),
        (Action::Fill, "fill"),
        (Action::Undo, "undo"),
        (Action::Redo, "redo"),
        (Action::Clear, "trash"),
    ]
    .into_iter()
    .enumerate()
    {
        buttons.push(Button {
            x: 430. + (i / 2) as f32 * 34.,
            y: if i == 4 {
                31.
            } else {
                16. + (i % 2) as f32 * 30.
            },
            w: 28.,
            h: 26.,
            action,
            icon,
        });
    }
    buttons.push(Button {
        x: 542.,
        y: 16.,
        w: 56.,
        h: 56.,
        action: Action::Desktop,
        icon: "desktop",
    });
    buttons.push(Button {
        x: 610.,
        y: 16.,
        w: 56.,
        h: 56.,
        action: Action::Exit,
        icon: "cross",
    });
    buttons
}
pub fn hit(x: f32, y: f32) -> Option<Action> {
    buttons()
        .iter()
        .find(|b| x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h)
        .map(|b| b.action)
}

pub fn enabled(action: Action, app: &App) -> bool {
    action != Action::Fill || app.supports_fill()
}
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.quad_to(x + w, y, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.quad_to(x + w, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.quad_to(x, y + h, x, y + h - r);
    p.line_to(x, y + r);
    p.quad_to(x, y, x + r, y);
    p.close();
    p.finish()
}
fn paint(color: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    p
}
fn lines(
    pix: &mut Pixmap,
    coords: &[(f32, f32)],
    color: [u8; 4],
    width: f32,
    t: Transform,
    closed: bool,
) {
    let mut path = PathBuilder::new();
    for (i, (x, y)) in coords.iter().enumerate() {
        if i == 0 {
            path.move_to(*x, *y)
        } else {
            path.line_to(*x, *y)
        }
    }
    if closed {
        path.close();
    }
    if let Some(path) = path.finish() {
        if closed {
            pix.fill_path(&path, &paint(color), FillRule::Winding, t, None);
        } else {
            pix.stroke_path(
                &path,
                &paint(color),
                &Stroke {
                    width,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Stroke::default()
                },
                t,
                None,
            );
        }
    }
}
pub fn draw(pix: &mut Pixmap, app: &App, x: f32, y: f32, scale: f32) {
    let t = Transform::from_scale(scale, scale).post_translate(x * scale, y * scale);
    if let Some(path) = rounded(0., 0., WIDTH, HEIGHT, 12.) {
        pix.fill_path(&path, &paint([6, 14, 18, 245]), FillRule::Winding, t, None);
    }
    for b in buttons() {
        let enabled = enabled(b.action, app);
        let selected = matches!(b.action,Action::Tool(tool) if app.tool==tool && app.drawing)
            || (b.action == Action::Fill && app.supports_fill() && app.filled)
            || matches!(b.action,Action::Color(i) if i==app.palette.selected)
            || (b.action == Action::Desktop && !app.drawing);
        let fill = match b.action {
            Action::Color(i) => app.palette.colors[i],
            Action::Picker => app.color,
            _ if !enabled => [8, 16, 20, 255],
            _ if selected => [40, 65, 67, 255],
            _ => [6, 15, 20, 255],
        };
        if let Some(path) = rounded(b.x, b.y, b.w, b.h, if b.w > 30. { 10. } else { 8. }) {
            pix.fill_path(&path, &paint(fill), FillRule::Winding, t, None);
            pix.stroke_path(
                &path,
                &paint(if !enabled {
                    [27, 43, 51, 255]
                } else if selected {
                    [158, 234, 190, 255]
                } else {
                    [48, 81, 99, 255]
                }),
                &Stroke {
                    width: if selected { 2. } else { 1.2 },
                    ..Stroke::default()
                },
                t,
                None,
            );
        }
        let color = if !enabled {
            [72, 84, 91, 255]
        } else if b.x < 320. {
            [255, 241, 177, 255]
        } else if b.x < 530. {
            [255, 177, 192, 255]
        } else if b.x < 760. {
            [155, 238, 189, 255]
        } else {
            [167, 222, 241, 255]
        };
        let cx = b.x + b.w / 2.;
        let cy = b.y + b.h / 2.;
        let it =
            Transform::from_scale(scale, scale).post_translate((x + cx) * scale, (y + cy) * scale);
        match b.icon {
            "width" => {
                if let Some(p) =
                    PathBuilder::from_circle(cx, cy, (8. + app.width / 2.).clamp(3., 20.))
                {
                    pix.fill_path(&p, &paint(color), FillRule::Winding, t, None);
                }
            }
            "eraser" => {
                lines(
                    pix,
                    &[(-7., 3.), (1., -6.), (7., 0.), (-1., 8.), (-7., 3.)],
                    color,
                    1.5,
                    it,
                    false,
                );
                lines(pix, &[(-3., -1.), (3., 5.)], color, 1.5, it, false);
            }
            "pen" => {
                lines(
                    pix,
                    &[
                        (-7., 7.),
                        (-5., 1.),
                        (4., -8.),
                        (8., -4.),
                        (-1., 5.),
                        (-7., 7.),
                    ],
                    color,
                    1.5,
                    it,
                    false,
                );
                lines(pix, &[(2., -6.), (6., -2.)], color, 1.5, it, false);
                lines(pix, &[(-7., 7.), (-5., 2.), (-2., 5.)], color, 1., it, true);
            }
            "highlighter" => {
                lines(
                    pix,
                    &[(-6., 2.), (2., -7.), (7., -2.), (-1., 7.), (-6., 2.)],
                    color,
                    1.5,
                    it,
                    false,
                );
                lines(pix, &[(-7., 7.), (-1., 7.)], color, 3., it, false);
            }
            "line" => lines(pix, &[(-6., -6.), (6., 6.)], color, 2., it, false),
            "undo" | "redo" => {
                let d = if b.icon == "undo" { 1. } else { -1. };
                let mut path = PathBuilder::new();
                path.move_to(6. * d, 6.);
                path.cubic_to(11. * d, -5., -3. * d, -9., -7. * d, -2.);
                if let Some(path) = path.finish() {
                    pix.stroke_path(
                        &path,
                        &paint(color),
                        &Stroke {
                            width: 2.,
                            line_cap: tiny_skia::LineCap::Round,
                            ..Stroke::default()
                        },
                        it,
                        None,
                    );
                }
                lines(
                    pix,
                    &[(-7. * d, -7.), (-7. * d, -2.), (-2. * d, -2.)],
                    color,
                    2.,
                    it,
                    false,
                );
            }
            "desktop" => {
                lines(
                    pix,
                    &[
                        (-15., -11.),
                        (15., -11.),
                        (15., 8.),
                        (-15., 8.),
                        (-15., -11.),
                    ],
                    color,
                    2.,
                    it,
                    false,
                );
                lines(
                    pix,
                    &[(0., 8.), (0., 14.), (-7., 14.), (7., 14.)],
                    color,
                    2.,
                    it,
                    false,
                );
            }
            "select" => lines(
                pix,
                &[
                    (-7., -9.),
                    (-7., 8.),
                    (-2., 3.),
                    (2., 9.),
                    (5., 7.),
                    (1., 1.),
                    (8., 1.),
                    (-7., -9.),
                ],
                color,
                1.5,
                it,
                false,
            ),
            "fill" => lines(
                pix,
                &[(-6., -6.), (6., -6.), (6., 6.), (-6., 6.)],
                color,
                1.,
                it,
                true,
            ),
            "rect" => {
                lines(
                    pix,
                    &[(-6., -5.), (6., -5.), (6., 5.), (-6., 5.), (-6., -5.)],
                    color,
                    2.,
                    it,
                    false,
                );
            }
            "ellipse" => {
                let a = Annotation {
                    tool: Tool::Ellipse,
                    points: vec![Point::new(-6., -4.), Point::new(6., 4.)],
                    color,
                    width: 2.,
                    filled: false,
                    text: String::new(),
                    font_size: 10.,
                };
                a.render_shape(pix, it);
            }
            "cross" => {
                lines(pix, &[(-5., -5.), (5., 5.)], color, 2., it, false);
                lines(pix, &[(5., -5.), (-5., 5.)], color, 2., it, false);
            }
            "text" => {
                lines(pix, &[(-6., -5.), (6., -5.)], color, 2., it, false);
                lines(pix, &[(0., -5.), (0., 6.)], color, 2., it, false);
            }
            "arrow" => {
                let d = 1.;
                lines(pix, &[(-6. * d, 1.), (5. * d, 1.)], color, 2.5, it, false);
                lines(
                    pix,
                    &[(1. * d, -4.), (6. * d, 1.), (1. * d, 6.)],
                    color,
                    2.5,
                    it,
                    false,
                );
            }
            "pointer" => {
                // Handheld laser pointer aimed up-right, with its beam and spot.
                lines(
                    pix,
                    &[
                        (-8.4, 5.6),
                        (-5.6, 8.4),
                        (2.4, 0.4),
                        (-0.4, -2.4),
                        (-8.4, 5.6),
                    ],
                    color,
                    1.5,
                    it,
                    true,
                );
                lines(pix, &[(2.4, 0.4), (5.2, -2.4)], color, 0.7, it, false);
                if let Some(glow) = PathBuilder::from_circle(7.6, -4.8, 1.6) {
                    pix.stroke_path(
                        &glow,
                        &paint([color[0], color[1], color[2], 100]),
                        &Stroke {
                            width: 1.,
                            ..Stroke::default()
                        },
                        it,
                        None,
                    );
                }
                if let Some(dot) = PathBuilder::from_circle(7.6, -4.8, 1.1) {
                    pix.fill_path(&dot, &paint(color), FillRule::Winding, it, None);
                }
            }
            "trash" => {
                lines(
                    pix,
                    &[(-5., -3.), (-4., 6.), (4., 6.), (5., -3.)],
                    color,
                    1.,
                    it,
                    true,
                );
                lines(pix, &[(-6., -5.), (6., -5.)], color, 2., it, false);
            }
            _ => {}
        }
    }
    if app.color_picker {
        for row in 0..PICKER_ROWS {
            for col in 0..PICKER_COLS {
                if let Some(r) = tiny_skia::Rect::from_xywh(
                    PICKER_X + col as f32 * PICKER_CELL,
                    PICKER_Y + row as f32 * PICKER_CELL,
                    PICKER_CELL,
                    PICKER_CELL,
                ) {
                    pix.fill_rect(
                        r,
                        &paint(hsv(
                            col as f32 / PICKER_COLS as f32,
                            (row + 1) as f32 / PICKER_ROWS as f32,
                        )),
                        t,
                        None,
                    );
                }
            }
        }
    }
}

pub fn picker_color(x: f32, y: f32) -> Option<[u8; 4]> {
    if !(PICKER_X..PICKER_X + PICKER_WIDTH).contains(&x) || !(PICKER_Y..PICKER_HEIGHT).contains(&y)
    {
        return None;
    }
    let col = ((x - PICKER_X) / PICKER_CELL).floor() as usize;
    let row = ((y - PICKER_Y) / PICKER_CELL).floor() as usize;
    Some(hsv(
        col as f32 / PICKER_COLS as f32,
        (row + 1) as f32 / PICKER_ROWS as f32,
    ))
}

pub fn hsv(h: f32, s: f32) -> [u8; 4] {
    let h = h * 6.;
    let f = h - h.floor();
    let p = 1. - s;
    let q = 1. - f * s;
    let v = 1. - (1. - f) * s;
    let (r, g, b) = match h as i32 % 6 {
        0 => (1., v, p),
        1 => (q, 1., p),
        2 => (p, 1., v),
        3 => (p, q, 1.),
        4 => (v, p, 1.),
        _ => (1., p, q),
    };
    [(r * 255.) as u8, (g * 255.) as u8, (b * 255.) as u8, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_and_exit_buttons_have_equal_size() {
        let buttons = buttons();
        let desktop = buttons
            .iter()
            .find(|button| button.action == Action::Desktop)
            .expect("desktop button");
        let exit = buttons
            .iter()
            .find(|button| button.action == Action::Exit)
            .expect("exit button");
        assert_eq!((desktop.w, desktop.h), (exit.w, exit.h));
        assert!(exit.x + exit.w <= WIDTH);
    }

    #[test]
    fn picker_geometry_stays_inside_toolbar_width() {
        assert!(PICKER_X + PICKER_WIDTH <= WIDTH);
        assert!(PICKER_HEIGHT > HEIGHT);
        assert_eq!(PICKER_WIDTH, 336.);
    }

    #[test]
    fn picker_color_maps_cells_to_distinct_colors() {
        let top_left = picker_color(PICKER_X + 1., PICKER_Y + 1.).unwrap();
        let next_column = picker_color(PICKER_X + PICKER_CELL + 1., PICKER_Y + 1.).unwrap();
        let next_row = picker_color(PICKER_X + 1., PICKER_Y + PICKER_CELL + 1.).unwrap();
        assert_ne!(top_left, next_column);
        assert_ne!(top_left, next_row);
        assert!(picker_color(PICKER_X - 1., PICKER_Y + 1.).is_none());
        assert!(picker_color(PICKER_X + PICKER_WIDTH, PICKER_Y + 1.).is_none());
        assert!(picker_color(PICKER_X + 1., PICKER_HEIGHT).is_none());
    }
}
