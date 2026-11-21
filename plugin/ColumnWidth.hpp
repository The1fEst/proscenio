#pragma once

struct SColumnWidthRequest {
    double viewportSize = 0;
    double outerGaps    = 0;
    double innerGaps    = 0;
    double peek         = 0;
};

float computePeekColumnWidth(const SColumnWidthRequest& request);
