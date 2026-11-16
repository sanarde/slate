// SPDX-License-Identifier: GPL-3.0-only

#ifndef SLATE_X11_H
#define SLATE_X11_H

#include <stddef.h>
#include <stdint.h>

struct sl_rect {
	int32_t x;
	int32_t y;
	uint32_t w;
	uint32_t h;
};

struct sl_strut {
	uint32_t left;
	uint32_t right;
	uint32_t top;
	uint32_t bottom;
};

int sl_rect_contains(const struct sl_rect *rect, int32_t x, int32_t y);
int sl_strut_parse(const uint32_t *values, size_t count, struct sl_strut *strut);
void sl_strut_merge(struct sl_strut *into, const struct sl_strut *other);
struct sl_rect sl_bounds(const struct sl_rect *monitors, size_t count, const struct sl_rect *fallback);
void sl_work_areas(const struct sl_rect *monitors, size_t count, const struct sl_rect *bounds,
		   const struct sl_strut *strut, struct sl_rect *areas);
uint8_t sl_keycode_for(const uint32_t *keysyms, size_t per_keycode, size_t count, uint8_t min_keycode,
		       uint32_t keysym);
size_t sl_monitor_index(const struct sl_rect *monitors, size_t count, const struct sl_rect *rect);

#endif
