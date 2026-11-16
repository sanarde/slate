// SPDX-License-Identifier: GPL-3.0-only

#include <xcb/xcb.h>

#include "slate_x11.h"

#define SL_REQUEST_MARGIN 64

size_t sl_rows_per_request(size_t max_request_bytes, size_t row_bytes, size_t height)
{
	size_t budget;
	size_t rows;

	if (row_bytes == 0 || height == 0)
		return 0;
	budget = max_request_bytes > SL_REQUEST_MARGIN ? max_request_bytes - SL_REQUEST_MARGIN : 0;
	if (budget < row_bytes)
		budget = row_bytes;
	rows = budget / row_bytes;
	if (rows < 1)
		rows = 1;
	return rows < height ? rows : height;
}

int sl_put_image_rows(void *connection, uint32_t drawable, uint32_t gc, uint16_t width, uint16_t height,
		      uint8_t depth, size_t max_request_bytes, const uint8_t *data)
{
	xcb_connection_t *xcb = connection;
	size_t row_bytes = (size_t)width * 4;
	size_t rows_per_request = sl_rows_per_request(max_request_bytes, row_bytes, height);
	size_t row = 0;

	if (xcb == NULL || data == NULL || rows_per_request == 0)
		return -1;

	while (row < height) {
		size_t rows = rows_per_request < (size_t)height - row ? rows_per_request : (size_t)height - row;

		xcb_put_image(xcb, XCB_IMAGE_FORMAT_Z_PIXMAP, drawable, gc, width, (uint16_t)rows, 0, (int16_t)row, 0,
			      depth, (uint32_t)(rows * row_bytes), data + row * row_bytes);
		row += rows;
	}
	return xcb_connection_has_error(xcb) ? -1 : 0;
}
