// SPDX-License-Identifier: GPL-3.0-only

#include "slate_x11.h"

#define SL_KEYSYM_COLUMNS 2
#define SL_MAX_KEYCODE 255

uint8_t sl_keycode_for(const uint32_t *keysyms, size_t per_keycode, size_t count, uint8_t min_keycode,
		       uint32_t keysym)
{
	size_t columns = per_keycode < SL_KEYSYM_COLUMNS ? per_keycode : SL_KEYSYM_COLUMNS;
	size_t index;
	size_t column;

	if (per_keycode == 0)
		return 0;

	for (index = 0; index < count; index++) {
		size_t keycode = (size_t)min_keycode + index;

		if (keycode > SL_MAX_KEYCODE)
			return 0;
		for (column = 0; column < columns; column++) {
			if (keysyms[index * per_keycode + column] == keysym)
				return (uint8_t)keycode;
		}
	}
	return 0;
}
