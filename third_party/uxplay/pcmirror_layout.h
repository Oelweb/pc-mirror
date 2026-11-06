// PC-Mirror display policy; AirPlay protocol and encoded frames stay unchanged.
#pragma once
#include <cstdint>
#include <cstring>

namespace pcmirror {
inline bool is_ipad(const char *model) {
    return model && std::strncmp(model, "iPad", 4) == 0;
}
struct Viewport { int x, y, width, height; };
inline Viewport viewport(int width, int height, bool fill, bool ipad,
                         std::int64_t sourceWidth, std::int64_t sourceHeight) {
    Viewport fit{0, 0, width, height};
    if (!fill || !ipad || width <= 0 || height <= 0 || sourceWidth <= 0 || sourceHeight <= 0
        || sourceWidth > 1000000000 || sourceHeight > 1000000000) return fit;
    const auto w = (std::int64_t(height) * sourceWidth + sourceHeight - 1) / sourceHeight;
    const auto h = (std::int64_t(width) * sourceHeight + sourceWidth - 1) / sourceWidth;
    if (w > 32767 || h > 32767) return fit;
    const int coveredWidth = w > width ? int(w) : width;
    const int coveredHeight = h > height ? int(h) : height;
    return {(width - coveredWidth) / 2, (height - coveredHeight) / 2, coveredWidth, coveredHeight};
}
}
