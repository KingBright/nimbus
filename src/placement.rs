/// Desktop coordinates and dimensions in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct BubblePlacement {
    pub x: i32,
    pub y: i32,
    pub on_bottom: bool,
    pub tail_x: f32,
}

fn inset_bounds(screen: ScreenRect, margin: u32) -> (i64, i64, i64, i64) {
    let inset_x = margin.min(screen.width.saturating_sub(1) / 2) as i64;
    let inset_y = margin.min(screen.height.saturating_sub(1) / 2) as i64;
    (
        screen.x as i64 + inset_x,
        screen.y as i64 + inset_y,
        screen.x as i64 + screen.width.max(1) as i64 - inset_x,
        screen.y as i64 + screen.height.max(1) as i64 - inset_y,
    )
}

fn coordinate(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

pub fn default_pet_position(
    screen: ScreenRect,
    pet_size: (u32, u32),
    margins: (u32, u32),
) -> (i32, i32) {
    let x = screen.x as i64 + (screen.width as i64 - pet_size.0 as i64 - margins.0 as i64).max(0);
    let y = screen.y as i64 + (screen.height as i64 - pet_size.1 as i64 - margins.1 as i64).max(0);
    (coordinate(x), coordinate(y))
}

/// Prefer the larger vertical space next to the pet, retaining room for screen edges.
pub fn bubble_limits(screen: ScreenRect, pet: ScreenRect, margin: u32) -> (u32, u32) {
    let (left, top, right, bottom) = inset_bounds(screen, margin);
    let above = (pet.y as i64 - top).clamp(0, bottom - top);
    let below = (bottom - pet.y as i64 - pet.height as i64).clamp(0, bottom - top);
    let height = above.max(below);
    (
        (right - left) as u32,
        if height == 0 {
            (bottom - top) as u32
        } else {
            height as u32
        },
    )
}

pub fn place_bubble(
    screen: ScreenRect,
    pet: ScreenRect,
    bubble_size: (u32, u32),
    margin: u32,
) -> BubblePlacement {
    let (left, top, right, bottom) = inset_bounds(screen, margin);
    let width = bubble_size.0 as i64;
    let height = bubble_size.1 as i64;
    let pet_bottom = pet.y as i64 + pet.height as i64;
    let above = (pet.y as i64 - top).max(0);
    let below = (bottom - pet_bottom).max(0);
    let on_bottom = above < height && below > above;
    let pet_center_x = pet.x as i64 + pet.width as i64 / 2;
    let x = (pet_center_x - width / 2).clamp(left, (right - width).max(left));
    let ideal_y = if on_bottom {
        pet_bottom
    } else {
        pet.y as i64 - height
    };
    let y = ideal_y.clamp(top, (bottom - height).max(top));
    BubblePlacement {
        x: coordinate(x),
        y: coordinate(y),
        on_bottom,
        tail_x: (pet_center_x - x) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn default_position_respects_negative_screen_origin_and_oversized_pet() {
        let screen = rect(-1920, -1080, 1920, 1080);
        assert_eq!(
            default_pet_position(screen, (200, 180), (50, 100)),
            (-250, -280)
        );
        assert_eq!(
            default_pet_position(screen, (3000, 2000), (50, 100)),
            (-1920, -1080)
        );
    }

    #[test]
    fn bubble_uses_screen_top_instead_of_desktop_zero() {
        for top in [-1080, 0, 1080] {
            let screen = rect(-1920, top, 1920, 1080);
            let pet = rect(-220, top + 20, 180, 160);
            let placement = place_bubble(screen, pet, (420, 240), 8);
            assert!(placement.on_bottom);
            assert_eq!(placement.y, top + 180);
            assert_eq!(placement.x, -428);
            assert_eq!(placement.tail_x, 298.0);
        }
    }

    #[test]
    fn bubble_stays_inside_every_screen_corner() {
        let screen = rect(-1280, 900, 1280, 720);
        for (x, y) in [(-1280, 900), (-180, 900), (-1280, 1460), (-180, 1460)] {
            let pet = rect(x, y, 180, 160);
            let limits = bubble_limits(screen, pet, 8);
            let size = (420.min(limits.0), 400.min(limits.1));
            let p = place_bubble(screen, pet, size, 8);
            assert!(p.x >= screen.x + 8 && p.y >= screen.y + 8);
            assert!(p.x as i64 + size.0 as i64 <= -8);
            assert!(p.y as i64 + size.1 as i64 <= 1612);
        }
    }

    #[test]
    fn tiny_screens_and_large_dimensions_do_not_underflow() {
        let screen = rect(-20, -10, 1, 1);
        let pet = rect(-20, -10, u32::MAX, u32::MAX);
        assert_eq!(bubble_limits(screen, pet, 8), (1, 1));
        let p = place_bubble(screen, pet, (u32::MAX, u32::MAX), 8);
        assert_eq!((p.x, p.y), (-20, -10));
    }
}
