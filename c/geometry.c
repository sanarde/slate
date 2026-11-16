// SPDX-License-Identifier: GPL-3.0-only

#include "slate_x11.h"

static int64_t right_of(const struct sl_rect *rect)
{
	return (int64_t)rect->x + rect->w;
}

static int64_t bottom_of(const struct sl_rect *rect)
{
	return (int64_t)rect->y + rect->h;
}

static int64_t max64(int64_t a, int64_t b)
{
	return a > b ? a : b;
}

static int64_t min64(int64_t a, int64_t b)
{
	return a < b ? a : b;
}

static uint32_t saturating_sub(uint32_t value, uint32_t amount)
{
	return value > amount ? value - amount : 0;
}

int sl_rect_contains(const struct sl_rect *rect, int32_t x, int32_t y)
{
	return x >= rect->x && y >= rect->y && x < right_of(rect) && y < bottom_of(rect);
}

struct sl_rect sl_rect_union(const struct sl_rect *a, const struct sl_rect *b)
{
	struct sl_rect result;
	int64_t x = min64(a->x, b->x);
	int64_t y = min64(a->y, b->y);

	result.x = (int32_t)x;
	result.y = (int32_t)y;
	result.w = (uint32_t)(max64(right_of(a), right_of(b)) - x);
	result.h = (uint32_t)(max64(bottom_of(a), bottom_of(b)) - y);
	return result;
}

int64_t sl_rect_overlap_area(const struct sl_rect *a, const struct sl_rect *b)
{
	int64_t width = min64(right_of(a), right_of(b)) - max64(a->x, b->x);
	int64_t height = min64(bottom_of(a), bottom_of(b)) - max64(a->y, b->y);

	if (width <= 0 || height <= 0)
		return 0;
	return width * height;
}

struct sl_rect sl_rect_work_area(const struct sl_rect *monitor, const struct sl_rect *bounds,
				 const struct sl_strut *strut)
{
	struct sl_rect area = *monitor;

	if (monitor->x == bounds->x) {
		area.x += (int32_t)strut->left;
		area.w = saturating_sub(area.w, strut->left);
	}
	if (right_of(monitor) == right_of(bounds))
		area.w = saturating_sub(area.w, strut->right);
	if (monitor->y == bounds->y) {
		area.y += (int32_t)strut->top;
		area.h = saturating_sub(area.h, strut->top);
	}
	if (bottom_of(monitor) == bottom_of(bounds))
		area.h = saturating_sub(area.h, strut->bottom);
	return area;
}

size_t sl_monitor_index(const struct sl_rect *monitors, size_t count, const struct sl_rect *rect)
{
	size_t best = 0;
	int64_t best_area = -1;
	size_t index;

	for (index = 0; index < count; index++) {
		int64_t area = sl_rect_overlap_area(&monitors[index], rect);

		if (area >= best_area) {
			best_area = area;
			best = index;
		}
	}
	return best;
}
