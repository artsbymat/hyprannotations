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
                if let Some(a) = &v.session.draft {
                    if a.tool == Tool::Text && self.caret_visible {
                        self.renderer.draw_editing(pix, a, v.scale);
                    } else {
                        self.renderer.draw(pix, a, v.scale);
                    }
                }
                if self.app.drawing
                    && self.app.tool == Tool::Select
                    && let Some(a) = v
                        .session
                        .selected
                        .and_then(|i| v.session.document.objects.get(i))
                    && let Some(r) = a.bounds()
                {
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
                    };
                    self.renderer.draw(pix, &marker, v.scale);
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
