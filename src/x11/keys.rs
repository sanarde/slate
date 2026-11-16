// SPDX-License-Identifier: GPL-3.0-only

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, Keycode};

unsafe extern "C" {
    fn sl_keycode_for(keysyms: *const u32, per_keycode: usize, count: usize, min_keycode: u8, keysym: u32) -> u8;
}

pub struct Keymap {
    keysyms: Vec<u32>,
    per_keycode: usize,
    min_keycode: u8,
}

impl Keymap {
    pub fn fetch(conn: &impl Connection) -> Result<Keymap, Box<dyn std::error::Error>> {
        let setup = conn.setup();
        let min_keycode = setup.min_keycode;
        let count = setup.max_keycode - min_keycode + 1;
        let mapping = conn.get_keyboard_mapping(min_keycode, count)?.reply()?;
        Ok(Keymap {
            keysyms: mapping.keysyms,
            per_keycode: mapping.keysyms_per_keycode as usize,
            min_keycode,
        })
    }

    #[cfg(test)]
    pub fn from_table(keysyms: Vec<u32>, per_keycode: usize, min_keycode: u8) -> Keymap {
        Keymap {
            keysyms,
            per_keycode,
            min_keycode,
        }
    }

    pub fn keycode_for(&self, keysym: u32) -> Option<Keycode> {
        let count = self.keysyms.len().checked_div(self.per_keycode).unwrap_or(0);
        let keycode = unsafe { sl_keycode_for(self.keysyms.as_ptr(), self.per_keycode, count, self.min_keycode, keysym) };
        (keycode != 0).then_some(keycode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u32 = 0x61;
    const SHIFT_A: u32 = 0x41;
    const B: u32 = 0x62;
    const LATER_COLUMN: u32 = 0x1008ff11;

    fn keymap() -> Keymap {
        Keymap::from_table(
            vec![
                A, SHIFT_A, 0, 0, //
                B, 0, 0, LATER_COLUMN, //
                A, 0, 0, 0,
            ],
            4,
            8,
        )
    }

    #[test]
    fn the_first_keycode_with_the_keysym_in_one_of_the_two_first_columns_wins() {
        let keymap = keymap();
        assert_eq!(keymap.keycode_for(A), Some(8));
        assert_eq!(keymap.keycode_for(SHIFT_A), Some(8));
        assert_eq!(keymap.keycode_for(B), Some(9));
    }

    #[test]
    fn keysyms_in_the_other_columns_and_missing_ones_are_not_found() {
        let keymap = keymap();
        assert_eq!(keymap.keycode_for(LATER_COLUMN), None);
        assert_eq!(keymap.keycode_for(0x7a), None);
    }

    #[test]
    fn an_empty_or_malformed_table_finds_nothing_instead_of_panicking() {
        assert_eq!(Keymap::from_table(Vec::new(), 0, 8).keycode_for(A), None);
        assert_eq!(Keymap::from_table(vec![A, B], 0, 8).keycode_for(A), None);
        assert_eq!(Keymap::from_table(vec![A], 1, 8).keycode_for(A), Some(8));
    }

    #[test]
    fn keycodes_past_255_are_refused() {
        let keymap = Keymap::from_table(vec![0, 0, 0, 0, A, 0], 2, 254);
        assert_eq!(keymap.keycode_for(A), None);
    }
}
