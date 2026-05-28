use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetState {
    Idle,
    Tickled,
    Surprised,
    Thinking,
    Muttering,
    Working,
    Tired,
    Bored,
    Sleeping,
}

impl PetState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Tickled => "tickled",
            Self::Surprised => "surprised",
            Self::Thinking => "thinking",
            Self::Muttering => "muttering",
            Self::Working => "working",
            Self::Tired => "tired",
            Self::Bored => "bored",
            Self::Sleeping => "sleeping",
        }
    }
}

pub struct StateContext<'a> {
    pub surprised: bool,
    pub pointer_on_pet: bool,
    pub dreaming: bool,
    pub muttering: bool,
    pub thinking: bool,
    pub idle_duration: Duration,
    pub active_app: &'a str,
    pub hour: u32,
}

#[cfg(test)]
const CLOUD_OUTLINE: [(f64, f64); 5] = [
    (8.0, 24.0),
    (10.0, 10.0),
    (22.0, 8.0),
    (38.0, 12.0),
    (38.0, 24.0),
];
#[cfg(test)]
const CLOUD_CUBICS: [[(f64, f64); 4]; 4] = [
    [(8.0, 24.0), (4.0, 20.0), (4.0, 12.0), (10.0, 10.0)],
    [(10.0, 10.0), (10.0, 4.0), (18.0, 4.0), (22.0, 8.0)],
    [(22.0, 8.0), (26.0, 2.0), (36.0, 4.0), (38.0, 12.0)],
    [(38.0, 12.0), (44.0, 14.0), (44.0, 22.0), (38.0, 24.0)],
];

pub fn is_dev_app(app: &str) -> bool {
    let app_lower = app.to_lowercase();
    app_lower.contains("code")
        || app_lower.contains("terminal")
        || app_lower.contains("xcode")
        || app_lower.contains("cargo")
        || app_lower.contains("idea")
        || app_lower.contains("warp")
}

#[cfg(test)]
pub fn pet_visual_size(window_width: f64) -> f64 {
    (window_width * 0.56).clamp(120.0, 280.0)
}

#[cfg(test)]
pub fn pet_local_point(x: f64, y: f64, window_width: f64, window_height: f64) -> (f64, f64) {
    let pet_size = pet_visual_size(window_width);
    let pet_x = (window_width - pet_size) / 2.0;
    let pet_y = window_height - pet_size * 0.85;
    let view_scale = pet_size / 48.0;
    let view_tx = pet_x + pet_size / 2.0;
    let view_ty = pet_y + pet_size / 2.0;

    (
        (x - view_tx) / view_scale + 12.0,
        (y - view_ty) / view_scale + 12.0,
    )
}

#[cfg(test)]
pub fn is_on_pet(x: f64, y: f64, window_width: f64, window_height: f64) -> bool {
    if window_width <= 0.0 || window_height <= 0.0 {
        return false;
    }

    let (local_x, local_y) = pet_local_point(x, y, window_width, window_height);
    is_on_cloud_body(local_x, local_y)
}

#[cfg(test)]
fn is_on_cloud_body(x: f64, y: f64) -> bool {
    if !(4.0..=44.0).contains(&x) || !(2.0..=24.5).contains(&y) {
        return false;
    }

    let mut outline = Vec::with_capacity(CLOUD_CUBICS.len() * 10 + 2);
    outline.push(CLOUD_OUTLINE[0]);
    for cubic in CLOUD_CUBICS {
        for step in 1..=10 {
            outline.push(cubic_point(cubic, step as f64 / 10.0));
        }
    }
    outline.push(CLOUD_OUTLINE[0]);

    point_in_polygon((x, y), &outline)
}

#[cfg(test)]
fn cubic_point(points: [(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * points[0].0 + b * points[1].0 + c * points[2].0 + d * points[3].0,
        a * points[0].1 + b * points[1].1 + c * points[2].1 + d * points[3].1,
    )
}

#[cfg(test)]
fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let (x, y) = point;
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];

    for &current in polygon {
        if ((current.1 > y) != (previous.1 > y))
            && (x
                < (previous.0 - current.0) * (y - current.1) / (previous.1 - current.1) + current.0)
        {
            inside = !inside;
        }
        previous = current;
    }

    inside
}

pub fn transition_delay(from: PetState, to: PetState) -> Duration {
    if from == to {
        Duration::ZERO
    } else if matches!(
        to,
        PetState::Thinking | PetState::Muttering | PetState::Surprised
    ) {
        Duration::from_millis(80)
    } else if matches!(to, PetState::Tickled) {
        Duration::from_millis(260)
    } else if matches!(to, PetState::Sleeping) {
        Duration::from_secs(12)
    } else if matches!(from, PetState::Sleeping) {
        Duration::from_millis(900)
    } else {
        Duration::from_millis(420)
    }
}

pub fn decide_state(ctx: &StateContext<'_>) -> PetState {
    if ctx.surprised {
        PetState::Surprised
    } else if ctx.pointer_on_pet {
        PetState::Tickled
    } else if ctx.dreaming {
        PetState::Sleeping
    } else if ctx.muttering {
        if ctx.thinking {
            PetState::Thinking
        } else {
            PetState::Muttering
        }
    } else if ctx.idle_duration > Duration::from_secs(900) {
        PetState::Sleeping
    } else if ctx.idle_duration > Duration::from_secs(90) {
        PetState::Bored
    } else if is_dev_app(ctx.active_app) {
        PetState::Working
    } else if ctx.hour >= 23 || ctx.hour <= 5 {
        PetState::Tired
    } else {
        PetState::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> StateContext<'static> {
        StateContext {
            surprised: false,
            pointer_on_pet: false,
            dreaming: false,
            muttering: false,
            thinking: false,
            idle_duration: Duration::from_secs(1),
            active_app: "Finder",
            hour: 12,
        }
    }

    #[test]
    fn prioritizes_direct_interaction() {
        let mut ctx = base();
        ctx.pointer_on_pet = true;
        ctx.muttering = true;
        assert_eq!(decide_state(&ctx), PetState::Tickled);
    }

    #[test]
    fn sleeping_dream_keeps_sleep_visual_state() {
        let mut ctx = base();
        ctx.dreaming = true;
        ctx.muttering = true;
        assert_eq!(decide_state(&ctx), PetState::Sleeping);
    }

    #[test]
    fn detects_working_context() {
        let mut ctx = base();
        ctx.active_app = "Visual Studio Code";
        assert_eq!(decide_state(&ctx), PetState::Working);
    }

    #[test]
    fn pet_hitbox_is_stable() {
        assert!(is_on_pet(180.0, 370.0, 360.0, 440.0));
        assert!(!is_on_pet(180.0, 300.0, 360.0, 440.0));
        assert!(!is_on_pet(320.0, 370.0, 360.0, 440.0));
    }

    #[test]
    fn cloud_hitbox_follows_rendered_body_not_window_edges() {
        assert!(is_on_cloud_body(24.0, 18.0));
        assert!(is_on_cloud_body(37.0, 18.0));
        assert!(!is_on_cloud_body(24.0, 4.0));
        assert!(!is_on_cloud_body(4.0, 24.0));
    }

    #[test]
    fn short_idle_does_not_force_sleep() {
        let ctx = base();
        assert_eq!(decide_state(&ctx), PetState::Idle);
    }

    #[test]
    fn sleep_requires_long_idle_or_dream() {
        let mut ctx = base();
        ctx.idle_duration = Duration::from_secs(901);
        assert_eq!(decide_state(&ctx), PetState::Sleeping);
    }

    #[test]
    fn sleeping_transition_is_deliberate() {
        assert!(transition_delay(PetState::Idle, PetState::Sleeping) >= Duration::from_secs(10));
        assert!(transition_delay(PetState::Sleeping, PetState::Idle) >= Duration::from_millis(800));
    }
}
