#include "../ButtonStateMachine.hpp"
#include "Expect.hpp"

#include <format>
#include <vector>

enum eInput : uint8_t {
    INPUT_MOVE = 0,
    INPUT_LEAVE,
    INPUT_PRESS,
    INPUT_RELEASE,
};

struct SPoint {
    double x = 0;
    double y = 0;
};

struct SStep {
    eInput             input;
    SPoint             point;
    SButtonState       expectedState;
    SButtonStateUpdate expectedUpdate;
};

struct SScenario {
    const char*        name;
    eButtonSide        side;
    std::vector<SStep> steps;
};

constexpr SPoint       RIGHT_CLOSE    = {280, 20};
constexpr SPoint       RIGHT_MAXIMIZE = {250, 20};
constexpr SPoint       RIGHT_MINIMIZE = {228, 20};
constexpr SPoint       RIGHT_FREE     = {100, 20};
constexpr SPoint       LEFT_CLOSE     = {20, 20};
constexpr SPoint       LEFT_MINIMIZE  = {48, 20};
constexpr SPoint       LEFT_MAXIMIZE  = {72, 20};
constexpr SPoint       LEFT_FREE      = {200, 20};

constexpr SButtonState IDLE_STATE           = {};
constexpr SButtonState ARMED_IN_GROUP_STATE = {.hoveredButton = std::nullopt, .pressedButton = std::nullopt, .isOverGroup = true};

static SStep           makeMove(SPoint point, SButtonState expectedState, bool isStateChanged) {
    return {INPUT_MOVE, point, expectedState, {.isStateChanged = isStateChanged}};
}

static SStep makeLeave(SButtonState expectedState, bool isStateChanged) {
    return {INPUT_LEAVE, {}, expectedState, {.isStateChanged = isStateChanged}};
}

static SStep makePress(SButtonState expectedState, bool isStateChanged) {
    return {INPUT_PRESS, {}, expectedState, {.isStateChanged = isStateChanged}};
}

static SStep makeRelease(SButtonState expectedState, bool isStateChanged, std::optional<eBarButton> firedButton = std::nullopt) {
    return {INPUT_RELEASE, {}, expectedState, {.isStateChanged = isStateChanged, .firedButton = firedButton}};
}

static SButtonState makeHoverState(eBarButton button) {
    return {.hoveredButton = button, .pressedButton = std::nullopt, .isOverGroup = true};
}

static SButtonState makePressState(eBarButton button) {
    return {.hoveredButton = button, .pressedButton = button, .isOverGroup = true};
}

static SScenario makeClickScenario(const char* name, eButtonSide side, SPoint point, eBarButton button) {
    return {name, side, {makeMove(point, makeHoverState(button), true), makePress(makePressState(button), true), makeRelease(makeHoverState(button), true, button)}};
}

static SButtonStateUpdate applyStep(CButtonStateMachine& machine, const SBarLayout& layout, const SStep& step) {
    switch (step.input) {
        case INPUT_MOVE: return machine.onPointerMove(layout, step.point.x, step.point.y);
        case INPUT_LEAVE: return machine.onPointerLeave();
        case INPUT_PRESS: return machine.onPress();
        case INPUT_RELEASE: return machine.onRelease();
    }

    return {};
}

int main() {
    const std::vector<SScenario> SCENARIOS = {
        makeClickScenario("click fires close on the right", BUTTON_SIDE_RIGHT, RIGHT_CLOSE, BAR_BUTTON_CLOSE),
        makeClickScenario("click fires maximize on the right", BUTTON_SIDE_RIGHT, RIGHT_MAXIMIZE, BAR_BUTTON_MAXIMIZE),
        makeClickScenario("click fires minimize on the right", BUTTON_SIDE_RIGHT, RIGHT_MINIMIZE, BAR_BUTTON_MINIMIZE),
        makeClickScenario("click fires close on the left", BUTTON_SIDE_LEFT, LEFT_CLOSE, BAR_BUTTON_CLOSE),
        makeClickScenario("click fires minimize on the left", BUTTON_SIDE_LEFT, LEFT_MINIMIZE, BAR_BUTTON_MINIMIZE),
        makeClickScenario("click fires maximize on the left", BUTTON_SIDE_LEFT, LEFT_MAXIMIZE, BAR_BUTTON_MAXIMIZE),
        {"hover follows the pointer and only reports real changes",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_FREE, IDLE_STATE, false),
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makeMove({282, 22}, makeHoverState(BAR_BUTTON_CLOSE), false),
             makeMove(RIGHT_MAXIMIZE, makeHoverState(BAR_BUTTON_MAXIMIZE), true),
             makeMove(RIGHT_FREE, IDLE_STATE, true),
             makeMove(RIGHT_MINIMIZE, makeHoverState(BAR_BUTTON_MINIMIZE), true),
             makeLeave(IDLE_STATE, true),
             makeLeave(IDLE_STATE, false),
         }},
        {"release on the free area cancels",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makePress(makePressState(BAR_BUTTON_CLOSE), true),
             makeMove(RIGHT_FREE, IDLE_STATE, true),
             makeRelease(IDLE_STATE, false),
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
         }},
        {"release on another button cancels and hovers that button",
         BUTTON_SIDE_LEFT,
         {
             makeMove(LEFT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makePress(makePressState(BAR_BUTTON_CLOSE), true),
             makeMove(LEFT_MAXIMIZE, ARMED_IN_GROUP_STATE, true),
             makeRelease(makeHoverState(BAR_BUTTON_MAXIMIZE), true),
         }},
        {"release off the bar cancels",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_MINIMIZE, makeHoverState(BAR_BUTTON_MINIMIZE), true),
             makePress(makePressState(BAR_BUTTON_MINIMIZE), true),
             makeLeave(IDLE_STATE, true),
             makeRelease(IDLE_STATE, false),
             makePress(IDLE_STATE, false),
             makeRelease(IDLE_STATE, false),
         }},
        {"leaving and coming back while armed restores the press and fires",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_MAXIMIZE, makeHoverState(BAR_BUTTON_MAXIMIZE), true),
             makePress(makePressState(BAR_BUTTON_MAXIMIZE), true),
             makeLeave(IDLE_STATE, true),
             makeMove(RIGHT_MAXIMIZE, makePressState(BAR_BUTTON_MAXIMIZE), true),
             makeMove(RIGHT_FREE, IDLE_STATE, true),
             makeMove(RIGHT_MAXIMIZE, makePressState(BAR_BUTTON_MAXIMIZE), true),
             makeRelease(makeHoverState(BAR_BUTTON_MAXIMIZE), true, BAR_BUTTON_MAXIMIZE),
         }},
        {"other buttons stay inert while one is armed",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makePress(makePressState(BAR_BUTTON_CLOSE), true),
             makeMove(RIGHT_MAXIMIZE, ARMED_IN_GROUP_STATE, true),
             makeMove(RIGHT_MINIMIZE, ARMED_IN_GROUP_STATE, false),
             makeMove(RIGHT_CLOSE, makePressState(BAR_BUTTON_CLOSE), true),
         }},
        {"a press on the free area never arms",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_FREE, IDLE_STATE, false),
             makePress(IDLE_STATE, false),
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makeRelease(makeHoverState(BAR_BUTTON_CLOSE), false),
         }},
        {"a press off the bar never arms",
         BUTTON_SIDE_LEFT,
         {
             makeLeave(IDLE_STATE, false),
             makePress(IDLE_STATE, false),
             makeMove(LEFT_FREE, IDLE_STATE, false),
             makeMove(LEFT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makeRelease(makeHoverState(BAR_BUTTON_CLOSE), false),
         }},
        {"a release with no press does nothing",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makeRelease(makeHoverState(BAR_BUTTON_CLOSE), false),
         }},
        {"a second press re-arms at the pointer",
         BUTTON_SIDE_RIGHT,
         {
             makeMove(RIGHT_CLOSE, makeHoverState(BAR_BUTTON_CLOSE), true),
             makePress(makePressState(BAR_BUTTON_CLOSE), true),
             makeMove(RIGHT_MINIMIZE, ARMED_IN_GROUP_STATE, true),
             makePress(makePressState(BAR_BUTTON_MINIMIZE), true),
             makeRelease(makeHoverState(BAR_BUTTON_MINIMIZE), true, BAR_BUTTON_MINIMIZE),
         }},
    };

    for (const auto& scenario : SCENARIOS) {
        const auto          LAYOUT = computeBarLayout(300, 40, scenario.side, 16);
        CButtonStateMachine machine;

        for (size_t i = 0; i < scenario.steps.size(); ++i) {
            const auto& STEP   = scenario.steps[i];
            const auto  UPDATE = applyStep(machine, LAYOUT, STEP);

            expect(machine.state() == STEP.expectedState, std::format("{}: state after step {}", scenario.name, i));
            expect(UPDATE == STEP.expectedUpdate, std::format("{}: update from step {}", scenario.name, i));
        }
    }

    return reportResult("ButtonStateMachineTests");
}
