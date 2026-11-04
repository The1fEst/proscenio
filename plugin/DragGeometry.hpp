#pragma once

struct SWindowPosition {
    double x = 0;
    double y = 0;

    bool   operator==(const SWindowPosition&) const = default;
};

struct SRestoreRequest {
    double pointerX      = 0;
    double pointerY      = 0;
    double grabX         = 0;
    double grabY         = 0;
    double barWidth      = 0;
    double barOffsetY    = 0;
    double restoredWidth = 0;
};

SWindowPosition computeRestoredPosition(const SRestoreRequest& request);
