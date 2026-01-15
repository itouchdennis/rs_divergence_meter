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
const LEAD_RATE: i32 = 30;
const SPACING: f64 = 12.0;
const TUBE_SCALE: f64 = 1.0;
const STEP_MS: u64 = 50;
const HOLD_MS: u64 = 5000;
const HOLD_STEPS: i32 = (HOLD_MS / STEP_MS) as i32;
const STEP_INTERVAL_RANGE: i32 = 3;

fn main() {
    let app = Application::new(Some("com.example.nixiewallpaper"), Default::default());
    app.connect_activate(build_ui);
    app.run();
}

struct Assets {
    background: ImageSurface,
    digits: Vec<ImageSurface>,
    dot: ImageSurface,
    empty: ImageSurface,
}

struct ScaledSurface {
    width: i32,
    height: i32,
    surface: ImageSurface,
}

struct RenderCache {
    bg_scaled: Option<ScaledSurface>,
}

struct DigitState {
    current: usize,
    previous: usize,
    target: usize,
    step_interval: i32,
}

struct AnimState {
    digits: Vec<DigitState>,
    hold_left: i32,
    cycle: i32,
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
    let state = Rc::new(RefCell::new(init_state()));
    let cache = Rc::new(RefCell::new(RenderCache { bg_scaled: None }));

    let assets_for_draw = Rc::clone(&assets);
    let tick_for_draw = Rc::clone(&tick);
    let state_for_draw = Rc::clone(&state);
    let cache_for_draw = Rc::clone(&cache);
    area.set_draw_func(move |_, cr, width, height| {
        draw_nixies(
            cr,
            width,
            height,
            &assets_for_draw,
            &cache_for_draw,
            &state_for_draw,
            tick_for_draw.get(),
        );
    });

    window.set_child(Some(&area));
    window.present();

    let tick_for_timer = Rc::clone(&tick);
    let state_for_timer = Rc::clone(&state);
    glib::timeout_add_local(std::time::Duration::from_millis(STEP_MS), move || {
        let current = tick_for_timer.get();
        let next = current.wrapping_add(1);
        tick_for_timer.set(next);
        if update_state(next, &state_for_timer) {
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
    let empty_raw = load_surface(base.join("empty.png"));
    let empty = scale_surface(&empty_raw, TUBE_SCALE);

    Assets {
        background,
        digits,
        dot,
        empty,
    }
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

fn random_digit(step: i32, idx: i32, salt: u32) -> usize {
    let mut x = (step as u32).wrapping_add(salt);
    x = x.wrapping_mul(1664525).wrapping_add(1013904223);
    x ^= (idx as u32).wrapping_mul(2246822519);
    x = x ^ (x >> 16);
    (x % 10) as usize
}

fn init_state() -> AnimState {
    let mut digits = Vec::with_capacity(TUBE_COUNT as usize);
    for _ in 0..TUBE_COUNT {
        digits.push(DigitState {
            current: 0,
            previous: 0,
            target: 0,
            step_interval: 1,
        });
    }
    let mut state = AnimState {
        digits,
        hold_left: HOLD_STEPS,
        cycle: 0,
    };
    generate_targets(&mut state);
    for idx in 0..TUBE_COUNT {
        if idx as i32 == DOT_IDX {
            continue;
        }
        let digit = &mut state.digits[idx as usize];
        digit.current = digit.target;
        digit.previous = digit.target;
    }
    state
}

fn generate_targets(state: &mut AnimState) {
    for idx in 0..TUBE_COUNT {
        let idx_i32 = idx as i32;
        if idx_i32 == DOT_IDX {
            continue;
        }
        let cycle_key = if idx_i32 == 0 {
            state.cycle / LEAD_RATE
        } else {
            state.cycle
        };
        let target = if idx_i32 == 0 {
            random_digit(cycle_key, idx_i32, 0xA341_316C) % 2
        } else if idx_i32 < FIRST_GROUP_END {
            random_digit(cycle_key, idx_i32, 0x9E37_79B9)
        } else {
            random_digit(cycle_key, idx_i32, 0x85EB_CA6B)
        };
        let step_interval =
            1 + (random_digit(state.cycle, idx_i32, 0xC2B2_AE35) as i32 % STEP_INTERVAL_RANGE);
        let digit = &mut state.digits[idx as usize];
        digit.target = target;
        digit.step_interval = step_interval;
    }
}

fn update_state(tick: i32, state: &RefCell<AnimState>) -> bool {
    let mut changed = false;
    let mut state = state.borrow_mut();

    if state.hold_left > 0 {
        state.hold_left -= 1;
        return false;
    }

    let mut all_reached = true;
    for idx in 0..TUBE_COUNT {
        if idx as i32 == DOT_IDX {
            continue;
        }
        let digit = &state.digits[idx as usize];
        if digit.current != digit.target {
            all_reached = false;
            break;
        }
    }

    if all_reached {
        state.cycle += 1;
        generate_targets(&mut state);
    }

    for idx in 0..TUBE_COUNT {
        let idx_i32 = idx as i32;
        if idx_i32 == DOT_IDX {
            continue;
        }
        let digit = &mut state.digits[idx as usize];
        digit.previous = digit.current;

        if digit.current == digit.target {
            continue;
        }
        if tick % digit.step_interval != 0 {
            continue;
        }

        let radix = if idx_i32 == 0 { 2 } else { 10 };
        digit.current = ((digit.current as i32 + 1).rem_euclid(radix)) as usize;
        changed = true;
    }

    let mut all_reached_after = true;
    for idx in 0..TUBE_COUNT {
        if idx as i32 == DOT_IDX {
            continue;
        }
        let digit = &state.digits[idx as usize];
        if digit.current != digit.target {
            all_reached_after = false;
            break;
        }
    }

    if all_reached_after {
        state.hold_left = HOLD_STEPS;
    }

    changed
}

fn should_show_empty(tick: i32, idx: i32) -> bool {
    if idx != 6 && idx != 7 {
        return false;
    }
    let hash = (tick as u32)
        .wrapping_mul(1103515245)
        .wrapping_add(12345)
        .wrapping_add(idx as u32 * 9973);
    hash % 20 == 0
}

// Use cairo from GTK; the context is provided by the draw callback.
fn draw_nixies(
    cr: &Context,
    width: i32,
    height: i32,
    assets: &Assets,
    cache: &RefCell<RenderCache>,
    state: &RefCell<AnimState>,
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

    let state = state.borrow();
    for idx in 0..TUBE_COUNT {
        let idx_i32 = idx as i32;
        let x = start_x + idx as f64 * (digit_w + SPACING);
        let y = start_y;

        if idx_i32 == DOT_IDX {
            draw_surface(&assets.dot, x, y, 1.0);
            continue;
        }

        if should_show_empty(tick, idx_i32) {
            draw_surface(&assets.empty, x, y, 1.0);
            continue;
        }

        let digit = &state.digits[idx as usize];
        let current = digit.current;
        let previous = digit.previous;

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
