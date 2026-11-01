use super::*;

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(k) => self.keyboard = Some(k),
                Err(e) => eprintln!("Keyboard: {e}"),
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(p) => {
                    self.cursor_device = self
                        .cursor_manager
                        .as_ref()
                        .map(|m| m.get_pointer(&p, qh, ()));
                    self.pointer = Some(p);
                }
                Err(e) => eprintln!("Pointer: {e}"),
            }
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
        if capability == Capability::Pointer {
            if let Some(device) = self.cursor_device.take() {
                device.destroy();
            }
            if let Some(p) = self.pointer.take() {
                p.release();
            }
            self.pressed = false;
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.focused = self
            .views
            .iter()
            .position(|v| v.layer.wl_surface() == surface);
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.focused = None;
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            let Some(index) = self
                .views
                .iter()
                .position(|v| v.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            let p = Point::new(event.position.0 as f32, event.position.1 as f32);
            let (tx, ty, ts) = self.toolbar(index);
            let ux = (p.x - tx) / ts;
            let uy = (p.y - ty) / ts;
            if matches!(event.kind, Enter { .. } | Motion { .. }) {
                let hovered = ui::inside_background(&self.app, ux, uy)
                    .then(|| ui::hit(&self.app, ux, uy))
                    .flatten();
                self.update_hover(index, hovered);
            } else if matches!(event.kind, Leave { .. }) {
                self.update_hover(index, None);
            }
            match event.kind {
                PointerEventKind::Leave { .. } => self.pointer_position = None,
                _ => self.pointer_position = Some((index, p)),
            }
            match event.kind {
                Enter { serial } => {
                    self.pointer_enter_serial = Some(serial);
                    self.update_cursor(index, p);
                }
                Leave { .. } => {
                    self.pointer_enter_serial = None;
                    self.finish_gesture(index);
                }
                Press { button: 0x111, .. } => {
                    self.app.cancel_exit_request();
                    if let Some(Action::Color(slot)) = ui::hit(&self.app, ux, uy) {
                        self.app.select_color(slot);
                        self.app.color_picker = true;
                        self.dirty_all();
                    }
                }
                Press { button: 0x110, .. } => {
                    self.focused = Some(index);
                    if ui::hit(&self.app, ux, uy) != Some(Action::Exit) {
                        self.app.cancel_exit_request();
                    }
                    if self.app.drawing
                        && self.app.color_picker
                        && let Some(color) = ui::picker_color(ux, uy)
                    {
                        self.app.replace_color(color);
                        self.app.save_palette();
                        self.app.color_picker = false;
                        self.dirty_all();
                        continue;
                    }
                    if ui::inside_background(&self.app, ux, uy) {
                        if let Some(action) = ui::hit(&self.app, ux, uy) {
                            self.action(index, action);
                            self.update_cursor(index, p);
                        } else {
                            let (x, y, _) = self.toolbar(index);
                            self.drag_origin = Some((p, [x, y]));
                            self.update_cursor(index, p);
                        }
                        continue;
                    }
                    self.commit_text(index);
                    if !self.app.drawing {
                        continue;
                    }
                    self.pressed = true;
                    self.views[index]
                        .session
                        .begin_additive(&self.app, p, self.modifiers.ctrl);
                    self.views[index].dirty = true;
                    if self.app.tool == Tool::Text {
                        self.reset_caret_blink();
                    }
                    self.update_cursor(index, p);
                }
                Motion { .. } => {
                    self.update_cursor(index, p);
                    if let Some((origin, pos)) = self.drag_origin {
                        let (_, _, scale) = self.toolbar(index);
                        let view = &self.views[index];
                        self.app.settings.toolbar_position = ui::clamp_toolbar_position(
                            &self.app,
                            [pos[0] + p.x - origin.x, pos[1] + p.y - origin.y],
                            view.width as f32,
                            view.height as f32,
                            scale,
                        );
                        self.dirty_all();
                        self.update_cursor(index, p);
                        continue;
                    }
                    if !self.pressed {
                        continue;
                    }
                    self.views[index]
                        .session
                        .motion_free(&self.app, p, self.modifiers.shift);
                    self.views[index].dirty = true;
                    self.update_cursor(index, p);
                }
                Release { button: 0x110, .. } => {
                    self.finish_gesture(index);
                    self.drag_origin = None;
                    self.update_cursor(index, p);
                }
                Axis { vertical, .. } => {
                    self.app.width =
                        (self.app.width - vertical.absolute as f32 / 10.).clamp(1., 80.);
                    self.dirty_all();
                }
                _ => {}
            }
        }
    }
}

impl State {
    fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            message: message.into(),
            error,
            until: std::time::Instant::now() + std::time::Duration::from_secs(2),
        });
        self.dirty_all();
    }

    fn update_hover(&mut self, view: usize, action: Option<Action>) {
        let unchanged = self
            .hover_tip
            .as_ref()
            .is_some_and(|hover| hover.view == view && Some(hover.action) == action);
        if unchanged {
            return;
        }
        let was_visible = self.hover_tip.as_ref().is_some_and(|hover| hover.visible);
        self.hover_tip = action.map(|action| HoverTip {
            view,
            action,
            started: std::time::Instant::now(),
            visible: false,
        });
        if was_visible {
            self.dirty_all();
        }
    }

    fn update_cursor(&self, index: usize, point: Point) {
        let (Some(device), Some(serial)) = (&self.cursor_device, self.pointer_enter_serial) else {
            return;
        };
        let shape = if self.drag_origin.is_some()
            || (self.pressed
                && self.app.tool == Tool::Select
                && self.views[index].session.selected.is_some())
        {
            Shape::Grabbing
        } else {
            let (toolbar_x, toolbar_y, toolbar_scale) = self.toolbar(index);
            let ui_x = (point.x - toolbar_x) / toolbar_scale;
            let ui_y = (point.y - toolbar_y) / toolbar_scale;
            let over_toolbar = ui::inside_background(&self.app, ui_x, ui_y);

            if over_toolbar {
                match ui::hit(&self.app, ui_x, ui_y) {
                    None => Shape::Grab,
                    Some(action) if !ui::enabled(action, &self.app) => Shape::NotAllowed,
                    Some(_) => Shape::Default,
                }
            } else if !self.app.drawing {
                Shape::Default
            } else if self.app.tool == Tool::Select
                && self.views[index].session.over_resize_handle(point)
            {
                Shape::Grab
            } else {
                match self.app.tool {
                    Tool::Text => Shape::Text,
                    Tool::Select
                        if self.views[index]
                            .session
                            .document
                            .objects
                            .iter()
                            .any(|annotation| annotation.hit(point, 5.)) =>
                    {
                        Shape::Grab
                    }
                    Tool::Select => Shape::Default,
                    _ => Shape::Crosshair,
                }
            }
        };
        device.set_shape(serial, shape);
    }

    fn request_screenshot(&mut self, index: usize) {
        if self.screenshot.is_some() {
            return;
        }
        self.commit_text(index);
        self.finish_gesture(index);
        self.screenshot = Some(ScreenshotRequest {
            view: index,
            stage: ScreenshotStage::HidingToolbar,
        });
        self.views[index].dirty = true;
    }

    pub(super) fn capture_ready_screenshot(&mut self) {
        let Some(request) = self.screenshot.as_ref() else {
            return;
        };
        if request.stage != ScreenshotStage::Ready {
            return;
        }
        let view_index = request.view;
        let result = self.capture_output_to_clipboard(view_index);
        self.screenshot = None;
        if let Some(view) = self.views.get_mut(view_index) {
            view.dirty = true;
        }
        match result {
            Ok(()) => self.notify("Screenshot copied to clipboard", false),
            Err(error) => {
                eprintln!("Copy screenshot: {error}");
                self.notify(format!("Screenshot failed: {error}"), true);
            }
        }
    }

    fn capture_output_to_clipboard(&mut self, index: usize) -> anyhow::Result<()> {
        let view = self
            .views
            .get(index)
            .context("Screenshot output disappeared")?;
        let output_name = self
            .output_state
            .info(&view.output)
            .and_then(|info| info.name)
            .context("Compositor did not provide the output name")?;
        let screenshot = std::process::Command::new("grim")
            .args(["-t", "ppm", "-o", &output_name, "-"])
            .output()
            .context("Could not run grim; install grim to use Ctrl+C screenshots")?;
        anyhow::ensure!(
            screenshot.status.success(),
            "grim failed: {}",
            String::from_utf8_lossy(&screenshot.stderr).trim()
        );
        let (width, height, rgba) = decode_ppm(&screenshot.stdout)?;
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|error| anyhow::anyhow!("Cannot access clipboard: {error}"))?;
        clipboard
            .set_image(arboard::ImageData {
                width,
                height,
                bytes: std::borrow::Cow::Owned(rgba),
            })
            .map_err(|error| anyhow::anyhow!("Cannot copy screenshot: {error}"))?;
        // Keep the Wayland selection owner alive so other apps can paste the image.
        self.clipboard = Some(clipboard);
        Ok(())
    }

    fn paste_clipboard_image(&mut self, index: usize) {
        self.commit_text(index);
        let result = (|| -> anyhow::Result<Annotation> {
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|error| anyhow::anyhow!("Cannot access clipboard: {error}"))?;
            let image = clipboard.get_image().map_err(|error| {
                anyhow::anyhow!("Clipboard does not contain a supported image: {error}")
            })?;
            let image_width = u32::try_from(image.width)?;
            let image_height = u32::try_from(image.height)?;
            let expected = image
                .width
                .checked_mul(image.height)
                .and_then(|pixels| pixels.checked_mul(4))
                .context("Clipboard image is too large")?;
            anyhow::ensure!(
                image.bytes.len() == expected,
                "Clipboard returned invalid image data"
            );

            let mut pixels = tiny_skia::Pixmap::new(image_width, image_height)
                .context("Clipboard image dimensions are invalid or too large")?;
            pixels.data_mut().copy_from_slice(&image.bytes);
            // arboard returns straight-alpha RGBA; tiny-skia expects premultiplied RGBA.
            for pixel in pixels.data_mut().as_chunks_mut::<4>().0 {
                let alpha = pixel[3] as u16;
                pixel[0] = ((pixel[0] as u16 * alpha + 127) / 255) as u8;
                pixel[1] = ((pixel[1] as u16 * alpha + 127) / 255) as u8;
                pixel[2] = ((pixel[2] as u16 * alpha + 127) / 255) as u8;
            }

            let view = &self.views[index];
            let max_width = view.width as f32 * 0.7;
            let max_height = view.height as f32 * 0.7;
            let fit = (max_width / image_width as f32)
                .min(max_height / image_height as f32)
                .min(1.0);
            let width = image_width as f32 * fit;
            let height = image_height as f32 * fit;
            let preferred = self
                .pointer_position
                .filter(|(view_index, _)| *view_index == index)
                .map(|(_, point)| point)
                .unwrap_or_else(|| Point::new(view.width as f32 / 2.0, view.height as f32 / 2.0));
            let x = (preferred.x - width / 2.0).clamp(0.0, (view.width as f32 - width).max(0.0));
            let y = (preferred.y - height / 2.0).clamp(0.0, (view.height as f32 - height).max(0.0));

            Ok(Annotation {
                tool: Tool::Image,
                points: vec![Point::new(x, y), Point::new(x + width, y + height)],
                color: [255, 255, 255, 255],
                width: 1.0,
                filled: false,
                text: String::new(),
                font_size: 12.0,
                image: Some(std::sync::Arc::new(pixels)),
                number: None,
            })
        })();

        match result {
            Ok(mut annotation) => {
                let view = &mut self.views[index];
                if self.app.auto_number {
                    annotation.number = Some(view.session.document.next_number());
                }
                view.session.document.add(annotation);
                view.session.set_selection(
                    view.session
                        .document
                        .objects
                        .len()
                        .checked_sub(1)
                        .into_iter()
                        .collect(),
                );
                view.dirty = true;
            }
            Err(error) => {
                eprintln!("Paste image: {error}");
                self.notify("Clipboard does not contain a supported image", true);
            }
        }
    }

    pub(super) fn commit_text(&mut self, index: usize) {
        let v = &mut self.views[index];
        if v.session
            .draft
            .as_ref()
            .is_some_and(|a| a.tool == Tool::Text)
        {
            if let Some(a) = v.session.draft.take()
                && !a.text.is_empty()
            {
                v.session.document.add(a);
            }
            v.dirty = true;
        }
    }
}

impl State {
    pub(super) fn action(&mut self, index: usize, action: Action) {
        if !ui::enabled(action, &self.app) {
            return;
        }
        if action != Action::Exit {
            self.app.cancel_exit_request();
        }
        // Toggling the text background must not end the active text edit.
        if !matches!(action, Action::Fill) {
            self.commit_text(index);
        }
        if matches!(action, Action::Undo | Action::Redo | Action::Clear) {
            self.views[index].session.clear_selection();
        }
        match action {
            Action::Tool(t) => {
                self.app.tool = t;
                self.app.drawing = true;
            }
            Action::Color(c) => {
                self.app.select_color(c);
                self.app.save_palette();
            }
            Action::Picker => self.app.color_picker = !self.app.color_picker,
            Action::Width => {
                self.app.width = if self.app.width >= 16. {
                    2.
                } else {
                    self.app.width + 2.
                }
            }
            Action::Fill => {
                self.app.toggle_fill();
                if let Some(draft) = &mut self.views[index].session.draft
                    && draft.tool == Tool::Text
                {
                    draft.filled = self.app.filled;
                    draft.color = self.app.color;
                }
            }
            Action::Numbering => self.app.auto_number = !self.app.auto_number,
            Action::Undo => self.views[index].session.document.undo(),
            Action::Redo => self.views[index].session.document.redo(),
            Action::Clear => self.views[index].session.document.clear(),
            Action::Delete => {
                self.views[index].session.delete_selected();
            }
            Action::Desktop => {
                self.finish_gesture(index);
                for i in 0..self.views.len() {
                    self.commit_text(i);
                }
                self.app.drawing = !self.app.drawing;
                self.app.color_picker = false;
                self.app.help_visible = false;
                self.drag_origin = None;
                self.modifiers = Modifiers::default();
            }
            Action::Exit => {
                if !self.app.request_exit() {
                    self.notify("Press Escape or click Exit again to close", false);
                }
            }
        }
        self.dirty_all();
    }
}

impl State {
    pub(super) fn key(&mut self, event: KeyEvent) {
        if !self.app.drawing {
            return;
        }
        let Some(index) = self.focused.filter(|i| *i < self.views.len()) else {
            return;
        };
        if event.keysym != Keysym::Escape {
            self.app.cancel_exit_request();
        }
        if event.keysym == Keysym::F1 {
            self.app.help_visible = !self.app.help_visible;
            self.app.color_picker = false;
            self.dirty_all();
            return;
        }
        if self.app.help_visible {
            if event.keysym == Keysym::Escape {
                self.app.help_visible = false;
                self.dirty_all();
            }
            return;
        }
        if self.modifiers.ctrl
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'v' || key == 'V')
        {
            self.paste_clipboard_image(index);
            return;
        }
        if self.modifiers.ctrl
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'h' || key == 'H')
        {
            if let Some((view, point)) = self.pointer_position {
                self.cursor_highlights.push(CursorHighlight {
                    view,
                    point,
                    started: std::time::Instant::now(),
                });
                self.views[view].dirty = true;
            }
            return;
        }
        if self.modifiers.ctrl
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'r' || key == 'R')
        {
            self.views[index].session.rotate_selected_images();
            self.views[index].dirty = true;
            return;
        }
        if !self.modifiers.ctrl
            && !self.modifiers.alt
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'c' || key == 'C')
            && self.app.tool == Tool::Select
        {
            let enabled = self.views[index].session.toggle_crop_mode();
            self.views[index].dirty = true;
            match enabled {
                Some(true) => self.notify("Crop mode enabled — drag a corner handle", false),
                Some(false) => self.notify("Crop mode disabled", false),
                None => self.notify("Select a pasted image before cropping", true),
            }
            return;
        }
        if self.modifiers.ctrl
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'c' || key == 'C')
        {
            self.request_screenshot(index);
            return;
        }
        if self.modifiers.ctrl
            && event
                .keysym
                .key_char()
                .is_some_and(|key| key == 'd' || key == 'D')
        {
            self.views[index].session.duplicate_selected();
            self.views[index].dirty = true;
            return;
        }
        if self.modifiers.ctrl
            && let Some(key) = event.keysym.key_char()
            && matches!(key, '[' | ']')
        {
            self.views[index].session.reorder_selected(key == ']');
            self.views[index].dirty = true;
            return;
        }
        if self.views[index]
            .session
            .draft
            .as_ref()
            .is_some_and(|a| a.tool == Tool::Text)
        {
            if event.keysym == Keysym::Escape {
                self.views[index].session.draft = None;
            } else if event.keysym == Keysym::Return && !self.modifiers.shift {
                self.commit_text(index);
            } else if let Some(a) = &mut self.views[index].session.draft {
                if event.keysym == Keysym::BackSpace {
                    a.text.pop();
                } else if event.keysym == Keysym::Return {
                    a.text.push('\n');
                } else if !self.modifiers.ctrl
                    && !self.modifiers.alt
                    && let Some(s) = event.utf8
                {
                    a.text.push_str(&s);
                }
            }
            self.views[index].dirty = true;
            self.reset_caret_blink();
            return;
        }
        let base = if event.keysym == Keysym::Escape {
            "Escape".into()
        } else if event.keysym == Keysym::Tab {
            "Tab".into()
        } else if event.keysym == Keysym::Delete {
            "Delete".into()
        } else {
            event
                .keysym
                .key_char()
                .map(|c| c.to_ascii_lowercase().to_string())
                .unwrap_or_default()
        };
        let key = format!(
            "{}{}{}{}",
            if self.modifiers.ctrl { "ctrl+" } else { "" },
            if self.modifiers.shift { "shift+" } else { "" },
            if self.modifiers.alt { "alt+" } else { "" },
            base
        );
        let action = self.app.settings.shortcuts.get(&key).cloned();
        if let Some(action) = action {
            let action = match action.as_str() {
                "desktop" => Action::Desktop,
                "select" => Action::Tool(Tool::Select),
                "pen" => Action::Tool(Tool::Pen),
                "highlighter" => Action::Tool(Tool::Highlighter),
                "eraser" => Action::Tool(Tool::Eraser),
                "line" => Action::Tool(Tool::Line),
                "arrow" => Action::Tool(Tool::Arrow),
                "rectangle" => Action::Tool(Tool::Rectangle),
                "ellipse" => Action::Tool(Tool::Ellipse),
                "text" => Action::Tool(Tool::Text),
                "pointer" => Action::Tool(Tool::Pointer),
                "undo" => Action::Undo,
                "redo" => Action::Redo,
                "clear" => Action::Clear,
                "delete" => Action::Delete,
                "exit" => Action::Exit,
                _ => return,
            };
            self.action(index, action);
        }
    }
}

impl State {
    const CARET_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    pub(super) fn animation_timeout(&self) -> Option<std::time::Duration> {
        let mut timeout = self
            .has_text_draft()
            .then(|| Self::CARET_INTERVAL.saturating_sub(self.caret_last_toggle.elapsed()));
        let mut include = |duration| {
            timeout = Some(timeout.map_or(duration, |current| current.min(duration)));
        };
        if self.views.iter().any(|view| view.session.pointer_active())
            || !self.cursor_highlights.is_empty()
        {
            include(std::time::Duration::from_millis(16));
        }
        if let Some(toast) = &self.toast {
            include(
                toast
                    .until
                    .saturating_duration_since(std::time::Instant::now()),
            );
        }
        if let Some(hover) = &self.hover_tip
            && !hover.visible
        {
            include(std::time::Duration::from_millis(600).saturating_sub(hover.started.elapsed()));
        }
        timeout
    }

    pub(super) fn update_animations(&mut self) {
        self.update_caret_blink();
        let now = std::time::Instant::now();
        for view in &mut self.views {
            if view.session.animate_pointer(now) {
                view.dirty = true;
            }
        }
        if self.toast.as_ref().is_some_and(|toast| now >= toast.until) {
            self.toast = None;
            self.dirty_all();
        }
        if let Some(hover) = &mut self.hover_tip
            && !hover.visible
            && hover.started.elapsed() >= std::time::Duration::from_millis(600)
        {
            hover.visible = true;
            if let Some(view) = self.views.get_mut(hover.view) {
                view.dirty = true;
            }
        }
        let mut dirty_views = std::collections::BTreeSet::new();
        self.cursor_highlights.retain(|highlight| {
            let active = now.saturating_duration_since(highlight.started)
                < std::time::Duration::from_millis(900);
            dirty_views.insert(highlight.view);
            active
        });
        for view in dirty_views {
            if let Some(view) = self.views.get_mut(view) {
                view.dirty = true;
            }
        }
    }

    fn update_caret_blink(&mut self) {
        if !self.has_text_draft() || self.caret_last_toggle.elapsed() < Self::CARET_INTERVAL {
            return;
        }
        self.caret_visible = !self.caret_visible;
        self.caret_last_toggle = std::time::Instant::now();
        for view in &mut self.views {
            if view
                .session
                .draft
                .as_ref()
                .is_some_and(|draft| draft.tool == Tool::Text)
            {
                view.dirty = true;
            }
        }
    }

    fn has_text_draft(&self) -> bool {
        self.views.iter().any(|view| {
            view.session
                .draft
                .as_ref()
                .is_some_and(|draft| draft.tool == Tool::Text)
        })
    }

    fn reset_caret_blink(&mut self) {
        self.caret_visible = true;
        self.caret_last_toggle = std::time::Instant::now();
        for view in &mut self.views {
            if view
                .session
                .draft
                .as_ref()
                .is_some_and(|draft| draft.tool == Tool::Text)
            {
                view.dirty = true;
            }
        }
    }

    pub(super) fn finish_gesture(&mut self, index: usize) {
        if !self.pressed {
            return;
        }
        self.pressed = false;
        self.views[index].session.finish(&self.app);
        self.views[index].dirty = true;
    }
}
