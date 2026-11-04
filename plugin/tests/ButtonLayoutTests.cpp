#include "../ButtonLayout.hpp"
#include "Expect.hpp"

#include <algorithm>
#include <format>
#include <vector>

constexpr double       DISC_SIZE = 16.0;

static SButtonGeometry makeGeometry(double discX, double discY, double discSize, double hitX, double hitWidth, double barHeight) {
    return {.discBox = {discX, discY, discSize, discSize}, .hitBox = {hitX, 0, hitWidth, barHeight}};
}

static void testExactLayouts() {
    struct SCase {
        const char* name;
        double      width;
        double      height;
        eButtonSide side;
        double      discSize;
        SBarLayout  expected;
    };

    SBarLayout rightLayout;
    rightLayout.buttons[BAR_BUTTON_CLOSE]    = makeGeometry(268, 12, 16, 264, 36, 40);
    rightLayout.buttons[BAR_BUTTON_MAXIMIZE] = makeGeometry(244, 12, 16, 240, 24, 40);
    rightLayout.buttons[BAR_BUTTON_MINIMIZE] = makeGeometry(220, 12, 16, 216, 24, 40);
    rightLayout.groupBox                     = {216, 0, 84, 40};

    SBarLayout leftLayout;
    leftLayout.buttons[BAR_BUTTON_CLOSE]    = makeGeometry(16, 12, 16, 0, 36, 40);
    leftLayout.buttons[BAR_BUTTON_MINIMIZE] = makeGeometry(40, 12, 16, 36, 24, 40);
    leftLayout.buttons[BAR_BUTTON_MAXIMIZE] = makeGeometry(64, 12, 16, 60, 24, 40);
    leftLayout.groupBox                     = {0, 0, 84, 40};

    SBarLayout shortBarLayout;
    shortBarLayout.buttons[BAR_BUTTON_CLOSE]    = makeGeometry(274, 0, 10, 270, 30, 10);
    shortBarLayout.buttons[BAR_BUTTON_MAXIMIZE] = makeGeometry(256, 0, 10, 252, 18, 10);
    shortBarLayout.buttons[BAR_BUTTON_MINIMIZE] = makeGeometry(238, 0, 10, 234, 18, 10);
    shortBarLayout.groupBox                     = {234, 0, 66, 10};

    SBarLayout closeOnlyRightLayout;
    closeOnlyRightLayout.buttons[BAR_BUTTON_CLOSE] = makeGeometry(8, 12, 16, 4, 36, 40);
    closeOnlyRightLayout.groupBox                  = {4, 0, 36, 40};

    SBarLayout twoButtonsLeftLayout;
    twoButtonsLeftLayout.buttons[BAR_BUTTON_CLOSE]    = makeGeometry(16, 12, 16, 0, 36, 40);
    twoButtonsLeftLayout.buttons[BAR_BUTTON_MINIMIZE] = makeGeometry(40, 12, 16, 36, 24, 40);
    twoButtonsLeftLayout.groupBox                     = {0, 0, 60, 40};

    SBarLayout allFitRightLayout;
    allFitRightLayout.buttons[BAR_BUTTON_CLOSE]    = makeGeometry(52, 12, 16, 48, 36, 40);
    allFitRightLayout.buttons[BAR_BUTTON_MAXIMIZE] = makeGeometry(28, 12, 16, 24, 24, 40);
    allFitRightLayout.buttons[BAR_BUTTON_MINIMIZE] = makeGeometry(4, 12, 16, 0, 24, 40);
    allFitRightLayout.groupBox                     = {0, 0, 84, 40};

    SBarLayout noRoomLayout;
    noRoomLayout.groupBox = {30, 0, 0, 40};

    const std::vector<SCase> CASES = {
        {"right side", 300, 40, BUTTON_SIDE_RIGHT, DISC_SIZE, rightLayout},
        {"left side", 300, 40, BUTTON_SIDE_LEFT, DISC_SIZE, leftLayout},
        {"disc taller than the bar", 300, 10, BUTTON_SIDE_RIGHT, DISC_SIZE, shortBarLayout},
        {"room for close only", 40, 40, BUTTON_SIDE_RIGHT, DISC_SIZE, closeOnlyRightLayout},
        {"room for two buttons", 60, 40, BUTTON_SIDE_LEFT, DISC_SIZE, twoButtonsLeftLayout},
        {"exact fit", 84, 40, BUTTON_SIDE_RIGHT, DISC_SIZE, allFitRightLayout},
        {"no room", 30, 40, BUTTON_SIDE_RIGHT, DISC_SIZE, noRoomLayout},
    };

    for (const auto& testCase : CASES)
        expect(computeBarLayout(testCase.width, testCase.height, testCase.side, testCase.discSize) == testCase.expected, std::format("exact layout: {}", testCase.name));
}

static void testHitBoxesCoverFullHeightWithNoGaps() {
    struct SCase {
        double      width;
        double      height;
        eButtonSide side;
        double      discSize;
    };

    const std::vector<SCase> CASES = {
        {300, 40, BUTTON_SIDE_RIGHT, 16}, {300, 40, BUTTON_SIDE_LEFT, 16}, {301, 33, BUTTON_SIDE_RIGHT, 15}, {301, 33, BUTTON_SIDE_LEFT, 15},
        {70, 24, BUTTON_SIDE_RIGHT, 12},  {70, 24, BUTTON_SIDE_LEFT, 12},  {45, 40, BUTTON_SIDE_RIGHT, 16},  {45, 40, BUTTON_SIDE_LEFT, 16},
    };

    for (const auto& testCase : CASES) {
        const auto LAYOUT = computeBarLayout(testCase.width, testCase.height, testCase.side, testCase.discSize);
        const auto LABEL  = std::format("{}x{} side {} disc {}", testCase.width, testCase.height, static_cast<int>(testCase.side), testCase.discSize);

        for (const auto& button : LAYOUT.buttons) {
            if (button.hitBox.w == 0)
                continue;

            expect(button.hitBox.y == 0 && button.hitBox.h == testCase.height, "hit box covers the full height: " + LABEL);
            expect(button.discBox.w == button.discBox.h, "disc box is square: " + LABEL);
            expect(button.discBox.x >= button.hitBox.x && button.discBox.x + button.discBox.w <= button.hitBox.x + button.hitBox.w, "disc sits inside its hit box: " + LABEL);
        }

        for (double x = 0; x < testCase.width; x += 0.25) {
            const auto HIT_COUNT = std::ranges::count_if(LAYOUT.buttons, [x](const auto& button) { return button.hitBox.containsPoint(x, 0); });
            expect(HIT_COUNT == (LAYOUT.groupBox.containsPoint(x, 0) ? 1 : 0), std::format("one hit box per group point at x={}: {}", x, LABEL));
        }
    }
}

static void testButtonOrder() {
    struct SCase {
        eButtonSide               side;
        std::array<eBarButton, 3> leftToRight;
    };

    const std::vector<SCase> CASES = {
        {BUTTON_SIDE_RIGHT, {BAR_BUTTON_MINIMIZE, BAR_BUTTON_MAXIMIZE, BAR_BUTTON_CLOSE}},
        {BUTTON_SIDE_LEFT, {BAR_BUTTON_CLOSE, BAR_BUTTON_MINIMIZE, BAR_BUTTON_MAXIMIZE}},
    };

    for (const auto& testCase : CASES) {
        const auto LAYOUT = computeBarLayout(300, 40, testCase.side, DISC_SIZE);

        for (size_t i = 0; i + 1 < testCase.leftToRight.size(); ++i) {
            const auto& CURRENT = LAYOUT.buttons[testCase.leftToRight[i]];
            const auto& NEXT    = LAYOUT.buttons[testCase.leftToRight[i + 1]];
            expect(CURRENT.discBox.x < NEXT.discBox.x, std::format("disc order at {} on side {}", i, static_cast<int>(testCase.side)));
            expect(CURRENT.hitBox.x + CURRENT.hitBox.w == NEXT.hitBox.x, std::format("hit boxes meet at {} on side {}", i, static_cast<int>(testCase.side)));
        }
    }
}

static void testFindButtonAt() {
    struct SCase {
        const char*               name;
        eButtonSide               side;
        double                    x;
        double                    y;
        std::optional<eBarButton> expected;
    };

    const std::vector<SCase> CASES = {
        {"right: close corner", BUTTON_SIDE_RIGHT, 299.5, 0, BAR_BUTTON_CLOSE},
        {"right: close inner edge", BUTTON_SIDE_RIGHT, 264, 39.5, BAR_BUTTON_CLOSE},
        {"right: maximize next to close", BUTTON_SIDE_RIGHT, 263.5, 20, BAR_BUTTON_MAXIMIZE},
        {"right: minimize inner edge", BUTTON_SIDE_RIGHT, 216, 20, BAR_BUTTON_MINIMIZE},
        {"right: free area next to the group", BUTTON_SIDE_RIGHT, 215.5, 20, std::nullopt},
        {"right: past the bar end", BUTTON_SIDE_RIGHT, 300, 20, std::nullopt},
        {"right: below the bar", BUTTON_SIDE_RIGHT, 280, 40, std::nullopt},
        {"right: above the bar", BUTTON_SIDE_RIGHT, 280, -1, std::nullopt},
        {"left: close corner", BUTTON_SIDE_LEFT, 0, 0, BAR_BUTTON_CLOSE},
        {"left: minimize next to close", BUTTON_SIDE_LEFT, 36, 20, BAR_BUTTON_MINIMIZE},
        {"left: maximize outer end", BUTTON_SIDE_LEFT, 83.5, 20, BAR_BUTTON_MAXIMIZE},
        {"left: free area next to the group", BUTTON_SIDE_LEFT, 84, 20, std::nullopt},
    };

    for (const auto& testCase : CASES)
        expect(computeBarLayout(300, 40, testCase.side, DISC_SIZE).findButtonAt(testCase.x, testCase.y) == testCase.expected, std::format("find button: {}", testCase.name));
}

int main() {
    testExactLayouts();
    testHitBoxesCoverFullHeightWithNoGaps();
    testButtonOrder();
    testFindButtonAt();

    return reportResult("ButtonLayoutTests");
}
