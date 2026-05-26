use tiny_skia::{
    Color, FillRule, LinearGradient, Paint, PathBuilder, Pixmap, Rect,
    Stroke, Transform,
};
use cosmic_text::{Buffer, Color as CosmicColor, FontSystem, SwashCache, SwashImage, SwashContent};

pub struct PetRenderer {
    pixmap: Pixmap,
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
    top_lid_lerp: f32,       // -1.0 (standard open) to 0.4 (sleep close) or -0.45 (smile up)
    bottom_lid_lerp: f32,    // 1.0 (standard open) to 0.42 (sleep close) or -0.4 (smile up)
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
}

impl PetRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width, height).unwrap();
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        
        let mut text_buffer = Buffer::new(&mut font_system, cosmic_text::Metrics::new(24.0, 36.0));
        text_buffer.set_size(&mut font_system, Some((width - 45) as f32), None);

        Self {
            pixmap,
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
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        self.mouse_pos = pos;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        let line_height = font_size * 1.5;
        let metrics = cosmic_text::Metrics::new(font_size, line_height);
        self.text_buffer.set_metrics(&mut self.font_system, metrics);
        self.text_buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(new_pixmap) = Pixmap::new(width, height) {
            self.pixmap = new_pixmap;
            self.text_buffer.set_size(&mut self.font_system, Some((width - 45) as f32), None);
        }
    }

    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    pub fn set_text(&mut self, text: &str) {
        let mut attrs = cosmic_text::Attrs::new();
        // Set dark slate blue and Medium weight for standard pet chat style
        attrs = attrs.color(CosmicColor::rgb(0x2d, 0x37, 0x48))
            .weight(cosmic_text::Weight::MEDIUM);
        
        self.text_buffer.set_text(&mut self.font_system, text, attrs, cosmic_text::Shaping::Advanced);
        self.text_buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn update_and_draw(&mut self) {
        // Clear background with fully transparent color (0,0,0,0)
        self.pixmap.fill(Color::TRANSPARENT);
        
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let dt = elapsed - self.last_frame_time;
        self.last_frame_time = elapsed;

        // Fix pet position to the bottom of the window
        let pet_y = self.pixmap.height() as f32 - 160.0 - 10.0;

        // Draw speech bubble if there is text in the buffer
        let has_text = self.text_buffer.lines.iter().any(|line| !line.text().is_empty());
        if has_text {
            self.draw_bubble(pet_y);
        }

        // Draw the cloud pet
        self.draw_pet(elapsed, dt, pet_y);
    }

    fn draw_bubble(&mut self, pet_y: f32) {
        let padding = 18.0;
        let mut max_width = 0.0f32;
        
        // Max bubble height from top padding (10.0) to pet top offset (pet_y - 30.0)
        let max_bubble_h = (pet_y - 35.0).max(50.0);
        let max_text_h = max_bubble_h - padding * 2.0;
        
        let mut height = 0.0f32;
        let mut runs_to_draw = 0;
        
        for run in self.text_buffer.layout_runs() {
            if run.line_y + run.line_height > max_text_h {
                break;
            }
            max_width = max_width.max(run.line_w);
            height = run.line_y + run.line_height;
            runs_to_draw += 1;
        }
        
        if height == 0.0 {
            return;
        }

        let bubble_w = (max_width + padding * 2.0).clamp(100.0, self.pixmap.width() as f32 - 20.0);
        let bubble_h = height + padding * 2.0;
        
        let x = (self.pixmap.width() as f32 - bubble_w) / 2.0;
        // Bubble bottom aligns to pet_y - 15.0, grows upwards
        let y = pet_y - 15.0 - bubble_h;

        // 1. Draw rounded bubble shape using tiny-skia
        let mut pb = PathBuilder::new();
        let radius = 14.0f32; // slightly more rounded
        
        // Rounded rect path
        pb.move_to(x + radius, y);
        pb.line_to(x + bubble_w - radius, y);
        pb.quad_to(x + bubble_w, y, x + bubble_w, y + radius);
        pb.line_to(x + bubble_w, y + bubble_h - radius);
        pb.quad_to(x + bubble_w, y + bubble_h, x + bubble_w - radius, y + bubble_h);
        
        // Add bubble pointer triangle at the bottom center
        let cx = self.pixmap.width() as f32 / 2.0;
        pb.line_to(cx + 8.0, y + bubble_h);
        pb.line_to(cx, y + bubble_h + 8.0);
        pb.line_to(cx - 8.0, y + bubble_h);
        
        pb.line_to(x + radius, y + bubble_h);
        pb.quad_to(x, y + bubble_h, x, y + bubble_h - radius);
        pb.line_to(x, y + radius);
        pb.quad_to(x, y, x + radius, y);
        pb.close();
        
        let path = pb.finish().unwrap();

        // Premium Ice-Blue Glassmorphism Gradient
        let bubble_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(x, y),
            tiny_skia::Point::from_xy(x + bubble_w, y + bubble_h),
            vec![
                tiny_skia::GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 245)),
                tiny_skia::GradientStop::new(1.0, Color::from_rgba8(240, 246, 255, 230)),
            ],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        ).unwrap();

        let mut paint = Paint::default();
        paint.shader = bubble_grad;
        paint.anti_alias = true;
        
        // Draw shadow (soft offset overlay)
        let mut shadow_paint = Paint::default();
        shadow_paint.set_color(Color::from_rgba8(57, 65, 255, 12));
        shadow_paint.anti_alias = true;
        let shadow_transform = Transform::from_translate(0.0, 4.0);
        self.pixmap.fill_path(&path, &shadow_paint, FillRule::Winding, shadow_transform, None);

        // Draw bubble body
        self.pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

        // Draw premium border with higher opacity
        let mut stroke_paint = Paint::default();
        stroke_paint.set_color(Color::from_rgba8(130, 150, 255, 90));
        stroke_paint.anti_alias = true;
        let stroke = Stroke {
            width: 1.5,
            ..Default::default()
        };
        self.pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);

        // 2. Draw text using cosmic-text SwashCache
        let text_color = CosmicColor::rgb(0x2d, 0x37, 0x48);
        for run in self.text_buffer.layout_runs().take(runs_to_draw) {
            let line_y = y + padding + run.line_y;
            for glyph in run.glyphs {
                let physical_glyph = glyph.physical((x + padding, line_y), 1.0);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical_glyph.cache_key) {
                    if image.placement.width > 0 && image.placement.height > 0 {
                        let draw_x = physical_glyph.x + image.placement.left;
                        let draw_y = physical_glyph.y - image.placement.top;
                        Self::draw_glyph(&mut self.pixmap, &image, draw_x, draw_y, text_color);
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
                    let index = (pixel_y as usize * pixmap_mut.width() as usize + pixel_x as usize) * 4;
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
                target_eye_width = 0.9;
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
                target_eye_width = 1.1;
                target_top_lid = -0.45;
                target_bottom_lid = -0.4;
                target_pupil_scale = 0.0;
                target_pupil_opacity = 0.0;
                target_mouth_open = 0.9;
                target_mouth_width = 1.8;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.08;
                target_body_scale_y = 1.04;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 0.2;
            }
            "surprised" => {
                target_eye_height = 1.4;
                target_eye_width = 1.4;
                target_top_lid = -1.2;
                target_bottom_lid = 1.2;
                target_pupil_scale = 0.25;
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
                target_eye_height = 0.6;
                target_eye_width = 1.0;
                target_top_lid = -0.1;
                target_bottom_lid = 1.0;
                target_pupil_scale = 0.5;
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
                target_pupil_scale = 0.5;
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
            "attentive" => {
                target_eye_height = 1.2;
                target_eye_width = 1.15;
                target_top_lid = -1.1;
                target_bottom_lid = 1.1;
                target_pupil_scale = 0.55;
                target_pupil_opacity = 1.0;
                target_mouth_open = 0.0;
                target_mouth_width = 1.3;
                target_mouth_skew = 0.0;
                target_body_scale_x = 1.02;
                target_body_scale_y = 1.02;
                target_body_shake = 0.0;
                target_sleep_bubble = 0.0;
                target_gaze_active = 1.0;
            }
            "working" => {
                target_eye_height = 0.85;
                target_eye_width = 0.95;
                target_top_lid = -0.3;
                target_bottom_lid = 0.9;
                target_pupil_scale = 0.48;
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
                target_eye_height = 0.55;
                target_eye_width = 0.95;
                target_top_lid = -0.1;
                target_bottom_lid = 0.5;
                target_pupil_scale = 0.4;
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
            _ => { // idle
                target_eye_height = 1.0;
                target_eye_width = 1.0;
                target_top_lid = -1.0;
                target_bottom_lid = 1.0;
                target_pupil_scale = 0.5;
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

        // --- 2. Exponential Lerp (dt-dependent smooth transitions) ---
        let ease_speed = 9.5f32; // ~0.15s transition
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

        // Bouncing spring
        let spring = if self.pointer_inside {
            let u = (t - 0.10) / 1.12;
            if u > 0.0 && u < 1.0 {
                let envelope = (1.0 - u).powf(1.35);
                (u * std::f32::consts::PI * 4.8).sin() * envelope * 1.16
            } else {
                0.0
            }
        } else {
            0.0
        };

        let hit = spring.abs();
        let hold = (2.0 * std::f32::consts::PI * t / 3.9).sin();
        let live = if t > 0.65 { 1.0f32 } else { 0.0f32 };
        
        // Eye track target calculation
        let dx = self.pointer_x - 12.0;
        let dy = self.pointer_y - 12.0;
        let dist = (dx * dx + dy * dy).sqrt();
        let (idle_eye_x, idle_eye_y) = if dist > 0.001 {
            let sat = (dist / 6.0).tanh();
            ((dx / dist) * sat * 0.50, ((dy / dist) * sat * 0.50).clamp(-0.50, 0.50))
        } else {
            (0.0, 0.0)
        };

        // Pose transforms
        let body_start = if self.pointer_inside { 0.0f32 } else { 1.0f32 };
        let mut pose_tx = settle_tx - body_start * 0.28 + spring * 0.33 + live * (2.0 * std::f32::consts::PI * t / 3.1 + 0.8).sin() * 0.018;
        let mut pose_ty = settle_ty - hit * 0.10 + hold * 0.022 * live;
        let mut pose_rot = settle_rot - body_start * 1.15 + spring * 4.20 + hold * 0.12 * live;
        
        // Jitter (Shake) application for surprised
        if self.body_shake_lerp > 0.001 {
            pose_tx += (t * 85.0).sin() * 0.08 * self.body_shake_lerp;
            pose_ty += (t * 75.0).cos() * 0.08 * self.body_shake_lerp;
            pose_rot += (t * 90.0).sin() * 0.04 * self.body_shake_lerp;
        }

        // Curried scale factors
        let sx = (settle_scale + hit * 0.048) * self.body_scale_x_lerp;
        let sy = (settle_scale - hit * 0.048 * 0.72) * self.body_scale_y_lerp;

        // Custom breath scale
        let (breath_cycle, breath_depth) = match self.pet_state.as_str() {
            "thinking" => (2.0f32, 0.018f32),
            "sleeping" => (6.0f32, 0.005f32),
            "tired" => (5.0f32, 0.012f32),
            "bored" => (5.5f32, 0.006f32),
            "tickled" => (1.8f32, 0.015f32),
            _ => (4.8f32, 0.008f32),
        };
        let breath_scale = 1.0 + (2.0 * std::f32::consts::PI * t / breath_cycle).sin() * breath_depth * live;
        
        let eye_tx = idle_eye_x * self.gaze_active_lerp;
        let eye_ty = idle_eye_y * self.gaze_active_lerp;

        // Viewport scale (48x48 up to width)
        let pet_width = 200.0f32;
        let pet_height = 200.0f32;
        let pet_x = (self.pixmap.width() as f32 - pet_width) / 2.0;
        let view_scale = pet_width / 48.0;
        let view_tx = pet_x + pet_width / 2.0;
        let view_ty = pet_y + pet_height / 2.0;

        let base_transform = Transform::from_translate(view_tx, view_ty)
            .pre_scale(view_scale, view_scale)
            .pre_translate(pose_tx, pose_ty)
            .pre_rotate(pose_rot)
            .pre_scale(sx, sy)
            .pre_translate(-12.0, -12.0);

        let body_transform = base_transform
            .pre_translate(12.0, 12.0)
            .pre_scale(breath_scale, breath_scale)
            .pre_translate(-12.0, -12.0);

        // Cartoon cloud path
        let mut pb = PathBuilder::new();
        pb.move_to(8.0, 24.0);
        pb.cubic_to(4.0, 20.0, 4.0, 12.0, 10.0, 10.0);
        pb.cubic_to(10.0, 4.0, 18.0, 4.0, 22.0, 8.0);
        pb.cubic_to(26.0, 2.0, 36.0, 4.0, 38.0, 12.0);
        pb.cubic_to(44.0, 14.0, 44.0, 22.0, 38.0, 24.0);
        pb.line_to(8.0, 24.0);
        pb.close();
        let cloud_path = pb.finish().unwrap();

        // 1. Draw Rim & Body
        let mut rim_paint = Paint::default();
        rim_paint.set_color(Color::from_rgba8(130, 160, 255, 30));
        rim_paint.anti_alias = true;
        self.pixmap.stroke_path(&cloud_path, &rim_paint, &Stroke { width: 0.9, ..Default::default() }, body_transform, None);

        let mut rim_tight_paint = Paint::default();
        rim_tight_paint.set_color(Color::from_rgba8(255, 255, 255, 65));
        rim_tight_paint.anti_alias = true;
        self.pixmap.stroke_path(&cloud_path, &rim_tight_paint, &Stroke { width: 0.45, ..Default::default() }, body_transform, None);

        let body_grad = LinearGradient::new(
            tiny_skia::Point::from_xy(12.0, 2.0),
            tiny_skia::Point::from_xy(12.0, 22.0),
            vec![
                tiny_skia::GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 255)),
                tiny_skia::GradientStop::new(0.4, Color::from_rgba8(248, 252, 255, 255)),
                tiny_skia::GradientStop::new(1.0, Color::from_rgba8(218, 225, 240, 255)),
            ],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        ).unwrap();

        let mut body_paint = Paint::default();
        body_paint.shader = body_grad;
        body_paint.anti_alias = true;

        let mut shadow_paint = Paint::default();
        shadow_paint.set_color(Color::from_rgba8(57, 65, 255, 22));
        shadow_paint.anti_alias = true;
        self.pixmap.fill_path(&cloud_path, &shadow_paint, FillRule::Winding, body_transform.post_translate(0.0, 1.5), None);

        self.pixmap.fill_path(&cloud_path, &body_paint, FillRule::Winding, body_transform, None);

        // 2. Draw Eyes
        let eye_transform = base_transform.pre_translate(eye_tx, eye_ty);
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
        ).unwrap();

        let mut eye_paint = Paint::default();
        eye_paint.shader = eye_grad;
        eye_paint.anti_alias = true;

        let mut eye_border_paint = Paint::default();
        eye_border_paint.set_color(Color::from_rgba8(255, 255, 255, 180));
        eye_border_paint.anti_alias = true;
        let eye_border_stroke = Stroke { width: 0.08, ..Default::default() };

        // Draw Left Eye
        draw_eye(
            &mut self.pixmap,
            9.0,
            12.0,
            2.6 * self.eye_width_lerp,
            5.0 * self.eye_height_lerp,
            blink_scale,
            &eye_paint,
            &eye_border_paint,
            &eye_border_stroke,
            eye_transform,
            &self.pet_state,
            self.mouse_pos,
            self.top_lid_lerp,
            self.bottom_lid_lerp,
            self.pupil_scale_lerp,
            self.pupil_opacity_lerp,
            self.gaze_active_lerp,
        );

        // Draw Right Eye
        draw_eye(
            &mut self.pixmap,
            15.0,
            12.0,
            2.6 * self.eye_width_lerp,
            5.0 * self.eye_height_lerp,
            blink_scale,
            &eye_paint,
            &eye_border_paint,
            &eye_border_stroke,
            eye_transform,
            &self.pet_state,
            self.mouse_pos,
            self.top_lid_lerp,
            self.bottom_lid_lerp,
            self.pupil_scale_lerp,
            self.pupil_opacity_lerp,
            self.gaze_active_lerp,
        );

        // 3. Draw Blush
        let blush_alpha = if self.pet_state == "sleeping" { 90 } else { 160 };
        let blush_y_offset = (breath_scale - 1.0) * 0.3;
        
        let mut blush_paint = Paint::default();
        blush_paint.set_color(Color::from_rgba8(255, 160, 175, blush_alpha));
        blush_paint.anti_alias = true;

        if let Some(r) = Rect::from_xywh(7.0, 14.3 + blush_y_offset, 2.5, 1.2) {
            let path = PathBuilder::from_rect(r);
            self.pixmap.fill_path(&path, &blush_paint, FillRule::Winding, base_transform, None);
        }
        if let Some(r) = Rect::from_xywh(14.5, 14.3 + blush_y_offset, 2.5, 1.2) {
            let path = PathBuilder::from_rect(r);
            self.pixmap.fill_path(&path, &blush_paint, FillRule::Winding, base_transform, None);
        }

        // 4. Draw Eased Mouth
        let cx = 12.0f32;
        let mouth_y = 15.0 + blush_y_offset;
        
        let half_w = 0.5 * self.mouth_width_lerp;
        let lx = cx - half_w;
        let rx = cx + half_w;
        
        let ly = mouth_y + self.mouth_skew_lerp * 0.3;
        let ry = mouth_y - self.mouth_skew_lerp * 0.3;

        let mut mouth_pb = PathBuilder::new();
        mouth_pb.move_to(lx, ly);
        
        // Upper Lip Curve
        let ty = mouth_y + 0.15 * (1.0 - self.mouth_open_lerp);
        mouth_pb.quad_to(cx, ty, rx, ry);
        
        // Lower Lip Curve (Interpolates from closed smile to deep oval)
        let max_y_drop = if self.pet_state == "surprised" {
            2.2
        } else if self.pet_state == "tickled" {
            1.8
        } else {
            1.2
        };
        let by = mouth_y + 0.3 + max_y_drop * self.mouth_open_lerp;
        mouth_pb.quad_to(cx, by, lx, ly);
        mouth_pb.close();
        
        if let Some(mouth_path) = mouth_pb.finish() {
            let mut mouth_paint = Paint::default();
            mouth_paint.anti_alias = true;
            
            if self.mouth_open_lerp > 0.08 {
                // Fill mouth cavity in dark grey
                mouth_paint.set_color(Color::from_rgba8(74, 85, 104, 230));
                self.pixmap.fill_path(&mouth_path, &mouth_paint, FillRule::Winding, base_transform, None);
            }
            
            let mut stroke_paint = Paint::default();
            stroke_paint.set_color(Color::from_rgba8(45, 55, 72, 220));
            stroke_paint.anti_alias = true;
            self.pixmap.stroke_path(&mouth_path, &stroke_paint, &Stroke { width: 0.42, ..Default::default() }, base_transform, None);
        }

        // 5. Draw Sleep Bubbles & Floating Zs
        if self.sleep_bubble_lerp > 0.01 {
            // Draw mouth saliva bubble
            let bubble_pulse = (2.0 * std::f32::consts::PI * t / 6.0).sin().abs() * 0.6 + 0.4;
            let b_cx = 14.5f32;
            let b_cy = mouth_y + 0.1;
            let b_r = 0.65 * self.sleep_bubble_lerp * bubble_pulse;
            if let Some(br) = Rect::from_xywh(b_cx - b_r, b_cy - b_r, b_r * 2.0, b_r * 2.0) {
                let bp = PathBuilder::from_rect(br);
                let mut b_paint = Paint::default();
                b_paint.set_color(Color::from_rgba8(220, 240, 255, 140));
                b_paint.anti_alias = true;
                
                let mut b_stroke = Paint::default();
                b_stroke.set_color(Color::from_rgba8(255, 255, 255, 200));
                b_stroke.anti_alias = true;
                
                self.pixmap.fill_path(&bp, &b_paint, FillRule::Winding, base_transform, None);
                self.pixmap.stroke_path(&bp, &b_stroke, &Stroke { width: 0.12, ..Default::default() }, base_transform, None);
            }

            // Draw floating "zZ" in the sky
            let z_time = t * 1.5;
            let z_cycle = z_time.floor();
            let z_phase = z_time % 1.0;
            let z_fade = 1.0 - z_phase;
            
            let z_ox = (t * 2.0).sin() * 0.8 + 12.0;
            let z_oy = 2.0 - z_phase * 4.0;
            
            let mut z_pb = PathBuilder::new();
            let size_z = 0.5 * (1.0 - 0.25 * (z_cycle % 3.0));
            z_pb.move_to(z_ox - size_z, z_oy - size_z);
            z_pb.line_to(z_ox + size_z, z_oy - size_z);
            z_pb.line_to(z_ox - size_z, z_oy + size_z);
            z_pb.line_to(z_ox + size_z, z_oy + size_z);
            
            if let Some(z_path) = z_pb.finish() {
                let mut z_paint = Paint::default();
                z_paint.set_color(Color::from_rgba8(130, 160, 255, (z_fade * 180.0 * self.sleep_bubble_lerp) as u8));
                z_paint.anti_alias = true;
                self.pixmap.stroke_path(&z_path, &z_paint, &Stroke { width: 0.2, ..Default::default() }, base_transform, None);
            }
        }
    }
}

// Parametric dynamic eye renderer supporting high-differentiation blending and perfect continuity
fn draw_eye(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    blink_scale: f32,
    paint: &Paint,
    border_paint: &Paint,
    border_stroke: &Stroke,
    transform: Transform,
    pet_state: &str,
    mouse_pos: Option<(f32, f32)>,
    top_lid_lerp: f32,
    bottom_lid_lerp: f32,
    pupil_scale_lerp: f32,
    pupil_opacity_lerp: f32,
    gaze_active_lerp: f32,
) {
    let current_h = h * blink_scale;

    // Up and down lid offsets relative to center
    let top_lid_offset = (current_h / 2.0) * top_lid_lerp;
    let bottom_lid_offset = (current_h / 2.0) * bottom_lid_lerp;

    // Eye corners
    let lx = cx - w / 2.0;
    let rx = cx + w / 2.0;

    let mut pb = PathBuilder::new();
    pb.move_to(lx, cy);
    pb.quad_to(cx, cy + top_lid_offset, rx, cy);
    pb.quad_to(cx, cy + bottom_lid_offset, lx, cy);
    pb.close();

    if let Some(eye_path) = pb.finish() {
        let lid_diff = bottom_lid_lerp - top_lid_lerp;
        if lid_diff > 0.25 {
            // Opened eye: fill background and outline
            pixmap.fill_path(&eye_path, paint, FillRule::Winding, transform, None);
            pixmap.stroke_path(&eye_path, border_paint, border_stroke, transform, None);

            // Pupil rendering with gaze tracking
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

                let is_surprised = pet_state == "surprised";

                if is_surprised {
                    // Shocked white pupil
                    let pupil_r = (w * 0.22 * pupil_scale_lerp).max(0.18);
                    if let Some(pr) = Rect::from_xywh(pupil_cx - pupil_r, pupil_cy - pupil_r, pupil_r * 2.0, pupil_r * 2.0) {
                        let mut pupil_paint = Paint::default();
                        pupil_paint.set_color(Color::WHITE);
                        pupil_paint.anti_alias = true;
                        let pupil_path = PathBuilder::from_rect(pr);
                        pixmap.fill_path(&pupil_path, &pupil_paint, FillRule::Winding, transform, None);
                    }
                } else {
                    // Regular expressive eye details: deep pupil + highlight spark
                    // Deep iris/pupil overlay
                    let p_r = w * 0.35 * pupil_scale_lerp;
                    if let Some(pr) = Rect::from_xywh(pupil_cx - p_r, pupil_cy - p_r, p_r * 2.0, p_r * 2.0) {
                        let mut p_paint = Paint::default();
                        p_paint.set_color(Color::from_rgba8(30, 35, 120, (230.0 * pupil_opacity_lerp) as u8));
                        p_paint.anti_alias = true;
                        let p_path = PathBuilder::from_rect(pr);
                        pixmap.fill_path(&p_path, &p_paint, FillRule::Winding, transform, None);
                    }

                    // Bright white spark highlight (offset slightly upper-left)
                    let h_r = w * 0.14 * pupil_scale_lerp;
                    let h_cx = pupil_cx - w * 0.12;
                    let h_cy = pupil_cy - current_h * 0.12;
                    if let Some(hr) = Rect::from_xywh(h_cx - h_r, h_cy - h_r, h_r * 2.0, h_r * 2.0) {
                        let mut h_paint = Paint::default();
                        h_paint.set_color(Color::from_rgba8(255, 255, 255, (240.0 * pupil_opacity_lerp) as u8));
                        h_paint.anti_alias = true;
                        let h_path = PathBuilder::from_rect(hr);
                        pixmap.fill_path(&h_path, &h_paint, FillRule::Winding, transform, None);
                    }
                }
            }
        } else {
            // Closed/Smile line eye: draw as a pure stroke eyelash
            let mut lash_stroke = Stroke::default();
            lash_stroke.width = 0.52;
            pixmap.stroke_path(&eye_path, paint, &lash_stroke, transform, None);
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn rand_offset() -> f32 {
    let now = std::time::Instant::now().elapsed().as_nanos() as f32;
    now.sin() * 0.4
}
