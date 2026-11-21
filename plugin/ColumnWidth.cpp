#include "ColumnWidth.hpp"

#include <algorithm>

constexpr float MIN_COLUMN_WIDTH = 0.05F;
constexpr float MAX_COLUMN_WIDTH = 1.F;

float computePeekColumnWidth(const SColumnWidthRequest& request) {
    const double USABLE_SIZE = request.viewportSize - request.outerGaps;
    if (USABLE_SIZE <= 0)
        return MAX_COLUMN_WIDTH;

    const double WIDTH = request.viewportSize - request.innerGaps - 2 * request.peek;
    return std::clamp(static_cast<float>(WIDTH / USABLE_SIZE), MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
}
