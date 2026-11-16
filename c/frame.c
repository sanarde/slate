// SPDX-License-Identifier: GPL-3.0-only

#include <string.h>

#include "slate_x11.h"

#define SL_CONFIGURE_NOTIFY 22
#define SL_EVENT_SIZE 32

struct sl_configure_notify {
	uint8_t response_type;
	uint8_t pad0;
	uint16_t sequence;
	uint32_t event;
	uint32_t window;
	uint32_t above_sibling;
	int16_t x;
	int16_t y;
	uint16_t width;
	uint16_t height;
	uint16_t border_width;
	uint8_t override_redirect;
	uint8_t pad1[5];
};

_Static_assert(sizeof(struct sl_configure_notify) == SL_EVENT_SIZE, "a core event is 32 bytes");

struct sl_insets sl_frame_insets(int maximized, int fullscreen, int32_t border, int32_t titlebar)
{
	struct sl_insets insets = { 0, 0, 0, 0 };

	if (fullscreen)
		return insets;
	if (maximized) {
		insets.top = titlebar;
		return insets;
	}
	insets.left = border;
	insets.right = border;
	insets.top = titlebar + border;
	insets.bottom = border;
	return insets;
}

uint32_t sl_frame_edges(int32_t width, int32_t height, int32_t x, int32_t y, int32_t thickness,
			int32_t top_zone, int32_t corner_span)
{
	int left = x < thickness;
	int right = x >= width - thickness;
	int top = y < top_zone;
	int bottom = y >= height - thickness;
	int vertical = left || right;
	int horizontal = top || bottom;
	uint32_t edges = 0;

	if (!vertical && !horizontal)
		return 0;
	if (vertical && !horizontal) {
		top = y < corner_span;
		bottom = y >= height - corner_span;
	} else if (horizontal && !vertical) {
		left = x < corner_span;
		right = x >= width - corner_span;
	}

	if (left)
		edges |= SL_EDGE_LEFT;
	if (right)
		edges |= SL_EDGE_RIGHT;
	if (top)
		edges |= SL_EDGE_TOP;
	if (bottom)
		edges |= SL_EDGE_BOTTOM;
	return edges;
}

uint32_t sl_edge_cursor_glyph(uint32_t edges)
{
	int left = (edges & SL_EDGE_LEFT) != 0;
	int right = (edges & SL_EDGE_RIGHT) != 0;
	int top = (edges & SL_EDGE_TOP) != 0;
	int bottom = (edges & SL_EDGE_BOTTOM) != 0;

	if (left && top)
		return 134;
	if (right && top)
		return 136;
	if (left && bottom)
		return 12;
	if (right && bottom)
		return 14;
	if (left)
		return 70;
	if (right)
		return 96;
	if (top)
		return 138;
	return 16;
}

void sl_configure_notify(uint8_t *out, uint32_t window, int32_t x, int32_t y, uint32_t width,
			 uint32_t height, uint32_t border_width)
{
	struct sl_configure_notify event;

	memset(&event, 0, sizeof(event));
	event.response_type = SL_CONFIGURE_NOTIFY;
	event.event = window;
	event.window = window;
	event.x = (int16_t)x;
	event.y = (int16_t)y;
	event.width = (uint16_t)width;
	event.height = (uint16_t)height;
	event.border_width = (uint16_t)border_width;
	memcpy(out, &event, sizeof(event));
}
