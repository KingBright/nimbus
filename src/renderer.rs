use cosmic_text::{Buffer, Color as CosmicColor, FontSystem, SwashCache, SwashContent, SwashImage};
use tiny_skia::{
    Color, FillRule, LinearGradient, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

const BUBBLE_MAX_LOGICAL_W: f32 = 420.0;
const BUBBLE_MIN_LOGICAL_W: f32 = 120.0;

pub struct StyleConfig {
    pub name: &'static str,
    pub eye_base_w: f32,
    pub eye_base_h: f32,
    pub left_eye_x: f32,
    pub right_eye_x: f32,
    pub eye_y: f32,
    pub blush_left_x: f32,
    pub blush_right_x: f32,
    pub blush_y: f32,
    pub blush_w: f32,
    pub blush_h: f32,
    pub mouth_y: f32,
    pub max_y_drop_surprised: f32,
    pub max_y_drop_tickled: f32,
    pub max_y_drop_default: f32,
    pub body_color_stops: Vec<(f32, Color)>,
    pub shadow_color: Color,
    pub outer_rim_color: Color,
    pub inner_rim_color: Color,
}

pub struct PetRenderer {
    pixmap: Pixmap,
    body_mask: Pixmap,
    body_mask_ready: bool,
    font_system: FontSystem,
    swash_cache: SwashCache,

    // Animation state
    start_time: std::time::Instant,
    last_frame_time: f32,
    blink_active: bool,
    blink_start_time: f32,
    next_blink_time: f32,

    // Dynamic text buffer for bubbles
    text_buffer: Buffer,

    // Mouse hover state
    pub pointer_inside: bool,
    pub pointer_x: f32,
    pub pointer_y: f32,
    pub mouse_pos: Option<(f32, f32)>,

    // State machine
    pub pet_state: String,

    // Transition variables for smooth easing (natural animation transitions)
    eye_height_lerp: f32,
    eye_width_lerp: f32,
    top_lid_lerp: f32, // -1.0 (standard open) to 0.4 (sleep close) or -0.45 (smile up)
    bottom_lid_lerp: f32, // 1.0 (standard open) to 0.42 (sleep close) or -0.4 (smile up)
    pupil_scale_lerp: f32,
    pupil_opacity_lerp: f32,
    mouth_open_lerp: f32,
    mouth_width_lerp: f32,
    mouth_skew_lerp: f32,
    body_scale_x_lerp: f32,
    body_scale_y_lerp: f32,
    body_shake_lerp: f32,
    sleep_bubble_lerp: f32,
    gaze_active_lerp: f32,
    hover_lerp: f32,
    touch_lerp: f32,

    // Multi-style customization
    pub active_style: String,
    pub active_weather: String,

    // Hidden floating emotions
    pub happiness: f32,
    pub excitement: f32,

    // State-driven accumulated animation time
    pub anim_time: f32,

    pub draw_pet_enabled: bool,
    pub draw_bubble_enabled: bool,
    pub pet_y_override: Option<f32>,
    pub scale_factor: f32,
    pub bubble_on_bottom: bool,
}

impl PetRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width, height).unwrap();
        let body_mask = Pixmap::new(width, height).unwrap();
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        let mut text_buffer = Buffer::new(&mut font_system, cosmic_text::Metrics::new(24.0, 30.0));
        text_buffer.set_size(&mut font_system, Some((width - 45) as f32), None);

        Self {
            pixmap,
            body_mask,
            body_mask_ready: false,
            font_system,
            swash_cache,
            start_time: std::time::Instant::now(),
            last_frame_time: 0.0,
            blink_active: false,
            blink_start_time: 0.0,
            next_blink_time: 2.0,
            text_buffer,
            pointer_inside: false,
            pointer_x: 24.0,
            pointer_y: 24.0,
            mouse_pos: None,
            pet_state: "idle".to_string(),
            anim_time: 0.0,

            eye_height_lerp: 1.0,
            eye_width_lerp: 1.0,
            top_lid_lerp: -1.0,
            bottom_lid_lerp: 1.0,
            pupil_scale_lerp: 1.0,
            pupil_opacity_lerp: 1.0,
            mouth_open_lerp: 0.0,
            mouth_width_lerp: 1.4,
            mouth_skew_lerp: 0.0,
            body_scale_x_lerp: 1.0,
            body_scale_y_lerp: 1.0,
            body_shake_lerp: 0.0,
            sleep_bubble_lerp: 0.0,
            gaze_active_lerp: 1.0,
            hover_lerp: 0.0,
            touch_lerp: 0.0,
            active_style: "nimbus".to_string(),
            active_weather: "sunny".to_string(),

            happiness: 0.0,
            excitement: 0.0,

            draw_pet_enabled: true,
            draw_bubble_enabled: true,
            pet_y_override: None,
            scale_factor: 1.0,
            bubble_on_bottom: false,
        }
    }

    pub fn add_emotion(&mut self, happiness_gain: f32, excitement_gain: f32) {
        self.happiness = (self.happiness + happiness_gain).min(1.0);
        self.excitement = (self.excitement + excitement_gain).min(1.0);
    }

    pub fn get_style_config(&self) -> StyleConfig {
        match self.active_style.as_str() {
            "mochi" => StyleConfig {
                name: "mochi",
                eye_base_w: 3.9,
                eye_base_h: 4.4,
                left_eye_x: 9.2,
                right_eye_x: 14.8,
                eye_y: 12.5,
                blush_left_x: 7.5,
                blush_right_x: 14.0,
                blush_y: 14.6,
                blush_w: 2.2,
                blush_h: 1.2,
                mouth_y: 15.2,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                body_color_stops: vec![
                    (0.0, Color::from_rgba8(255, 255, 255, 255)),
                    (0.45, Color::from_rgba8(218, 245, 212, 255)),
                    (1.0, Color::from_rgba8(172, 224, 185, 255)),
                ],
                shadow_color: Color::from_rgba8(50, 180, 100, 20),
                outer_rim_color: Color::from_rgba8(110, 180, 130, 40),
                inner_rim_color: Color::from_rgba8(255, 255, 255, 80),
            },
            "starlit" => StyleConfig {
                name: "starlit",
                eye_base_w: 3.2,
                eye_base_h: 3.2,
                left_eye_x: 9.6,
                right_eye_x: 14.4,
                eye_y: 11.5,
                blush_left_x: 7.2,
                blush_right_x: 14.3,
                blush_y: 13.8,
                blush_w: 2.4,
                blush_h: 1.2,
                mouth_y: 14.2,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                body_color_stops: vec![
                    (0.0, Color::from_rgba8(255, 235, 245, 255)),
                    (0.5, Color::from_rgba8(220, 195, 250, 255)),
                    (1.0, Color::from_rgba8(165, 175, 245, 255)),
                ],
                shadow_color: Color::from_rgba8(120, 50, 255, 20),
                outer_rim_color: Color::from_rgba8(150, 120, 255, 35),
                inner_rim_color: Color::from_rgba8(255, 255, 255, 75),
            },
            "crab" => StyleConfig {
                name: "crab",
                eye_base_w: 2.5,
                eye_base_h: 2.5,
                left_eye_x: 21.0,
                right_eye_x: 27.0,
                eye_y: 11.2,
                blush_left_x: 17.9,
                blush_right_x: 27.9,
                blush_y: 13.8,
                blush_w: 2.2,
                blush_h: 1.1,
                mouth_y: 14.5,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                body_color_stops: vec![
                    (0.0, Color::from_rgba8(255, 130, 95, 255)),
                    (0.5, Color::from_rgba8(255, 80, 70, 255)),
                    (1.0, Color::from_rgba8(220, 45, 45, 255)),
                ],
                shadow_color: Color::from_rgba8(180, 30, 30, 20),
                outer_rim_color: Color::from_rgba8(200, 40, 40, 45),
                inner_rim_color: Color::from_rgba8(255, 255, 255, 80),
            },
            _ => StyleConfig {
                name: "nimbus",
                eye_base_w: 2.95,
                eye_base_h: 5.35,
                left_eye_x: 8.85,
                right_eye_x: 15.15,
                eye_y: 12.0,
                blush_left_x: 7.0,
                blush_right_x: 14.5,
                blush_y: 14.3,
                blush_w: 2.5,
                blush_h: 1.2,
                mouth_y: 15.0,
                max_y_drop_surprised: 2.2,
                max_y_drop_tickled: 1.8,
                max_y_drop_default: 1.2,
                body_color_stops: vec![
                    (0.0, Color::from_rgba8(255, 255, 255, 255)),
                    (0.4, Color::from_rgba8(248, 252, 255, 255)),
                    (1.0, Color::from_rgba8(218, 225, 240, 255)),
                ],
                shadow_color: Color::from_rgba8(57, 65, 255, 22),
                outer_rim_color: Color::from_rgba8(130, 160, 255, 30),
                inner_rim_color: Color::from_rgba8(255, 255, 255, 65),
            },
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        self.mouse_pos = pos;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        let physical_font_size = font_size * self.scale_factor;
        let line_height = physical_font_size * 1.25;
        let metrics = cosmic_text::Metrics::new(physical_font_size, line_height);
        self.text_buffer.set_metrics(&mut self.font_system, metrics);
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(new_pixmap) = Pixmap::new(width, height) {
            self.pixmap = new_pixmap;
            if let Some(new_mask) = Pixmap::new(width, height) {
                self.body_mask = new_mask;
                self.body_mask_ready = false;
            }
            let wrap_w = width as f32 - self.bubble_horizontal_inset() * 2.0;
            self.text_buffer.set_size(
                &mut self.font_system,
                Some(wrap_w.max(100.0 * self.scale_factor)),
                None,
            );
            self.text_buffer
                .shape_until_scroll(&mut self.font_system, false);
        }
    }

    pub fn has_text(&self) -> bool {
        self.text_buffer
            .lines
            .iter()
            .any(|line| !line.text().is_empty())
    }

    pub fn measure_bubble_size(&mut self) -> (f32, f32) {
        let max_window_w = BUBBLE_MAX_LOGICAL_W * self.scale_factor;
        let max_text_w =
            (max_window_w - self.bubble_horizontal_inset() * 2.0).max(80.0 * self.scale_factor);
        self.text_buffer
            .set_size(&mut self.font_system, Some(max_text_w), None);
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);

        let pad_x = 18.0 * self.scale_factor;
        let pad_top = 12.0 * self.scale_factor;
        let pad_bottom = 8.0 * self.scale_factor;

        let mut max_width = 0.0f32;
        let mut height = 0.0f32;
        let mut max_line_height = 0.0f32;
        let mut runs_to_draw = 0;

        for run in self.text_buffer.layout_runs() {
            max_width = max_width.max(run.line_w);
            max_line_height = max_line_height.max(run.line_height);
            height = run.line_y + run.line_height;
            runs_to_draw += 1;
        }

        if height == 0.0 {
            return (0.0, 0.0);
        }

        let tail_h = self.bubble_tail_height();
        let safe_pad = self.bubble_edge_safe_pad();
        let shadow_pad = self.bubble_shadow_pad();
        let bubble_w = (max_width + pad_x * 2.0 + safe_pad * 2.0)
            .clamp(BUBBLE_MIN_LOGICAL_W * self.scale_factor, max_window_w)
            .ceil();
        let visual_text_h = height.max(runs_to_draw as f32 * max_line_height);
        let bubble_h =
            (visual_text_h + pad_top + pad_bottom + tail_h + safe_pad * 2.0 + shadow_pad).ceil();

        (bubble_w, bubble_h)
    }

    fn bubble_horizontal_inset(&self) -> f32 {
        18.0 * self.scale_factor + self.bubble_edge_safe_pad()
    }

    fn bubble_tail_height(&self) -> f32 {
        8.0 * self.scale_factor
    }

    fn bubble_edge_safe_pad(&self) -> f32 {
        3.0 * self.scale_factor
    }

    fn bubble_shadow_pad(&self) -> f32 {
        5.0 * self.scale_factor
    }

    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    pub fn body_hit_test(&self, x: f64, y: f64) -> Option<bool> {
        if !self.body_mask_ready {
            return None;
        }

        let x = x.round() as i32;
        let y = y.round() as i32;
        let width = self.body_mask.width() as i32;
        let height = self.body_mask.height() as i32;
        if x < 0 || y < 0 || x >= width || y >= height {
            return Some(false);
        }

        let data = self.body_mask.data();
        for sample_y in (y - 1).max(0)..=(y + 1).min(height - 1) {
            for sample_x in (x - 1).max(0)..=(x + 1).min(width - 1) {
                let idx =
                    ((sample_y as u32 * self.body_mask.width() + sample_x as u32) * 4 + 3) as usize;
                if data.get(idx).copied().unwrap_or(0) > 40 {
                    return Some(true);
                }
            }
        }

        Some(false)
    }

    pub fn set_text(&mut self, text: &str) {
        let mut attrs = cosmic_text::Attrs::new();
        // Set dark slate blue and Medium weight for standard pet chat style
        attrs = attrs
            .color(CosmicColor::rgb(0x2d, 0x37, 0x48))
            .weight(cosmic_text::Weight::MEDIUM);

        self.text_buffer.set_text(
            &mut self.font_system,
            text,
            attrs,
            cosmic_text::Shaping::Advanced,
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    pub fn update_and_draw(&mut self) {
        // Clear background with fully transparent color (0,0,0,0)
        self.pixmap.fill(Color::TRANSPARENT);
        self.body_mask.fill(Color::TRANSPARENT);
        self.body_mask_ready = false;

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let dt = elapsed - self.last_frame_time;
        self.last_frame_time = elapsed;

        // 情绪系统自然衰减与行为增益
        let dt_sec = dt.clamp(0.0, 0.1);
        self.happiness = (self.happiness - 0.05 * dt_sec).max(0.0);
        self.excitement = (self.excitement - 0.08 * dt_sec).max(0.0);

        if self.pet_state == "tickled" {
            self.happiness = (self.happiness + 0.42 * dt_sec).min(1.0);
            self.excitement = (self.excitement + 0.16 * dt_sec).min(1.0);
        }
        if self.pet_state == "surprised" {
            self.excitement = (self.excitement + 0.90 * dt_sec).min(1.0);
        }

        let pet_y = if let Some(y) = self.pet_y_override {
            y
        } else {
            let pet_size = self.pet_visual_size();
            self.pixmap.height() as f32 - pet_size * 0.85
        };

        // Draw speech bubble if there is text in the buffer
        let has_text = self
            .text_buffer
            .lines
            .iter()
            .any(|line| !line.text().is_empty());
        if self.draw_bubble_enabled && has_text {
            self.draw_bubble(pet_y);
        }

        // Draw the cloud pet
        if self.draw_pet_enabled {
            self.draw_pet(elapsed, dt, pet_y);
        }
    }

    fn pet_visual_size(&self) -> f32 {
        self.pixmap.width() as f32 * 0.56
    }

    fn draw_bubble(&mut self, pet_y: f32) {
        let pad_x = 18.0 * self.scale_factor;
        let pad_top = 12.0 * self.scale_factor;
        let pad_bottom = 8.0 * self.scale_factor;
        let tail_h = self.bubble_tail_height();
        let safe_pad = self.bubble_edge_safe_pad();
        let shadow_pad = self.bubble_shadow_pad();
        let mut max_width = 0.0f32;

        let max_w_limit = self.pixmap.width() as f32;
        let max_h_limit = self.pixmap.height() as f32;

        let is_independent_bubble_window = !self.draw_pet_enabled;

        let (bubble_w, bubble_h, x, y, runs_to_draw, _visual_text_h, _height) =
            if is_independent_bubble_window {
                let mut h = 0.0f32;
                let mut max_lh = 0.0f32;
                let mut runs = 0;
                for run in self.text_buffer.layout_runs() {
                    max_width = max_width.max(run.line_w);
                    max_lh = max_lh.max(run.line_height);
                    h = run.line_y + run.line_height;
                    runs += 1;
                }
                if runs == 0 {
                    return;
                }
                let bw = (max_w_limit - safe_pad * 2.0).max(1.0);
                let bh = (max_h_limit - tail_h - safe_pad * 2.0 - shadow_pad).max(1.0);
                let bx = safe_pad;
                let by = if self.bubble_on_bottom {
                    safe_pad + tail_h
                } else {
                    safe_pad
                };
                let v_text_h = h.max(runs as f32 * max_lh);
                (bw, bh, bx, by, runs, v_text_h, h)
            } else {
                // Max bubble height from padding to pet boundaries
                let max_bubble_h = if self.bubble_on_bottom {
                    (max_h_limit - pet_y - 25.0 * self.scale_factor).max(50.0 * self.scale_factor)
                } else {
                    (pet_y - 25.0 * self.scale_factor).max(50.0 * self.scale_factor)
                };
                let max_text_h = max_bubble_h - pad_top - pad_bottom - tail_h;

                let mut h = 0.0f32;
                let mut max_lh = 0.0f32;
                let mut runs = 0;

                for run in self.text_buffer.layout_runs() {
                    if run.line_y + run.line_height > max_text_h {
                        break;
                    }
                    max_width = max_width.max(run.line_w);
                    max_lh = max_lh.max(run.line_height);
                    h = run.line_y + run.line_height;
                    runs += 1;
                }

                if h == 0.0 {
                    return;
                }

                let bw = (max_width + pad_x * 2.0).clamp(
                    100.0 * self.scale_factor,
                    max_w_limit - 20.0 * self.scale_factor,
                );
                let v_text_h = h.max(runs as f32 * max_lh);
                let bh = v_text_h + pad_top + pad_bottom;

                let bx = (max_w_limit - bw) / 2.0;

                // Offset pet_y to y based on orientation (bubble on top or bottom of pet)
                let by = if self.bubble_on_bottom {
                    pet_y + 2.0 * self.scale_factor + tail_h
                } else {
                    pet_y - 2.0 * self.scale_factor - tail_h - bh
                };
                (bw, bh, bx, by, runs, v_text_h, h)
            };

        // 1. Draw rounded bubble shape using tiny-skia
        let mut pb = PathBuilder::new();
        let radius = 14.0f32 * self.scale_factor; // slightly more rounded

        // Rounded rect path
        pb.move_to(x + radius, y);
        pb.line_to(x + bubble_w - radius, y);
        pb.quad_to(x + bubble_w, y, x + bubble_w, y + radius);
        pb.line_to(x + bubble_w, y + bubble_h - radius);
        pb.quad_to(
            x + bubble_w,
            y + bubble_h,
            x + bubble_w - radius,
            y + bubble_h,
        );

        pb.line_to(x + radius, y + bubble_h);
        pb.quad_to(x, y + bubble_h, x, y + bubble_h - radius);
        pb.line_to(x, y + radius);
        pb.quad_to(x, y, x + radius, y);
        pb.close();

        let path = pb.finish().unwrap();
        let cx = max_w_limit / 2.0;
        let mut tail_pb = PathBuilder::new();
        if self.bubble_on_bottom {
            // Tail pointing upwards (at top of bubble)
            let apex_y = if is_independent_bubble_window {
                safe_pad
            } else {
                y - tail_h
            };
            tail_pb.move_to(cx - 5.5 * self.scale_factor, y + 0.35 * self.scale_factor);
            tail_pb.quad_to(
                cx - 1.9 * self.scale_factor,
                apex_y + tail_h * 0.34,
                cx,
                apex_y,
            );
            tail_pb.quad_to(
                cx + 1.9 * self.scale_factor,
                apex_y + tail_h * 0.34,
                cx + 5.5 * self.scale_factor,
                y + 0.35 * self.scale_factor,
            );
        } else {
            // Tail pointing downwards (at bottom of bubble)
            let apex_y = if is_independent_bubble_window {
                max_h_limit - safe_pad - shadow_pad
            } else {
                y + bubble_h + tail_h
            };
            tail_pb.move_to(
                cx - 5.5 * self.scale_factor,
                y + bubble_h - 0.35 * self.scale_factor,
            );
            tail_pb.quad_to(
                cx - 1.9 * self.scale_factor,
                apex_y - tail_h * 0.34,
                cx,
                apex_y,
            );
            tail_pb.quad_to(
                cx + 1.9 * self.scale_factor,
                apex_y - tail_h * 0.34,
                cx + 5.5 * self.scale_factor,
                y + bubble_h - 0.35 * self.scale_factor,
            );
        }
        tail_pb.close();
        let tail_path = tail_pb.finish().unwrap();

        // High contrast crisp white background gradient
        let bubble_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(x, y),
            tiny_skia::Point::from_xy(x + bubble_w, y + bubble_h),
            vec![
                tiny_skia::GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 255)),
                tiny_skia::GradientStop::new(1.0, Color::from_rgba8(252, 254, 255, 252)),
            ],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap();

        let paint = Paint {
            shader: bubble_grad,
            anti_alias: true,
            ..Default::default()
        };

        // Coordinates are already in physical space
        let global_ts = Transform::identity();

        // Draw shadow (soft offset overlay)
        let mut shadow_paint = Paint::default();
        shadow_paint.set_color(Color::from_rgba8(40, 50, 100, 18));
        shadow_paint.anti_alias = true;
        let shadow_transform = Transform::from_translate(0.0, 3.0 * self.scale_factor);
        self.pixmap.fill_path(
            &path,
            &shadow_paint,
            FillRule::Winding,
            shadow_transform,
            None,
        );
        self.pixmap.fill_path(
            &tail_path,
            &shadow_paint,
            FillRule::Winding,
            shadow_transform,
            None,
        );

        // Draw bubble body
        self.pixmap
            .fill_path(&tail_path, &paint, FillRule::Winding, global_ts, None);
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, global_ts, None);

        // High contrast beautiful outline
        let mut stroke_paint = Paint::default();
        stroke_paint.set_color(Color::from_rgba8(120, 140, 220, 160));
        stroke_paint.anti_alias = true;
        let stroke = Stroke {
            width: 1.5 * self.scale_factor,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &stroke_paint, &stroke, global_ts, None);
        self.pixmap
            .stroke_path(&tail_path, &stroke_paint, &stroke, global_ts, None);

        // 2. Draw text using cosmic-text SwashCache
        let text_color = CosmicColor::rgb(0x2d, 0x37, 0x48);
        let vertical_nudge = 0.0;
        for run in self.text_buffer.layout_runs().take(runs_to_draw) {
            let line_y = y + pad_top + vertical_nudge + run.line_y;
            for glyph in run.glyphs {
                let physical_glyph = glyph.physical((x + pad_x, line_y), 1.0);
                if let Some(image) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical_glyph.cache_key)
                {
                    if image.placement.width > 0 && image.placement.height > 0 {
                        let draw_x = physical_glyph.x + image.placement.left;
                        let draw_y = physical_glyph.y - image.placement.top;
                        Self::draw_glyph(&mut self.pixmap, image, draw_x, draw_y, text_color);
                    }
                }
            }
        }
    }

    fn draw_glyph(pixmap: &mut Pixmap, image: &SwashImage, x: i32, y: i32, color: CosmicColor) {
        let width = image.placement.width as usize;
        let height = image.placement.height as usize;

        let mut pixmap_mut = pixmap.as_mut();

        for row in 0..height {
            let pixel_y = y + row as i32;
            if pixel_y < 0 || pixel_y >= pixmap_mut.height() as i32 {
                continue;
            }

            for col in 0..width {
                let pixel_x = x + col as i32;
                if pixel_x < 0 || pixel_x >= pixmap_mut.width() as i32 {
                    continue;
                }

                let (r, g, b, alpha) = match image.content {
                    SwashContent::Mask => {
                        let alpha = image.data[row * width + col] as f32 / 255.0;
                        let r = color.r() as f32 / 255.0;
                        let g = color.g() as f32 / 255.0;
                        let b = color.b() as f32 / 255.0;
                        (r, g, b, alpha)
                    }
                    SwashContent::SubpixelMask => {
                        let idx = (row * width + col) * 3;
                        if idx + 2 < image.data.len() {
                            let sr = image.data[idx] as f32;
                            let sg = image.data[idx + 1] as f32;
                            let sb = image.data[idx + 2] as f32;
                            let alpha = (sr + sg + sb) / 3.0 / 255.0;
                            let r = color.r() as f32 / 255.0;
                            let g = color.g() as f32 / 255.0;
                            let b = color.b() as f32 / 255.0;
                            (r, g, b, alpha)
                        } else {
                            (0.0, 0.0, 0.0, 0.0)
                        }
                    }
                    SwashContent::Color => {
                        let idx = (row * width + col) * 4;
                        if idx + 3 < image.data.len() {
                            let r = image.data[idx] as f32 / 255.0;
                            let g = image.data[idx + 1] as f32 / 255.0;
                            let b = image.data[idx + 2] as f32 / 255.0;
                            let alpha = image.data[idx + 3] as f32 / 255.0;
                            (r, g, b, alpha)
                        } else {
                            (0.0, 0.0, 0.0, 0.0)
                        }
                    }
                };

                if alpha > 0.0 {
                    let index =
                        (pixel_y as usize * pixmap_mut.width() as usize + pixel_x as usize) * 4;
                    let dst_data = pixmap_mut.data_mut();
                    let dst_r = dst_data[index] as f32 / 255.0;
                    let dst_g = dst_data[index + 1] as f32 / 255.0;
                    let dst_b = dst_data[index + 2] as f32 / 255.0;
                    let dst_a = dst_data[index + 3] as f32 / 255.0;

                    let out_a = alpha + dst_a * (1.0 - alpha);
                    if out_a > 0.0 {
                        let out_r = (r * alpha + dst_r * dst_a * (1.0 - alpha)) / out_a;
                        let out_g = (g * alpha + dst_g * dst_a * (1.0 - alpha)) / out_a;
                        let out_b = (b * alpha + dst_b * dst_a * (1.0 - alpha)) / out_a;

                        dst_data[index] = (out_r * 255.0) as u8;
                        dst_data[index + 1] = (out_g * 255.0) as u8;
                        dst_data[index + 2] = (out_b * 255.0) as u8;
                        dst_data[index + 3] = (out_a * 255.0) as u8;
                    }
                }
            }
        }
    }

    fn draw_pet(&mut self, t: f32, dt: f32, pet_y: f32) {
        // --- 0. State-driven Animation Speed and Amplitude modifiers ---
        let (anim_speed, anim_amplitude) = match self.pet_state.as_str() {
            "sleeping" => (0.0f32, 0.0f32),
            "tickled" => (2.2f32, 1.8f32),
            "surprised" => (1.6f32, 1.4f32),
            "thinking" => (0.5f32, 0.4f32),
            "bored" | "tired" => (0.3f32, 0.3f32),
            _ => (1.0f32, 1.0f32),
        };
        self.anim_time += dt * anim_speed;
        let anim_t = self.anim_time;

        // --- 1. Target Value Determination for Smooth Transitions ---
        let target_eye_height: f32;
        let target_eye_width: f32;
        let target_top_lid: f32;
        let target_bottom_lid: f32;
        let target_pupil_scale: f32;
        let target_pupil_opacity: f32;
        let target_mouth_open: f32;
        let target_mouth_width: f32;
        let target_mouth_skew: f32;
        let target_body_scale_x: f32;
        let target_body_scale_y: f32;
        let target_body_shake: f32;
        let target_sleep_bubble: f32;
        let target_gaze_active: f32;

        // Settle parameters
        let settle_tx = -0.22f32;
        let settle_ty = -0.04f32;
        let settle_scale = 1.012f32;
        let settle_rot = -0.65f32;

        match self.pet_state.as_str() {
            "sleeping" => {
                target_eye_height = 1.0;
                target_eye_width = 1.0;
                target_top_lid = 0.4;
                target_bottom_lid = 0.42;
                target_pupil_scale = 0.0;
                target_pupil_opacity = 0.0;
                target_mouth_open = 0.0;
                target_mouth_width = 1.0;
                target_mouth_skew = 0.0;
                target_body_scale_x = 0.98;
                target_body_scale_y = 0.96;
                target_body_shake = 0.0;
                target_sleep_bubble = 1.0;
                target_gaze_active = 0.0;
            }
            "thinking" => {
                target_eye_height = 1.0;
                target_eye_width = 0.96;
                target_top_lid = 0.4;
                target_bottom_lid = 0.42;
                target_pupil_scale = 0.0;
                target_pupil_opacity = 0.0;
                target_mouth_open = 0.0;
                target_mouth_width = 0.8;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.0;
                target_body_scale_y = 1.0;
                target_body_shake = 0.15;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.0;
            }
            "tickled" => {
                target_eye_height = 1.0;
                target_eye_width = 1.04;
                target_top_lid = -0.45;
                target_bottom_lid = -0.4;
                target_pupil_scale = 0.0;
                target_pupil_opacity = 0.0;
                target_mouth_open = 0.9;
                target_mouth_width = 1.8;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.035;
                target_body_scale_y = 1.018;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.2;
            }
            "surprised" => {
                target_eye_height = 1.26;
                target_eye_width = 1.22;
                target_top_lid = -1.12;
                target_bottom_lid = 1.12;
                target_pupil_scale = 0.30;
                target_pupil_opacity = 1.0;
                target_mouth_open = 1.0;
                target_mouth_width = 1.2;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.04;
                target_body_scale_y = 1.04;
                target_body_shake = 1.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.8;
            }
            "bored" => {
                target_eye_height = 0.72;
                target_eye_width = 0.96;
                target_top_lid = -0.18;
                target_bottom_lid = 1.0;
                target_pupil_scale = 0.52;
                target_pupil_opacity = 1.0;
                target_mouth_open = 0.4;
                target_mouth_width = 1.6;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.02;
                target_body_scale_y = 0.97;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.4;
            }
            "muttering" => {
                target_eye_height = 1.0;
                target_eye_width = 1.0;
                target_top_lid = -1.0;
                target_bottom_lid = 1.0;
                target_pupil_scale = 0.56;
                target_pupil_opacity = 1.0;
                target_mouth_open = 0.25 + (t * 18.0).sin().abs() * 0.45;
                target_mouth_width = 1.4;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.0;
                target_body_scale_y = 1.0;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 1.0;
            }
            "working" => {
                target_eye_height = 0.90;
                target_eye_width = 0.98;
                target_top_lid = -0.38;
                target_bottom_lid = 0.92;
                target_pupil_scale = 0.54;
                target_pupil_opacity = 1.0;
                target_mouth_open = 0.0;
                target_mouth_width = 1.1;
                target_mouth_skew = 0.2;
                target_body_scale_x = 1.0;
                target_body_scale_y = 1.0;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.75;
            }
            "tired" => {
                target_eye_height = 0.70;
                target_eye_width = 0.96;
                target_top_lid = -0.18;
                target_bottom_lid = 0.58;
                target_pupil_scale = 0.48;
                target_pupil_opacity = 1.0;
                target_mouth_open = 0.0;
                target_mouth_width = 1.2;
                target_mouth_skew = -0.1;
                target_body_scale_x = 0.98;
                target_body_scale_y = 0.96;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.5;
            }
            _ => {
                // idle
                let loop_t = t % 20.0;
                if self.active_style == "mochi" && (10.0..15.0).contains(&loop_t) {
                    // 抹茶小猫在 10s-15s 是 舔爪状态
                    target_eye_height = 1.0;
                    target_eye_width = 1.0;
                    target_top_lid = 0.38; // 眯眼/闭眼
                    target_bottom_lid = 0.40;
                    target_pupil_scale = 0.0;
                    target_pupil_opacity = 0.0;
                    // 嘴巴微张并伴随小猫舔爪子而急促颤动
                    target_mouth_open = 0.15 + (t * 22.0).sin().abs() * 0.15;
                    target_mouth_width = 0.8;
                    target_mouth_skew = -0.15;
                    target_body_scale_x = 0.98;
                    target_body_scale_y = 0.96;
                    target_body_shake = 0.0;
                    target_sleep_bubble = 0.0;
                    target_gaze_active = 0.0;
                } else if self.active_style == "mochi" && (15.0..20.0).contains(&loop_t) {
                    // 抹茶小猫在 15s-20s 是 洗脸/抓耳状态
                    target_eye_height = 1.0;
                    target_eye_width = 1.0;
                    target_top_lid = 0.35; // 眯眼
                    target_bottom_lid = 0.38;
                    target_pupil_scale = 0.0;
                    target_pupil_opacity = 0.0;
                    target_mouth_open = 0.0;
                    target_mouth_width = 1.0;
                    target_mouth_skew = 0.0;
                    target_body_scale_x = 1.0;
                    target_body_scale_y = 1.0;
                    target_body_shake = 0.0;
                    target_sleep_bubble = 0.0;
                    target_gaze_active = 0.0;
                } else {
                    target_eye_height = 1.0;
                    target_eye_width = 1.0;
                    target_top_lid = -1.0;
                    target_bottom_lid = 1.0;
                    target_pupil_scale = 0.56;
                    target_pupil_opacity = 1.0;
                    target_mouth_open = 0.0;
                    target_mouth_width = 1.4;
                    target_mouth_skew = 0.0;
                    target_body_scale_x = 1.0;
                    target_body_scale_y = 1.0;
                    target_body_shake = 0.0;
                    target_sleep_bubble = 0.0;
                    target_gaze_active = 1.0;
                }
            }
        }

        // --- 2. Exponential Lerp (dt-dependent smooth transitions) ---
        let ease_speed = 7.0f32;
        let factor = 1.0 - (-dt * ease_speed).exp();

        self.eye_height_lerp += (target_eye_height - self.eye_height_lerp) * factor;
        self.eye_width_lerp += (target_eye_width - self.eye_width_lerp) * factor;
        self.top_lid_lerp += (target_top_lid - self.top_lid_lerp) * factor;
        self.bottom_lid_lerp += (target_bottom_lid - self.bottom_lid_lerp) * factor;
        self.pupil_scale_lerp += (target_pupil_scale - self.pupil_scale_lerp) * factor;
        self.pupil_opacity_lerp += (target_pupil_opacity - self.pupil_opacity_lerp) * factor;
        self.mouth_open_lerp += (target_mouth_open - self.mouth_open_lerp) * factor;
        self.mouth_width_lerp += (target_mouth_width - self.mouth_width_lerp) * factor;
        self.mouth_skew_lerp += (target_mouth_skew - self.mouth_skew_lerp) * factor;
        self.body_scale_x_lerp += (target_body_scale_x - self.body_scale_x_lerp) * factor;
        self.body_scale_y_lerp += (target_body_scale_y - self.body_scale_y_lerp) * factor;
        self.body_shake_lerp += (target_body_shake - self.body_shake_lerp) * factor;
        self.sleep_bubble_lerp += (target_sleep_bubble - self.sleep_bubble_lerp) * factor;
        self.gaze_active_lerp += (target_gaze_active - self.gaze_active_lerp) * factor;
        let target_hover = if self.pointer_inside { 1.0 } else { 0.0 };
        let target_touch = if self.pet_state == "tickled" {
            1.0
        } else {
            0.0
        };
        self.hover_lerp += (target_hover - self.hover_lerp) * factor;
        self.touch_lerp += (target_touch - self.touch_lerp) * factor;

        // Blink animation
        if !self.blink_active && t > self.next_blink_time {
            self.blink_active = true;
            self.blink_start_time = t;
        }

        let mut blink_scale = 1.0f32;
        if self.blink_active {
            let d = t - self.blink_start_time;
            let blink_dur = 0.14f32;
            let half = blink_dur / 2.0;
            if d < half {
                blink_scale = 1.0 - 0.94 * (d / half);
            } else if d < blink_dur {
                blink_scale = 0.06 + 0.94 * ((d - half) / half);
            } else {
                self.blink_active = false;
                self.next_blink_time = t + 5.2 * (0.8 + rand_offset());
            }
        }

        // Gentle tactile motion. Keep this restrained so edge hover never reads as jitter.
        let spring = (t * 4.8).sin() * 0.012 * self.touch_lerp;

        let hit = self.touch_lerp * 0.16 + spring.abs();
        let hold = (2.0 * std::f32::consts::PI * t / 3.9).sin();
        let live = if t > 0.65 { 1.0f32 } else { 0.0f32 };

        // Custom breath scale. The body and face share this motion so idle feels alive.
        let (breath_cycle, breath_depth) = match self.pet_state.as_str() {
            "thinking" => (2.0f32, 0.020f32),
            "sleeping" => (6.0f32, 0.010f32),
            "tired" => (4.8f32, 0.018f32),
            "bored" => (5.2f32, 0.014f32),
            "tickled" => (2.5f32, 0.014f32),
            _ => (3.9f32, 0.026f32),
        };
        let breath_wave = (2.0 * std::f32::consts::PI * t / breath_cycle).sin() * live;
        let breath_scale_x = 1.0 + breath_wave * breath_depth * 0.55;
        let breath_scale_y = 1.0 + breath_wave * breath_depth * 1.18;

        // Eye track target calculation
        let dx = self.pointer_x - 12.0;
        let dy = self.pointer_y - 12.0;
        let dist = (dx * dx + dy * dy).sqrt();
        let (idle_eye_x, idle_eye_y) = if dist > 0.001 {
            let sat = (dist / 6.0).tanh();
            (
                (dx / dist) * sat * 0.50,
                ((dy / dist) * sat * 0.50).clamp(-0.50, 0.50),
            )
        } else {
            (0.0, 0.0)
        };

        // Pose transforms
        let body_start = 1.0 - self.hover_lerp;
        let mut pose_tx = settle_tx - body_start * 0.22
            + spring * 0.20
            + live * (2.0 * std::f32::consts::PI * t / 3.1 + 0.8).sin() * 0.014;
        let mut pose_ty = settle_ty - hit * 0.055 + hold * 0.018 * live;
        let mut pose_rot = settle_rot - body_start * 0.62 + spring * 0.35 + hold * 0.08 * live;
        pose_ty -= breath_wave * 0.10;

        // Mochi idle grooming cycle head movement
        let is_mochi_idle = self.active_style == "mochi"
            && (self.pet_state == "idle"
                || (![
                    "sleeping",
                    "thinking",
                    "tickled",
                    "surprised",
                    "bored",
                    "muttering",
                    "working",
                    "tired",
                ]
                .contains(&self.pet_state.as_str())));
        if is_mochi_idle {
            let loop_t = t % 20.0;
            if (10.0..15.0).contains(&loop_t) {
                // 舔爪：向下低头，向左微偏
                let lick_phase = (loop_t - 10.0) / 5.0;
                let w = if lick_phase < 0.15 {
                    lick_phase / 0.15
                } else if lick_phase >= 0.85 {
                    (1.0 - lick_phase) / 0.15
                } else {
                    1.0
                };
                pose_ty += w * 0.45;
                pose_rot += w * 0.025;
            } else if (15.0..20.0).contains(&loop_t) {
                // 洗脸：随右爪划圈而上下左右微晃，向右偏头
                let groom_phase = (loop_t - 15.0) / 5.0;
                let w = if groom_phase < 0.15 {
                    groom_phase / 0.15
                } else if groom_phase >= 0.85 {
                    (1.0 - groom_phase) / 0.15
                } else {
                    1.0
                };
                pose_ty += w * 0.15;
                pose_rot += w * (-0.035 + (t * 7.5).cos() * 0.015);
            }
        }

        // Nimbus windy weather shake
        if self.active_style == "nimbus" && self.active_weather == "windy" {
            pose_tx += (t * 12.0).sin() * 0.24;
            pose_rot += (t * 8.0).cos() * 0.055;
        }

        // Jitter (Shake) application for surprised
        if self.body_shake_lerp > 0.001 {
            pose_tx += (t * 34.0).sin() * 0.035 * self.body_shake_lerp;
            pose_ty += (t * 29.0).cos() * 0.032 * self.body_shake_lerp;
            pose_rot += (t * 31.0).sin() * 0.016 * self.body_shake_lerp;
        }

        // Curried scale factors
        let sx = (settle_scale + hit * 0.048) * self.body_scale_x_lerp;
        let sy = (settle_scale - hit * 0.048 * 0.72) * self.body_scale_y_lerp;

        let eye_tx = idle_eye_x * self.gaze_active_lerp;
        let eye_ty = idle_eye_y * self.gaze_active_lerp;

        // Viewport scale (48x48 up to width)
        let pet_width = if !self.draw_bubble_enabled {
            self.pixmap.width() as f32 * 0.80
        } else {
            self.pet_visual_size()
        };
        let pet_height = pet_width;
        let view_scale = pet_width / 48.0;
        let view_tx = if !self.draw_bubble_enabled {
            self.pixmap.width() as f32 / 2.0 - 12.0 * view_scale
        } else {
            self.pixmap.width() as f32 / 2.0
        };
        let view_ty = if !self.draw_bubble_enabled {
            self.pixmap.height() as f32 - 14.5 * view_scale
        } else {
            pet_y + pet_height / 2.0
        };

        let base_transform = Transform::from_translate(view_tx, view_ty)
            .pre_scale(view_scale, view_scale)
            .pre_translate(pose_tx, pose_ty)
            .pre_rotate(pose_rot)
            .pre_scale(sx, sy)
            .pre_translate(-12.0, -12.0);

        let body_transform = base_transform
            .pre_translate(12.0, 12.0)
            .pre_scale(breath_scale_x, breath_scale_y)
            .pre_translate(-12.0, -12.0);

        let style = self.get_style_config();

        // Custom shape paths based on style
        let mut pb = PathBuilder::new();
        match style.name {
            "mochi" => {
                // 抹茶小猫主体 (融入挺立左偏猫耳 + 侧趴后背与脸部下方前爪起伏)
                pb.move_to(8.0, 21.5);
                // 左脸边缘
                pb.quad_to(5.8, 21.5, 5.8, 19.0);
                pb.cubic_to(5.5, 14.5, 6.5, 10.0, 7.5, 8.5);

                // 左耳尖 (8.5, 2.0)
                pb.cubic_to(7.2, 5.5, 7.8, 3.5, 8.5, 2.0);
                // 左耳内侧
                pb.cubic_to(9.2, 3.5, 10.5, 5.5, 11.5, 8.0);

                // 头顶 (两耳间)
                pb.quad_to(13.0, 8.2, 14.5, 8.0);

                // 右耳尖 (15.5, 2.0)
                pb.cubic_to(14.8, 5.5, 15.2, 3.5, 15.5, 2.0);
                // 右耳外侧 (到后背连接处)
                pb.cubic_to(16.2, 3.5, 17.2, 5.5, 17.8, 8.5);

                // 圆润后背与大臀部 (延伸到 42.0 臀部)
                pb.cubic_to(22.0, 7.5, 38.0, 9.0, 42.0, 16.0);
                pb.quad_to(43.0, 18.0, 42.0, 20.0);
                pb.quad_to(41.5, 21.5, 39.5, 21.5);

                // 底部腹部
                pb.line_to(8.0, 21.5);
                pb.close();
            }
            "starlit" => {
                // 卡通章鱼头：顶部圆润饱满，下部在眼部下方略微收脖子，底口平滑
                pb.move_to(11.0, 17.5);
                pb.cubic_to(5.0, 16.0, 5.0, 4.0, 24.0, 4.0);
                pb.cubic_to(43.0, 4.0, 44.0, 16.0, 37.0, 17.5);
                pb.line_to(11.0, 17.5);
                pb.close();
            }
            "crab" => {
                // 螃蟹主体：左右略扁的红润大椭圆
                pb.move_to(9.0, 16.0);
                pb.cubic_to(9.0, 9.5, 39.0, 9.5, 39.0, 16.0);
                pb.cubic_to(39.0, 22.5, 9.0, 22.5, 9.0, 16.0);
                pb.close();
            }
            _ => {
                // 经典的云朵
                pb.move_to(8.0, 24.0);
                pb.cubic_to(4.0, 20.0, 4.0, 12.0, 10.0, 10.0);
                pb.cubic_to(10.0, 4.0, 18.0, 4.0, 22.0, 8.0);
                pb.cubic_to(26.0, 2.0, 36.0, 4.0, 38.0, 12.0);
                pb.cubic_to(44.0, 14.0, 44.0, 22.0, 38.0, 24.0);
                pb.line_to(8.0, 24.0);
                pb.close();
            }
        }
        let body_path = pb.finish().unwrap();

        let mut mask_paint = Paint::default();
        mask_paint.set_color(Color::from_rgba8(255, 255, 255, 255));
        mask_paint.anti_alias = true;
        self.body_mask.fill_path(
            &body_path,
            &mask_paint,
            FillRule::Winding,
            body_transform,
            None,
        );
        self.body_mask_ready = true;

        if style.name == "mochi" {
            // Swinging Tail (盘旋向上的 S 形猫尾巴)
            let tail_speed = match self.pet_state.as_str() {
                "sleeping" => 1.8,
                "surprised" => 7.0,
                _ => 3.5,
            };
            let tail_amp = match self.pet_state.as_str() {
                "sleeping" => 0.08,
                "surprised" => 0.28,
                _ => 0.16,
            };
            let tail_rot = (anim_t * tail_speed).sin() * tail_amp * anim_amplitude;

            let mut tail_pb = PathBuilder::new();
            tail_pb.move_to(37.0, 19.5);
            tail_pb.cubic_to(
                40.0 + tail_rot * 6.0,
                20.5,
                45.0 + tail_rot * 8.0,
                16.0 - tail_rot * 5.0,
                43.5 + tail_rot * 4.0,
                9.0 - tail_rot * 8.0,
            );
            tail_pb.cubic_to(
                42.5 + tail_rot * 2.0,
                5.5 - tail_rot * 9.0,
                39.5 + tail_rot * 2.0,
                7.5 - tail_rot * 8.0,
                40.0 + tail_rot * 3.0,
                11.5 - tail_rot * 6.0,
            );
            tail_pb.cubic_to(
                40.5 + tail_rot * 4.0,
                15.0,
                38.5 + tail_rot * 3.0,
                20.0,
                37.0,
                21.0,
            );
            tail_pb.close();
            if let Some(tail_path) = tail_pb.finish() {
                let mut tail_paint = Paint::default();
                tail_paint.set_color(Color::from_rgba8(200, 235, 210, 255));
                tail_paint.anti_alias = true;
                self.pixmap.fill_path(
                    &tail_path,
                    &tail_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );

                let mut tail_stroke = Paint::default();
                tail_stroke.set_color(style.outer_rim_color);
                tail_stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &tail_path,
                    &tail_stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }
        }

        // 1. Draw Rim & Body
        let mut rim_paint = Paint::default();
        rim_paint.set_color(style.outer_rim_color);
        rim_paint.anti_alias = true;
        self.pixmap.stroke_path(
            &body_path,
            &rim_paint,
            &Stroke {
                width: 0.9,
                ..Default::default()
            },
            body_transform,
            None,
        );

        let mut rim_tight_paint = Paint::default();
        rim_tight_paint.set_color(style.inner_rim_color);
        rim_tight_paint.anti_alias = true;
        self.pixmap.stroke_path(
            &body_path,
            &rim_tight_paint,
            &Stroke {
                width: 0.45,
                ..Default::default()
            },
            body_transform,
            None,
        );

        let gradient_stops: Vec<tiny_skia::GradientStop> = style
            .body_color_stops
            .iter()
            .map(|(pos, color)| tiny_skia::GradientStop::new(*pos, *color))
            .collect();

        let body_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(12.0, 2.0),
            tiny_skia::Point::from_xy(12.0, 22.0),
            gradient_stops,
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap();

        let body_paint = Paint {
            shader: body_grad,
            anti_alias: true,
            ..Default::default()
        };

        let mut shadow_paint = Paint::default();
        shadow_paint.set_color(style.shadow_color);
        shadow_paint.anti_alias = true;
        self.pixmap.fill_path(
            &body_path,
            &shadow_paint,
            FillRule::Winding,
            body_transform.post_translate(0.0, 1.5),
            None,
        );

        self.pixmap.fill_path(
            &body_path,
            &body_paint,
            FillRule::Winding,
            body_transform,
            None,
        );

        // --- Render style-specific accessories / decor ---
        if style.name == "mochi" {
            // Draw Dynamic Paw Kneading (前爪交替踩奶动效)
            let loop_t = t % 20.0;
            let is_idle = self.pet_state == "idle"
                || (![
                    "sleeping",
                    "thinking",
                    "tickled",
                    "surprised",
                    "bored",
                    "muttering",
                    "working",
                    "tired",
                ]
                .contains(&self.pet_state.as_str()));

            let mut paw_l_transform = body_transform;
            let mut paw_r_transform = body_transform;

            if is_idle && (10.0..15.0).contains(&loop_t) {
                // 舔爪状态
                let lick_phase = (loop_t - 10.0) / 5.0;
                let w = if lick_phase < 0.15 {
                    lick_phase / 0.15
                } else if lick_phase >= 0.85 {
                    (1.0 - lick_phase) / 0.15
                } else {
                    1.0
                };

                // 左爪抬起舔爪子，中点为 (10.2, 21.0)
                let paw_l_tx = w * (10.5 - 10.2) + w * (t * 22.0).cos() * 0.12;
                let paw_l_ty = w * (16.5 - 21.0) + w * (t * 18.0).sin() * 0.15;
                let paw_l_rot = w * 0.22; // 微微倾斜
                paw_l_transform = body_transform
                    .pre_translate(10.2, 21.0)
                    .pre_translate(paw_l_tx, paw_l_ty)
                    .pre_rotate(paw_l_rot)
                    .pre_translate(-10.2, -21.0);

                // 右爪静止趴在默认位置
            } else if is_idle && (15.0..20.0).contains(&loop_t) {
                // 洗脸状态
                let groom_phase = (loop_t - 15.0) / 5.0;
                let w = if groom_phase < 0.15 {
                    groom_phase / 0.15
                } else if groom_phase >= 0.85 {
                    (1.0 - groom_phase) / 0.15
                } else {
                    1.0
                };

                // 右前爪抬升至右眼和右耳上方 (17.5, 8.0) 处，随后大范围划沙子洗脸，中点为 (15.5, 21.0)
                let circle_t = t * 7.5;
                let groom_x = w * (17.5 - 15.5) + w * (circle_t.cos() * 1.5);
                let groom_y = w * (11.0 - 21.0) + w * (circle_t.sin() * 2.5);
                let paw_r_rot = -w * 0.4 + w * (circle_t.cos() * 0.2);
                paw_r_transform = body_transform
                    .pre_translate(15.5, 21.0)
                    .pre_translate(groom_x, groom_y)
                    .pre_rotate(paw_r_rot)
                    .pre_translate(-15.5, -21.0);

                // 左爪静止趴在默认位置
            } else {
                // 默认交替踩奶状态 (0s-10s)
                let paw_speed = match self.pet_state.as_str() {
                    "sleeping" => 1.8,
                    "tickled" => 8.0,
                    "surprised" => 0.0,
                    _ => 3.5,
                };
                let paw_amp = match self.pet_state.as_str() {
                    "sleeping" => 0.06,
                    "tickled" => 0.65,
                    "surprised" => 0.0,
                    _ => 0.28,
                };

                let paw_l_dy = (anim_t * paw_speed).sin() * paw_amp * anim_amplitude;
                let paw_r_dy =
                    (anim_t * paw_speed + std::f32::consts::PI).sin() * paw_amp * anim_amplitude;

                paw_l_transform = body_transform.pre_translate(0.0, paw_l_dy);
                paw_r_transform = body_transform.pre_translate(0.0, paw_r_dy);
            }

            let mut paw_fill_paint = Paint::default();
            paw_fill_paint.set_color(Color::from_rgba8(195, 235, 210, 255)); // 抹茶绿底部浅色
            paw_fill_paint.anti_alias = true;

            let mut paw_stroke_paint = Paint::default();
            paw_stroke_paint.set_color(style.outer_rim_color);
            paw_stroke_paint.anti_alias = true;
            let paw_stroke = Stroke {
                width: 0.82,
                ..Default::default()
            };

            // 1. 左前爪 (x 范围约 8.2 到 12.2)
            let mut left_fill_pb = PathBuilder::new();
            left_fill_pb.move_to(8.2, 21.0);
            left_fill_pb.cubic_to(8.2, 24.2, 12.2, 24.2, 12.2, 21.0);
            left_fill_pb.close();

            let mut left_stroke_pb = PathBuilder::new();
            left_stroke_pb.move_to(8.2, 21.0);
            left_stroke_pb.cubic_to(8.2, 24.2, 12.2, 24.2, 12.2, 21.0);

            if let (Some(fp), Some(sp)) = (left_fill_pb.finish(), left_stroke_pb.finish()) {
                self.pixmap.fill_path(
                    &fp,
                    &paw_fill_paint,
                    FillRule::Winding,
                    paw_l_transform,
                    None,
                );
                self.pixmap
                    .stroke_path(&sp, &paw_stroke_paint, &paw_stroke, paw_l_transform, None);
            }

            // 2. 右前爪 (x 范围约 13.5 到 17.5)
            let mut right_fill_pb = PathBuilder::new();
            right_fill_pb.move_to(13.5, 21.0);
            right_fill_pb.cubic_to(13.5, 24.2, 17.5, 24.2, 17.5, 21.0);
            right_fill_pb.close();

            let mut right_stroke_pb = PathBuilder::new();
            right_stroke_pb.move_to(13.5, 21.0);
            right_stroke_pb.cubic_to(13.5, 24.2, 17.5, 24.2, 17.5, 21.0);

            if let (Some(fp), Some(sp)) = (right_fill_pb.finish(), right_stroke_pb.finish()) {
                self.pixmap.fill_path(
                    &fp,
                    &paw_fill_paint,
                    FillRule::Winding,
                    paw_r_transform,
                    None,
                );
                self.pixmap
                    .stroke_path(&sp, &paw_stroke_paint, &paw_stroke, paw_r_transform, None);
            }

            // Draw Cat Inner Ears (粉嫩内耳廓，适配立耳位置)
            // Left Inner Ear
            let mut inner_l_pb = PathBuilder::new();
            inner_l_pb.move_to(8.1, 7.8);
            inner_l_pb.cubic_to(8.3, 5.0, 8.4, 3.8, 8.5, 2.8);
            inner_l_pb.cubic_to(8.8, 3.8, 9.4, 5.0, 10.8, 7.5);
            inner_l_pb.close();
            if let Some(ipath) = inner_l_pb.finish() {
                let mut inner_paint = Paint::default();
                inner_paint.set_color(Color::from_rgba8(255, 190, 200, 255));
                inner_paint.anti_alias = true;
                self.pixmap.fill_path(
                    &ipath,
                    &inner_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );

                let mut inner_stroke = Paint::default();
                inner_stroke.set_color(Color::from_rgba8(255, 255, 255, 120));
                inner_stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &ipath,
                    &inner_stroke,
                    &Stroke {
                        width: 0.35,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // Right Inner Ear
            let mut inner_r_pb = PathBuilder::new();
            inner_r_pb.move_to(17.1, 7.8);
            inner_r_pb.cubic_to(16.7, 5.0, 16.6, 3.8, 15.5, 2.8);
            inner_r_pb.cubic_to(15.2, 3.8, 15.6, 5.0, 15.0, 7.5);
            inner_r_pb.close();
            if let Some(ipath) = inner_r_pb.finish() {
                let mut inner_paint = Paint::default();
                inner_paint.set_color(Color::from_rgba8(255, 190, 200, 255));
                inner_paint.anti_alias = true;
                self.pixmap.fill_path(
                    &ipath,
                    &inner_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );

                let mut inner_stroke = Paint::default();
                inner_stroke.set_color(Color::from_rgba8(255, 255, 255, 120));
                inner_stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &ipath,
                    &inner_stroke,
                    &Stroke {
                        width: 0.35,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // Draw Cat Whiskers (猫胡须)
            let mut whiskers_paint = Paint::default();
            whiskers_paint.set_color(Color::from_rgba8(35, 40, 68, 180));
            whiskers_paint.anti_alias = true;
            let whiskers_stroke = Stroke {
                width: 0.42,
                ..Default::default()
            };

            // 左脸胡须
            let mut w_l1 = PathBuilder::new();
            w_l1.move_to(7.5, 15.0);
            w_l1.line_to(3.5, 14.3);
            if let Some(p) = w_l1.finish() {
                self.pixmap.stroke_path(
                    &p,
                    &whiskers_paint,
                    &whiskers_stroke,
                    body_transform,
                    None,
                );
            }
            let mut w_l2 = PathBuilder::new();
            w_l2.move_to(7.8, 16.0);
            w_l2.line_to(3.7, 17.1);
            if let Some(p) = w_l2.finish() {
                self.pixmap.stroke_path(
                    &p,
                    &whiskers_paint,
                    &whiskers_stroke,
                    body_transform,
                    None,
                );
            }

            // 右脸胡须 (透视缩短)
            let mut w_r1 = PathBuilder::new();
            w_r1.move_to(16.5, 15.0);
            w_r1.line_to(19.5, 14.5);
            if let Some(p) = w_r1.finish() {
                self.pixmap.stroke_path(
                    &p,
                    &whiskers_paint,
                    &whiskers_stroke,
                    body_transform,
                    None,
                );
            }
            let mut w_r2 = PathBuilder::new();
            w_r2.move_to(16.2, 15.8);
            w_r2.line_to(19.0, 16.6);
            if let Some(p) = w_r2.finish() {
                self.pixmap.stroke_path(
                    &p,
                    &whiskers_paint,
                    &whiskers_stroke,
                    body_transform,
                    None,
                );
            }

            // Draw Red Neck Collar (脖子红颈圈，围在左侧脖子区域)
            let mut collar_pb = PathBuilder::new();
            collar_pb.move_to(6.5, 19.2);
            collar_pb.quad_to(12.0, 20.6, 17.5, 19.2);
            if let Some(cp) = collar_pb.finish() {
                let mut collar_paint = Paint::default();
                collar_paint.set_color(Color::from_rgba8(255, 60, 60, 255));
                collar_paint.anti_alias = true;
                self.pixmap.stroke_path(
                    &cp,
                    &collar_paint,
                    &Stroke {
                        width: 1.1,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // Draw Golden Bell (金色小铃铛，精准对齐 x=12.0 胸口)
            let bell_cx = 12.0;
            let bell_cy = 20.0;
            let bell_r = 1.3;
            if let Some(bell_rect) = Rect::from_xywh(
                bell_cx - bell_r,
                bell_cy - bell_r,
                bell_r * 2.0,
                bell_r * 2.0,
            ) {
                let bell_pb = PathBuilder::from_rect(bell_rect);
                let mut bell_paint = Paint::default();
                bell_paint.set_color(Color::from_rgba8(255, 215, 0, 255));
                bell_paint.anti_alias = true;
                self.pixmap.fill_path(
                    &bell_pb,
                    &bell_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );

                let mut stroke_paint = Paint::default();
                stroke_paint.set_color(Color::from_rgba8(35, 40, 68, 240));
                stroke_paint.anti_alias = true;
                self.pixmap.stroke_path(
                    &bell_pb,
                    &stroke_paint,
                    &Stroke {
                        width: 0.45,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );

                // 铃铛中线
                let mut line_pb = PathBuilder::new();
                line_pb.move_to(bell_cx - bell_r * 0.9, bell_cy + 0.1);
                line_pb.line_to(bell_cx + bell_r * 0.9, bell_cy + 0.1);
                if let Some(lp) = line_pb.finish() {
                    self.pixmap.stroke_path(
                        &lp,
                        &stroke_paint,
                        &Stroke {
                            width: 0.35,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }

                // 铃铛底孔
                if let Some(hole_rect) = Rect::from_xywh(bell_cx - 0.35, bell_cy + 0.5, 0.7, 0.7) {
                    let hole_pb = PathBuilder::from_rect(hole_rect);
                    self.pixmap.fill_path(
                        &hole_pb,
                        &stroke_paint,
                        FillRule::Winding,
                        body_transform,
                        None,
                    );
                }
            }
        }

        if style.name == "starlit" {
            // Draw 6 Chubby-and-Curved Soft Tentacles (粗壮微卷软体触手)
            let tentacle_x_anchors = [9.0, 14.0, 19.0, 29.0, 34.0, 39.0];
            for (i, &x_anchor) in tentacle_x_anchors.iter().enumerate() {
                let idx = i as f32;
                let phase = idx * 1.35 + (idx * 2.1).cos() * 0.6;
                let speed_factor = 2.2 + (idx * 1.6).sin().abs() * 1.0;
                let tentacle_t = anim_t * speed_factor + phase;
                let wave_x =
                    ((tentacle_t).sin() * 1.6 + (tentacle_t * 0.55).cos() * 0.35) * anim_amplitude;
                let wave_y = ((tentacle_t + 1.15).cos() * 0.8) * anim_amplitude;

                let mut leg_pb = PathBuilder::new();
                leg_pb.move_to(x_anchor, 16.5);

                // 根部 ctrl_x1/y1 仅微弱颤动 (定锚)
                let ctrl_x1 = x_anchor + wave_x * 0.15 - if i < 3 { 2.2 } else { -2.2 };
                let ctrl_y1 = 20.0 + wave_y * 0.10;
                // 中部 ctrl_x2/y2 幅度适中
                let ctrl_x2 = x_anchor + wave_x * 0.75 - if i < 3 { 3.2 } else { -3.2 };
                let ctrl_y2 = 24.0 + wave_y * 0.60;
                // 梢部 end_x/y 幅度最大
                let end_x = x_anchor + wave_x * 1.7 - if i < 3 { 2.0 } else { -2.0 };
                let end_y = 25.5 + wave_y * 1.1;

                leg_pb.cubic_to(ctrl_x1, ctrl_y1, ctrl_x2, ctrl_y2, end_x, end_y);

                let leg_w = 2.4f32; // 触手宽度/厚度
                leg_pb.cubic_to(
                    ctrl_x2 + if i < 3 { leg_w } else { -leg_w },
                    ctrl_y2,
                    ctrl_x1 + if i < 3 { leg_w * 0.8 } else { -leg_w * 0.8 },
                    ctrl_y1,
                    x_anchor + if i < 3 { leg_w } else { -leg_w },
                    16.5,
                );
                leg_pb.close();

                if let Some(leg_path) = leg_pb.finish() {
                    let mut leg_paint = Paint::default();
                    leg_paint.set_color(Color::from_rgba8(190, 185, 245, 255));
                    leg_paint.anti_alias = true;
                    self.pixmap.fill_path(
                        &leg_path,
                        &leg_paint,
                        FillRule::Winding,
                        body_transform,
                        None,
                    );

                    let mut leg_stroke = Paint::default();
                    leg_stroke.set_color(style.outer_rim_color);
                    leg_stroke.anti_alias = true;
                    self.pixmap.stroke_path(
                        &leg_path,
                        &leg_stroke,
                        &Stroke {
                            width: 0.5,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );

                    // Draw Suction Cups (可爱的两颗亮色吸盘小点)
                    let mut cup_paint = Paint::default();
                    cup_paint.set_color(Color::from_rgba8(255, 245, 200, 235));
                    cup_paint.anti_alias = true;

                    let mut cup_stroke = Paint::default();
                    cup_stroke.set_color(style.outer_rim_color);
                    cup_stroke.anti_alias = true;

                    // 吸盘 1 (中下部外侧)
                    let cup1_cx = ctrl_x2 + if i < 3 { -0.3 } else { 0.3 };
                    let cup1_cy = ctrl_y2 - 0.2;
                    let cup_r = 0.55f32;
                    let mut cup1_pb_builder = PathBuilder::new();
                    cup1_pb_builder.push_circle(cup1_cx, cup1_cy, cup_r);
                    if let Some(cup_pb) = cup1_pb_builder.finish() {
                        self.pixmap.fill_path(
                            &cup_pb,
                            &cup_paint,
                            FillRule::Winding,
                            body_transform,
                            None,
                        );
                        self.pixmap.stroke_path(
                            &cup_pb,
                            &cup_stroke,
                            &Stroke {
                                width: 0.3,
                                ..Default::default()
                            },
                            body_transform,
                            None,
                        );
                    }

                    // 吸盘 2 (末端前侧)
                    let cup2_cx = end_x + if i < 3 { -0.2 } else { 0.2 };
                    let cup2_cy = end_y - 1.2;
                    let mut cup2_pb_builder = PathBuilder::new();
                    cup2_pb_builder.push_circle(cup2_cx, cup2_cy, cup_r);
                    if let Some(cup_pb) = cup2_pb_builder.finish() {
                        self.pixmap.fill_path(
                            &cup_pb,
                            &cup_paint,
                            FillRule::Winding,
                            body_transform,
                            None,
                        );
                        self.pixmap.stroke_path(
                            &cup_pb,
                            &cup_stroke,
                            &Stroke {
                                width: 0.3,
                                ..Default::default()
                            },
                            body_transform,
                            None,
                        );
                    }
                }
            }

            // Shining Star
            let star_alpha = if self.pet_state == "sleeping" {
                (70.0 + (t * 2.0).sin().abs() * 50.0) as u8
            } else {
                245
            };

            let star_size = 1.6 + (t * 3.0).sin().abs() * 0.3;
            let star_x = 24.0;
            let star_y = 3.5 - breath_wave * 0.3;

            let mut star_pb = PathBuilder::new();
            star_pb.move_to(star_x, star_y - star_size);
            star_pb.quad_to(star_x, star_y, star_x + star_size, star_y);
            star_pb.quad_to(star_x, star_y, star_x, star_y + star_size);
            star_pb.quad_to(star_x, star_y, star_x - star_size, star_y);
            star_pb.quad_to(star_x, star_y, star_x, star_y - star_size);
            star_pb.close();

            if let Some(star_path) = star_pb.finish() {
                let mut glow_paint = Paint::default();
                glow_paint.set_color(Color::from_rgba8(
                    255,
                    235,
                    140,
                    (star_alpha as f32 * 0.18) as u8,
                ));
                glow_paint.anti_alias = true;
                let glow_stroke = Stroke {
                    width: 2.2,
                    ..Default::default()
                };
                self.pixmap.stroke_path(
                    &star_path,
                    &glow_paint,
                    &glow_stroke,
                    body_transform,
                    None,
                );

                let mut star_paint = Paint::default();
                star_paint.set_color(Color::from_rgba8(255, 238, 110, star_alpha));
                star_paint.anti_alias = true;
                self.pixmap.fill_path(
                    &star_path,
                    &star_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );

                let mut star_core_pb = PathBuilder::new();
                let core_size = star_size * 0.42;
                star_core_pb.move_to(star_x, star_y - core_size);
                star_core_pb.quad_to(star_x, star_y, star_x + core_size, star_y);
                star_core_pb.quad_to(star_x, star_y, star_x, star_y + core_size);
                star_core_pb.quad_to(star_x, star_y, star_x - core_size, star_y);
                star_core_pb.quad_to(star_x, star_y, star_x, star_y - core_size);
                star_core_pb.close();
                if let Some(core_path) = star_core_pb.finish() {
                    let mut core_paint = Paint::default();
                    core_paint.set_color(Color::from_rgba8(255, 255, 255, star_alpha));
                    core_paint.anti_alias = true;
                    self.pixmap.fill_path(
                        &core_path,
                        &core_paint,
                        FillRule::Winding,
                        body_transform,
                        None,
                    );
                }
            }

            // 2. Draw Floating Starry Crown (星光金皇冠，随呼吸上下差向漂浮)
            let crown_dy = -breath_wave * 0.18;
            let crown_transform = body_transform.pre_translate(0.0, crown_dy);

            let mut crown_pb = PathBuilder::new();
            crown_pb.move_to(20.0, 3.8);
            crown_pb.line_to(20.0, 1.8);
            crown_pb.line_to(22.0, 2.8);
            crown_pb.line_to(24.0, 0.8); // 皇冠中尖
            crown_pb.line_to(26.0, 2.8);
            crown_pb.line_to(28.0, 1.8);
            crown_pb.line_to(28.0, 3.8);
            crown_pb.close();

            if let Some(cp) = crown_pb.finish() {
                let crown_stops = vec![
                    tiny_skia::GradientStop::new(0.0, Color::from_rgba8(255, 235, 120, 255)),
                    tiny_skia::GradientStop::new(1.0, Color::from_rgba8(255, 170, 40, 255)),
                ];
                if let Some(crown_grad) = LinearGradient::new(
                    tiny_skia::Point::from_xy(24.0, 0.8),
                    tiny_skia::Point::from_xy(24.0, 3.8),
                    crown_stops,
                    tiny_skia::SpreadMode::Pad,
                    Transform::identity(),
                ) {
                    let crown_paint = Paint {
                        shader: crown_grad,
                        anti_alias: true,
                        ..Default::default()
                    };
                    self.pixmap.fill_path(
                        &cp,
                        &crown_paint,
                        FillRule::Winding,
                        crown_transform,
                        None,
                    );

                    let mut crown_stroke_paint = Paint::default();
                    crown_stroke_paint.set_color(Color::from_rgba8(200, 80, 20, 220)); // 橘红细边
                    crown_stroke_paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &cp,
                        &crown_stroke_paint,
                        &Stroke {
                            width: 0.42,
                            ..Default::default()
                        },
                        crown_transform,
                        None,
                    );
                }
            }

            // 绘制皇冠顶端的 3 颗微缩星光珍珠
            let pearl_color = Color::from_rgba8(255, 255, 255, 245);
            let mut pearl_paint = Paint::default();
            pearl_paint.set_color(pearl_color);
            pearl_paint.anti_alias = true;
            let pearl_r = 0.45f32;
            for &(px, py) in &[(20.0f32, 1.8f32), (24.0f32, 0.8f32), (28.0f32, 1.8f32)] {
                if let Some(pr) =
                    Rect::from_xywh(px - pearl_r, py - pearl_r, pearl_r * 2.0, pearl_r * 2.0)
                {
                    let pp = PathBuilder::from_rect(pr);
                    self.pixmap.fill_path(
                        &pp,
                        &pearl_paint,
                        FillRule::Winding,
                        crown_transform,
                        None,
                    );
                }
            }

            // 3. Draw Cosmic Dust Particles (宇宙星尘闪烁小四角星)
            let star_fade_l = (t * 2.2).sin().abs() * 0.82 + 0.18;
            let star_fade_r = (t * 2.2 + std::f32::consts::FRAC_PI_2).sin().abs() * 0.82 + 0.18;

            let star_dy_l = (t * 1.6).cos() * 0.50;
            let star_dy_r = (t * 1.6 + 1.3).sin() * 0.50;

            let build_four_star = |cx: f32, cy: f32, r: f32| -> Option<tiny_skia::Path> {
                let mut pb = PathBuilder::new();
                pb.move_to(cx, cy - r);
                pb.quad_to(cx, cy, cx + r, cy);
                pb.quad_to(cx, cy, cx, cy + r);
                pb.quad_to(cx, cy, cx - r, cy);
                pb.quad_to(cx, cy, cx, cy - r);
                pb.close();
                pb.finish()
            };

            // 左侧星 (x=6.5, y=9.5)
            if let Some(sp_l) = build_four_star(6.5, 9.5 + star_dy_l, 1.25) {
                let mut p_paint = Paint::default();
                p_paint.set_color(Color::from_rgba8(
                    255,
                    255,
                    255,
                    (255.0 * star_fade_l) as u8,
                ));
                p_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&sp_l, &p_paint, FillRule::Winding, body_transform, None);
            }
            // 右侧星 (x=41.5, y=12.5)
            if let Some(sp_r) = build_four_star(41.5, 12.5 + star_dy_r, 1.25) {
                let mut p_paint = Paint::default();
                p_paint.set_color(Color::from_rgba8(
                    255,
                    255,
                    255,
                    (255.0 * star_fade_r) as u8,
                ));
                p_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&sp_r, &p_paint, FillRule::Winding, body_transform, None);
            }
        }

        if style.name == "nimbus" {
            // 根据 active_weather 来选择渲染气象粒子
            match self.active_weather.as_str() {
                "sunny" => {
                    let sun_cx = 35.0f32;
                    let sun_cy = 5.0f32;
                    let mut sun_paint = Paint::default();
                    sun_paint.set_color(Color::from_rgba8(255, 215, 0, 240)); // 亮金
                    sun_paint.anti_alias = true;

                    // 画太阳核心
                    let mut sun_pb = PathBuilder::new();
                    sun_pb.push_circle(sun_cx, sun_cy, 2.5);
                    if let Some(sun_path) = sun_pb.finish() {
                        self.pixmap.fill_path(
                            &sun_path,
                            &sun_paint,
                            FillRule::Winding,
                            body_transform,
                            None,
                        );
                    }

                    // 画 4 道暖日晕光茫
                    let mut ray_paint = Paint::default();
                    ray_paint.set_color(Color::from_rgba8(255, 235, 120, 180)); // 亮黄半透明
                    ray_paint.anti_alias = true;
                    for i in 0..4 {
                        let angle = (i as f32) * std::f32::consts::FRAC_PI_2 + t * 0.4;
                        let dx = angle.cos();
                        let dy = angle.sin();
                        // 光芒线段的起点和终点
                        let start_dist = 3.5f32;
                        let end_dist = 5.8f32;
                        let x1 = sun_cx + dx * start_dist;
                        let y1 = sun_cy + dy * start_dist;
                        let x2 = sun_cx + dx * end_dist;
                        let y2 = sun_cy + dy * end_dist;

                        let mut ray_pb = PathBuilder::new();
                        ray_pb.move_to(x1, y1);
                        ray_pb.line_to(x2, y2);
                        if let Some(ray_path) = ray_pb.finish() {
                            self.pixmap.stroke_path(
                                &ray_path,
                                &ray_paint,
                                &Stroke {
                                    width: 0.8,
                                    ..Default::default()
                                },
                                body_transform,
                                None,
                            );
                        }
                    }
                }
                "windy" => {
                    // 风线 1
                    let mut wind_pb = PathBuilder::new();
                    let y1_offset = (t * 4.0).sin() * 0.6;
                    wind_pb.move_to(8.0, 24.0 + y1_offset);
                    wind_pb.cubic_to(
                        18.0,
                        22.0 - y1_offset,
                        28.0,
                        26.0 + y1_offset,
                        40.0,
                        24.0 - y1_offset,
                    );
                    // 风线 2
                    let mut wind_pb2 = PathBuilder::new();
                    let y2_offset = (t * 4.0 + 2.0).sin() * 0.5;
                    wind_pb2.move_to(6.0, 26.5 + y2_offset);
                    wind_pb2.cubic_to(
                        16.0,
                        28.5 - y2_offset,
                        26.0,
                        24.5 + y2_offset,
                        38.0,
                        26.5 - y2_offset,
                    );

                    let mut wind_paint = Paint::default();
                    wind_paint.set_color(Color::from_rgba8(240, 248, 255, 120)); // 爱丽丝蓝
                    wind_paint.anti_alias = true;
                    let wind_stroke = Stroke {
                        width: 0.45,
                        ..Default::default()
                    };
                    if let Some(wp1) = wind_pb.finish() {
                        self.pixmap.stroke_path(
                            &wp1,
                            &wind_paint,
                            &wind_stroke,
                            body_transform,
                            None,
                        );
                    }
                    if let Some(wp2) = wind_pb2.finish() {
                        self.pixmap.stroke_path(
                            &wp2,
                            &wind_paint,
                            &wind_stroke,
                            body_transform,
                            None,
                        );
                    }
                }
                "snowy" => {
                    let build_snowflake_path = || -> Option<tiny_skia::Path> {
                        let mut pb = PathBuilder::new();
                        // 三条交叉的线段
                        pb.move_to(-0.7, 0.0);
                        pb.line_to(0.7, 0.0);
                        pb.move_to(0.0, -0.7);
                        pb.line_to(0.0, 0.7);
                        // 对角线 1
                        pb.move_to(-0.5, -0.5);
                        pb.line_to(0.5, 0.5);
                        // 对角线 2
                        pb.move_to(-0.5, 0.5);
                        pb.line_to(0.5, -0.5);
                        pb.finish()
                    };

                    let snow_y_start = 23.0f32;
                    let snow_y_end = 32.0f32;
                    let snow_range = snow_y_end - snow_y_start;
                    for i in 0..2 {
                        let phase_offset = if i == 0 { 0.0f32 } else { 0.5f32 };
                        let snow_t = (t * 0.6 + phase_offset) % 1.0;
                        let fade = 1.0 - snow_t;

                        let drift = (t * 2.0 + i as f32 * std::f32::consts::PI).cos() * 1.2;
                        let snow_x = if i == 0 { 18.0f32 } else { 30.0f32 } + drift;
                        let snow_y = snow_y_start + snow_t * snow_range;
                        let snow_r = 1.0 * fade; // 越往下越小

                        let snow_rot = t * 1.5 + i as f32 * 0.8; // 缓缓旋转

                        if let Some(sp) = build_snowflake_path() {
                            let mut s_paint = Paint::default();
                            s_paint.set_color(Color::from_rgba8(
                                255,
                                255,
                                255,
                                (230.0 * fade) as u8,
                            ));
                            s_paint.anti_alias = true;

                            // 构造变换
                            let snow_transform = body_transform
                                .pre_translate(snow_x, snow_y)
                                .pre_scale(snow_r, snow_r)
                                .pre_rotate(snow_rot);

                            self.pixmap.stroke_path(
                                &sp,
                                &s_paint,
                                &Stroke {
                                    width: 0.35,
                                    ..Default::default()
                                },
                                snow_transform,
                                None,
                            );
                        }
                    }
                }
                _ => {
                    // "rainy" 细雨或默认
                    let build_drop_path = |cx: f32, cy: f32, r: f32| -> Option<tiny_skia::Path> {
                        let mut pb = PathBuilder::new();
                        pb.move_to(cx, cy - r * 1.4);
                        pb.cubic_to(cx + r, cy - r * 0.5, cx + r, cy + r, cx, cy + r);
                        pb.cubic_to(cx - r, cy + r, cx - r, cy - r * 0.5, cx, cy - r * 1.4);
                        pb.close();
                        pb.finish()
                    };

                    let drop_y_start = 23.0f32;
                    let drop_y_end = 30.5f32;
                    let drop_range = drop_y_end - drop_y_start;

                    for i in 0..2 {
                        let phase_offset = if i == 0 { 0.0f32 } else { 0.5f32 };
                        let drop_t = (t * 1.4 + phase_offset) % 1.0;
                        let fade = 1.0 - drop_t;

                        let drop_x = if i == 0 { 16.0f32 } else { 32.0f32 }
                            + (t * 2.2 + i as f32 * 1.8).sin() * 0.40;
                        let drop_y = drop_y_start + drop_t * drop_range;
                        let drop_r = 0.52 * fade;

                        if let Some(dp) = build_drop_path(drop_x, drop_y, drop_r) {
                            let mut d_paint = Paint::default();
                            d_paint.set_color(Color::from_rgba8(
                                130,
                                195,
                                255,
                                (160.0 * fade) as u8,
                            ));
                            d_paint.anti_alias = true;
                            self.pixmap.fill_path(
                                &dp,
                                &d_paint,
                                FillRule::Winding,
                                body_transform,
                                None,
                            );

                            let mut d_stroke_paint = Paint::default();
                            d_stroke_paint.set_color(Color::from_rgba8(
                                255,
                                255,
                                255,
                                (170.0 * fade) as u8,
                            ));
                            d_stroke_paint.anti_alias = true;
                            self.pixmap.stroke_path(
                                &dp,
                                &d_stroke_paint,
                                &Stroke {
                                    width: 0.22,
                                    ..Default::default()
                                },
                                body_transform,
                                None,
                            );
                        }
                    }
                }
            }

            // 2. Draw Emotion Lightning (在 surprised 或 thinking 状态下高频闪烁黄色迷你雷电)
            let show_lightning = match self.pet_state.as_str() {
                "surprised" => (t * 22.0).cos() > 0.15,
                "thinking" => (t * 12.0).cos() > 0.45,
                _ => false,
            };

            if show_lightning {
                let mut bolt_pb = PathBuilder::new();
                bolt_pb.move_to(22.5, 22.8);
                bolt_pb.line_to(20.2, 26.5);
                bolt_pb.line_to(23.2, 26.5);
                bolt_pb.line_to(21.2, 30.5);

                if let Some(bp) = bolt_pb.finish() {
                    let mut bolt_paint = Paint::default();
                    bolt_paint.set_color(Color::from_rgba8(255, 230, 30, 245));
                    bolt_paint.anti_alias = true;
                    self.pixmap.fill_path(
                        &bp,
                        &bolt_paint,
                        FillRule::Winding,
                        body_transform,
                        None,
                    );

                    let mut bolt_stroke_paint = Paint::default();
                    bolt_stroke_paint.set_color(Color::from_rgba8(220, 110, 10, 240));
                    bolt_stroke_paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &bp,
                        &bolt_stroke_paint,
                        &Stroke {
                            width: 0.45,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }
        }

        if style.name == "crab" {
            // 1. 螃蟹特有的背甲对称纹理与呼吸小泡泡
            // 1.1 背壳对称弧线纹理 (Carapace Grooves)
            let mut shell_pb = PathBuilder::new();
            // 左侧壳纹
            shell_pb.move_to(14.0, 13.5);
            shell_pb.quad_to(17.0, 14.8, 16.5, 17.5);
            // 右侧壳纹
            shell_pb.move_to(34.0, 13.5);
            shell_pb.quad_to(31.0, 14.8, 31.5, 17.5);

            if let Some(sp) = shell_pb.finish() {
                let mut shell_paint = Paint::default();
                shell_paint.set_color(Color::from_rgba8(255, 155, 120, 190)); // 浅橙红色半透明
                shell_paint.anti_alias = true;
                self.pixmap.stroke_path(
                    &sp,
                    &shell_paint,
                    &Stroke {
                        width: 0.6,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // 1.2 呼吸泡泡动效：从腮部 15.0 升起，随时间淡出并变小破裂
            if self.pet_state != "sleeping" && (t * 0.35).cos() > -0.3 {
                // 泡泡 1 (左腮)
                let bubble_cycle1 = (t * 1.5) % 4.0;
                let scale1 = bubble_cycle1 / 4.0;
                let bubble_y1 = 15.0 - bubble_cycle1 * 2.3;
                let bubble_x1 = 13.0 + (t * 3.5).sin() * 0.9;
                let r1 = 0.70 * (1.1 - scale1);
                if r1 > 0.05 {
                    let mut b1 = PathBuilder::new();
                    b1.push_circle(bubble_x1, bubble_y1, r1);
                    if let Some(bp1) = b1.finish() {
                        let mut b_fill = Paint::default();
                        b_fill.set_color(Color::from_rgba8(180, 230, 255, 90));
                        b_fill.anti_alias = true;
                        self.pixmap.fill_path(
                            &bp1,
                            &b_fill,
                            FillRule::Winding,
                            body_transform,
                            None,
                        );

                        let mut b_stroke = Paint::default();
                        b_stroke.set_color(Color::from_rgba8(255, 255, 255, 175));
                        b_stroke.anti_alias = true;
                        self.pixmap.stroke_path(
                            &bp1,
                            &b_stroke,
                            &Stroke {
                                width: 0.3,
                                ..Default::default()
                            },
                            body_transform,
                            None,
                        );
                    }
                }

                // 泡泡 2 (右腮，相位错开)
                let bubble_cycle2 = (t * 1.5 + 2.0) % 4.0;
                let scale2 = bubble_cycle2 / 4.0;
                let bubble_y2 = 15.0 - bubble_cycle2 * 2.3;
                let bubble_x2 = 35.0 + (t * 3.5 + std::f32::consts::PI).sin() * 0.9;
                let r2 = 0.50 * (1.1 - scale2);
                if r2 > 0.05 {
                    let mut b2 = PathBuilder::new();
                    b2.push_circle(bubble_x2, bubble_y2, r2);
                    if let Some(bp2) = b2.finish() {
                        let mut b_fill = Paint::default();
                        b_fill.set_color(Color::from_rgba8(180, 230, 255, 75));
                        b_fill.anti_alias = true;
                        self.pixmap.fill_path(
                            &bp2,
                            &b_fill,
                            FillRule::Winding,
                            body_transform,
                            None,
                        );

                        let mut b_stroke = Paint::default();
                        b_stroke.set_color(Color::from_rgba8(255, 255, 255, 150));
                        b_stroke.anti_alias = true;
                        self.pixmap.stroke_path(
                            &bp2,
                            &b_stroke,
                            &Stroke {
                                width: 0.25,
                                ..Default::default()
                            },
                            body_transform,
                            None,
                        );
                    }
                }
            }

            // 2. Draw 6 Crab legs (左右各 3 条蟹脚)
            // 间歇性运动包络：每隔 11 秒，划沙子 3.5 秒，且和钳子时间错开
            let leg_active_wave = (anim_t * 0.57 + 1.8).cos();
            let leg_intensity = ((leg_active_wave - 0.2) / 0.45).clamp(0.0, 1.0);

            let leg_x_anchors_l = [11.0, 15.0, 19.0];
            let leg_x_anchors_r = [29.0, 33.0, 37.0];

            for i in 0..3 {
                let idx = i as f32;
                let leg_t = anim_t * 12.0 + idx * 2.0;
                let dx = leg_t.sin() * 1.4 * leg_intensity * anim_amplitude;
                let dy = (leg_t + 0.8).cos() * 0.7 * leg_intensity * anim_amplitude;

                // 左侧脚 (向左下方弯曲)
                let x_l = leg_x_anchors_l[i];
                let mut leg_pb = PathBuilder::new();
                leg_pb.move_to(x_l, 18.5);
                leg_pb.cubic_to(
                    x_l - 1.5 + dx,
                    20.5 + dy,
                    x_l - 3.5 + dx,
                    22.5 + dy,
                    x_l - 3.0 + dx,
                    24.5 + dy,
                );
                leg_pb.cubic_to(
                    x_l - 2.0 + dx,
                    22.5 + dy,
                    x_l - 0.5 + dx,
                    21.0 + dy,
                    x_l + 1.2,
                    18.5,
                );
                leg_pb.close();
                if let Some(p) = leg_pb.finish() {
                    let mut leg_paint = Paint::default();
                    leg_paint.set_color(Color::from_rgba8(255, 120, 100, 255));
                    leg_paint.anti_alias = true;
                    self.pixmap
                        .fill_path(&p, &leg_paint, FillRule::Winding, body_transform, None);

                    let mut stroke = Paint::default();
                    stroke.set_color(style.outer_rim_color);
                    stroke.anti_alias = true;
                    self.pixmap.stroke_path(
                        &p,
                        &stroke,
                        &Stroke {
                            width: 0.5,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }

                // 右侧脚 (向右下方弯曲)
                let x_r = leg_x_anchors_r[i];
                let mut leg_r_pb = PathBuilder::new();
                leg_r_pb.move_to(x_r, 18.5);
                leg_r_pb.cubic_to(
                    x_r + 1.5 + dx,
                    20.5 + dy,
                    x_r + 3.5 + dx,
                    22.5 + dy,
                    x_r + 3.0 + dx,
                    24.5 + dy,
                );
                leg_r_pb.cubic_to(
                    x_r + 2.0 + dx,
                    22.5 + dy,
                    x_r + 0.5 + dx,
                    21.0 + dy,
                    x_r - 1.2,
                    18.5,
                );
                leg_r_pb.close();
                if let Some(p) = leg_r_pb.finish() {
                    let mut leg_paint = Paint::default();
                    leg_paint.set_color(Color::from_rgba8(255, 120, 100, 255));
                    leg_paint.anti_alias = true;
                    self.pixmap
                        .fill_path(&p, &leg_paint, FillRule::Winding, body_transform, None);

                    let mut stroke = Paint::default();
                    stroke.set_color(style.outer_rim_color);
                    stroke.anti_alias = true;
                    self.pixmap.stroke_path(
                        &p,
                        &stroke,
                        &Stroke {
                            width: 0.5,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }

            // 3. Draw Crab Claws (左右大螯，间歇性开合与挥舞)
            // 钳子动作包络：每隔约 17 秒，舞动和快速“咔嚓” 4.5 秒
            let claw_active_wave = (anim_t * 0.37 + 0.8).sin();
            let claw_intensity = ((claw_active_wave - 0.25) / 0.4).clamp(0.0, 1.0);

            let claw_t = anim_t * 6.5;
            let claw_open = (claw_t * 3.0).sin().abs() * 3.8 * claw_intensity * anim_amplitude; // 仅在包络期开合
            let left_claw_wave = (claw_t).cos() * 0.16 * claw_intensity * anim_amplitude;
            let right_claw_wave = -(claw_t + 0.6).cos() * 0.16 * claw_intensity * anim_amplitude;

            // 左大螯臂
            let mut left_arm = PathBuilder::new();
            left_arm.move_to(11.0, 14.5);
            left_arm.cubic_to(
                7.0,
                13.0 + left_claw_wave * 5.0,
                4.0,
                10.0 + left_claw_wave * 8.0,
                5.0,
                8.0 + left_claw_wave * 6.0,
            );
            left_arm.line_to(7.5, 9.5 + left_claw_wave * 5.0);
            left_arm.close();
            if let Some(ap) = left_arm.finish() {
                let mut arm_paint = Paint::default();
                arm_paint.set_color(Color::from_rgba8(255, 100, 80, 255));
                arm_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&ap, &arm_paint, FillRule::Winding, body_transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(style.outer_rim_color);
                stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &ap,
                    &stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // 左大螯钳子 (内固定瓣 + 外活动瓣)
            let mut l_claw_inner = PathBuilder::new();
            // 内侧咬合面：从底座跟外瓣贴合，到尖端 (几乎平直)
            l_claw_inner.move_to(5.0, 8.0 + left_claw_wave * 6.0);
            l_claw_inner.cubic_to(
                4.4,
                6.5 + left_claw_wave * 6.0,
                3.8,
                4.5 + left_claw_wave * 6.0,
                2.6,
                2.2 + left_claw_wave * 6.0,
            );
            // 身体侧边缘：从尖端回到内侧底座 (7.5, 9.5)，平缓弧线
            l_claw_inner.cubic_to(
                4.0,
                4.0 + left_claw_wave * 6.0,
                5.8,
                6.5 + left_claw_wave * 5.0,
                7.5,
                9.5 + left_claw_wave * 5.0,
            );
            l_claw_inner.close();

            let mut l_claw_outer = PathBuilder::new();
            l_claw_outer.move_to(5.0, 8.0 + left_claw_wave * 6.0);
            // 外侧圆润曲线 (大幅向外鼓起)
            l_claw_outer.cubic_to(
                0.5 - claw_open * 0.8,
                7.0 + left_claw_wave * 6.0,
                0.8 - claw_open * 0.9,
                4.0 + left_claw_wave * 6.0,
                2.6 - claw_open,
                2.2 + left_claw_wave * 6.0,
            );
            // 内侧咬合面 (相对平直)
            l_claw_outer.cubic_to(
                3.8 - claw_open * 0.5,
                4.5 + left_claw_wave * 6.0,
                4.4 - claw_open * 0.2,
                6.5 + left_claw_wave * 6.0,
                5.0,
                8.0 + left_claw_wave * 6.0,
            );
            l_claw_outer.close();

            for p in [l_claw_inner.finish(), l_claw_outer.finish()]
                .into_iter()
                .flatten()
            {
                let mut claw_paint = Paint::default();
                claw_paint.set_color(Color::from_rgba8(255, 75, 60, 255));
                claw_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&p, &claw_paint, FillRule::Winding, body_transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(style.outer_rim_color);
                stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &p,
                    &stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // 右大螯臂
            let mut right_arm = PathBuilder::new();
            right_arm.move_to(37.0, 14.5);
            right_arm.cubic_to(
                41.0,
                13.0 + right_claw_wave * 5.0,
                44.0,
                10.0 + right_claw_wave * 8.0,
                43.0,
                8.0 + right_claw_wave * 6.0,
            );
            right_arm.line_to(40.5, 9.5 + right_claw_wave * 5.0);
            right_arm.close();
            if let Some(ap) = right_arm.finish() {
                let mut arm_paint = Paint::default();
                arm_paint.set_color(Color::from_rgba8(255, 100, 80, 255));
                arm_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&ap, &arm_paint, FillRule::Winding, body_transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(style.outer_rim_color);
                stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &ap,
                    &stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }

            // 右大螯钳子 (内固定瓣 + 外活动瓣)
            let mut r_claw_inner = PathBuilder::new();
            // 内侧咬合面：从底座跟外瓣贴合，到尖端 (几乎平直)
            r_claw_inner.move_to(43.0, 8.0 + right_claw_wave * 6.0);
            r_claw_inner.cubic_to(
                43.6,
                6.5 + right_claw_wave * 6.0,
                44.2,
                4.5 + right_claw_wave * 6.0,
                45.4,
                2.2 + right_claw_wave * 6.0,
            );
            // 身体侧边缘：从尖端回到内侧底座 (40.5, 9.5)，平缓弧线
            r_claw_inner.cubic_to(
                44.0,
                4.0 + right_claw_wave * 6.0,
                42.2,
                6.5 + right_claw_wave * 5.0,
                40.5,
                9.5 + right_claw_wave * 5.0,
            );
            r_claw_inner.close();

            let mut r_claw_outer = PathBuilder::new();
            r_claw_outer.move_to(43.0, 8.0 + right_claw_wave * 6.0);
            // 外侧圆润曲线 (大幅向外鼓起)
            r_claw_outer.cubic_to(
                47.5 + claw_open * 0.8,
                7.0 + right_claw_wave * 6.0,
                47.2 + claw_open * 0.9,
                4.0 + right_claw_wave * 6.0,
                45.4 + claw_open,
                2.2 + right_claw_wave * 6.0,
            );
            // 内侧咬合面 (相对平直)
            r_claw_outer.cubic_to(
                44.2 + claw_open * 0.5,
                4.5 + right_claw_wave * 6.0,
                43.6 + claw_open * 0.2,
                6.5 + right_claw_wave * 6.0,
                43.0,
                8.0 + right_claw_wave * 6.0,
            );
            r_claw_outer.close();

            for p in [r_claw_inner.finish(), r_claw_outer.finish()]
                .into_iter()
                .flatten()
            {
                let mut claw_paint = Paint::default();
                claw_paint.set_color(Color::from_rgba8(255, 75, 60, 255));
                claw_paint.anti_alias = true;
                self.pixmap
                    .fill_path(&p, &claw_paint, FillRule::Winding, body_transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(style.outer_rim_color);
                stroke.anti_alias = true;
                self.pixmap.stroke_path(
                    &p,
                    &stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    body_transform,
                    None,
                );
            }
        }

        // 2. Draw Eyes
        let eye_transform = body_transform.pre_translate(eye_tx, eye_ty);
        let flow_offset = (2.0 * std::f32::consts::PI * t / 4.8).sin() * 4.8;
        let eye_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(12.0, 3.0 + flow_offset),
            tiny_skia::Point::from_xy(12.0, 21.0 + flow_offset),
            vec![
                tiny_skia::GradientStop::new(0.0, Color::from_rgba8(177, 167, 255, 255)),
                tiny_skia::GradientStop::new(0.5, Color::from_rgba8(122, 157, 255, 255)),
                tiny_skia::GradientStop::new(1.0, Color::from_rgba8(57, 65, 255, 255)),
            ],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap();

        let eye_paint = Paint {
            shader: eye_grad,
            anti_alias: true,
            ..Default::default()
        };

        let mut eye_border_paint = Paint::default();
        eye_border_paint.set_color(Color::from_rgba8(255, 255, 255, 180));
        eye_border_paint.anti_alias = true;
        let eye_border_stroke = Stroke {
            width: 0.08,
            ..Default::default()
        };

        let eye_base_w = style.eye_base_w;
        let eye_base_h = style.eye_base_h;
        let left_eye_x = style.left_eye_x;
        let right_eye_x = style.right_eye_x;
        let eye_y = style.eye_y;

        // Draw Left Eye
        draw_eye(
            &mut self.pixmap,
            left_eye_x,
            eye_y,
            eye_base_w * self.eye_width_lerp,
            eye_base_h * self.eye_height_lerp,
            blink_scale,
            &eye_paint,
            &eye_border_paint,
            &eye_border_stroke,
            eye_transform,
            &self.pet_state,
            style.name,
            self.mouse_pos,
            self.top_lid_lerp,
            self.bottom_lid_lerp,
            self.pupil_scale_lerp,
            self.pupil_opacity_lerp,
            self.gaze_active_lerp,
            self.happiness,
            self.excitement,
            t,
        );

        // Draw Right Eye
        draw_eye(
            &mut self.pixmap,
            right_eye_x,
            eye_y,
            eye_base_w * self.eye_width_lerp,
            eye_base_h * self.eye_height_lerp,
            blink_scale,
            &eye_paint,
            &eye_border_paint,
            &eye_border_stroke,
            eye_transform,
            &self.pet_state,
            style.name,
            self.mouse_pos,
            self.top_lid_lerp,
            self.bottom_lid_lerp,
            self.pupil_scale_lerp,
            self.pupil_opacity_lerp,
            self.gaze_active_lerp,
            self.happiness,
            self.excitement,
            t,
        );

        // 2.5 Draw Glasses for Working State
        if self.pet_state == "working" {
            let left_cx = left_eye_x;
            let right_cx = right_eye_x;
            let cy = eye_y;
            let r = (eye_base_w * 0.9).max(3.0); // 镜框半径稍微比眼睛宽一些

            let mut glasses_paint = Paint::default();
            glasses_paint.set_color(Color::from_rgba8(35, 40, 68, 255)); // 黑色镜框
            glasses_paint.anti_alias = true;
            let glasses_stroke = Stroke {
                width: 0.7,
                ..Default::default()
            };

            // 左镜圈
            let mut gl_pb = PathBuilder::new();
            gl_pb.move_to(left_cx + r, cy);
            gl_pb.quad_to(left_cx + r, cy + r, left_cx, cy + r);
            gl_pb.quad_to(left_cx - r, cy + r, left_cx - r, cy);
            gl_pb.quad_to(left_cx - r, cy - r, left_cx, cy - r);
            gl_pb.quad_to(left_cx + r, cy - r, left_cx + r, cy);
            gl_pb.close();
            if let Some(p) = gl_pb.finish() {
                // 填充镜片为半透明防蓝光（淡蓝色）
                let mut glass_fill = Paint::default();
                glass_fill.set_color(Color::from_rgba8(135, 206, 250, 45)); // 浅蓝
                glass_fill.anti_alias = true;
                self.pixmap
                    .fill_path(&p, &glass_fill, FillRule::Winding, body_transform, None);
                self.pixmap
                    .stroke_path(&p, &glasses_paint, &glasses_stroke, body_transform, None);

                // 镜片斜反光
                let mut reflect_pb = PathBuilder::new();
                reflect_pb.move_to(left_cx - r * 0.5, cy - r * 0.5);
                reflect_pb.line_to(left_cx + r * 0.5, cy + r * 0.5);
                if let Some(rp) = reflect_pb.finish() {
                    let mut reflect_paint = Paint::default();
                    reflect_paint.set_color(Color::from_rgba8(255, 255, 255, 120));
                    reflect_paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &rp,
                        &reflect_paint,
                        &Stroke {
                            width: 0.35,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }

            // 右镜圈
            let mut gr_pb = PathBuilder::new();
            gr_pb.move_to(right_cx + r, cy);
            gr_pb.quad_to(right_cx + r, cy + r, right_cx, cy + r);
            gr_pb.quad_to(right_cx - r, cy + r, right_cx - r, cy);
            gr_pb.quad_to(right_cx - r, cy - r, right_cx, cy - r);
            gr_pb.quad_to(right_cx + r, cy - r, right_cx + r, cy);
            gr_pb.close();
            if let Some(p) = gr_pb.finish() {
                // 填充镜片为半透明防蓝光
                let mut glass_fill = Paint::default();
                glass_fill.set_color(Color::from_rgba8(135, 206, 250, 45));
                glass_fill.anti_alias = true;
                self.pixmap
                    .fill_path(&p, &glass_fill, FillRule::Winding, body_transform, None);
                self.pixmap
                    .stroke_path(&p, &glasses_paint, &glasses_stroke, body_transform, None);

                // 镜片斜反光
                let mut reflect_pb = PathBuilder::new();
                reflect_pb.move_to(right_cx - r * 0.5, cy - r * 0.5);
                reflect_pb.line_to(right_cx + r * 0.5, cy + r * 0.5);
                if let Some(rp) = reflect_pb.finish() {
                    let mut reflect_paint = Paint::default();
                    reflect_paint.set_color(Color::from_rgba8(255, 255, 255, 120));
                    reflect_paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &rp,
                        &reflect_paint,
                        &Stroke {
                            width: 0.35,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }

            // 眼镜中梁
            let mut bridge_pb = PathBuilder::new();
            bridge_pb.move_to(left_cx + r, cy);
            bridge_pb.quad_to((left_cx + right_cx) * 0.5, cy - 0.4, right_cx - r, cy);
            if let Some(bp) = bridge_pb.finish() {
                self.pixmap
                    .stroke_path(&bp, &glasses_paint, &glasses_stroke, body_transform, None);
            }

            // 两侧镜腿
            let mut leg_pb = PathBuilder::new();
            leg_pb.move_to(left_cx - r, cy);
            leg_pb.line_to(left_cx - r - 1.2, cy + 0.3);
            leg_pb.move_to(right_cx + r, cy);
            leg_pb.line_to(right_cx + r + 1.2, cy + 0.3);
            if let Some(lp) = leg_pb.finish() {
                self.pixmap
                    .stroke_path(&lp, &glasses_paint, &glasses_stroke, body_transform, None);
            }
        }

        // 3. Draw Blush
        let blush_alpha = if self.pet_state == "sleeping" {
            90
        } else {
            160
        };
        let blush_y_offset = breath_wave * 0.10;

        let mut blush_paint = Paint::default();
        blush_paint.set_color(Color::from_rgba8(255, 160, 175, blush_alpha));
        blush_paint.anti_alias = true;

        let build_blush_oval = |bx: f32, by: f32, bw: f32, bh: f32| -> Option<tiny_skia::Path> {
            let mut pb = PathBuilder::new();
            let cx = bx + bw / 2.0;
            let cy = by + bh / 2.0;
            let rx = bw / 2.0;
            let ry = bh / 2.0;
            let kx = rx * 0.552_284_8;
            let ky = ry * 0.552_284_8;
            pb.move_to(cx - rx, cy);
            pb.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
            pb.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
            pb.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
            pb.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
            pb.close();
            pb.finish()
        };

        if let Some(path) = build_blush_oval(
            style.blush_left_x,
            style.blush_y + blush_y_offset,
            style.blush_w,
            style.blush_h,
        ) {
            self.pixmap
                .fill_path(&path, &blush_paint, FillRule::Winding, body_transform, None);
        }
        if let Some(path) = build_blush_oval(
            style.blush_right_x,
            style.blush_y + blush_y_offset,
            style.blush_w,
            style.blush_h,
        ) {
            self.pixmap
                .fill_path(&path, &blush_paint, FillRule::Winding, body_transform, None);
        }

        // 4. Draw Eased Mouth
        let cx = if style.name == "crab" {
            24.0f32
        } else {
            12.0f32
        };
        let mouth_y = style.mouth_y + blush_y_offset;

        let half_w = 0.5 * self.mouth_width_lerp;
        let lx = cx - half_w;
        let rx = cx + half_w;

        let ly = mouth_y + self.mouth_skew_lerp * 0.3;
        let ry = mouth_y - self.mouth_skew_lerp * 0.3;

        let mut mouth_pb = PathBuilder::new();
        if style.name == "mochi" && self.mouth_open_lerp <= 0.08 {
            let half_w = 0.65 * self.mouth_width_lerp;
            let lx = cx - half_w;
            let rx = cx + half_w;
            let my = mouth_y + 0.1;

            // Left curve
            mouth_pb.move_to(lx, my - 0.2);
            mouth_pb.quad_to(cx - half_w * 0.5, my + 0.45, cx, my);
            // Right curve
            mouth_pb.quad_to(cx + half_w * 0.5, my + 0.45, rx, my - 0.2);
        } else {
            mouth_pb.move_to(lx, ly);

            // Upper Lip Curve
            let ty = mouth_y + 0.15 * (1.0 - self.mouth_open_lerp);
            mouth_pb.quad_to(cx, ty, rx, ry);

            // Lower Lip Curve (Interpolates from closed smile to deep oval)
            let max_y_drop = if self.pet_state == "surprised" {
                style.max_y_drop_surprised
            } else if self.pet_state == "tickled" {
                style.max_y_drop_tickled
            } else {
                style.max_y_drop_default
            };
            let by = mouth_y + 0.3 + max_y_drop * self.mouth_open_lerp;
            mouth_pb.quad_to(cx, by, lx, ly);
            mouth_pb.close();
        }

        if let Some(mouth_path) = mouth_pb.finish() {
            let mut mouth_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };

            if self.mouth_open_lerp > 0.08 {
                // Fill mouth cavity in dark grey
                mouth_paint.set_color(Color::from_rgba8(74, 85, 104, 230));
                self.pixmap.fill_path(
                    &mouth_path,
                    &mouth_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );
            }

            let mut stroke_paint = Paint::default();
            stroke_paint.set_color(Color::from_rgba8(45, 55, 72, 220));
            stroke_paint.anti_alias = true;
            self.pixmap.stroke_path(
                &mouth_path,
                &stroke_paint,
                &Stroke {
                    width: 0.42,
                    ..Default::default()
                },
                body_transform,
                None,
            );
        }

        // 5. Draw Sleep Bubbles & Floating Zs
        if self.sleep_bubble_lerp > 0.01 {
            // Draw mouth/gills saliva bubble
            let bubble_pulse = (2.0 * std::f32::consts::PI * t / 6.0).sin().abs() * 0.45 + 0.65;
            let centers = if style.name == "crab" {
                vec![13.0f32, 35.0f32]
            } else {
                vec![14.5f32]
            };

            for &b_cx in &centers {
                let b_cy = if style.name == "crab" {
                    15.0
                } else {
                    mouth_y + 0.1
                };
                let b_r = 0.95 * self.sleep_bubble_lerp * bubble_pulse;
                let mut bp_builder = PathBuilder::new();
                bp_builder.push_circle(b_cx, b_cy, b_r);
                if let Some(bp) = bp_builder.finish() {
                    let mut b_paint = Paint::default();
                    b_paint.set_color(Color::from_rgba8(220, 240, 255, 140));
                    b_paint.anti_alias = true;

                    let mut b_stroke = Paint::default();
                    b_stroke.set_color(Color::from_rgba8(255, 255, 255, 200));
                    b_stroke.anti_alias = true;

                    self.pixmap
                        .fill_path(&bp, &b_paint, FillRule::Winding, body_transform, None);
                    self.pixmap.stroke_path(
                        &bp,
                        &b_stroke,
                        &Stroke {
                            width: 0.12,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }

            // Draw floating visible zZ glyphs above the head.
            for i in 0..3 {
                let phase = (t * 0.45 + i as f32 * 0.32) % 1.0;
                let fade = (1.0 - phase).powf(0.8);
                let drift = (t * 1.3 + i as f32).sin() * 0.65;
                let base_z_x = if style.name == "crab" { 26.0 } else { 17.0 };
                let z_ox = base_z_x + drift + i as f32 * 1.2;
                let z_oy = 4.2 - phase * 8.0 - i as f32 * 1.7;
                let size_z = 1.45 + i as f32 * 0.28 + phase * 0.78;

                let mut z_pb = PathBuilder::new();
                z_pb.move_to(z_ox - size_z, z_oy - size_z * 0.72);
                z_pb.line_to(z_ox + size_z, z_oy - size_z * 0.72);
                z_pb.line_to(z_ox - size_z, z_oy + size_z * 0.72);
                z_pb.line_to(z_ox + size_z, z_oy + size_z * 0.72);

                if let Some(z_path) = z_pb.finish() {
                    let mut z_paint = Paint::default();
                    z_paint.set_color(Color::from_rgba8(
                        110,
                        140,
                        255,
                        (fade * 210.0 * self.sleep_bubble_lerp) as u8,
                    ));
                    z_paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &z_path,
                        &z_paint,
                        &Stroke {
                            width: 0.56,
                            ..Default::default()
                        },
                        body_transform,
                        None,
                    );
                }
            }
        }
    }
}

// Parametric dynamic eye renderer supporting high-differentiation blending and perfect continuity
#[allow(clippy::too_many_arguments)]
fn draw_eye(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    blink_scale: f32,
    paint: &Paint,
    _border_paint: &Paint,
    _border_stroke: &Stroke,
    transform: Transform,
    pet_state: &str,
    pet_style: &str,
    mouse_pos: Option<(f32, f32)>,
    top_lid_lerp: f32,
    bottom_lid_lerp: f32,
    pupil_scale_lerp: f32,
    pupil_opacity_lerp: f32,
    gaze_active_lerp: f32,
    happiness: f32,
    excitement: f32,
    t: f32,
) {
    if pet_style == "crab" {
        // 1. 绘制眼柄 (Eyestalk)
        // 从上方 cy 延伸到螃蟹头顶 cy + 2.4
        let stalk_top_y = cy - 0.2;
        let stalk_bottom_y = cy + 2.4;

        let mut stalk_pb = PathBuilder::new();
        stalk_pb.move_to(cx - 0.35, stalk_bottom_y);
        stalk_pb.line_to(cx - 0.2, stalk_top_y);
        stalk_pb.line_to(cx + 0.2, stalk_top_y);
        stalk_pb.line_to(cx + 0.35, stalk_bottom_y);
        stalk_pb.close();

        if let Some(stalk_path) = stalk_pb.finish() {
            let mut stalk_paint = Paint::default();
            stalk_paint.set_color(Color::from_rgba8(255, 100, 80, 255)); // 珊瑚红橙色
            stalk_paint.anti_alias = true;
            pixmap.fill_path(
                &stalk_path,
                &stalk_paint,
                FillRule::Winding,
                transform,
                None,
            );

            let mut stalk_stroke = Paint::default();
            stalk_stroke.set_color(Color::from_rgba8(100, 20, 15, 255));
            stalk_stroke.anti_alias = true;
            pixmap.stroke_path(
                &stalk_path,
                &stalk_stroke,
                &Stroke {
                    width: 0.5,
                    ..Default::default()
                },
                transform,
                None,
            );
        }

        // 2. 绘制顶端眼球
        if pet_state == "sleeping" {
            // 闭眼：小弧线
            let mut closed_pb = PathBuilder::new();
            closed_pb.move_to(cx - 0.6, cy);
            closed_pb.quad_to(cx, cy + 0.25, cx + 0.6, cy);
            if let Some(p) = closed_pb.finish() {
                let mut lash_paint = Paint::default();
                lash_paint.set_color(Color::from_rgba8(35, 40, 68, 240));
                lash_paint.anti_alias = true;
                pixmap.stroke_path(
                    &p,
                    &lash_paint,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    transform,
                    None,
                );
            }
            return;
        }

        // 正常睁眼
        let eye_r = 1.05 * blink_scale;
        if eye_r > 0.05 {
            let mut eye_pb = PathBuilder::new();
            eye_pb.push_circle(cx, cy, eye_r);
            if let Some(p) = eye_pb.finish() {
                let mut eye_p = Paint::default();
                eye_p.set_color(Color::from_rgba8(22, 28, 45, 255)); // 深亮乌黑眼珠
                eye_p.anti_alias = true;
                pixmap.fill_path(&p, &eye_p, FillRule::Winding, transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(Color::from_rgba8(100, 20, 15, 255));
                stroke.anti_alias = true;
                pixmap.stroke_path(
                    &p,
                    &stroke,
                    &Stroke {
                        width: 0.5,
                        ..Default::default()
                    },
                    transform,
                    None,
                );
            }

            // 3. 高光追随
            let mut hx = cx - 0.28 * eye_r;
            let mut hy = cy - 0.28 * eye_r;
            if let Some(mpos) = mouse_pos {
                let ex = cx * transform.sx + cy * transform.kx + transform.tx;
                let ey = cx * transform.ky + cy * transform.sy + transform.ty;
                let dx = mpos.0 - ex;
                let dy = mpos.1 - ey;
                let dist = (dx * dx + dy * dy).sqrt().max(0.1);
                hx += (dx / dist) * 0.16 * eye_r;
                hy += (dy / dist) * 0.16 * eye_r;
            }

            // 主高光
            let mut highlight_pb = PathBuilder::new();
            highlight_pb.push_circle(hx, hy, 0.32 * eye_r);
            if let Some(p) = highlight_pb.finish() {
                let mut hl_paint = Paint::default();
                hl_paint.set_color(Color::from_rgba8(255, 255, 255, 255));
                hl_paint.anti_alias = true;
                pixmap.fill_path(&p, &hl_paint, FillRule::Winding, transform, None);
            }

            // 副高光
            let mut highlight_pb2 = PathBuilder::new();
            highlight_pb2.push_circle(cx + 0.32 * eye_r, cy + 0.32 * eye_r, 0.14 * eye_r);
            if let Some(p) = highlight_pb2.finish() {
                let mut hl_paint2 = Paint::default();
                hl_paint2.set_color(Color::from_rgba8(255, 255, 255, 180));
                hl_paint2.anti_alias = true;
                pixmap.fill_path(&p, &hl_paint2, FillRule::Winding, transform, None);
            }
        }
        return;
    }

    let current_h = h * blink_scale;
    let lx = cx - w / 2.0;
    let rx = cx + w / 2.0;

    // 1. Sleeping State: Downward crescent curves with upward feather eyelashes
    if pet_state == "sleeping" {
        let mut sleep_pb = PathBuilder::new();
        sleep_pb.move_to(lx, cy - 0.2);
        sleep_pb.quad_to(cx, cy + 0.6, rx, cy - 0.2);
        if cx < 12.0 {
            // Left eye eyelash
            sleep_pb.move_to(lx, cy - 0.2);
            sleep_pb.quad_to(lx - 0.4, cy - 0.5, lx - 0.6, cy - 0.8);
        } else {
            // Right eye eyelash
            sleep_pb.move_to(rx, cy - 0.2);
            sleep_pb.quad_to(rx + 0.4, cy - 0.5, rx + 0.6, cy - 0.8);
        }
        if let Some(p) = sleep_pb.finish() {
            let mut lash_paint = Paint::default();
            lash_paint.set_color(Color::from_rgba8(35, 40, 68, 230));
            lash_paint.anti_alias = true;
            pixmap.stroke_path(
                &p,
                &lash_paint,
                &Stroke {
                    width: 0.45,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
        return;
    }

    // 2. Tickled State (Excited): Upward smiley arcs with downward feather eyelashes
    if pet_state == "tickled" {
        let mut tickled_pb = PathBuilder::new();
        tickled_pb.move_to(lx, cy + 0.5);
        tickled_pb.quad_to(cx, cy - 0.7, rx, cy + 0.5);
        if cx < 12.0 {
            // Left eye eyelash
            tickled_pb.move_to(lx, cy + 0.5);
            tickled_pb.quad_to(lx - 0.4, cy + 0.7, lx - 0.6, cy + 0.9);
        } else {
            // Right eye eyelash
            tickled_pb.move_to(rx, cy + 0.5);
            tickled_pb.quad_to(rx + 0.4, cy + 0.7, rx + 0.6, cy + 0.9);
        }
        if let Some(p) = tickled_pb.finish() {
            let mut lash_paint = Paint::default();
            lash_paint.set_color(Color::from_rgba8(35, 40, 68, 230));
            lash_paint.anti_alias = true;
            pixmap.stroke_path(
                &p,
                &lash_paint,
                &Stroke {
                    width: 0.45,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
        return;
    }

    // 3. Surprised State: Shocked spiral dizzy eyes
    if pet_state == "surprised" {
        let mut spiral_pb = PathBuilder::new();
        let steps = 40;
        let max_theta = 3.2 * std::f32::consts::PI;
        let base_r = w * 0.48;
        for i in 0..=steps {
            let theta = (i as f32 / steps as f32) * max_theta;
            let r = base_r * (1.0 - 0.78 * (i as f32 / steps as f32));
            let dx = r * theta.cos();
            let dy = r * theta.sin() * (h / w);
            if i == 0 {
                spiral_pb.move_to(cx + dx, cy + dy);
            } else {
                spiral_pb.line_to(cx + dx, cy + dy);
            }
        }
        if let Some(p) = spiral_pb.finish() {
            let mut spiral_paint = Paint::default();
            spiral_paint.set_color(Color::from_rgba8(35, 40, 68, 220));
            spiral_paint.anti_alias = true;
            pixmap.stroke_path(
                &p,
                &spiral_paint,
                &Stroke {
                    width: 0.50,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
        return;
    }

    // 4. Thinking State: Left eye is Wink (smiley curve), Right eye is normal watery star eye
    if pet_state == "thinking" && cx < 12.0 {
        let mut wink_pb = PathBuilder::new();
        wink_pb.move_to(lx, cy + 0.5);
        wink_pb.quad_to(cx, cy - 0.7, rx, cy + 0.5);
        wink_pb.move_to(lx, cy + 0.5);
        wink_pb.quad_to(lx - 0.4, cy + 0.7, lx - 0.6, cy + 0.9);
        if let Some(p) = wink_pb.finish() {
            let mut lash_paint = Paint::default();
            lash_paint.set_color(Color::from_rgba8(35, 40, 68, 230));
            lash_paint.anti_alias = true;
            pixmap.stroke_path(
                &p,
                &lash_paint,
                &Stroke {
                    width: 0.45,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
        return;
    }

    // 5. Bored / Tired State: Flat dead-pan half-closed eyes with crying tear drops on outer corners
    if pet_state == "bored" || pet_state == "tired" {
        let mut bored_pb = PathBuilder::new();
        bored_pb.move_to(lx, cy);
        bored_pb.line_to(rx, cy);
        bored_pb.quad_to(cx, cy + current_h * 0.45, lx, cy);
        bored_pb.close();

        if let Some(p) = bored_pb.finish() {
            pixmap.fill_path(&p, paint, FillRule::Winding, transform, None);

            let mut lash_paint = Paint::default();
            lash_paint.set_color(Color::from_rgba8(35, 40, 68, 230));
            lash_paint.anti_alias = true;
            pixmap.stroke_path(
                &p,
                &lash_paint,
                &Stroke {
                    width: 0.38,
                    ..Default::default()
                },
                transform,
                None,
            );

            // Dropping unenthusiastic pupil
            let p_w = w * 0.32;
            let p_h = h * 0.20;
            let p_cx = cx;
            let p_cy = cy + current_h * 0.22;
            if let Some(pr) = Rect::from_xywh(p_cx - p_w, p_cy - p_h * 0.5, p_w * 2.0, p_h) {
                let pupil_color = match pet_style {
                    "mochi" => Color::from_rgba8(25, 60, 42, 230),
                    "starlit" => Color::from_rgba8(55, 25, 78, 230),
                    _ => Color::from_rgba8(25, 30, 95, 230),
                };
                let mut p_paint = Paint::default();
                p_paint.set_color(pupil_color);
                p_paint.anti_alias = true;
                let p_path = PathBuilder::from_rect(pr);
                pixmap.fill_path(&p_path, &p_paint, FillRule::Winding, transform, None);
            }

            // Tear drops on the outer corners
            let tear_cx = if cx < 12.0 { lx - 0.6 } else { rx + 0.6 };
            let tear_cy = cy + current_h * 0.3;
            let mut tear_pb = PathBuilder::new();
            tear_pb.move_to(tear_cx, tear_cy - 0.7);
            tear_pb.cubic_to(
                tear_cx + 0.5,
                tear_cy,
                tear_cx + 0.5,
                tear_cy + 0.8,
                tear_cx,
                tear_cy + 0.8,
            );
            tear_pb.cubic_to(
                tear_cx - 0.5,
                tear_cy + 0.8,
                tear_cx - 0.5,
                tear_cy,
                tear_cx,
                tear_cy - 0.7,
            );
            tear_pb.close();

            if let Some(tp) = tear_pb.finish() {
                let mut tear_fill = Paint::default();
                tear_fill.set_color(Color::from_rgba8(135, 206, 250, 180));
                tear_fill.anti_alias = true;
                pixmap.fill_path(&tp, &tear_fill, FillRule::Winding, transform, None);

                let mut tear_stroke = Paint::default();
                tear_stroke.set_color(Color::from_rgba8(255, 255, 255, 220));
                tear_stroke.anti_alias = true;
                pixmap.stroke_path(
                    &tp,
                    &tear_stroke,
                    &Stroke {
                        width: 0.25,
                        ..Default::default()
                    },
                    transform,
                    None,
                );
            }
        }
        return;
    }

    // 6. Idle / Muttering / Working / Right-Thinking eye: Watery gorgeous double-sparkle anime eye
    let top_lid_offset = (current_h / 2.0) * top_lid_lerp;
    let bottom_lid_offset = (current_h / 2.0) * bottom_lid_lerp;

    let mut pb = PathBuilder::new();
    pb.move_to(lx, cy);
    pb.quad_to(cx, cy + top_lid_offset, rx, cy);
    pb.quad_to(cx, cy + bottom_lid_offset, lx, cy);
    pb.close();

    if let Some(eye_path) = pb.finish() {
        pixmap.fill_path(&eye_path, paint, FillRule::Winding, transform, None);

        // 1. Draw bold slate eyeliner
        let mut eyeliner_paint = Paint::default();
        eyeliner_paint.set_color(Color::from_rgba8(35, 40, 68, 230));
        eyeliner_paint.anti_alias = true;
        let eyeliner_stroke = Stroke {
            width: 0.38,
            ..Default::default()
        };
        pixmap.stroke_path(
            &eye_path,
            &eyeliner_paint,
            &eyeliner_stroke,
            transform,
            None,
        );

        // 2. Draw Bottom Iris Reflection Glow
        let glow_r_w = w * 0.32;
        let glow_r_h = h * 0.16;
        let glow_cy = cy + h * 0.22;
        if let Some(gr) = Rect::from_xywh(
            cx - glow_r_w,
            glow_cy - glow_r_h,
            glow_r_w * 2.0,
            glow_r_h * 2.0,
        ) {
            let glow_path = PathBuilder::from_rect(gr);
            let mut glow_paint = Paint::default();
            let gc = match pet_style {
                "mochi" => Color::from_rgba8(190, 255, 170, 140),
                "starlit" => Color::from_rgba8(255, 185, 215, 140),
                _ => Color::from_rgba8(175, 220, 255, 140),
            };
            glow_paint.set_color(gc);
            glow_paint.anti_alias = true;
            pixmap.fill_path(&glow_path, &glow_paint, FillRule::Winding, transform, None);
        }

        // 3. Pupil with Gaze Tracking
        if pupil_opacity_lerp > 0.01 {
            let mut ox = 0.0f32;
            let mut oy = 0.0f32;
            if let Some(mpos) = mouse_pos {
                let ex = cx * transform.sx + cy * transform.kx + transform.tx;
                let ey = cx * transform.ky + cy * transform.sy + transform.ty;
                let dx = mpos.0 - ex;
                let dy = mpos.1 - ey;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 0.0 {
                    let scale = (transform.sx * transform.sx + transform.ky * transform.ky).sqrt();
                    let local_dist = dist / scale;
                    let amount = (local_dist * 0.15).min(0.45) * gaze_active_lerp;
                    ox += (dx / dist) * amount;
                    oy += (dy / dist) * amount;
                }
            }
            let ox = ox.clamp(-0.45, 0.45);
            let oy = oy.clamp(-0.45, 0.45);

            let pupil_cx = cx + ox;
            let pupil_cy = cy + oy;

            if happiness > 0.70 {
                // 爱慕粉红桃心眼 (Heart eyes)
                let r = w * 0.38 * pupil_scale_lerp;
                let mut heart_pb = PathBuilder::new();
                heart_pb.move_to(pupil_cx, pupil_cy - r * 0.35);
                heart_pb.cubic_to(
                    pupil_cx - r * 1.15,
                    pupil_cy - r * 1.05,
                    pupil_cx - r * 1.15,
                    pupil_cy + r * 0.25,
                    pupil_cx,
                    pupil_cy + r * 0.75,
                );
                heart_pb.cubic_to(
                    pupil_cx + r * 1.15,
                    pupil_cy + r * 0.25,
                    pupil_cx + r * 1.15,
                    pupil_cy - r * 1.05,
                    pupil_cx,
                    pupil_cy - r * 0.35,
                );
                heart_pb.close();

                if let Some(heart_path) = heart_pb.finish() {
                    let mut heart_paint = Paint::default();
                    heart_paint.set_color(Color::from_rgba8(
                        255,
                        120,
                        160,
                        (245.0 * pupil_opacity_lerp) as u8,
                    ));
                    heart_paint.anti_alias = true;
                    pixmap.fill_path(
                        &heart_path,
                        &heart_paint,
                        FillRule::Winding,
                        transform,
                        None,
                    );

                    let mut stroke_paint = Paint::default();
                    stroke_paint.set_color(Color::from_rgba8(
                        230,
                        20,
                        80,
                        (245.0 * pupil_opacity_lerp) as u8,
                    ));
                    stroke_paint.anti_alias = true;
                    pixmap.stroke_path(
                        &heart_path,
                        &stroke_paint,
                        &Stroke {
                            width: 0.35,
                            ..Default::default()
                        },
                        transform,
                        None,
                    );

                    // Add dynamic white highlight to upper left
                    let h_r = r * 0.22;
                    let h_cx = pupil_cx - r * 0.28;
                    let h_cy = pupil_cy - r * 0.28;
                    if let Some(hr) = Rect::from_xywh(h_cx - h_r, h_cy - h_r, h_r * 2.0, h_r * 2.0)
                    {
                        let mut h_paint = Paint::default();
                        h_paint.set_color(Color::from_rgba8(
                            255,
                            255,
                            255,
                            (250.0 * pupil_opacity_lerp) as u8,
                        ));
                        h_paint.anti_alias = true;
                        let h_path = PathBuilder::from_rect(hr);
                        pixmap.fill_path(&h_path, &h_paint, FillRule::Winding, transform, None);
                    }
                }
            } else if excitement > 0.65 {
                // 明黄色旋转四角星星眼 (Starry eyes)
                let star_r = w * 0.40 * pupil_scale_lerp;
                let theta = t * 1.6; // 随时间旋转
                let mut star_pb = PathBuilder::new();
                for i in 0..4 {
                    let angle_v = theta + i as f32 * std::f32::consts::FRAC_PI_2;
                    let angle_c = angle_v + std::f32::consts::FRAC_PI_4;

                    let vx = pupil_cx + star_r * angle_v.cos();
                    let vy = pupil_cy + star_r * angle_v.sin();

                    let cx_val = pupil_cx + star_r * 0.28 * angle_c.cos();
                    let cy_val = pupil_cy + star_r * 0.28 * angle_c.sin();

                    if i == 0 {
                        star_pb.move_to(vx, vy);
                    } else {
                        star_pb.line_to(vx, vy);
                    }
                    star_pb.line_to(cx_val, cy_val);
                }
                star_pb.close();

                if let Some(star_path) = star_pb.finish() {
                    let mut star_paint = Paint::default();
                    star_paint.set_color(Color::from_rgba8(
                        255,
                        220,
                        50,
                        (245.0 * pupil_opacity_lerp) as u8,
                    ));
                    star_paint.anti_alias = true;
                    pixmap.fill_path(&star_path, &star_paint, FillRule::Winding, transform, None);

                    let mut stroke_paint = Paint::default();
                    stroke_paint.set_color(Color::from_rgba8(
                        200,
                        140,
                        0,
                        (245.0 * pupil_opacity_lerp) as u8,
                    ));
                    stroke_paint.anti_alias = true;
                    pixmap.stroke_path(
                        &star_path,
                        &stroke_paint,
                        &Stroke {
                            width: 0.35,
                            ..Default::default()
                        },
                        transform,
                        None,
                    );

                    // Add white sparkle highlight
                    let h_r = star_r * 0.20;
                    let h_cx = pupil_cx - star_r * 0.22;
                    let h_cy = pupil_cy - star_r * 0.22;
                    if let Some(hr) = Rect::from_xywh(h_cx - h_r, h_cy - h_r, h_r * 2.0, h_r * 2.0)
                    {
                        let mut h_paint = Paint::default();
                        h_paint.set_color(Color::from_rgba8(
                            255,
                            255,
                            255,
                            (250.0 * pupil_opacity_lerp) as u8,
                        ));
                        h_paint.anti_alias = true;
                        let h_path = PathBuilder::from_rect(hr);
                        pixmap.fill_path(&h_path, &h_paint, FillRule::Winding, transform, None);
                    }
                }
            } else if pet_style == "starlit" {
                // 太空章鱼专属：深紫黑色大底色，配合一个金黄色横向椭圆的瞳孔芯，并且有白色闪烁四角星高光
                let pupil_base_color =
                    Color::from_rgba8(45, 20, 68, (245.0 * pupil_opacity_lerp) as u8);
                let p_r = w * 0.38 * pupil_scale_lerp;

                // 1. 绘制深紫黑色瞳底
                if let Some(pr) =
                    Rect::from_xywh(pupil_cx - p_r, pupil_cy - p_r, p_r * 2.0, p_r * 2.0)
                {
                    let mut p_paint = Paint::default();
                    p_paint.set_color(pupil_base_color);
                    p_paint.anti_alias = true;
                    let p_path = PathBuilder::from_rect(pr);
                    pixmap.fill_path(&p_path, &p_paint, FillRule::Winding, transform, None);
                }

                // 2. 绘制金黄色横向椭圆瞳孔芯
                let core_w = p_r * 0.82;
                let core_h = p_r * 0.36;
                let mut core_pb = PathBuilder::new();
                core_pb.move_to(pupil_cx - core_w, pupil_cy);
                core_pb.cubic_to(
                    pupil_cx - core_w,
                    pupil_cy - core_h,
                    pupil_cx + core_w,
                    pupil_cy - core_h,
                    pupil_cx + core_w,
                    pupil_cy,
                );
                core_pb.cubic_to(
                    pupil_cx + core_w,
                    pupil_cy + core_h,
                    pupil_cx - core_w,
                    pupil_cy + core_h,
                    pupil_cx - core_w,
                    pupil_cy,
                );
                core_pb.close();
                if let Some(core_path) = core_pb.finish() {
                    let mut core_paint = Paint::default();
                    core_paint.set_color(Color::from_rgba8(
                        255,
                        215,
                        60,
                        (230.0 * pupil_opacity_lerp) as u8,
                    ));
                    core_paint.anti_alias = true;
                    pixmap.fill_path(&core_path, &core_paint, FillRule::Winding, transform, None);
                }

                // 3. 绘制星光高光 (Starlit 四角星白色发光高光)
                let star_r = w * 0.18 * pupil_scale_lerp;
                let star_cx = pupil_cx - w * 0.12;
                let star_cy = pupil_cy - current_h * 0.12;

                let mut star_pb = PathBuilder::new();
                star_pb.move_to(star_cx, star_cy - star_r);
                star_pb.quad_to(star_cx, star_cy, star_cx + star_r, star_cy);
                star_pb.quad_to(star_cx, star_cy, star_cx, star_cy + star_r);
                star_pb.quad_to(star_cx, star_cy, star_cx - star_r, star_cy);
                star_pb.quad_to(star_cx, star_cy, star_cx, star_cy - star_r);
                star_pb.close();

                if let Some(sp) = star_pb.finish() {
                    let mut star_paint = Paint::default();
                    star_paint.set_color(Color::from_rgba8(
                        255,
                        255,
                        255,
                        (250.0 * pupil_opacity_lerp) as u8,
                    ));
                    star_paint.anti_alias = true;
                    pixmap.fill_path(&sp, &star_paint, FillRule::Winding, transform, None);
                }

                // 4. 副高光 (微小圆形，右下方)
                let mut h2 = PathBuilder::new();
                h2.push_circle(
                    pupil_cx + w * 0.15,
                    pupil_cy + current_h * 0.15,
                    w * 0.08 * pupil_scale_lerp,
                );
                if let Some(hp2) = h2.finish() {
                    let mut hp2_paint = Paint::default();
                    hp2_paint.set_color(Color::from_rgba8(
                        255,
                        255,
                        255,
                        (170.0 * pupil_opacity_lerp) as u8,
                    ));
                    hp2_paint.anti_alias = true;
                    pixmap.fill_path(&hp2, &hp2_paint, FillRule::Winding, transform, None);
                }
            } else {
                // 普通瞳孔绘制
                let pupil_color = match pet_style {
                    "mochi" => Color::from_rgba8(25, 60, 42, (240.0 * pupil_opacity_lerp) as u8),
                    _ => Color::from_rgba8(25, 30, 95, (240.0 * pupil_opacity_lerp) as u8),
                };

                let p_r = w * 0.35 * pupil_scale_lerp;
                if let Some(pr) =
                    Rect::from_xywh(pupil_cx - p_r, pupil_cy - p_r, p_r * 2.0, p_r * 2.0)
                {
                    let mut p_paint = Paint::default();
                    p_paint.set_color(pupil_color);
                    p_paint.anti_alias = true;
                    let p_path = PathBuilder::from_rect(pr);
                    pixmap.fill_path(&p_path, &p_paint, FillRule::Winding, transform, None);
                }

                // 4. Double Sparkle Highlights
                // Primary Highlight (Large, Upper-Left)
                let h_r = w * 0.17 * pupil_scale_lerp;
                let h_cx = pupil_cx - w * 0.11;
                let h_cy = pupil_cy - current_h * 0.11;
                if let Some(hr) = Rect::from_xywh(h_cx - h_r, h_cy - h_r, h_r * 2.0, h_r * 2.0) {
                    let mut h_paint = Paint::default();
                    h_paint.set_color(Color::from_rgba8(
                        255,
                        255,
                        255,
                        (245.0 * pupil_opacity_lerp) as u8,
                    ));
                    h_paint.anti_alias = true;
                    let h_path = PathBuilder::from_rect(hr);
                    pixmap.fill_path(&h_path, &h_paint, FillRule::Winding, transform, None);
                }

                // Secondary Highlight (Small, Semi-transparent, Bottom-Right)
                let h2_r = w * 0.09 * pupil_scale_lerp;
                let h2_cx = pupil_cx + w * 0.13;
                let h2_cy = pupil_cy + current_h * 0.13;
                if let Some(h2r) =
                    Rect::from_xywh(h2_cx - h2_r, h2_cy - h2_r, h2_r * 2.0, h2_r * 2.0)
                {
                    let mut h2_paint = Paint::default();
                    h2_paint.set_color(Color::from_rgba8(
                        255,
                        255,
                        255,
                        (180.0 * pupil_opacity_lerp) as u8,
                    ));
                    h2_paint.anti_alias = true;
                    let h2_path = PathBuilder::from_rect(h2r);
                    pixmap.fill_path(&h2_path, &h2_paint, FillRule::Winding, transform, None);
                }
            }
        }
    }
}

fn rand_offset() -> f32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as f32;
    now.sin() * 0.4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(renderer: &PetRenderer, x: u32, y: u32) -> u8 {
        let idx = ((y * renderer.pixmap.width() + x) * 4 + 3) as usize;
        renderer.pixmap.data()[idx]
    }

    #[test]
    fn bubble_measure_can_expand_after_small_window() {
        let mut renderer = PetRenderer::new(120, 120);
        renderer.draw_pet_enabled = false;
        renderer.draw_bubble_enabled = true;
        renderer.scale_factor = 2.0;
        renderer.set_font_size(22.0);
        renderer.set_text("我醒来啦，先把屏幕边缘的小光点整理好。");

        let (w, h) = renderer.measure_bubble_size();

        assert!(w > 120.0);
        assert!(w <= BUBBLE_MAX_LOGICAL_W * renderer.scale_factor);
        assert!(h > 50.0);
    }

    #[test]
    fn independent_bubble_tail_stays_inside_pixmap() {
        let mut renderer = PetRenderer::new(320, 120);
        renderer.draw_pet_enabled = false;
        renderer.draw_bubble_enabled = true;
        renderer.set_text("轻轻醒来，屏幕还亮着");

        let (w, h) = renderer.measure_bubble_size();
        renderer.resize(w as u32, h as u32);

        renderer.bubble_on_bottom = true;
        renderer.update_and_draw();
        let top_tail_alpha = alpha_at(
            &renderer,
            renderer.pixmap.width() / 2,
            renderer.bubble_edge_safe_pad().ceil() as u32,
        );
        assert!(top_tail_alpha > 0);

        renderer.bubble_on_bottom = false;
        renderer.update_and_draw();
        let bottom_tail_y = renderer.pixmap.height().saturating_sub(
            (renderer.bubble_edge_safe_pad() + renderer.bubble_shadow_pad()).ceil() as u32,
        );
        let bottom_tail_alpha = alpha_at(&renderer, renderer.pixmap.width() / 2, bottom_tail_y);
        assert!(bottom_tail_alpha > 0);
    }

    #[test]
    fn body_hit_test_uses_rendered_cloud_mask() {
        let mut renderer = PetRenderer::new(360, 440);
        assert_eq!(renderer.body_hit_test(180.0, 370.0), None);

        renderer.update_and_draw();

        assert_eq!(renderer.body_hit_test(180.0, 370.0), Some(true));
        assert_eq!(renderer.body_hit_test(180.0, 300.0), Some(false));
        assert_eq!(renderer.body_hit_test(45.0, 370.0), Some(false));
    }
}
