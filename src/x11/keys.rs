// SPDX-License-Identifier: GPL-3.0-only

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, Keycode};

pub fn keycode_for(conn: &impl Connection, keysym: u32) -> Result<Option<Keycode>, Box<dyn std::error::Error>> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode - min + 1;
    let mapping = conn.get_keyboard_mapping(min, count)?.reply()?;
    let per_keycode = mapping.keysyms_per_keycode as usize;

    for (index, keysyms) in mapping.keysyms.chunks(per_keycode).enumerate() {
        if keysyms.iter().take(2).any(|&candidate| candidate == keysym) {
            return Ok(Some(min + index as u8));
        }
    }
    Ok(None)
}
