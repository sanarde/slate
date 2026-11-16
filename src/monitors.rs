// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Next,
    Previous,
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn parse(text: &str) -> Result<Direction, String> {
        Ok(match text.trim() {
            "next" => Direction::Next,
            "prev" | "previous" => Direction::Previous,
            "left" => Direction::Left,
            "right" => Direction::Right,
            "up" => Direction::Up,
            "down" => Direction::Down,
            other => return Err(format!("unknown direction {other}, use next, prev, left, right, up or down")),
        })
    }
}

pub type Bounds = (i32, i32, i32, i32);

fn center(bounds: Bounds) -> (i64, i64) {
    (
        i64::from(bounds.0) + i64::from(bounds.2) / 2,
        i64::from(bounds.1) + i64::from(bounds.3) / 2,
    )
}

pub fn pick(monitors: &[Bounds], current: usize, direction: Direction) -> Option<usize> {
    if monitors.len() < 2 || current >= monitors.len() {
        return None;
    }

    if matches!(direction, Direction::Next | Direction::Previous) {
        let mut order: Vec<usize> = (0..monitors.len()).collect();
        order.sort_by_key(|index| (monitors[*index].0, monitors[*index].1));
        let position = order.iter().position(|index| *index == current)?;
        let step = if direction == Direction::Next { 1 } else { order.len() - 1 };
        return Some(order[(position + step) % order.len()]);
    }

    let (cx, cy) = center(monitors[current]);
    monitors
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != current)
        .filter_map(|(index, bounds)| {
            let (x, y) = center(*bounds);
            let (dx, dy) = (x - cx, y - cy);
            let ahead = match direction {
                Direction::Left => dx < 0 && dx.abs() >= dy.abs(),
                Direction::Right => dx > 0 && dx.abs() >= dy.abs(),
                Direction::Up => dy < 0 && dy.abs() > dx.abs(),
                _ => dy > 0 && dy.abs() > dx.abs(),
            };
            ahead.then_some((index, dx * dx + dy * dy))
        })
        .min_by_key(|(_, distance)| *distance)
        .map(|(index, _)| index)
}

pub const GRIP: i32 = 48;

pub fn keep_reachable(frame: Bounds, bounds: Bounds) -> (i32, i32) {
    let x = frame.0.clamp(bounds.0 - frame.2 + GRIP, (bounds.0 + bounds.2 - GRIP).max(bounds.0 - frame.2 + GRIP));
    let y = frame.1.clamp(bounds.1, (bounds.1 + bounds.3 - GRIP).max(bounds.1));
    (x, y)
}

pub fn clamp_into(frame: Bounds, source: Bounds, target: Bounds) -> (i32, i32) {
    let offset_x = (frame.0 - source.0).clamp(0, (target.2 - frame.2).max(0));
    let offset_y = (frame.1 - source.1).clamp(0, (target.3 - frame.3).max(0));
    (target.0 + offset_x, target.1 + offset_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: [Bounds; 3] = [(1920, 0, 1920, 1080), (0, 0, 1920, 1080), (3840, 0, 1280, 1024)];

    #[test]
    fn next_and_previous_follow_the_screen_order() {
        assert_eq!(pick(&ROW, 1, Direction::Next), Some(0));
        assert_eq!(pick(&ROW, 0, Direction::Next), Some(2));
        assert_eq!(pick(&ROW, 2, Direction::Next), Some(1));
        assert_eq!(pick(&ROW, 1, Direction::Previous), Some(2));
    }

    #[test]
    fn directions_pick_the_nearest_neighbour() {
        assert_eq!(pick(&ROW, 1, Direction::Right), Some(0));
        assert_eq!(pick(&ROW, 0, Direction::Left), Some(1));
        assert_eq!(pick(&ROW, 1, Direction::Left), None);
        assert_eq!(pick(&ROW, 0, Direction::Up), None);

        let stacked = [(0, 0, 1920, 1080), (0, 1080, 1920, 1080)];
        assert_eq!(pick(&stacked, 0, Direction::Down), Some(1));
        assert_eq!(pick(&stacked, 1, Direction::Up), Some(0));
    }

    #[test]
    fn one_monitor_has_no_neighbour() {
        assert_eq!(pick(&[(0, 0, 100, 100)], 0, Direction::Next), None);
    }

    #[test]
    fn a_moved_frame_keeps_its_offset_and_stays_inside() {
        let source = (0, 0, 1920, 1080);
        let target = (1920, 0, 1280, 720);
        assert_eq!(clamp_into((100, 50, 400, 300), source, target), (2020, 50));
        assert_eq!(clamp_into((1500, 900, 400, 300), source, target), (1920 + 880, 420));
    }

    #[test]
    fn a_dragged_frame_keeps_a_grip_on_the_screen() {
        let screen = (0, 0, 1920, 1080);
        assert_eq!(keep_reachable((500, 300, 400, 300), screen), (500, 300));
        assert_eq!(keep_reachable((-900, -50, 400, 300), screen), (-352, 0));
        assert_eq!(keep_reachable((3000, 2000, 400, 300), screen), (1872, 1032));
    }

    #[test]
    fn directions_parse() {
        assert_eq!(Direction::parse("prev"), Ok(Direction::Previous));
        assert!(Direction::parse("sideways").is_err());
    }
}
