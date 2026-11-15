// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Layout {
    #[default]
    Floating,
    MasterStack,
    Spiral,
    Grid,
    Monocle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub gap: i32,
    pub outer_gap: i32,
    pub master_ratio: f32,
    pub master_count: usize,
}

#[derive(Clone, Copy)]
pub struct WorkspaceLayout {
    pub layout: Layout,
    pub ratio: f32,
    pub masters: usize,
}

impl WorkspaceLayout {
    pub fn new(tiling: &crate::config::Tiling) -> WorkspaceLayout {
        WorkspaceLayout {
            layout: tiling.layout,
            ratio: tiling.master_ratio,
            masters: tiling.master_count,
        }
    }
}

pub const ALL: [Layout; 5] = [
    Layout::Floating,
    Layout::MasterStack,
    Layout::Spiral,
    Layout::Grid,
    Layout::Monocle,
];

impl Layout {
    pub fn parse(name: &str) -> Option<Layout> {
        match name {
            "floating" => Some(Layout::Floating),
            "master-stack" => Some(Layout::MasterStack),
            "spiral" => Some(Layout::Spiral),
            "grid" => Some(Layout::Grid),
            "monocle" => Some(Layout::Monocle),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layout::Floating => "floating",
            Layout::MasterStack => "master-stack",
            Layout::Spiral => "spiral",
            Layout::Grid => "grid",
            Layout::Monocle => "monocle",
        }
    }

    pub fn next(self) -> Layout {
        let index = ALL.iter().position(|layout| *layout == self).unwrap_or(0);
        ALL[(index + 1) % ALL.len()]
    }

    pub fn tiles(self) -> bool {
        self != Layout::Floating
    }
}

fn shrink(area: Area, left: i32, top: i32, right: i32, bottom: i32) -> Area {
    Area {
        x: area.x + left,
        y: area.y + top,
        w: (area.w - left - right).max(1),
        h: (area.h - top - bottom).max(1),
    }
}

fn rows(area: Area, count: usize) -> Vec<Area> {
    let count = count.max(1) as i32;
    let base = area.h / count;
    let extra = area.h % count;
    let mut y = area.y;
    (0..count)
        .map(|index| {
            let h = base + i32::from(index < extra);
            let tile = Area { x: area.x, y, w: area.w, h };
            y += h;
            tile
        })
        .collect()
}

fn columns(area: Area, count: usize) -> Vec<Area> {
    let count = count.max(1) as i32;
    let base = area.w / count;
    let extra = area.w % count;
    let mut x = area.x;
    (0..count)
        .map(|index| {
            let w = base + i32::from(index < extra);
            let tile = Area { x, y: area.y, w, h: area.h };
            x += w;
            tile
        })
        .collect()
}

fn master_stack(area: Area, count: usize, params: &Params) -> Vec<Area> {
    let masters = params.master_count.clamp(1, count);
    if count <= masters {
        return rows(area, count);
    }

    let master_width = ((area.w as f32 * params.master_ratio).round() as i32).clamp(1, area.w - 1);
    let master_area = Area { w: master_width, ..area };
    let stack_area = Area {
        x: area.x + master_width,
        w: area.w - master_width,
        ..area
    };

    let mut tiles = rows(master_area, masters);
    tiles.extend(rows(stack_area, count - masters));
    tiles
}

fn spiral(area: Area, count: usize, params: &Params) -> Vec<Area> {
    let mut tiles = Vec::with_capacity(count);
    let mut rest = area;

    for index in 0..count {
        if index + 1 == count {
            tiles.push(rest);
            break;
        }
        if index % 2 == 0 {
            let width = ((rest.w as f32 * params.master_ratio).round() as i32).clamp(1, (rest.w - 1).max(1));
            tiles.push(Area { w: width, ..rest });
            rest = Area {
                x: rest.x + width,
                w: (rest.w - width).max(1),
                ..rest
            };
        } else {
            let height = ((rest.h as f32 * params.master_ratio).round() as i32).clamp(1, (rest.h - 1).max(1));
            tiles.push(Area { h: height, ..rest });
            rest = Area {
                y: rest.y + height,
                h: (rest.h - height).max(1),
                ..rest
            };
        }
    }
    tiles
}

fn grid(area: Area, count: usize) -> Vec<Area> {
    let columns_count = (count as f32).sqrt().ceil() as usize;
    let rows_count = count.div_ceil(columns_count);
    let mut tiles = Vec::with_capacity(count);

    for (row_index, row) in rows(area, rows_count).into_iter().enumerate() {
        let in_row = if row_index + 1 == rows_count {
            count - columns_count * (rows_count - 1)
        } else {
            columns_count
        };
        tiles.extend(columns(row, in_row));
    }
    tiles
}

pub fn arrange(layout: Layout, area: Area, count: usize, params: &Params) -> Vec<Area> {
    if count == 0 || !layout.tiles() {
        return Vec::new();
    }

    let half = params.gap / 2;
    let rest = params.gap - half;
    let inner = shrink(
        area,
        params.outer_gap - half,
        params.outer_gap - half,
        params.outer_gap - rest,
        params.outer_gap - rest,
    );

    let tiles = match layout {
        Layout::Floating => Vec::new(),
        Layout::MasterStack => master_stack(inner, count, params),
        Layout::Spiral => spiral(inner, count, params),
        Layout::Grid => grid(inner, count),
        Layout::Monocle => vec![inner; count],
    };

    tiles
        .into_iter()
        .map(|tile| shrink(tile, half, half, rest, rest))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Area = Area {
        x: 0,
        y: 0,
        w: 1000,
        h: 600,
    };

    fn params(gap: i32, outer_gap: i32) -> Params {
        Params {
            gap,
            outer_gap,
            master_ratio: 0.5,
            master_count: 1,
        }
    }

    fn overlap(a: &Area, b: &Area) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn every_tiling_layout_fills_the_area_without_overlap() {
        for layout in [Layout::MasterStack, Layout::Spiral, Layout::Grid] {
            for count in 1..=9 {
                let tiles = arrange(layout, AREA, count, &params(0, 0));
                assert_eq!(tiles.len(), count, "{layout:?} {count}");

                let covered: i64 = tiles.iter().map(|tile| tile.w as i64 * tile.h as i64).sum();
                assert_eq!(covered, 1000 * 600, "{layout:?} {count}");
                for (index, tile) in tiles.iter().enumerate() {
                    assert!(tile.w > 0 && tile.h > 0);
                    for other in &tiles[index + 1..] {
                        assert!(!overlap(tile, other), "{layout:?} {count}: {tile:?} {other:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn master_stack_puts_the_master_on_the_left() {
        let tiles = arrange(Layout::MasterStack, AREA, 3, &params(0, 0));
        assert_eq!(tiles[0], Area { x: 0, y: 0, w: 500, h: 600 });
        assert_eq!(tiles[1], Area { x: 500, y: 0, w: 500, h: 300 });
        assert_eq!(tiles[2], Area { x: 500, y: 300, w: 500, h: 300 });

        let two_masters = arrange(
            Layout::MasterStack,
            AREA,
            3,
            &Params {
                master_count: 2,
                ..params(0, 0)
            },
        );
        assert_eq!(two_masters[0].h, 300);
        assert_eq!(two_masters[1].h, 300);
        assert_eq!(two_masters[2].w, 500);
    }

    #[test]
    fn gaps_separate_tiles_and_keep_the_outer_margin() {
        let tiles = arrange(Layout::MasterStack, AREA, 2, &params(10, 20));

        assert_eq!(tiles[0].x, 20);
        assert_eq!(tiles[0].y, 20);
        assert_eq!(tiles[1].x + tiles[1].w, 1000 - 20);
        assert_eq!(tiles[1].x - (tiles[0].x + tiles[0].w), 10);
        assert_eq!(tiles[0].y + tiles[0].h, 600 - 20);
    }

    #[test]
    fn a_single_window_fills_the_area_and_monocle_stacks_all() {
        let single = arrange(Layout::Spiral, AREA, 1, &params(0, 0));
        assert_eq!(single, vec![AREA]);

        let monocle = arrange(Layout::Monocle, AREA, 4, &params(0, 8));
        assert_eq!(monocle.len(), 4);
        assert!(monocle.iter().all(|tile| *tile == monocle[0]));
        assert!(arrange(Layout::Floating, AREA, 3, &params(0, 0)).is_empty());
    }

    #[test]
    fn layouts_cycle_and_parse() {
        let mut layout = Layout::Floating;
        for _ in 0..ALL.len() {
            layout = layout.next();
        }
        assert_eq!(layout, Layout::Floating);
        for candidate in ALL {
            assert_eq!(Layout::parse(candidate.name()), Some(candidate));
        }
    }
}
