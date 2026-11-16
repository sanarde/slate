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

#define SL_EDGE_LEFT 1u
#define SL_EDGE_RIGHT 2u
#define SL_EDGE_TOP 4u
#define SL_EDGE_BOTTOM 8u

struct sl_insets {
	int32_t left;
	int32_t right;
	int32_t top;
	int32_t bottom;
};

struct sl_insets sl_frame_insets(int maximized, int fullscreen, int32_t border, int32_t titlebar);
uint32_t sl_frame_edges(int32_t width, int32_t height, int32_t x, int32_t y, int32_t thickness,
			int32_t top_zone, int32_t corner_span);
uint32_t sl_edge_cursor_glyph(uint32_t edges);
void sl_configure_notify(uint8_t *out, uint32_t window, int32_t x, int32_t y, uint32_t width,
			 uint32_t height, uint32_t border_width);

struct sl_span {
	int32_t row;
	int32_t first;
	int32_t last;
};

void sl_composite_over(const uint8_t *argb, size_t pixels, uint32_t background, uint8_t *out);
size_t sl_shape_spans(const uint8_t *argb, size_t length, int32_t width, int32_t rows, uint8_t threshold,
		      struct sl_span *spans);

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
