use super::*;

impl State {
    pub(super) fn toolbar(&self, index: usize) -> (f32, f32, f32) {
        let v = &self.views[index];
        let scale = ((v.width as f32 - 8.) / ui::WIDTH).clamp(0.2, 1.0);
        let [x, y] = ui::clamp_toolbar_position(
            &self.app,
            self.app.settings.toolbar_position,
            v.width as f32,
            v.height as f32,
            scale,
        );
        (x, y, scale)
    }
}

impl State {
    pub(super) fn redraw(&mut self, _qh: &QueueHandle<Self>) -> Result<()> {
        for i in 0..self.views.len() {
            self.sync_input_region(i)?;
            if !self.views[i].dirty || self.views[i].pending || !self.views[i].configured {
                continue;
            }
            let (tx, ty, ts) = self.toolbar(i);
            let v = &mut self.views[i];
            let width = (v.width as f32 * v.scale).ceil() as u32;
            let height = (v.height as f32 * v.scale).ceil() as u32;
            if v.pixels
                .as_ref()
                .is_none_or(|p| p.width() != width || p.height() != height)
            {
                v.pixels = Some(
                    tiny_skia::Pixmap::new(width, height)
                        .context("Cannot allocate output buffer")?,
                );
            }
            let pix = v.pixels.as_mut().context("Missing render buffer")?;
            pix.fill(tiny_skia::Color::TRANSPARENT);
            {
                for a in &v.session.document.objects {
                    self.renderer.draw(pix, a, v.scale);
                }
                self.renderer.draw_pointer_trail(
                    pix,
                    &v.session.pointer_trail,
                    v.scale,
                    std::time::Instant::now(),
                );
                let now = std::time::Instant::now();
                for highlight in self
                    .cursor_highlights
                    .iter()
                    .filter(|highlight| highlight.view == i)
                {
                    let opacity = 1.0
                        - now
                            .saturating_duration_since(highlight.started)
                            .as_secs_f32()
                            / 0.9;
                    ui::draw_cursor_highlight(pix, highlight.point, v.scale, opacity);
                }
                if let Some(a) = &v.session.draft {
                    if a.tool == Tool::Text && self.caret_visible {
                        self.renderer.draw_editing(pix, a, v.scale);
                    } else {
                        self.renderer.draw(pix, a, v.scale);
                    }
                }
                if self.app.drawing && self.app.tool == Tool::Select && self.screenshot.is_none() {
                    for selected in v.session.selected_indices() {
                        let Some(a) = v.session.document.objects.get(selected) else {
                            continue;
                        };
                        let Some(r) = a.bounds() else { continue };
                        let marker = Annotation {
                            tool: Tool::Rectangle,
                            points: vec![
                                Point::new(r.left() - 5., r.top() - 5.),
                                Point::new(r.right() + 5., r.bottom() + 5.),
                            ],
                            color: [155, 216, 244, 200],
                            width: 1.,
                            filled: false,
                            text: String::new(),
                            font_size: 12.,
                            image: None,
                            number: None,
                        };
                        self.renderer.draw(pix, &marker, v.scale);
                        if v.session.selected == Some(selected)
                            && matches!(a.tool, Tool::Rectangle | Tool::Ellipse | Tool::Image)
                        {
                            for point in [
                                Point::new(r.left(), r.top()),
                                Point::new(r.right(), r.top()),
                                Point::new(r.left(), r.bottom()),
                                Point::new(r.right(), r.bottom()),
                            ] {
                                let handle = Annotation {
                                    tool: Tool::Rectangle,
                                    points: vec![
                                        Point::new(point.x - 4.5, point.y - 4.5),
                                        Point::new(point.x + 4.5, point.y + 4.5),
                                    ],
                                    color: if v.session.crop_mode {
                                        [255, 177, 92, 255]
                                    } else {
                                        [155, 216, 244, 255]
                                    },
                                    width: 1.,
                                    filled: true,
                                    text: String::new(),
                                    font_size: 12.,
                                    image: None,
                                    number: None,
                                };
                                self.renderer.draw(pix, &handle, v.scale);
                            }
                        }
                    }
                }
                if self.app.help_visible {
                    ui::draw_help_background(pix, v.width as f32, v.height as f32, v.scale);
                    for text in ui::help_text(v.width as f32, v.height as f32) {
                        self.renderer.draw(pix, &text, v.scale);
                    }
                }
            }
            let hide_toolbar = self.screenshot.as_ref().is_some_and(|request| {
                request.view == i
                    && matches!(
                        request.stage,
                        ScreenshotStage::HidingToolbar | ScreenshotStage::WaitingForFrame
                    )
            });
            if !hide_toolbar {
                ui::draw(pix, &self.app, tx / ts, ty / ts, ts * v.scale);
            }
            if self.screenshot.is_none() {
                if let Some(toast) = &self.toast {
                    let estimated_width = toast.message.chars().count() as f32 * 9.0 + 24.0;
                    let message = Annotation {
                        tool: Tool::Text,
                        points: vec![Point::new(
                            ((v.width as f32 - estimated_width) / 2.0).max(12.0),
                            24.0,
                        )],
                        color: if toast.error {
                            [180, 55, 70, 245]
                        } else {
                            [45, 120, 82, 245]
                        },
                        width: 1.0,
                        filled: true,
                        text: toast.message.clone(),
                        font_size: 17.0,
                        image: None,
                        number: None,
                    };
                    self.renderer.draw(pix, &message, v.scale);
                }
                if let Some(hover) = &self.hover_tip
                    && hover.visible
                    && hover.view == i
                    && let Some((pointer_view, point)) = self.pointer_position
                    && pointer_view == i
                {
                    let tip = Annotation {
                        tool: Tool::Text,
                        points: vec![Point::new(point.x + 14.0, point.y + 18.0)],
                        color: [18, 38, 46, 245],
                        width: 1.0,
                        filled: true,
                        text: ui::tooltip(hover.action).into(),
                        font_size: 14.0,
                        image: None,
                        number: None,
                    };
                    self.renderer.draw(pix, &tip, v.scale);
                }
            }
            let (buffer, canvas) = self.pool.create_buffer(
                width as i32,
                height as i32,
                width as i32 * 4,
                wl_shm::Format::Argb8888,
            )?;
            // tiny-skia stores premultiplied RGBA; wl_shm ARGB8888 is native-endian.
            for (source, dest) in pix
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(canvas.as_chunks_mut::<4>().0.iter_mut())
            {
                let argb = ((source[3] as u32) << 24)
                    | ((source[0] as u32) << 16)
                    | ((source[1] as u32) << 8)
                    | source[2] as u32;
                dest.copy_from_slice(&argb.to_ne_bytes());
            }
            if let Some(viewport) = &v.viewport {
                v.layer.wl_surface().set_buffer_scale(1);
                viewport.set_destination(v.width as i32, v.height as i32);
            } else {
                v.layer.wl_surface().set_buffer_scale(v.scale as i32);
            }
            v.layer
                .wl_surface()
                .damage_buffer(0, 0, width as i32, height as i32);
            v.layer
                .wl_surface()
                .frame(_qh, v.layer.wl_surface().clone());
            buffer.attach_to(v.layer.wl_surface())?;
            v.layer.commit();
            v.dirty = false;
            v.pending = true;
            if let Some(request) = &mut self.screenshot
                && request.view == i
                && request.stage == ScreenshotStage::HidingToolbar
            {
                request.stage = ScreenshotStage::WaitingForFrame;
            }
        }
        Ok(())
    }
}

impl State {
    /// Input regions use logical surface coordinates, independent of buffer scale.
    /// Commit input changes even while a frame callback is pending: pausing must
    /// release the desktop immediately, including on non-visible outputs.
    pub(super) fn sync_input_region(&mut self, index: usize) -> Result<()> {
        let (x, y, scale) = self.toolbar(index);
        let signature = (
            self.app.drawing,
            self.app.color_picker,
            x as i32,
            y as i32,
            (scale * 10000.) as i32,
        );
        let view = &mut self.views[index];
        if view.input_state == Some(signature) {
            return Ok(());
        }
        if self.app.drawing {
            view.layer.wl_surface().set_input_region(None);
            view.layer
                .set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        } else {
            let region = smithay_client_toolkit::compositor::Region::new(&self.compositor)?;
            let (bx, by, bw, bh) = ui::toolbar_background(&self.app);
            region.add(
                (x + bx * scale).floor() as i32,
                (y + by * scale).floor() as i32,
                (bw * scale).ceil() as i32,
                (bh * scale).ceil() as i32,
            );
            if self.app.drawing && self.app.color_picker {
                region.add(
                    (x + ui::PICKER_X * scale).floor() as i32,
                    (y + ui::PICKER_Y * scale).floor() as i32,
                    (ui::PICKER_WIDTH * scale).ceil() as i32,
                    ((ui::PICKER_HEIGHT - ui::PICKER_Y) * scale).ceil() as i32,
                );
            }
            view.layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
            view.layer
                .set_keyboard_interactivity(KeyboardInteractivity::None);
        }
        view.layer.commit();
        view.input_state = Some(signature);
        Ok(())
    }
}
