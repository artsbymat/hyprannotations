use crate::{
    canvas::{Annotation, Point, Tool},
    config::Settings,
};

pub struct App {
    pub settings: Settings,
    pub tool: Tool,
    pub color: [u8; 4],
    pub width: f32,
    pub filled: bool,
    pub drawing: bool,
    pub palette: crate::config::palette::Palette,
    pub exit: bool,
    pub color_picker: bool,
    pub help_visible: bool,
}
impl App {
    pub fn supports_fill(&self) -> bool {
        self.drawing && matches!(self.tool, Tool::Rectangle | Tool::Ellipse | Tool::Text)
    }

    pub fn toggle_fill(&mut self) {
        if self.supports_fill() {
            self.filled = !self.filled;
        }
    }

    pub fn new(settings: Settings) -> Self {
        Self {
            tool: settings.default_tool,
            color: settings.color,
            width: settings.stroke_width,
            settings,
            filled: false,
            drawing: true,
            palette: crate::config::palette::Palette::default(),
            exit: false,
            color_picker: false,
            help_visible: false,
        }
    }
    pub fn load_palette(&mut self) {
        let path = Settings::path().with_file_name("palette.toml");
        if path.exists() {
            match crate::config::palette::Palette::load(&path) {
                Ok(palette) => {
                    self.color = palette.colors[palette.selected];
                    self.palette = palette;
                }
                Err(error) => eprintln!("Cannot load palette: {error}"),
            }
        } else {
            self.palette.colors[0] = self.color;
        }
    }
    pub fn select_color(&mut self, index: usize) {
        if index < self.palette.colors.len() {
            self.palette.selected = index;
            self.color = self.palette.colors[index];
        }
    }
    pub fn replace_color(&mut self, color: [u8; 4]) {
        self.color = color;
        self.palette.colors[self.palette.selected] = color;
        self.color_picker = false;
    }
    pub fn save_palette(&self) {
        if let Err(error) = self
            .palette
            .save(&Settings::path().with_file_name("palette.toml"))
        {
            eprintln!("Cannot save palette: {error}");
        }
    }
    pub fn annotation(&self, point: Point) -> Annotation {
        let mut color = self.color;
        if self.tool == Tool::Highlighter {
            color[3] = (self.settings.highlighter_opacity * 255.0) as u8;
        }
        Annotation {
            tool: self.tool,
            points: vec![point],
            color,
            width: if self.tool == Tool::Highlighter {
                self.width * 4.0
            } else {
                self.width
            },
            filled: self.supports_fill() && self.filled,
            text: String::new(),
            font_size: self.settings.font_size,
            image: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_a_slot_does_not_overwrite_other_slots() {
        let mut app = App::new(Settings::default());
        let original = app.palette.colors;
        app.select_color(2);
        app.replace_color([1, 2, 3, 255]);
        app.select_color(4);
        app.replace_color([4, 5, 6, 255]);
        app.select_color(2);
        assert_eq!(app.color, [1, 2, 3, 255]);
        assert_eq!(app.palette.colors[4], [4, 5, 6, 255]);
        for i in [0, 1, 3, 5] {
            assert_eq!(app.palette.colors[i], original[i]);
        }
    }

    #[test]
    fn fill_toggles_for_shape_and_text_tools() {
        let mut app = App::new(Settings::default());
        app.tool = Tool::Pen;
        app.toggle_fill();
        assert!(!app.filled);

        app.tool = Tool::Rectangle;
        app.toggle_fill();
        assert!(app.filled);

        app.tool = Tool::Text;
        app.toggle_fill();
        assert!(!app.filled);
        app.toggle_fill();
        assert!(app.filled);
        assert!(app.annotation(Point::new(0., 0.)).filled);

        app.tool = Tool::Ellipse;
        app.toggle_fill();
        assert!(!app.filled);
    }
}
