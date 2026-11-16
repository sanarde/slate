// SPDX-License-Identifier: GPL-3.0-only

#include <math.h>

#include "slate_x11.h"

#define SL_CHANNEL_MAX 255

void sl_composite_over(const uint8_t *argb, size_t pixels, uint32_t background, uint8_t *out)
{
	float red = (float)((background >> 16) & 0xff);
	float green = (float)((background >> 8) & 0xff);
	float blue = (float)(background & 0xff);
	size_t index;

	for (index = 0; index < pixels; index++) {
		const uint8_t *pixel = argb + index * 4;
		uint8_t *target = out + index * 4;
		float covered = 1.0f - (float)pixel[3] / 255.0f;
		float values[3];
		size_t channel;

		values[0] = roundf((float)pixel[0] + blue * covered);
		values[1] = roundf((float)pixel[1] + green * covered);
		values[2] = roundf((float)pixel[2] + red * covered);
		for (channel = 0; channel < 3; channel++)
			target[channel] = values[channel] > SL_CHANNEL_MAX ? SL_CHANNEL_MAX : (uint8_t)values[channel];
		target[3] = 0;
	}
}

size_t sl_shape_spans(const uint8_t *argb, size_t length, int32_t width, int32_t rows, uint8_t threshold,
		      struct sl_span *spans)
{
	size_t count = 0;
	int32_t row;

	if (width <= 0 || rows <= 0 || (size_t)width * (size_t)rows * 4 > length)
		return 0;

	for (row = 0; row < rows; row++) {
		const uint8_t *line = argb + (size_t)row * (size_t)width * 4;
		int32_t first = -1;
		int32_t last = -1;
		int32_t column;

		for (column = 0; column < width; column++) {
			if (line[column * 4 + 3] >= threshold) {
				if (first < 0)
					first = column;
				last = column;
			}
		}
		if (first < 0)
			continue;
		spans[count].row = row;
		spans[count].first = first;
		spans[count].last = last;
		count++;
	}
	return count;
}
