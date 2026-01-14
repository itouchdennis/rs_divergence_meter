use gtk4::prelude::*;
use cairo::{Context, Format, ImageSurface};
use gtk4::{Application, ApplicationWindow, DrawingArea};
use gtk4_layer_shell::{Layer, LayerShell, KeyboardMode};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::BufReader;
use std::rc::Rc;
use std::path::Path;

const TUBE_COUNT: i32 = 12;
const DOT_IDX: i32 = 1;
const FIRST_GROUP_END: i32 = 6;
const LEAD_CYCLE: [i32; 3] = [0, 1, 3];
const LEAD_RATE: i32 = 8;
const TAIL_RATE: i32 = 2;
const FIRST_GROUP_RATE: i32 = 10;
const SPACING: f64 = 12.0;
const TUBE_SCALE: f64 = 1.0;

fn main() {
    let app = Application::new(Some("com.example.nixiewallpaper"), Default::default());
    app.connect_activate(build_ui);
    app.run();
}

struct Assets {
    background: ImageSurface,
    digits: Vec<ImageSurface>,
    dot: ImageSurface,
}

struct ScaledSurface {
    width: i32,
    height: i32,
    surface: ImageSurface,
}

struct RenderCache {
    bg_scaled: Option<ScaledSurface>,
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::new(app);

    // Layer-shell methods are provided via the LayerShell trait.
    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_keyboard_mode(KeyboardMode::None);

    // Anchor to all edges to fill the output (monitor).
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    window.set_anchor(gtk4_layer_shell::Edge::Left, true);
    window.set_anchor(gtk4_layer_shell::Edge::Right, true);

    window.set_decorated(false);

    let area = DrawingArea::new();
    let assets = Rc::new(load_assets());
    let tick = Rc::new(Cell::new(0));
    let cache = Rc::new(RefCell::new(RenderCache { bg_scaled: None }));

    let assets_for_draw = Rc::clone(&assets);
    let tick_for_draw = Rc::clone(&tick);
    let cache_for_draw = Rc::clone(&cache);
    area.set_draw_func(move |_, cr, width, height| {
        draw_nixies(
            cr,
            width,
            height,
            &assets_for_draw,
            &cache_for_draw,
            tick_for_draw.get(),
        );
    });

    window.set_child(Some(&area));
    window.present();

    let tick_for_timer = Rc::clone(&tick);
    glib::timeout_add_local(std::time::Duration::from_millis(700), move || {
        let current = tick_for_timer.get();
        let next = current.wrapping_add(1);
        tick_for_timer.set(next);
        if should_redraw(current, next) {
            area.queue_draw();
        }
        glib::ControlFlow::Continue
    });
}

fn load_assets() -> Assets {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assets");
    let background = load_surface(base.join("fond.png"));

    let mut digits = Vec::with_capacity(10);
    for i in 0..10 {
        let raw = load_surface(base.join(format!("{i}.png")));
        digits.push(scale_surface(&raw, TUBE_SCALE));
    }
    let dot_raw = load_surface(base.join("dot.png"));
    let dot = scale_surface(&dot_raw, TUBE_SCALE);

    Assets { background, digits, dot }
}

fn load_surface(path: impl AsRef<Path>) -> ImageSurface {
    let file = File::open(path).expect("Failed to open image");
    let mut reader = BufReader::new(file);
    ImageSurface::create_from_png(&mut reader).expect("Failed to load PNG")
}

fn scale_surface(surface: &ImageSurface, scale: f64) -> ImageSurface {
    let src_w = surface.width();
    let src_h = surface.height();
    let dst_w = ((src_w as f64) * scale).round().max(1.0) as i32;
    let dst_h = ((src_h as f64) * scale).round().max(1.0) as i32;
    let dst = ImageSurface::create(Format::ARgb32, dst_w, dst_h)
        .expect("Failed to create scaled surface");
    let ctx = Context::new(&dst).expect("Failed to create cairo context");
    ctx.scale(scale, scale);
    ctx.set_source_surface(surface, 0.0, 0.0).unwrap();
    ctx.paint().unwrap();
    dst
}

fn digit_for_idx(tick: i32, idx: i32) -> Option<usize> {
    if idx == DOT_IDX {
        return None;
    }
    if idx == 0 {
        let group_tick = tick / FIRST_GROUP_RATE;
        let step = (group_tick / LEAD_RATE).rem_euclid(LEAD_CYCLE.len() as i32) as usize;
        return Some(LEAD_CYCLE[step] as usize);
    }
    if idx < FIRST_GROUP_END {
        let group_tick = tick / FIRST_GROUP_RATE;
        let value = (group_tick + idx).rem_euclid(10) as usize;
        return Some(value);
    }
    let value = ((tick / TAIL_RATE) + idx).rem_euclid(10) as usize;
    Some(value)
}

fn should_redraw(current_tick: i32, next_tick: i32) -> bool {
    for idx in 0..TUBE_COUNT {
        if digit_for_idx(current_tick, idx) != digit_for_idx(next_tick, idx) {
            return true;
        }
    }
    false
}

// Use cairo from GTK; the context is provided by the draw callback.
fn draw_nixies(
    cr: &Context,
    width: i32,
    height: i32,
    assets: &Assets,
    cache: &RefCell<RenderCache>,
    tick: i32,
) {
    let bg_w = assets.background.width() as f64;
    let bg_h = assets.background.height() as f64;
    let bg_surface = {
        let mut cache = cache.borrow_mut();
        let needs_resize = match &cache.bg_scaled {
            Some(s) => s.width != width || s.height != height,
            None => true,
        };
        if needs_resize {
            let scaled = ImageSurface::create(Format::ARgb32, width, height)
                .expect("Failed to create background surface");
            let ctx = Context::new(&scaled).expect("Failed to create cairo context");
            let scale_x = width as f64 / bg_w;
            let scale_y = height as f64 / bg_h;
            ctx.scale(scale_x, scale_y);
            ctx.set_source_surface(&assets.background, 0.0, 0.0).unwrap();
            ctx.paint().unwrap();
            cache.bg_scaled = Some(ScaledSurface {
                width,
                height,
                surface: scaled,
            });
        }
        cache.bg_scaled.as_ref().unwrap().surface.clone()
    };
    cr.set_source_surface(&bg_surface, 0.0, 0.0).unwrap();
    cr.paint().unwrap();

    let digit_w = assets.digits[0].width() as f64;
    let digit_h = assets.digits[0].height() as f64;
    let total_w = TUBE_COUNT as f64 * digit_w + (TUBE_COUNT as f64 - 1.0) * SPACING;
    let start_x = (width as f64 - total_w) / 2.0;
    let start_y = (height as f64 - digit_h) / 2.0;

    let draw_surface = |surface: &ImageSurface, x: f64, y: f64, alpha: f64| {
        cr.set_source_surface(surface, x, y).unwrap();
        if alpha < 1.0 {
            cr.paint_with_alpha(alpha).unwrap();
        } else {
            cr.paint().unwrap();
        }
    };

    for idx in 0..TUBE_COUNT {
        let idx_i32 = idx as i32;
        let x = start_x + idx as f64 * (digit_w + SPACING);
        let y = start_y;

        if idx_i32 == DOT_IDX {
            draw_surface(&assets.dot, x, y, 1.0);
            continue;
        }

        let current = digit_for_idx(tick, idx_i32).unwrap();
        let previous = digit_for_idx(tick - 1, idx_i32).unwrap();

        if current != previous {
            let prev_surface = &assets.digits[previous];
            draw_surface(prev_surface, x - 2.0, y, 0.25);
            draw_surface(prev_surface, x + 2.0, y, 0.25);
            draw_surface(prev_surface, x, y - 2.0, 0.25);
            draw_surface(prev_surface, x, y + 2.0, 0.25);
        }

        let tex = &assets.digits[current];
        draw_surface(tex, x, y, 1.0);
    }
}
