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
struct sl_rect sl_rect_union(const struct sl_rect *a, const struct sl_rect *b);
int64_t sl_rect_overlap_area(const struct sl_rect *a, const struct sl_rect *b);
struct sl_rect sl_rect_work_area(const struct sl_rect *monitor, const struct sl_rect *bounds,
				 const struct sl_strut *strut);
size_t sl_monitor_index(const struct sl_rect *monitors, size_t count, const struct sl_rect *rect);

#endif
