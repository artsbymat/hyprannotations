use crate::{
    app::App,
    canvas::{Annotation, Point, Tool},
};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const WIDTH: f32 = 682.0;
pub const HEIGHT: f32 = 88.0;
pub const DESKTOP_PADDING: f32 = 8.0;
pub const DESKTOP_PANEL_X: f32 = 542.0 - DESKTOP_PADDING;
pub const DESKTOP_PANEL_Y: f32 = 16.0 - DESKTOP_PADDING;
pub const DESKTOP_PANEL_WIDTH: f32 = 610.0 + 56.0 + DESKTOP_PADDING - DESKTOP_PANEL_X;
pub const DESKTOP_PANEL_HEIGHT: f32 = 56.0 + DESKTOP_PADDING * 2.0;
pub const PICKER_X: f32 = 19.0;
pub const PICKER_Y: f32 = 94.0;
pub const PICKER_COLS: usize = 24;
pub const PICKER_ROWS: usize = 6;
pub const PICKER_CELL: f32 = 14.0;
pub const PICKER_WIDTH: f32 = PICKER_COLS as f32 * PICKER_CELL;
pub const PICKER_HEIGHT: f32 = PICKER_Y + PICKER_ROWS as f32 * PICKER_CELL;
pub const HELP_WIDTH: f32 = 700.0;
pub const HELP_HEIGHT: f32 = 520.0;
const HELP_ROW_HEIGHT: f32 = 34.0;
const HELP_TOOLS: &[(&str, &str)] = &[
    ("P", "Pen"),
    ("H", "Highlighter"),
    ("E", "Eraser"),
    ("L", "Line"),
    ("A", "Arrow"),
    ("R", "Rectangle"),
    ("O", "Ellipse"),
    ("T", "Text"),
    ("Q", "Laser pointer"),
    ("V", "Select object"),
];
const HELP_ACTIONS: &[(&str, &str)] = &[
    ("Delete", "Delete selection"),
    ("Ctrl + Z", "Undo"),
    ("Ctrl + Shift + Z", "Redo"),
    ("Tab", "Desktop mode"),
    ("Mouse wheel", "Change stroke width"),
    ("Shift + Enter", "New text line"),
    ("Enter", "Commit text"),
    ("Escape", "Cancel or exit"),
    ("F1", "Close this help"),
];
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
/// Visible toolbar background. In desktop mode only the two remaining buttons
/// and their padding are covered, so the rest of the surface is click-through.
pub fn toolbar_background(app: &App) -> (f32, f32, f32, f32) {
    if app.drawing {
        (0., 0., WIDTH, HEIGHT)
    } else {
        (
            DESKTOP_PANEL_X,
            DESKTOP_PANEL_Y,
            DESKTOP_PANEL_WIDTH,
            DESKTOP_PANEL_HEIGHT,
        )
    }
}

pub fn inside_background(app: &App, x: f32, y: f32) -> bool {
    let (bx, by, bw, bh) = toolbar_background(app);
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

/// Keep the visible background on screen. In desktop mode the compact panel is
/// offset inside the surface, so the toolbar origin may become negative.
pub fn clamp_toolbar_position(
    app: &App,
    position: [f32; 2],
    surface_width: f32,
    surface_height: f32,
    scale: f32,
) -> [f32; 2] {
    let (bx, by, bw, bh) = toolbar_background(app);
    let visible_height = if app.drawing { toolbar_height(app) } else { bh };
    let min_x = -bx * scale;
    let min_y = -by * scale;
    let max_x = (surface_width - (bx + bw) * scale).max(min_x);
    let max_y = (surface_height - (by + visible_height) * scale).max(min_y);
    [
        position[0].clamp(min_x, max_x),
        position[1].clamp(min_y, max_y),
    ]
}

pub fn toolbar_height(app: &App) -> f32 {
    if app.drawing && app.color_picker {
        PICKER_HEIGHT
    } else {
        HEIGHT
    }
}

fn buttons(app: &App) -> Vec<Button> {
    if !app.drawing {
        return vec![
            Button {
                x: 542.,
                y: 16.,
                w: 56.,
                h: 56.,
                action: Action::Desktop,
                icon: "desktop",
            },
            Button {
                x: 610.,
                y: 16.,
                w: 56.,
                h: 56.,
                action: Action::Exit,
                icon: "cross",
            },
        ];
    }
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
pub fn hit(app: &App, x: f32, y: f32) -> Option<Action> {
    buttons(app)
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

pub fn draw_help_background(pix: &mut Pixmap, width: f32, height: f32, scale: f32) {
    let panel_width = HELP_WIDTH.min(width - 32.0).max(280.0);
    let panel_height = HELP_HEIGHT.min(height - 32.0).max(280.0);
    let x = (width - panel_width) / 2.0;
    let y = (height - panel_height) / 2.0;
    let transform = Transform::from_scale(scale, scale);

    if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, width, height) {
        pix.fill_rect(rect, &paint([0, 0, 0, 125]), transform, None);
    }
    if let Some(path) = rounded(x, y, panel_width, panel_height, 18.0) {
        pix.fill_path(
            &path,
            &paint([6, 14, 18, 248]),
            FillRule::Winding,
            transform,
            None,
        );
        pix.stroke_path(
            &path,
            &paint([88, 145, 158, 255]),
            &Stroke {
                width: 1.5,
                ..Stroke::default()
            },
            transform,
            None,
        );
    }

    let card_y = y + 82.0;
    let card_height = panel_height - 134.0;
    let gap = 14.0;
    let card_width = (panel_width - 56.0 - gap) / 2.0;
    for card_x in [x + 28.0, x + 28.0 + card_width + gap] {
        if let Some(path) = rounded(card_x, card_y, card_width, card_height, 12.0) {
            pix.fill_path(
                &path,
                &paint([12, 26, 32, 245]),
                FillRule::Winding,
                transform,
                None,
            );
            pix.stroke_path(
                &path,
                &paint([36, 67, 78, 255]),
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                transform,
                None,
            );
        }
    }

    for (column, items) in [HELP_TOOLS, HELP_ACTIONS].into_iter().enumerate() {
        let card_x = x + 28.0 + column as f32 * (card_width + gap);
        for row in 0..items.len() {
            let key_x = card_x + 16.0;
            let key_y = card_y + 48.0 + row as f32 * HELP_ROW_HEIGHT;
            let key_width = if column == 0 { 72.0 } else { 130.0 };
            if let Some(path) = rounded(key_x, key_y, key_width, 25.0, 6.0) {
                pix.fill_path(
                    &path,
                    &paint([25, 48, 57, 255]),
                    FillRule::Winding,
                    transform,
                    None,
                );
                pix.stroke_path(
                    &path,
                    &paint([77, 122, 132, 255]),
                    &Stroke {
                        width: 1.0,
                        ..Stroke::default()
                    },
                    transform,
                    None,
                );
            }
        }
    }
}

pub fn help_text(width: f32, height: f32) -> Vec<Annotation> {
    let panel_width = HELP_WIDTH.min(width - 32.0).max(280.0);
    let panel_height = HELP_HEIGHT.min(height - 32.0).max(280.0);
    let panel_x = (width - panel_width) / 2.0;
    let panel_y = (height - panel_height) / 2.0;
    let card_width = (panel_width - 70.0) / 2.0;
    let annotation = |x, y, text: &str, font_size, color| Annotation {
        tool: Tool::Text,
        points: vec![Point::new(x, y)],
        color,
        width: 1.0,
        filled: false,
        text: text.into(),
        font_size,
    };
    let mut text = vec![
        annotation(
            panel_x + 28.0,
            panel_y + 24.0,
            "Keyboard Shortcuts",
            27.0,
            [158, 234, 190, 255],
        ),
        annotation(
            panel_x + 30.0,
            panel_y + panel_height - 34.0,
            "Press F1 or Escape to close",
            15.0,
            [126, 157, 166, 255],
        ),
    ];
    for (column, (title, items)) in [("DRAWING TOOLS", HELP_TOOLS), ("ACTIONS", HELP_ACTIONS)]
        .into_iter()
        .enumerate()
    {
        let card_x = panel_x + 28.0 + column as f32 * (card_width + 14.0);
        text.push(annotation(
            card_x + 16.0,
            panel_y + 98.0,
            title,
            14.0,
            [126, 181, 192, 255],
        ));
        for (row, (key, label)) in items.iter().enumerate() {
            let row_y = panel_y + 137.0 + row as f32 * HELP_ROW_HEIGHT;
            text.push(annotation(
                card_x + 24.0,
                row_y,
                key,
                14.0,
                [226, 246, 239, 255],
            ));
            text.push(annotation(
                card_x + if column == 0 { 102.0 } else { 160.0 },
                row_y,
                label,
                15.0,
                [210, 221, 224, 255],
            ));
        }
    }
    text
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
    let (bx, by, bw, bh) = toolbar_background(app);
    if let Some(path) = rounded(bx, by, bw, bh, 12.) {
        pix.fill_path(&path, &paint([6, 14, 18, 245]), FillRule::Winding, t, None);
    }
    for b in buttons(app) {
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
        } else if matches!(b.action, Action::Desktop | Action::Exit) {
            [155, 238, 189, 255]
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
        let app = App::new(crate::config::Settings::default());
        let buttons = buttons(&app);
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
    fn desktop_mode_only_keeps_desktop_and_exit() {
        let mut app = App::new(crate::config::Settings::default());
        app.drawing = false;
        let actions: Vec<_> = buttons(&app).iter().map(|button| button.action).collect();
        assert_eq!(actions, vec![Action::Desktop, Action::Exit]);
        assert_eq!(toolbar_background(&app).2, DESKTOP_PANEL_WIDTH);
        assert_eq!(toolbar_height(&app), HEIGHT);
        // Buttons keep their drawing-mode positions.
        assert!(hit(&app, 542., 16.) == Some(Action::Desktop));
        assert!(hit(&app, 610., 16.) == Some(Action::Exit));
        // Only the padding around the buttons stays interactive for dragging.
        assert!(inside_background(
            &app,
            DESKTOP_PANEL_X + 1.,
            DESKTOP_PANEL_Y + 1.
        ));
        assert!(inside_background(&app, 534., 16.));
        assert!(!inside_background(&app, 160., 40.));
        assert!(!inside_background(&app, 533., 40.));
        assert!(!inside_background(&app, 674., 40.));
    }

    #[test]
    fn desktop_mode_can_reach_the_left_screen_edge() {
        let mut app = App::new(crate::config::Settings::default());
        app.drawing = false;
        let [x, y] = clamp_toolbar_position(&app, [-1000., -1000.], 1920., 1080., 1.);
        assert_eq!(x, -DESKTOP_PANEL_X);
        assert_eq!(y, -DESKTOP_PANEL_Y);
        // The visible panel now starts at the screen origin.
        assert_eq!(x + DESKTOP_PANEL_X, 0.);
        assert_eq!(y + DESKTOP_PANEL_Y, 0.);
    }

    #[test]
    fn returning_to_drawing_mode_shifts_the_toolbar_right() {
        let mut app = App::new(crate::config::Settings::default());
        app.drawing = false;
        let desktop_position = clamp_toolbar_position(&app, [-1000., 40.], 1920., 1080., 1.);

        app.drawing = true;
        let drawing_position = clamp_toolbar_position(&app, desktop_position, 1920., 1080., 1.);

        assert_eq!(drawing_position[0], 0.);
        assert!(drawing_position[0] > desktop_position[0]);
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
