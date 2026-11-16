// SPDX-License-Identifier: GPL-3.0-only

#include <stdlib.h>

#include <xcb/shape.h>
#include <xcb/xcb.h>

#include "slate_x11.h"

#define SL_SHAPE_CORNER_ALPHA 128
#define SL_COORDINATE_MAX 32767
#define SL_SIZE_MAX 65535

_Static_assert(sizeof(struct sl_rectangle) == sizeof(xcb_rectangle_t), "same layout as the protocol rectangle");

size_t sl_shape_rectangles(const uint8_t *argb, size_t length, int32_t width, int32_t height, int32_t top,
			   int32_t radius, struct sl_rectangle *out, size_t capacity)
{
	int32_t corner_rows = radius + 1 < top ? radius + 1 : top;
	struct sl_span *spans;
	size_t span_count;
	size_t count = 0;
	size_t index;

	if (width <= 0 || height <= 0 || width > SL_SIZE_MAX || height > SL_SIZE_MAX)
		return 0;
	if (corner_rows < 0)
		corner_rows = 0;
	if (corner_rows > height)
		corner_rows = height;
	if (capacity < (size_t)corner_rows + 1)
		return 0;

	spans = corner_rows > 0 ? malloc((size_t)corner_rows * sizeof(*spans)) : NULL;
	if (corner_rows > 0 && spans == NULL)
		return 0;
	span_count = corner_rows > 0
			     ? sl_shape_spans(argb, length, width, corner_rows, SL_SHAPE_CORNER_ALPHA, spans)
			     : 0;

	for (index = 0; index < span_count; index++) {
		out[count].x = (int16_t)spans[index].first;
		out[count].y = (int16_t)spans[index].row;
		out[count].width = (uint16_t)(spans[index].last - spans[index].first + 1);
		out[count].height = 1;
		count++;
	}
	free(spans);

	if (height > corner_rows && corner_rows <= SL_COORDINATE_MAX) {
		out[count].x = 0;
		out[count].y = (int16_t)corner_rows;
		out[count].width = (uint16_t)width;
		out[count].height = (uint16_t)(height - corner_rows);
		count++;
	}
	return count;
}

int sl_shape_clear(void *connection, uint32_t frame)
{
	xcb_connection_t *xcb = connection;

	if (xcb == NULL)
		return -1;
	xcb_shape_mask(xcb, XCB_SHAPE_SO_SET, XCB_SHAPE_SK_BOUNDING, frame, 0, 0, XCB_NONE);
	return xcb_connection_has_error(xcb) ? -1 : 0;
}

int sl_shape_rounded(void *connection, uint32_t frame, const uint8_t *argb, size_t length, int32_t width,
		     int32_t height, int32_t top, int32_t radius)
{
	xcb_connection_t *xcb = connection;
	struct sl_rectangle *rectangles;
	size_t capacity;
	size_t count;

	if (xcb == NULL || radius <= 0 || top < 0 || height <= 0)
		return -1;
	capacity = (size_t)(radius + 1 < top ? radius + 1 : top) + 1;
	rectangles = malloc(capacity * sizeof(*rectangles));
	if (rectangles == NULL)
		return -1;

	count = sl_shape_rectangles(argb, length, width, height, top, radius, rectangles, capacity);
	if (count == 0) {
		free(rectangles);
		return -1;
	}
	xcb_shape_rectangles(xcb, XCB_SHAPE_SO_SET, XCB_SHAPE_SK_BOUNDING, XCB_CLIP_ORDERING_UNSORTED, frame, 0, 0,
			     (uint32_t)count, (const xcb_rectangle_t *)rectangles);
	free(rectangles);
	return xcb_connection_has_error(xcb) ? -1 : 0;
}
