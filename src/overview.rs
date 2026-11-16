// SPDX-License-Identifier: GPL-3.0-only

pub type Bounds = (i32, i32, i32, i32);

pub fn grid(count: usize, area: Bounds, gap: i32) -> Vec<Bounds> {
    if count == 0 || area.2 <= 0 || area.3 <= 0 {
        return Vec::new();
    }
    let columns = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let width = ((area.2 - gap * (columns as i32 + 1)) / columns as i32).max(1);
    let height = ((area.3 - gap * (rows as i32 + 1)) / rows as i32).max(1);

    (0..count)
        .map(|index| {
            let row = index / columns;
            let in_row = if row == rows - 1 { count - row * columns } else { columns };
            let used = in_row as i32 * width + (in_row as i32 - 1) * gap;
            let left = area.0 + (area.2 - used) / 2;
            let column = (index - row * columns) as i32;
            (
                left + column * (width + gap),
                area.1 + gap + row as i32 * (height + gap),
                width,
                height,
            )
        })
        .collect()
}

pub fn row(count: usize, area: Bounds, cell: (i32, i32), gap: i32) -> (Vec<Bounds>, Bounds) {
    if count == 0 {
        return (Vec::new(), (0, 0, 0, 0));
    }
    let count_i = count as i32;
    let available = (area.2 * 9 / 10 - gap * (count_i + 1)).max(count_i);
    let width = cell.0.min(available / count_i).max(1);
    let height = (cell.1 * width / cell.0.max(1)).max(1);
    let total = count_i * width + (count_i + 1) * gap;
    let left = area.0 + (area.2 - total) / 2;
    let top = area.1 + (area.3 - height - 2 * gap) / 2;

    let cells = (0..count_i)
        .map(|index| (left + gap + index * (width + gap), top + gap, width, height))
        .collect();
    (cells, (left, top, total, height + 2 * gap))
}

pub fn fit(size: (i32, i32), cell: Bounds) -> (f64, (i32, i32)) {
    let scale = (f64::from(cell.2) / f64::from(size.0.max(1)))
        .min(f64::from(cell.3) / f64::from(size.1.max(1)))
        .min(1.0);
    let width = (f64::from(size.0) * scale).round() as i32;
    let height = (f64::from(size.1) * scale).round() as i32;
    (scale, (cell.0 + (cell.2 - width) / 2, cell.1 + (cell.3 - height) / 2))
}

pub fn contains(bounds: Bounds, x: f64, y: f64) -> bool {
    x >= f64::from(bounds.0)
        && y >= f64::from(bounds.1)
        && x < f64::from(bounds.0 + bounds.2)
        && y < f64::from(bounds.1 + bounds.3)
}

pub fn step(selected: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if forward { (selected + 1) % count } else { (selected + count - 1) % count }
}

pub fn move_in_grid(selected: usize, count: usize, dx: i32, dy: i32) -> usize {
    if count == 0 {
        return 0;
    }
    let columns = (count as f64).sqrt().ceil() as i32;
    let target = selected as i32 + dx + dy * columns;
    if (0..count as i32).contains(&target) { target as usize } else { selected }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Bounds = (0, 0, 1920, 1080);

    #[test]
    fn a_grid_fits_inside_the_area_without_overlap() {
        for count in 1..=12 {
            let cells = grid(count, SCREEN, 24);
            assert_eq!(cells.len(), count);
            for (index, cell) in cells.iter().enumerate() {
                assert!(cell.0 >= 0 && cell.1 >= 0 && cell.0 + cell.2 <= SCREEN.2 && cell.1 + cell.3 <= SCREEN.3);
                for other in &cells[index + 1..] {
                    let apart = cell.0 + cell.2 <= other.0
                        || other.0 + other.2 <= cell.0
                        || cell.1 + cell.3 <= other.1
                        || other.1 + other.3 <= cell.1;
                    assert!(apart, "{count}: {cell:?} {other:?}");
                }
            }
        }
        assert!(grid(0, SCREEN, 24).is_empty());
    }

    #[test]
    fn the_last_row_of_a_grid_is_centered() {
        let cells = grid(5, SCREEN, 20);
        let left = cells[3].0;
        let right = cells[4].0 + cells[4].2;
        assert_eq!(left - SCREEN.0, SCREEN.2 - right);
    }

    #[test]
    fn a_row_is_centered_and_shrinks_for_many_windows() {
        let (cells, panel) = row(3, SCREEN, (320, 200), 16);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].2, 320);
        assert_eq!(panel.0 - SCREEN.0, SCREEN.2 - (panel.0 + panel.2));

        let (many, panel) = row(20, SCREEN, (320, 200), 16);
        assert!(many[0].2 < 320);
        assert!(panel.2 <= SCREEN.2);
    }

    #[test]
    fn windows_are_scaled_down_to_fit_but_never_up() {
        let (scale, location) = fit((1000, 500), (0, 0, 400, 400));
        assert!((scale - 0.4).abs() < 1e-9);
        assert_eq!(location, (0, 100));
        assert_eq!(fit((100, 50), (0, 0, 400, 400)).0, 1.0);
    }

    #[test]
    fn selection_wraps_and_moves_over_the_grid() {
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(move_in_grid(0, 4, 1, 0), 1);
        assert_eq!(move_in_grid(0, 4, 0, 1), 2);
        assert_eq!(move_in_grid(3, 4, 1, 0), 3);
    }
}
