#include "DragGeometry.hpp"

#include <cmath>

SWindowPosition computeRestoredPosition(const SRestoreRequest& request) {
    const double GRAB_RATIO = request.grabX / request.barWidth;

    return {
        .x = std::round(request.pointerX - GRAB_RATIO * request.restoredWidth),
        .y = std::round(request.pointerY - request.grabY + request.barOffsetY),
    };
}
