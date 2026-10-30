use crate::{
    app::App,
    canvas::{Annotation, Point, Tool},
};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const WIDTH: f32 = 682.0;
pub const HEIGHT: f32 = 88.0;
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
        x: 548.,
        y: 16.,
        w: 56.,
        h: 56.,
        action: Action::Desktop,
        icon: "desktop",
    });
    buttons.push(Button {
        x: 626.,
        y: 16.,
        w: 40.,
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
            Action::Tool(Tool::Eraser) => [255, 177, 192, 255],
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
                    &[(-8., 3.), (2., -8.), (8., -2.), (-2., 8.), (-5., 8.)],
                    [7, 17, 21, 255],
                    1.,
                    it,
                    true,
                );
                lines(
                    pix,
                    &[(-4., -1.), (3., 6.)],
                    [255, 177, 192, 255],
                    1.,
                    it,
                    false,
                );
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
                lines(
                    pix,
                    &[(-8., 6.), (-4., 3.), (-1., 4.), (2., 0.)],
                    [255, 48, 82, 120],
                    3.,
                    it,
                    false,
                );
                if let Some(glow) = PathBuilder::from_circle(5., -4., 6.) {
                    pix.fill_path(
                        &glow,
                        &paint([255, 48, 82, 70]),
                        FillRule::Winding,
                        it,
                        None,
                    );
                }
                if let Some(dot) = PathBuilder::from_circle(5., -4., 3.) {
                    pix.fill_path(
                        &dot,
                        &paint([255, 80, 105, 255]),
                        FillRule::Winding,
                        it,
                        None,
                    );
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
        for row in 0..6 {
            for col in 0..24 {
                if let Some(r) = tiny_skia::Rect::from_xywh(
                    19. + col as f32 * 10.,
                    94. + row as f32 * 10.,
                    10.,
                    10.,
                ) {
                    pix.fill_rect(
                        r,
                        &paint(hsv(col as f32 / 24., (row + 1) as f32 / 6.)),
                        t,
                        None,
                    );
                }
            }
        }
    }
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
