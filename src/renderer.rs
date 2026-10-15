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
    pub outer_rim_color: Color,
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
    full_text: String,
    bubble_max_size: Option<(f32, f32)>,
    bubble_max_lines: usize,
    bubble_layout_dirty: bool,

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
    pub bubble_tail_x: Option<f32>,
}

impl PetRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width, height).unwrap();
        let body_mask = Pixmap::new(width, height).unwrap();
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        let mut text_buffer = Buffer::new(&mut font_system, cosmic_text::Metrics::new(24.0, 30.0));
        text_buffer.set_size(&mut font_system, Some(width.saturating_sub(45).max(1) as f32), None);

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
            full_text: String::new(),
            bubble_max_size: None,
            bubble_max_lines: 8,
            bubble_layout_dirty: true,
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
            bubble_tail_x: None,
        }
    }

    pub fn add_emotion(&mut self, happiness_gain: f32, excitement_gain: f32) {
        self.happiness = (self.happiness + happiness_gain).min(1.0);
        self.excitement = (self.excitement + excitement_gain).min(1.0);
    }

    pub fn get_style_config(&self) -> StyleConfig {
        let material = crate::art::palette(&self.active_style);
        match self.active_style.as_str() {
            "mochi" => StyleConfig {
                name: "mochi",
                eye_base_w: 3.35,
                eye_base_h: 4.25,
                left_eye_x: 9.3,
                right_eye_x: 15.0,
                eye_y: 12.3,
                blush_left_x: 6.6,
                blush_right_x: 16.2,
                blush_y: 14.3,
                blush_w: 2.2,
                blush_h: 1.0,
                mouth_y: 16.0,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                outer_rim_color: material.edge,
            },
            "starlit" => StyleConfig {
                name: "starlit",
                eye_base_w: 3.8,
                eye_base_h: 4.6,
                left_eye_x: 19.2,
                right_eye_x: 28.8,
                eye_y: 11.5,
                blush_left_x: 14.8,
                blush_right_x: 30.7,
                blush_y: 13.5,
                blush_w: 2.5,
                blush_h: 1.1,
                mouth_y: 14.7,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                outer_rim_color: material.edge,
            },
            "crab" => StyleConfig {
                name: "crab",
                eye_base_w: 2.5,
                eye_base_h: 2.5,
                left_eye_x: 20.0,
                right_eye_x: 28.0,
                eye_y: 10.8,
                blush_left_x: 15.1,
                blush_right_x: 30.4,
                blush_y: 14.5,
                blush_w: 2.5,
                blush_h: 1.1,
                mouth_y: 15.3,
                max_y_drop_surprised: 2.0,
                max_y_drop_tickled: 1.6,
                max_y_drop_default: 1.0,
                outer_rim_color: material.edge,
            },
            _ => StyleConfig {
                name: "nimbus",
                eye_base_w: 3.65,
                eye_base_h: 4.8,
                left_eye_x: 18.6,
                right_eye_x: 28.5,
                eye_y: 14.0,
                blush_left_x: 14.3,
                blush_right_x: 30.4,
                blush_y: 16.0,
                blush_w: 2.7,
                blush_h: 1.1,
                mouth_y: 17.5,
                max_y_drop_surprised: 2.2,
                max_y_drop_tickled: 1.8,
                max_y_drop_default: 1.2,
                outer_rim_color: material.edge,
            },
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        self.mouse_pos = pos;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        self.bubble_layout_dirty = true;
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
                Some(wrap_w.max(1.0)),
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
        let (max_window_w, max_window_h) = self.bubble_max_size
            .unwrap_or((BUBBLE_MAX_LOGICAL_W * self.scale_factor, 360.0 * self.scale_factor));
        let max_window_w = max_window_w.min(BUBBLE_MAX_LOGICAL_W * self.scale_factor).max(1.0);
        let max_text_w =
            (max_window_w - self.bubble_horizontal_inset() * 2.0).max(1.0);
        self.text_buffer
            .set_size(&mut self.font_system, Some(max_text_w), None);

        let pad_x = 18.0 * self.scale_factor;
        let pad_top = 12.0 * self.scale_factor;
        let pad_bottom = 8.0 * self.scale_factor;
        let overhead = pad_top + pad_bottom + self.bubble_tail_height()
            + self.bubble_edge_safe_pad() * 2.0 + self.bubble_shadow_pad();
        if self.bubble_layout_dirty {
            self.fit_bubble_text((max_window_h - overhead).max(0.0));
            self.bubble_layout_dirty = false;
        } else {
            self.text_buffer.shape_until_scroll(&mut self.font_system, false);
        }

        if !self.bubble_text_fits((max_window_h - overhead).max(0.0)) {
            return (0.0, 0.0);
        }
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
            .clamp((BUBBLE_MIN_LOGICAL_W * self.scale_factor).min(max_window_w), max_window_w)
            .ceil().min(max_window_w);
        let visual_text_h = height.max(runs_to_draw as f32 * max_line_height);
        let bubble_h =
            (visual_text_h + pad_top + pad_bottom + tail_h + safe_pad * 2.0 + shadow_pad)
                .ceil().min(max_window_h);

        (bubble_w, bubble_h)
    }

    /// Limits are physical pixels, matching the active monitor and text scale.
    pub fn set_bubble_limits(&mut self, max_width: f32, max_height: f32, max_lines: usize) {
        let max_size = Some((max_width.max(1.0), max_height.max(1.0)));
        let max_lines = max_lines.max(1);
        if self.bubble_max_size != max_size || self.bubble_max_lines != max_lines {
            self.bubble_layout_dirty = true;
        }
        self.bubble_max_size = max_size;
        self.bubble_max_lines = max_lines;
    }

    fn bubble_text_fits(&self, max_height: f32) -> bool {
        let mut lines = 0;
        for run in self.text_buffer.layout_runs() {
            lines += 1;
            if lines > self.bubble_max_lines || run.line_top + run.line_height > max_height
                || run.line_w > self.text_buffer.size().0.unwrap_or(f32::INFINITY) + 0.01 {
                return false;
            }
        }
        true
    }

    fn fit_bubble_text(&mut self, max_height: f32) {
        let full_text = self.full_text.clone();
        self.set_display_text(&full_text);
        if self.bubble_text_fits(max_height) {
            return;
        }

        // Keep the original message intact so a larger screen or smaller font restores it.
        let mut suffix = "\n…（托盘查看全文）";
        self.set_display_text(suffix);
        if !self.bubble_text_fits(max_height) {
            suffix = "…";
            self.set_display_text(suffix);
            if !self.bubble_text_fits(max_height) {
                return;
            }
        }
        let mut boundaries: Vec<usize> = full_text.char_indices().map(|(index, _)| index).collect();
        boundaries.push(full_text.len());
        let mut low = 0;
        let mut high = boundaries.len().saturating_sub(1);
        while low < high {
            let mid = low + (high - low + 1) / 2;
            let candidate = format!("{}{}", full_text[..boundaries[mid]].trim_end(), suffix);
            self.set_display_text(&candidate);
            if self.bubble_text_fits(max_height) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let prefix = full_text[..boundaries[low]].trim_end();
        let suffix = if prefix.is_empty() { suffix.trim_start() } else { suffix };
        self.set_display_text(&format!("{prefix}{suffix}"));
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
        self.bubble_layout_dirty = true;
        self.full_text = text.to_string();
        self.set_display_text(text);
    }

    fn set_display_text(&mut self, text: &str) {
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
        let tail_margin = radius + 5.5 * self.scale_factor;
        let cx = if bubble_w >= tail_margin * 2.0 {
            self.bubble_tail_x.unwrap_or(max_w_limit / 2.0)
                .clamp(x + tail_margin, x + bubble_w - tail_margin)
        } else {
            x + bubble_w / 2.0
        };
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

        // Each silhouette is designed as a small, stuffed storybook character.
        let mut pb = PathBuilder::new();
        match style.name {
            "mochi" => {
                // A round cheek and low, bean-shaped body; ears have padded tips.
                pb.move_to(8.0, 21.2);
                pb.cubic_to(4.9, 20.4, 5.1, 12.3, 7.8, 9.2);
                pb.cubic_to(6.7, 7.4, 6.7, 3.1, 8.5, 2.5);
                pb.cubic_to(10.4, 3.6, 10.9, 6.3, 11.8, 8.0);
                pb.quad_to(13.3, 7.9, 14.4, 8.0);
                pb.cubic_to(14.2, 5.2, 14.4, 2.4, 16.0, 2.8);
                pb.cubic_to(18.0, 4.1, 18.8, 6.9, 18.0, 9.0);
                pb.cubic_to(20.3, 10.7, 20.1, 12.6, 20.8, 12.9);
                pb.cubic_to(24.8, 8.9, 35.7, 10.5, 38.3, 16.2);
                pb.cubic_to(40.0, 22.1, 32.0, 23.0, 28.0, 21.3);
                pb.cubic_to(23.0, 22.0, 19.8, 21.4, 17.4, 21.1);
                pb.cubic_to(15.3, 22.3, 9.0, 23.0, 8.0, 21.2);
                pb.close();
            }
            "starlit" => {
                // Pear-shaped stuffed mantle, rounded at the cheeks and tentacle seam.
                pb.move_to(10.0, 17.6);
                pb.cubic_to(5.3, 14.4, 6.7, 7.2, 14.3, 4.9);
                pb.cubic_to(20.0, 2.6, 29.5, 2.9, 34.2, 6.2);
                pb.cubic_to(41.0, 9.0, 44.0, 14.8, 38.8, 18.0);
                pb.cubic_to(31.4, 20.0, 16.5, 19.7, 10.0, 17.6);
                pb.close();
            }
            "crab" => {
                // A padded scallop shell instead of a flat ellipse.
                pb.move_to(9.4, 16.0);
                pb.cubic_to(9.8, 12.5, 15.3, 10.7, 19.3, 11.5);
                pb.quad_to(24.0, 9.5, 28.7, 11.5);
                pb.cubic_to(33.2, 10.7, 38.3, 12.7, 38.6, 16.1);
                pb.cubic_to(37.8, 22.2, 11.0, 22.5, 9.4, 16.0);
                pb.close();
            }
            _ => {
                // Three unequal wool-cloud lobes with a pillowy, curved underside.
                pb.move_to(9.0, 22.3);
                pb.cubic_to(3.0, 22.5, 3.5, 14.0, 8.6, 13.2);
                pb.cubic_to(7.4, 6.5, 14.4, 4.0, 18.8, 8.0);
                pb.cubic_to(22.3, 2.4, 33.1, 3.4, 35.3, 11.3);
                pb.cubic_to(42.4, 9.5, 45.1, 20.5, 38.0, 22.8);
                pb.cubic_to(31.0, 24.6, 17.0, 24.9, 9.0, 22.3);
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
            tail_pb.move_to(36.0, 19.5);
            tail_pb.cubic_to(
                39.0 + tail_rot * 6.0,
                20.5,
                44.6 + tail_rot * 8.0,
                16.0 - tail_rot * 5.0,
                43.1 + tail_rot * 4.0,
                9.0 - tail_rot * 8.0,
            );
            tail_pb.cubic_to(
                42.1 + tail_rot * 2.0,
                5.5 - tail_rot * 9.0,
                38.5 + tail_rot * 2.0,
                7.5 - tail_rot * 8.0,
                39.0 + tail_rot * 3.0,
                11.5 - tail_rot * 6.0,
            );
            tail_pb.cubic_to(
                39.5 + tail_rot * 4.0,
                15.0,
                37.5 + tail_rot * 3.0,
                20.0,
                36.0,
                21.0,
            );
            tail_pb.close();
            if let Some(tail_path) = tail_pb.finish() {
                let mut tail_paint = Paint::default();
                tail_paint.set_color(crate::art::palette("mochi").body);
                tail_paint.anti_alias = true;
                crate::art::material(&mut self.pixmap, &tail_path, body_transform, style.name);

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

        // A shared matte material keeps all five characters in one visual family.
        crate::art::material(&mut self.pixmap, &body_path, body_transform, style.name);

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
            paw_fill_paint.set_color(crate::art::palette("mochi").body); // 抹茶绿底部浅色
            paw_fill_paint.anti_alias = true;

            let mut paw_stroke_paint = Paint::default();
            paw_stroke_paint.set_color(style.outer_rim_color);
            paw_stroke_paint.anti_alias = true;
            let paw_stroke = Stroke {
                width: 0.38,
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
                crate::art::material(&mut self.pixmap, &fp, paw_l_transform, "mochi");
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
                crate::art::material(&mut self.pixmap, &fp, paw_r_transform, "mochi");
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
                inner_paint.set_color(Color::from_rgba8(219, 167, 147, 230));
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
                inner_paint.set_color(Color::from_rgba8(219, 167, 147, 230));
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
                collar_paint.set_color(Color::from_rgba8(162, 107, 95, 240));
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
            let bell_r = 0.95;
            if let Some(bell_rect) = Rect::from_xywh(
                bell_cx - bell_r,
                bell_cy - bell_r,
                bell_r * 2.0,
                bell_r * 2.0,
            ) {
                let bell_pb = PathBuilder::from_oval(bell_rect).unwrap();
                let mut bell_paint = Paint::default();
                bell_paint.set_color(Color::from_rgba8(223, 181, 111, 255));
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
                    let hole_pb = PathBuilder::from_oval(hole_rect).unwrap();
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
                let inward = if i < 3 { 1.0 } else { -1.0 };
                let ctrl_x1 = x_anchor - inward * 1.8 + wave_x * 0.15;
                let ctrl_y1 = 20.0 + wave_y * 0.10;
                let ctrl_x2 = x_anchor - inward * 2.4 + wave_x * 0.50;
                let ctrl_y2 = 22.4 + wave_y * 0.35;
                let end_x = x_anchor - inward * 1.9 + wave_x * 0.85;
                let end_y = 24.1 + wave_y * 0.45;
                leg_pb.move_to(x_anchor, 16.5);
                leg_pb.cubic_to(ctrl_x1, ctrl_y1, ctrl_x2, ctrl_y2, end_x, end_y - 0.7);
                leg_pb.cubic_to(end_x - inward * 0.25, end_y + 1.0, end_x + inward * 2.6, end_y + 1.1, end_x + inward * 2.8, end_y - 0.5);
                leg_pb.cubic_to(ctrl_x2 + inward * 3.0, ctrl_y2 - 0.2, ctrl_x1 + inward * 2.8, ctrl_y1, x_anchor + inward * 3.0, 16.5);
                leg_pb.close();

                if let Some(leg_path) = leg_pb.finish() {
                    let mut leg_paint = Paint::default();
                    leg_paint.set_color(crate::art::palette("starlit").body);
                    leg_paint.anti_alias = true;
                    crate::art::material(&mut self.pixmap, &leg_path, body_transform, style.name);

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

            crate::art::material(&mut self.pixmap, &body_path, body_transform, style.name);

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
                "none" => {}
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
            crate::art::crab_limbs(&mut self.pixmap, &mut self.body_mask, body_transform, anim_t, anim_amplitude);
            crate::art::material(&mut self.pixmap, &body_path, body_transform, style.name);
        }

        crate::art::sewn_details(&mut self.pixmap, body_transform, style.name);

        // 2. Draw Eyes
        let eye_transform = body_transform.pre_translate(eye_tx, eye_ty);
        let iris = crate::art::palette(style.name);
        let flow_offset = 0.0;
        let eye_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(12.0, style.eye_y - 2.0 + flow_offset),
            tiny_skia::Point::from_xy(12.0, style.eye_y + 2.0 + flow_offset),
            vec![
                tiny_skia::GradientStop::new(0.0, iris.ink),
                tiny_skia::GradientStop::new(0.5, iris.edge),
                tiny_skia::GradientStop::new(1.0, iris.shade),
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
        blush_paint.set_color(Color::from_rgba8(222, 149, 134, blush_alpha));
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
        let cx = if matches!(style.name, "crab" | "starlit" | "nimbus") {
            24.0f32
        } else {
            12.0f32
        };
        let mouth_y = style.mouth_y + blush_y_offset;
        crate::art::nose(&mut self.pixmap, body_transform, style.name, cx, mouth_y - 1.1);

        let half_w = if style.name == "starlit" { 0.7 } else { 0.62 } * self.mouth_width_lerp;
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
                mouth_paint.set_color(Color::from_rgba8(104, 65, 70, 240));
                self.pixmap.fill_path(
                    &mouth_path,
                    &mouth_paint,
                    FillRule::Winding,
                    body_transform,
                    None,
                );
            }

            let mut stroke_paint = Paint::default();
            stroke_paint.set_color(Color::from_rgba8(83, 63, 67, 235));
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
                vec![cx + 2.5]
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

            // Two quiet stitched sleep marks, contained in the physical window.
            for i in 0..2 {
                let phase = (t * 0.25 + i as f32 * 0.35) % 1.0;
                let fade = (1.0 - phase).powf(0.8);
                let z_x = cx + 5.8 + i as f32 * 3.8;
                let z_y = 3.2 - phase * 3.0 - i as f32 * 2.8;
                let size = 1.1 + i as f32 * 0.18;
                let mut path = PathBuilder::new();
                path.move_to(z_x - size, z_y - size * 0.72);
                path.line_to(z_x + size, z_y - size * 0.72);
                path.line_to(z_x - size, z_y + size * 0.72);
                path.line_to(z_x + size, z_y + size * 0.72);
                if let Some(path) = path.finish().and_then(|path| path.transform(body_transform)) {
                    let bounds = path.bounds();
                    let width = 0.38 * view_scale;
                    let margin = width * 0.5 + 1.0;
                    let max_x = (self.pixmap.width() as f32 - bounds.width() - margin).max(margin);
                    let max_y = (self.pixmap.height() as f32 - bounds.height() - margin).max(margin);
                    let dx = bounds.left().clamp(margin, max_x) - bounds.left();
                    let dy = bounds.top().clamp(margin, max_y) - bounds.top();
                    let mut color = crate::art::palette(style.name).edge;
                    color.set_alpha(fade * 0.8 * self.sleep_bubble_lerp);
                    let mut paint = Paint::default();
                    paint.set_color(color);
                    paint.anti_alias = true;
                    self.pixmap.stroke_path(
                        &path, &paint,
                        &Stroke { width, line_cap: tiny_skia::LineCap::Round, ..Stroke::default() },
                        Transform::from_translate(dx, dy), None,
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
            stalk_paint.set_color(crate::art::palette("crab").body); // 珊瑚红橙色
            stalk_paint.anti_alias = true;
            pixmap.fill_path(
                &stalk_path,
                &stalk_paint,
                FillRule::Winding,
                transform,
                None,
            );

            let mut stalk_stroke = Paint::default();
            stalk_stroke.set_color(crate::art::palette("crab").edge);
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
                lash_paint.set_color(Color::from_rgba8(72, 57, 62, 240));
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
                eye_p.set_color(Color::from_rgba8(67, 53, 55, 255)); // 深亮乌黑眼珠
                eye_p.anti_alias = true;
                pixmap.fill_path(&p, &eye_p, FillRule::Winding, transform, None);

                let mut stroke = Paint::default();
                stroke.set_color(crate::art::palette("crab").edge);
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
                hl_paint.set_color(Color::from_rgba8(255, 248, 224, 255));
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
                let p_path = PathBuilder::from_oval(pr).unwrap();
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
    pb.cubic_to(lx, cy + top_lid_offset * 1.3, rx, cy + top_lid_offset * 1.3, rx, cy);
    pb.cubic_to(rx, cy + bottom_lid_offset * 1.3, lx, cy + bottom_lid_offset * 1.3, lx, cy);
    pb.close();

    if let Some(eye_path) = pb.finish() {
        pixmap.fill_path(&eye_path, paint, FillRule::Winding, transform, None);

        // 1. Draw bold slate eyeliner
        let mut eyeliner_paint = Paint::default();
        eyeliner_paint.set_color(crate::art::palette(pet_style).ink);
        eyeliner_paint.anti_alias = true;
        let eyeliner_stroke = Stroke {
            width: 0.26,
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
        let glow_r_h = current_h * 0.10;
        let glow_cy = cy + current_h * 0.22;
        if let Some(gr) = Rect::from_xywh(
            cx - glow_r_w,
            glow_cy - glow_r_h,
            glow_r_w * 2.0,
            glow_r_h * 2.0,
        ) {
            let glow_path = PathBuilder::from_oval(gr).unwrap();
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
                        let h_path = PathBuilder::from_oval(hr).unwrap();
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
                        let h_path = PathBuilder::from_oval(hr).unwrap();
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
                    let p_path = PathBuilder::from_oval(pr).unwrap();
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
                    "mochi" => Color::from_rgba8(55, 62, 47, (240.0 * pupil_opacity_lerp) as u8),
                    _ => Color::from_rgba8(61, 52, 63, (240.0 * pupil_opacity_lerp) as u8),
                };

                let p_r = (w * 0.31 * pupil_scale_lerp).min(current_h * 0.27);
                if let Some(pr) =
                    Rect::from_xywh(pupil_cx - p_r, pupil_cy - p_r, p_r * 2.0, p_r * 2.0)
                {
                    let mut p_paint = Paint::default();
                    p_paint.set_color(pupil_color);
                    p_paint.anti_alias = true;
                    let p_path = PathBuilder::from_oval(pr).unwrap();
                    pixmap.fill_path(&p_path, &p_paint, FillRule::Winding, transform, None);
                }

                // 4. Double Sparkle Highlights
                // Primary Highlight (Large, Upper-Left)
                let h_r = (w * 0.12 * pupil_scale_lerp).min(current_h * 0.13);
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
                    let h_path = PathBuilder::from_oval(hr).unwrap();
                    pixmap.fill_path(&h_path, &h_paint, FillRule::Winding, transform, None);
                }

                // Secondary Highlight (Small, Semi-transparent, Bottom-Right)
                let h2_r = (w * 0.055 * pupil_scale_lerp).min(current_h * 0.08);
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
                    let h2_path = PathBuilder::from_oval(h2r).unwrap();
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
    #[ignore = "writes visual snapshots to /tmp for manual inspection"]
    fn render_style_snapshots() {
        let output_dir = std::path::Path::new("/tmp/nimbus-style-gallery");
        std::fs::create_dir_all(output_dir).unwrap();
        for style in ["nimbus", "mochi", "starlit", "crab"] {
            for state in ["idle", "tickled", "sleeping"] {
                let height = if style == "nimbus" { 189 } else { 175 };
                let mut renderer = PetRenderer::new(308, height);
                renderer.draw_bubble_enabled = false;
                renderer.active_style = style.to_string();
                renderer.active_weather = "none".to_string();
                renderer.pet_state = state.to_string();
                renderer.start_time = std::time::Instant::now() - std::time::Duration::from_secs(1);
                renderer.last_frame_time = 1.0;
                renderer.anim_time = 1.0;
                renderer.top_lid_lerp = -1.0;
                renderer.bottom_lid_lerp = 1.0;
                renderer.pupil_scale_lerp = 0.56;
                renderer.pointer_x = 19.0;
                renderer.pointer_y = 8.0;
                if state == "tickled" {
                    renderer.top_lid_lerp = -0.45;
                    renderer.bottom_lid_lerp = -0.4;
                    renderer.pupil_opacity_lerp = 0.0;
                    renderer.mouth_open_lerp = 0.9;
                    renderer.mouth_width_lerp = 1.8;
                    renderer.touch_lerp = 1.0;
                    renderer.happiness = 0.85;
                } else if state == "sleeping" {
                    renderer.pupil_opacity_lerp = 0.0;
                    renderer.sleep_bubble_lerp = 1.0;
                    renderer.gaze_active_lerp = 0.0;
                }
                renderer.update_and_draw();
                renderer.pixmap.save_png(output_dir.join(format!("{style}-{state}.png"))).unwrap();
            }
        }
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
    fn bubble_truncation_preserves_source_and_respects_visible_limits() {
        let mut renderer = PetRenderer::new(320, 120);
        renderer.draw_pet_enabled = false;
        renderer.set_font_size(32.0);
        let source = "云朵陪你慢慢阅读，今天也有值得记住的小事情。".repeat(12);
        renderer.set_text(&source);
        renderer.set_bubble_limits(300.0, 200.0, 4);

        let (width, height) = renderer.measure_bubble_size();
        let visible = renderer.text_buffer.lines.iter()
            .map(|line| line.text()).collect::<Vec<_>>().join("\n");
        assert!(width <= 300.0 && height <= 200.0);
        assert!(renderer.text_buffer.layout_runs().count() <= 4);
        assert!(visible.contains('…'));
        assert!(visible.contains("托盘查看全文"));
        assert_eq!(renderer.full_text, source);

        renderer.set_bubble_limits(420.0, 4000.0, 100);
        renderer.measure_bubble_size();
        let restored = renderer.text_buffer.lines.iter()
            .map(|line| line.text()).collect::<Vec<_>>().join("\n");
        assert_eq!(restored, source);
    }

    #[test]
    fn bubble_hides_without_room_for_a_glyph_and_recovers_when_space_returns() {
        let mut renderer = PetRenderer::new(120, 80);
        renderer.draw_pet_enabled = false;
        renderer.set_text("保留这条完整消息");
        for (width, height) in [(1.0, 500.0), (420.0, 1.0), (1.0, 1.0)] {
            renderer.set_bubble_limits(width, height, 8);
            assert_eq!(renderer.measure_bubble_size(), (0.0, 0.0));
            assert_eq!(renderer.measure_bubble_size(), (0.0, 0.0));
            assert!(renderer.has_text());
        }
        renderer.set_bubble_limits(420.0, 360.0, 8);
        let (width, height) = renderer.measure_bubble_size();
        assert!(width > 0.0 && height > 0.0);
        assert_eq!(renderer.text_buffer.lines[0].text(), "保留这条完整消息");
    }

    #[test]
    fn narrow_single_line_bubble_uses_a_compact_ellipsis() {
        let mut renderer = PetRenderer::new(120, 80);
        renderer.draw_pet_enabled = false;
        renderer.set_font_size(20.0);
        renderer.set_bubble_limits(96.0, 64.0, 1);
        renderer.set_text("这是一条需要省略但不能破坏气泡布局的长消息");
        let (width, height) = renderer.measure_bubble_size();
        assert!(width <= 96.0 && height <= 64.0);
        assert_eq!(renderer.text_buffer.layout_runs().count(), 1);
        assert!(renderer.text_buffer.lines[0].text().ends_with('…'));
    }

    #[test]
    fn independent_tail_follows_anchor_without_entering_rounded_corners() {
        let mut renderer = PetRenderer::new(320, 120);
        renderer.draw_pet_enabled = false;
        renderer.set_text("尾巴跟着身边的小云朵");
        let (width, height) = renderer.measure_bubble_size();
        renderer.resize(width as u32, height as u32);
        renderer.bubble_on_bottom = true;
        let y = renderer.bubble_edge_safe_pad().ceil() as u32;
        for anchor in [-100.0, width + 100.0] {
            renderer.bubble_tail_x = Some(anchor);
            renderer.update_and_draw();
            let inset = renderer.bubble_edge_safe_pad() + 19.5 * renderer.scale_factor;
            let expected_x = anchor.clamp(inset, width - inset).round() as u32;
            assert!((expected_x.saturating_sub(1)..=expected_x + 1)
                .any(|x| alpha_at(&renderer, x, y) > 0));
            assert_eq!(alpha_at(&renderer, renderer.pixmap.width() / 2, y), 0);
        }
    }

    #[test]
    #[ignore = "writes bubble snapshots to /tmp for manual inspection"]
    fn render_bubble_snapshots() {
        let output_dir = std::path::Path::new("/tmp/nimbus-bubble-review");
        std::fs::create_dir_all(output_dir).unwrap();
        for scale in [1.0, 2.0] {
            for on_bottom in [false, true] {
                let mut renderer = PetRenderer::new(320, 120);
                renderer.draw_pet_enabled = false;
                renderer.scale_factor = scale;
                renderer.set_font_size(32.0);
                renderer.set_bubble_limits(340.0 * scale, 220.0 * scale, 4);
                renderer.set_text(&"屏幕边缘也能读清消息。🌥️ Nimbus 会保留完整内容，方便稍后查看。".repeat(4));
                let (width, height) = renderer.measure_bubble_size();
                renderer.resize(width as u32, height as u32);
                renderer.bubble_on_bottom = on_bottom;
                renderer.bubble_tail_x = Some(if on_bottom { width - 12.0 } else { 12.0 });
                renderer.update_and_draw();
                renderer.pixmap.save_png(output_dir.join(format!(
                    "bubble-{}x-{}.png", scale as u32, if on_bottom { "below" } else { "above" }
                ))).unwrap();
            }
        }
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
