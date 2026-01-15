use gtk4::prelude::*;
use cairo::{Context, Format, ImageSurface};
use gtk4::{Application, ApplicationWindow, DrawingArea};
use gtk4_layer_shell::{Layer, LayerShell, KeyboardMode};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Config {
    tube_count: i32,
    dot_idx: i32,
    first_group_end: i32,
    lead_rate: i32,
    spacing: f64,
    tube_scale: f64,
    step_ms: u64,
    hold_ms: u64,
    step_interval_range: i32,
    empty_indices: Vec<i32>,
    empty_chance_mod: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tube_count: 12,
            dot_idx: 1,
            first_group_end: 6,
            lead_rate: 30,
            spacing: 12.0,
            tube_scale: 1.0,
            step_ms: 50,
            hold_ms: 5000,
            step_interval_range: 3,
            empty_indices: vec![6, 7],
            empty_chance_mod: 20,
        }
    }
}

impl Config {
    fn load() -> Self {
        let mut config = if let Some(path) = config_path() {
            ensure_config_file(&path);
            match fs::read_to_string(&path) {
                Ok(contents) => match serde_yaml::from_str::<Config>(&contents) {
                    Ok(cfg) => cfg,
                    Err(err) => {
                        eprintln!("Failed to parse config {:?}: {err}", path);
                        Config::default()
                    }
                },
                Err(_) => Config::default(),
            }
        } else {
            Config::default()
        };
        config.normalize();
        config
    }

    fn normalize(&mut self) {
        if self.tube_count < 1 {
            self.tube_count = 1;
        }
        self.dot_idx = self.dot_idx.clamp(0, self.tube_count - 1);
        if self.first_group_end < 0 {
            self.first_group_end = 0;
        }
        if self.first_group_end > self.tube_count {
            self.first_group_end = self.tube_count;
        }
        if self.lead_rate < 1 {
            self.lead_rate = 1;
        }
        if self.step_ms == 0 {
            self.step_ms = 50;
        }
        if self.hold_ms == 0 {
            self.hold_ms = 5000;
        }
        if self.step_interval_range < 1 {
            self.step_interval_range = 1;
        }
        if self.empty_chance_mod == 0 {
            self.empty_chance_mod = 20;
        }
        if self.empty_indices.is_empty() {
            self.empty_indices = vec![6, 7];
        }
    }

    fn hold_steps(&self) -> i32 {
        ((self.hold_ms / self.step_ms).max(1)) as i32
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join(".config/divergence_meter/config.yaml"))
}

fn ensure_config_file(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!("Failed to create config dir {:?}: {err}", parent);
            return;
        }
    }
    match serde_yaml::to_string(&Config::default()) {
        Ok(contents) => {
            if let Err(err) = fs::write(path, contents) {
                eprintln!("Failed to write config {:?}: {err}", path);
            }
        }
        Err(err) => {
            eprintln!("Failed to serialize default config: {err}");
        }
    }
}

struct ConfigState {
    config: Config,
    path: Option<std::path::PathBuf>,
    last_modified: Option<SystemTime>,
}

impl ConfigState {
    fn load() -> Self {
        let path = config_path();
        if let Some(ref p) = path {
            ensure_config_file(p);
        }
        let config = Config::load();
        let last_modified = path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
        Self {
            config,
            path,
            last_modified,
        }
    }

    fn reload_if_changed(&mut self) -> bool {
        let Some(ref path) = self.path else {
            return false;
        };
        let Some(modified) = fs::metadata(path).ok().and_then(|m| m.modified().ok()) else {
            return false;
        };
        let needs_reload = match self.last_modified {
            Some(prev) => modified > prev,
            None => true,
        };
        if !needs_reload {
            return false;
        }
        let config = Config::load();
        self.config = config;
        self.last_modified = Some(modified);
        true
    }
}

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

const BASE_TICK_MS: u64 = 10;

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
    let config_state = Rc::new(RefCell::new(ConfigState::load()));
    let assets = Rc::new(RefCell::new(load_assets(&config_state.borrow().config)));
    let tick = Rc::new(Cell::new(0));
    let acc_ms = Rc::new(Cell::new(0u64));
    let state = Rc::new(RefCell::new(init_state(&config_state.borrow().config)));
    let cache = Rc::new(RefCell::new(RenderCache { bg_scaled: None }));

    let config_for_draw = Rc::clone(&config_state);
    let assets_for_draw = Rc::clone(&assets);
    let tick_for_draw = Rc::clone(&tick);
    let state_for_draw = Rc::clone(&state);
    let cache_for_draw = Rc::clone(&cache);
    area.set_draw_func(move |_, cr, width, height| {
        let config = &config_for_draw.borrow().config;
        let assets = assets_for_draw.borrow();
        draw_nixies(
            cr,
            width,
            height,
            config,
            &assets,
            &cache_for_draw,
            &state_for_draw,
            tick_for_draw.get(),
        );
    });

    window.set_child(Some(&area));
    window.present();

    let tick_for_timer = Rc::clone(&tick);
    let acc_for_timer = Rc::clone(&acc_ms);
    let config_for_timer = Rc::clone(&config_state);
    let state_for_timer = Rc::clone(&state);
    let assets_for_timer = Rc::clone(&assets);
    let cache_for_timer = Rc::clone(&cache);
    glib::timeout_add_local(std::time::Duration::from_millis(BASE_TICK_MS), move || {
        let mut reloaded = false;
        {
            let mut cfg_state = config_for_timer.borrow_mut();
            if cfg_state.reload_if_changed() {
                reloaded = true;
                *assets_for_timer.borrow_mut() = load_assets(&cfg_state.config);
                *state_for_timer.borrow_mut() = init_state(&cfg_state.config);
                cache_for_timer.borrow_mut().bg_scaled = None;
            }
        }
        let mut should_draw = reloaded;
        let mut acc = acc_for_timer.get().saturating_add(BASE_TICK_MS);
        let step_ms = config_for_timer.borrow().config.step_ms;
        while acc >= step_ms {
            acc -= step_ms;
            let current = tick_for_timer.get();
            let next = current.wrapping_add(1);
            tick_for_timer.set(next);
            let cfg = config_for_timer.borrow();
            if update_state(next, &cfg.config, &state_for_timer) {
                should_draw = true;
            }
        }
        acc_for_timer.set(acc);
        if should_draw {
            area.queue_draw();
        }
        glib::ControlFlow::Continue
    });
}

fn load_assets(config: &Config) -> Assets {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assets");
    let background = load_surface(base.join("fond.png"));

    let mut digits = Vec::with_capacity(10);
    for i in 0..10 {
        let raw = load_surface(base.join(format!("{i}.png")));
        digits.push(scale_surface(&raw, config.tube_scale));
    }
    let dot_raw = load_surface(base.join("dot.png"));
    let dot = scale_surface(&dot_raw, config.tube_scale);
    let empty_raw = load_surface(base.join("empty.png"));
    let empty = scale_surface(&empty_raw, config.tube_scale);

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

fn init_state(config: &Config) -> AnimState {
    let mut digits = Vec::with_capacity(config.tube_count as usize);
    for _ in 0..config.tube_count {
        digits.push(DigitState {
            current: 0,
            previous: 0,
            target: 0,
            step_interval: 1,
        });
    }
    let mut state = AnimState {
        digits,
        hold_left: config.hold_steps(),
        cycle: 0,
    };
    generate_targets(config, &mut state);
    for idx in 0..config.tube_count {
        if idx as i32 == config.dot_idx {
            continue;
        }
        let digit = &mut state.digits[idx as usize];
        digit.current = digit.target;
        digit.previous = digit.target;
    }
    state
}

fn generate_targets(config: &Config, state: &mut AnimState) {
    for idx in 0..config.tube_count {
        let idx_i32 = idx as i32;
        if idx_i32 == config.dot_idx {
            continue;
        }
        let cycle_key = state.cycle;
        let target = if idx_i32 == 0 {
            let roll = random_digit(cycle_key, idx_i32, 0xA341_316C);
            if roll % ((config.lead_rate as usize * 4) / 5) == 0 {
                if (roll / config.lead_rate as usize) % 2 == 0 { 1 } else { 2 }
            } else {
                0
            }
        } else if idx_i32 < config.first_group_end {
            random_digit(cycle_key, idx_i32, 0x9E37_79B9)
        } else {
            random_digit(cycle_key, idx_i32, 0x85EB_CA6B)
        };
        let step_interval =
            1 + (random_digit(state.cycle, idx_i32, 0xC2B2_AE35) as i32 % config.step_interval_range);
        let digit = &mut state.digits[idx as usize];
        digit.target = target;
        digit.step_interval = step_interval;
    }
}

fn update_state(tick: i32, config: &Config, state: &RefCell<AnimState>) -> bool {
    let mut changed = false;
    let mut state = state.borrow_mut();

    if state.hold_left > 0 {
        state.hold_left -= 1;
        return false;
    }

    let mut all_reached = true;
    for idx in 0..config.tube_count {
        if idx as i32 == config.dot_idx {
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
        generate_targets(config, &mut state);
    }

    for idx in 0..config.tube_count {
        let idx_i32 = idx as i32;
        if idx_i32 == config.dot_idx {
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
    for idx in 0..config.tube_count {
        if idx as i32 == config.dot_idx {
            continue;
        }
        let digit = &state.digits[idx as usize];
        if digit.current != digit.target {
            all_reached_after = false;
            break;
        }
    }

    if all_reached_after {
        state.hold_left = config.hold_steps();
    }

    changed
}

fn should_show_empty(tick: i32, idx: i32, config: &Config) -> bool {
    if !config.empty_indices.contains(&idx) {
        return false;
    }
    let hash = (tick as u32)
        .wrapping_mul(1103515245)
        .wrapping_add(12345)
        .wrapping_add(idx as u32 * 9973);
    hash % config.empty_chance_mod == 0
}

// Use cairo from GTK; the context is provided by the draw callback.
fn draw_nixies(
    cr: &Context,
    width: i32,
    height: i32,
    config: &Config,
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
    let total_w =
        config.tube_count as f64 * digit_w + (config.tube_count as f64 - 1.0) * config.spacing;
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
    for idx in 0..config.tube_count {
        let idx_i32 = idx as i32;
        let x = start_x + idx as f64 * (digit_w + config.spacing);
        let y = start_y;

        if idx_i32 == config.dot_idx {
            draw_surface(&assets.dot, x, y, 1.0);
            continue;
        }

        if should_show_empty(tick, idx_i32, config) {
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
