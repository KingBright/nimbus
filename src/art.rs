//! Shared materials and small sewn details for the Nimbus storybook characters.
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder,
    Pixmap, Point, RadialGradient, Rect, SpreadMode, Stroke, Transform,
};

#[derive(Clone, Copy)]
pub struct Palette {
    pub light: Color,
    pub body: Color,
    pub shade: Color,
    pub edge: Color,
    pub accent: Color,
    pub ink: Color,
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

pub fn palette(style: &str) -> Palette {
    let (light, body, shade, edge, accent) = match style {
        "mochi" => (
            (240, 239, 211),
            (184, 203, 163),
            (139, 168, 146),
            (91, 116, 97),
            (227, 171, 142),
        ),
        "starlit" => (
            (239, 221, 225),
            (187, 166, 201),
            (143, 139, 174),
            (102, 85, 125),
            (236, 194, 128),
        ),
        "crab" => (
            (248, 197, 159),
            (222, 146, 121),
            (186, 107, 103),
            (132, 80, 77),
            (250, 216, 174),
        ),
        "luma" => (
            (255, 246, 222),
            (234, 219, 193),
            (190, 182, 182),
            (131, 117, 116),
            (218, 160, 149),
        ),
        _ => (
            (255, 250, 231),
            (242, 232, 208),
            (184, 198, 204),
            (126, 142, 151),
            (230, 180, 128),
        ),
    };
    Palette {
        light: rgb(light.0, light.1, light.2),
        body: rgb(body.0, body.1, body.2),
        shade: rgb(shade.0, shade.1, shade.2),
        edge: rgb(edge.0, edge.1, edge.2),
        accent: rgb(accent.0, accent.1, accent.2),
        ink: rgb(64, 53, 59),
    }
}

pub fn fill(pixmap: &mut Pixmap, path: &Path, transform: Transform, color: Color) {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    pixmap.fill_path(path, &paint, FillRule::Winding, transform, None);
}

fn line(pixmap: &mut Pixmap, path: &Path, transform: Transform, color: Color, width: f32) {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    pixmap.stroke_path(
        path,
        &paint,
        &Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        },
        transform,
        None,
    );
}

pub fn oval(
    pixmap: &mut Pixmap,
    transform: Transform,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    color: Color,
) {
    if let Some(path) =
        Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0).and_then(PathBuilder::from_oval)
    {
        fill(pixmap, &path, transform, color);
    }
}

pub fn material(pixmap: &mut Pixmap, path: &Path, transform: Transform, style: &str) {
    let colors = palette(style);
    let bounds = path.bounds();
    let light_x = bounds.left() + bounds.width() * 0.28;
    let light_y = bounds.top() + bounds.height() * 0.18;
    let gradient = LinearGradient::new(
        Point::from_xy(light_x, bounds.top()),
        Point::from_xy(bounds.right(), bounds.bottom()),
        vec![
            GradientStop::new(0.0, colors.light),
            GradientStop::new(0.43, colors.body),
            GradientStop::new(1.0, colors.shade),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let paint = Paint {
        shader: gradient,
        anti_alias: true,
        ..Paint::default()
    };
    pixmap.fill_path(path, &paint, FillRule::Winding, transform, None);

    // A wide diffuse light, clipped by the actual body, gives matte fabric its volume.
    let glow = RadialGradient::new(
        Point::from_xy(light_x, light_y),
        Point::from_xy(light_x, light_y),
        bounds.width().max(bounds.height()).max(1.0) * 0.72,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(255, 251, 232, 105)),
            GradientStop::new(1.0, Color::TRANSPARENT),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    pixmap.fill_path(
        path,
        &Paint {
            shader: glow,
            anti_alias: true,
            ..Paint::default()
        },
        FillRule::Winding,
        transform,
        None,
    );
    line(pixmap, path, transform, colors.edge, 0.42);
}

/// Patches and short seams stay in body coordinates, moving with the same breathing pose.
pub fn sewn_details(pixmap: &mut Pixmap, transform: Transform, style: &str) {
    let colors = palette(style);
    let mut seam = PathBuilder::new();
    match style {
        "mochi" => {
            oval(
                pixmap,
                transform,
                29.2,
                17.5,
                5.2,
                3.0,
                Color::from_rgba8(248, 240, 210, 150),
            );
            for x in [26.0, 29.0, 32.0] {
                seam.move_to(x, 11.0);
                seam.quad_to(x - 0.5, 12.3, x + 0.2, 13.0);
            }
            seam.move_to(18.5, 12.8);
            seam.quad_to(20.7, 15.6, 19.2, 19.0);
            oval(
                pixmap,
                transform,
                11.8,
                15.1,
                3.0,
                1.9,
                Color::from_rgba8(255, 247, 225, 185),
            );
        }
        "starlit" => {
            oval(
                pixmap,
                transform,
                24.0,
                14.7,
                9.5,
                3.8,
                Color::from_rgba8(253, 232, 219, 100),
            );
            let mut star = PathBuilder::new();
            for index in 0..10 {
                let angle = index as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                let radius = if index % 2 == 0 { 1.6 } else { 0.82 };
                let (x, y) = (24.0 + angle.cos() * radius, 6.8 + angle.sin() * radius);
                if index == 0 {
                    star.move_to(x, y);
                } else {
                    star.line_to(x, y);
                }
            }
            star.close();
            if let Some(path) = star.finish() {
                fill(pixmap, &path, transform, colors.accent);
                line(
                    pixmap,
                    &path,
                    transform,
                    Color::from_rgba8(142, 113, 102, 180),
                    0.18,
                );
            }
            for x in [13.5, 16.0, 18.5, 21.0, 23.5, 26.0, 28.5, 31.0, 33.5] {
                seam.move_to(x, 17.0);
                seam.line_to(x + 0.65, 17.25);
            }
        }
        "crab" => {
            oval(
                pixmap,
                transform,
                24.0,
                17.7,
                10.6,
                2.0,
                Color::from_rgba8(255, 219, 178, 125),
            );
            seam.move_to(13.4, 15.9);
            seam.quad_to(24.0, 19.8, 34.6, 15.9);
            for x in [19.4, 22.4, 25.4, 28.4] {
                oval(
                    pixmap,
                    transform,
                    x,
                    12.7,
                    0.45,
                    0.28,
                    Color::from_rgba8(255, 238, 208, 170),
                );
            }
        }
        "luma" => {
            oval(
                pixmap,
                transform,
                29.8,
                16.8,
                5.8,
                3.6,
                Color::from_rgba8(255, 246, 225, 180),
            );
            oval(
                pixmap,
                transform,
                12.1,
                15.0,
                3.4,
                2.1,
                Color::from_rgba8(255, 249, 233, 210),
            );
            for x in [22.5, 25.0, 27.5, 30.0, 32.5, 35.0] {
                seam.move_to(x, 19.6);
                seam.line_to(x + 0.65, 19.85);
            }
            let mut crescent = PathBuilder::new();
            crescent.move_to(22.3, 15.0);
            crescent.cubic_to(19.3, 14.8, 19.2, 19.2, 22.3, 19.0);
            crescent.cubic_to(20.5, 18.0, 20.5, 16.0, 22.3, 15.0);
            crescent.close();
            if let Some(path) = crescent.finish() {
                fill(
                    pixmap,
                    &path,
                    transform,
                    Color::from_rgba8(215, 177, 113, 230),
                );
            }
        }
        _ => {
            // The cloud is three stuffed lobes joined by a curved lower seam.
            seam.move_to(12.5, 20.5);
            seam.quad_to(24.0, 24.5, 35.5, 20.5);
            for (x, y) in [(12.0, 11.0), (13.0, 10.4), (33.0, 10.0), (34.0, 10.6)] {
                oval(
                    pixmap,
                    transform,
                    x,
                    y,
                    0.28,
                    0.14,
                    Color::from_rgba8(255, 253, 236, 170),
                );
            }
        }
    }
    if let Some(path) = seam.finish() {
        let mut color = colors.edge;
        color.set_alpha(0.42);
        line(pixmap, &path, transform, color, 0.23);
    }
    // A handful of visible threads gives scale to the fabric without noisy grain.
    let stitches = match style {
        "mochi" => [
            (24.5, 14.5),
            (27.0, 13.7),
            (33.0, 16.0),
            (30.0, 19.0),
            (22.0, 17.5),
        ],
        "starlit" => [
            (12.7, 12.5),
            (34.0, 12.0),
            (31.0, 7.5),
            (15.2, 7.5),
            (33.0, 15.0),
        ],
        "crab" => [
            (14.0, 15.0),
            (33.0, 15.0),
            (29.5, 18.0),
            (18.0, 18.0),
            (24.0, 12.0),
        ],
        "luma" => [
            (27.0, 11.5),
            (30.0, 10.7),
            (34.0, 16.0),
            (31.0, 19.0),
            (24.5, 18.0),
        ],
        _ => [
            (12.0, 15.0),
            (34.0, 14.0),
            (34.0, 19.0),
            (11.5, 18.0),
            (26.0, 7.5),
        ],
    };
    let mut threads = PathBuilder::new();
    for (x, y) in stitches {
        threads.move_to(x, y);
        threads.line_to(x + 0.6, y - 0.2);
    }
    if let Some(path) = threads.finish() {
        let mut color = colors.edge;
        color.set_alpha(0.22);
        line(pixmap, &path, transform, color, 0.16);
    }
}

pub fn nose(pixmap: &mut Pixmap, transform: Transform, style: &str, x: f32, y: f32) {
    if style != "mochi" && style != "luma" {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x - 0.66, y - 0.25);
    pb.quad_to(x, y - 0.48, x + 0.66, y - 0.25);
    pb.quad_to(x + 0.45, y + 0.16, x, y + 0.4);
    pb.quad_to(x - 0.45, y + 0.16, x - 0.66, y - 0.25);
    if let Some(path) = pb.finish() {
        fill(pixmap, &path, transform, palette(style).accent);
    }
    oval(
        pixmap,
        transform,
        x - 0.16,
        y - 0.13,
        0.18,
        0.09,
        Color::from_rgba8(255, 246, 225, 210),
    );
}

/// The crab's limbs are stuffed, rounded pieces, each included in the real touch mask.
pub fn crab_limbs(
    pixmap: &mut Pixmap,
    mask: &mut Pixmap,
    transform: Transform,
    time: f32,
    amplitude: f32,
) {
    let colors = palette("crab");
    for side in [-1.0f32, 1.0] {
        for index in 0..3 {
            let i = index as f32;
            let wave = (time * 3.0 + i * 0.9).sin() * 0.28 * amplitude;
            let x = 24.0 + side * (11.0 - i * 3.0);
            let end_x = x + side * (5.0 - i * 0.35);
            let y = 23.0 + i * 0.35 + wave;
            let mut leg = PathBuilder::new();
            leg.move_to(x - side * 1.0, 17.5);
            leg.cubic_to(
                x + side * 2.4,
                17.8,
                end_x + side * 0.2,
                20.8,
                end_x,
                y - 0.6,
            );
            leg.cubic_to(
                end_x + side * 0.2,
                y + 0.5,
                end_x - side * 1.4,
                y + 0.7,
                end_x - side * 1.6,
                y - 0.4,
            );
            leg.cubic_to(
                end_x - side * 1.8,
                21.0,
                x + side * 0.2,
                20.1,
                x - side * 1.8,
                18.8,
            );
            leg.close();
            if let Some(path) = leg.finish() {
                material(pixmap, &path, transform, "crab");
                fill(mask, &path, transform, Color::WHITE);
            }
        }

        let mut arm = PathBuilder::new();
        let x = 24.0 + side * 11.4;
        let elbow = 24.0 + side * 15.2;
        arm.move_to(x - side * 1.4, 14.0);
        arm.cubic_to(
            elbow - side * 0.8,
            14.0,
            elbow - side * 2.1,
            10.4,
            elbow,
            10.0,
        );
        arm.cubic_to(
            elbow + side * 2.7,
            10.0,
            elbow + side * 1.0,
            15.7,
            x + side * 1.2,
            16.2,
        );
        arm.close();
        if let Some(path) = arm.finish() {
            material(pixmap, &path, transform, "crab");
            fill(mask, &path, transform, Color::WHITE);
        }

        let wave = (time * 2.0 + if side < 0.0 { 0.0 } else { 1.4 }).sin();
        let claw_transform = transform
            .pre_translate(
                if side < 0.0 { 6.6 } else { 41.4 },
                7.7 + wave * 0.45 * amplitude,
            )
            .pre_rotate(side * wave * 5.0 * amplitude)
            .pre_scale(-side, if side < 0.0 { 0.96 } else { 1.05 });
        let opening = (wave * 0.5 + 0.5) * 0.65 * amplitude;
        let mut claw = PathBuilder::new();
        claw.move_to(2.4, 3.4);
        claw.cubic_to(4.4, 2.0, 4.4, -0.7, 3.0, -2.1);
        claw.cubic_to(2.0, -3.3, 0.5 + opening, -4.2, -0.5 + opening, -4.0);
        claw.cubic_to(-1.4 + opening, -3.7, -1.2 + opening, -2.4, -0.4, -1.0);
        claw.quad_to(0.5, 0.2, -0.1, 0.4);
        claw.cubic_to(-1.4, -0.4, -2.5 - opening, -2.9, -3.9 - opening, -3.0);
        claw.cubic_to(-6.2, -3.0, -6.4, -0.2, -4.6, 2.1);
        claw.cubic_to(-3.3, 4.3, 0.1, 4.7, 2.4, 3.4);
        claw.close();
        if let Some(path) = claw.finish() {
            material(pixmap, &path, claw_transform, "crab");
            fill(mask, &path, claw_transform, Color::WHITE);
            let mut crease = PathBuilder::new();
            crease.move_to(-3.0, 1.5);
            crease.quad_to(-0.4, 3.2, 2.0, 1.2);
            if let Some(path) = crease.finish() {
                let mut color = colors.edge;
                color.set_alpha(0.45);
                line(pixmap, &path, claw_transform, color, 0.23);
            }
        }
    }
}
