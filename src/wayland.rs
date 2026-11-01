mod input;
mod render;
mod surface;

use crate::{
    app::App,
    canvas::{Annotation, Point, Renderer, Tool},
    config::Settings,
    ui::{self, Action},
};
use anyhow::{Context, Result};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use wayland_client::{Dispatch, delegate_noop};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

struct View {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    width: u32,
    height: u32,
    scale: f32,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    session: crate::canvas::Session,
    pixels: Option<tiny_skia::Pixmap>,
    dirty: bool,
    pending: bool,
    configured: bool,
    input_state: Option<(bool, bool, i32, i32, i32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScreenshotStage {
    HidingToolbar,
    WaitingForFrame,
    Ready,
}

struct ScreenshotRequest {
    view: usize,
    stage: ScreenshotStage,
}

struct Toast {
    message: String,
    error: bool,
    until: std::time::Instant,
}

struct HoverTip {
    view: usize,
    action: Action,
    started: std::time::Instant,
    visible: bool,
}

struct CursorHighlight {
    view: usize,
    point: Point,
    started: std::time::Instant,
}

fn decode_ppm(data: &[u8]) -> anyhow::Result<(usize, usize, Vec<u8>)> {
    fn token<'a>(data: &'a [u8], cursor: &mut usize) -> anyhow::Result<&'a [u8]> {
        loop {
            while data.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
                *cursor += 1;
            }
            if data.get(*cursor) == Some(&b'#') {
                while data.get(*cursor).is_some_and(|byte| *byte != b'\n') {
                    *cursor += 1;
                }
                continue;
            }
            break;
        }
        let start = *cursor;
        while data
            .get(*cursor)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            *cursor += 1;
        }
        anyhow::ensure!(*cursor > start, "Invalid PPM header");
        Ok(&data[start..*cursor])
    }

    let mut cursor = 0;
    anyhow::ensure!(
        token(data, &mut cursor)? == b"P6",
        "grim returned a non-PPM image"
    );
    let width = std::str::from_utf8(token(data, &mut cursor)?)?.parse::<usize>()?;
    let height = std::str::from_utf8(token(data, &mut cursor)?)?.parse::<usize>()?;
    let maximum = std::str::from_utf8(token(data, &mut cursor)?)?.parse::<u16>()?;
    anyhow::ensure!(maximum == 255, "Unsupported PPM color depth");
    anyhow::ensure!(
        data.get(cursor).is_some_and(u8::is_ascii_whitespace),
        "Invalid PPM header"
    );
    cursor += 1;
    if data.get(cursor - 1) == Some(&b'\r') && data.get(cursor) == Some(&b'\n') {
        cursor += 1;
    }
    let pixels = width
        .checked_mul(height)
        .context("Screenshot dimensions are too large")?;
    let rgb_len = pixels
        .checked_mul(3)
        .context("Screenshot dimensions are too large")?;
    anyhow::ensure!(data.len() >= cursor + rgb_len, "Incomplete screenshot data");
    let mut rgba = Vec::with_capacity(pixels * 4);
    for rgb in data[cursor..cursor + rgb_len].as_chunks::<3>().0 {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    Ok((width, height, rgba))
}

struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    viewporter: Option<WpViewporter>,
    fractional_manager: Option<WpFractionalScaleManagerV1>,
    pool: SlotPool,
    views: Vec<View>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    cursor_manager: Option<WpCursorShapeManagerV1>,
    cursor_device: Option<WpCursorShapeDeviceV1>,
    pointer_enter_serial: Option<u32>,
    modifiers: Modifiers,
    focused: Option<usize>,
    pointer_position: Option<(usize, Point)>,
    pressed: bool,
    drag_origin: Option<(Point, [f32; 2])>,
    app: App,
    renderer: Renderer,
    caret_visible: bool,
    caret_last_toggle: std::time::Instant,
    screenshot: Option<ScreenshotRequest>,
    clipboard: Option<arboard::Clipboard>,
    toast: Option<Toast>,
    hover_tip: Option<HoverTip>,
    cursor_highlights: Vec<CursorHighlight>,
}
pub fn run(probe: bool, smoke_test: bool) -> Result<()> {
    let conn = Connection::connect_to_env()
        .context("Connect to Wayland; run inside your Hyprland session")?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    if probe {
        globals.contents().with_list(|list| {
            for g in list {
                println!("{} v{}", g.interface, g.version);
            }
        });
        return Ok(());
    }
    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).context("Compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh)?;
    let cursor_manager = globals
        .bind::<WpCursorShapeManagerV1, _, _>(&qh, 1..=1, ())
        .ok();
    let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();
    let fractional_manager = globals
        .bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
        .ok();
    let pool = SlotPool::new(4096, &shm)?;
    let mut state = State {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        pool,
        viewporter,
        fractional_manager,
        views: Vec::new(),
        keyboard: None,
        pointer: None,
        cursor_manager,
        cursor_device: None,
        pointer_enter_serial: None,
        modifiers: Modifiers::default(),
        focused: None,
        pointer_position: None,
        pressed: false,
        drag_origin: None,
        app: App::new(Settings::load()),
        renderer: Renderer::new(),
        caret_visible: true,
        caret_last_toggle: std::time::Instant::now(),
        screenshot: None,
        clipboard: None,
        toast: None,
        hover_tip: None,
        cursor_highlights: Vec::new(),
    };
    state.app.load_palette();
    queue.roundtrip(&mut state)?;
    for output in state.output_state.outputs().collect::<Vec<_>>() {
        state.add_output(output, &qh);
    }
    let mut event_loop = calloop::EventLoop::<State>::try_new()?;
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Wayland event source: {e}"))?;
    let started = std::time::Instant::now();
    let mut smoke_phase = 0;
    let mut check_pointer_release = false;
    while !state.app.exit {
        state.redraw(&qh)?;
        conn.flush()?;
        let dispatch_timeout = if smoke_test {
            Some(std::time::Duration::from_millis(100))
        } else {
            state.animation_timeout()
        };
        event_loop.dispatch(dispatch_timeout, &mut state)?;
        state.update_animations();
        state.capture_ready_screenshot();
        if smoke_test
            && smoke_phase == 0
            && started.elapsed() >= std::time::Duration::from_millis(700)
        {
            check_pointer_release = state.pointer_position.is_some_and(|(index, p)| {
                let (x, y, scale) = state.toolbar(index);
                p.x < x
                    || p.y < y
                    || p.x > x + ui::WIDTH * scale
                    || p.y > y + ui::toolbar_height(&state.app) * scale
            });
            state.action(0, Action::Desktop);
            smoke_phase = 1;
        }
        if smoke_test
            && smoke_phase == 1
            && started.elapsed() >= std::time::Duration::from_millis(1400)
        {
            anyhow::ensure!(
                state.focused.is_none(),
                "Desktop mode did not release keyboard focus"
            );
            if check_pointer_release && state.pointer_position.is_none() {
                println!("Desktop mode: compositor released keyboard and pointer outside toolbar");
            } else {
                println!(
                    "Desktop mode: keyboard released; pointer release requires motion outside toolbar"
                );
            }
            state.action(0, Action::Tool(Tool::Pen));
            smoke_phase = 2;
        }
        if smoke_test && started.elapsed() >= std::time::Duration::from_secs(2) {
            anyhow::ensure!(smoke_phase == 2, "Drawing mode was not restored");
            anyhow::ensure!(
                !state.views.is_empty()
                    && state
                        .views
                        .iter()
                        .all(|v| v.configured && v.pixels.is_some()),
                "Overlay did not configure and render on all outputs"
            );
            for v in &state.views {
                println!(
                    "Configured overlay: {}x{}, scale {}",
                    v.width, v.height, v.scale
                );
            }
            break;
        }
    }
    // Destroying the surfaces releases all keyboard and pointer interaction.
    Ok(())
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);

#[cfg(test)]
mod tests {
    use super::decode_ppm;

    #[test]
    fn ppm_decoder_preserves_binary_pixels_after_header() {
        let ppm = b"P6\n# generated by grim\n2 1\n255\n\n\x14\x1e\x28\x32\x3c";
        let (width, height, rgba) = decode_ppm(ppm).unwrap();
        assert_eq!((width, height), (2, 1));
        assert_eq!(rgba, [10, 20, 30, 255, 40, 50, 60, 255]);
    }
}
delegate_layer!(State);
delegate_registry!(State);
impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_noop!(State: ignore WpViewporter);
delegate_noop!(State: ignore WpViewport);
delegate_noop!(State: ignore WpFractionalScaleManagerV1);
impl Dispatch<WpFractionalScaleV1, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some(v) = state
                .views
                .iter_mut()
                .find(|v| v.layer.wl_surface() == surface)
        {
            v.scale = (scale as f32 / 120.).max(0.25);
            v.dirty = true;
        }
    }
}
impl Drop for View {
    fn drop(&mut self) {
        if let Some(f) = &self.fractional {
            f.destroy();
        }
        if let Some(v) = &self.viewport {
            v.destroy();
        }
    }
}

delegate_noop!(State: ignore WpCursorShapeManagerV1);
delegate_noop!(State: ignore WpCursorShapeDeviceV1);
