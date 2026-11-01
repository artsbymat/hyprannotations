use super::*;

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if let Some(v) = self
            .views
            .iter_mut()
            .find(|v| v.layer.wl_surface() == surface)
            && v.fractional.is_none()
        {
            v.scale = factor.max(1) as f32;
            v.dirty = true;
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(index) = self
            .views
            .iter()
            .position(|view| view.layer.wl_surface() == surface)
        {
            self.views[index].pending = false;
            if let Some(request) = &mut self.screenshot
                && request.view == index
                && request.stage == ScreenshotStage::WaitingForFrame
            {
                request.stage = ScreenshotStage::Ready;
            }
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(output, qh);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.dirty_all();
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.views.retain(|v| v.output != output);
        self.focused = None;
        self.pressed = false;
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.views.retain(|v| &v.layer != layer);
        self.focused = None;
        if self.views.is_empty() {
            self.app.exit = true;
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(v) = self.views.iter_mut().find(|v| &v.layer == layer) {
            v.width = configure.new_size.0.max(1);
            v.height = configure.new_size.1.max(1);
            v.configured = true;
            v.dirty = true;
        }
    }
}

impl State {
    pub(super) fn add_output(&mut self, output: wl_output::WlOutput, qh: &QueueHandle<Self>) {
        if self.views.iter().any(|v| v.output == output) {
            return;
        }
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("hyprannotations"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        // OnDemand avoids pinning focus to the workspace where the overlay started.
        // A click on the drawing surface still gives text/shortcut input to the overlay.
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        let viewport = self
            .viewporter
            .as_ref()
            .map(|manager| manager.get_viewport(layer.wl_surface(), qh, ()));
        let fractional = if viewport.is_some() {
            self.fractional_manager.as_ref().map(|manager| {
                manager.get_fractional_scale(layer.wl_surface(), qh, layer.wl_surface().clone())
            })
        } else {
            None
        };
        layer.commit();
        self.views.push(View {
            output,
            layer,
            width: 1,
            height: 1,
            scale: 1.,
            viewport,
            fractional,
            session: crate::canvas::Session::default(),
            pixels: None,
            dirty: true,
            pending: false,
            configured: false,
            input_state: None,
        });
    }
}

impl State {
    pub(super) fn dirty_all(&mut self) {
        for v in &mut self.views {
            v.dirty = true;
        }
    }
}
