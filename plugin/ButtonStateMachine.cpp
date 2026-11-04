#include "ButtonStateMachine.hpp"

SButtonStateUpdate CButtonStateMachine::onPointerMove(const SBarLayout& layout, double x, double y) {
    const auto PREVIOUS_STATE = state();

    m_buttonAtPointer = layout.findButtonAt(x, y);
    m_isOverGroup     = layout.groupBox.containsPoint(x, y);

    return {.isStateChanged = state() != PREVIOUS_STATE};
}

SButtonStateUpdate CButtonStateMachine::onPointerLeave() {
    const auto PREVIOUS_STATE = state();

    m_buttonAtPointer.reset();
    m_isOverGroup = false;

    return {.isStateChanged = state() != PREVIOUS_STATE};
}

SButtonStateUpdate CButtonStateMachine::onPress() {
    const auto PREVIOUS_STATE = state();

    m_armedButton = m_buttonAtPointer;

    return {.isStateChanged = state() != PREVIOUS_STATE};
}

SButtonStateUpdate CButtonStateMachine::onRelease() {
    const auto PREVIOUS_STATE = state();
    const auto FIRED_BUTTON   = m_armedButton == m_buttonAtPointer ? m_armedButton : std::nullopt;

    m_armedButton.reset();

    return {.isStateChanged = state() != PREVIOUS_STATE, .firedButton = FIRED_BUTTON};
}

SButtonState CButtonStateMachine::state() const {
    if (!m_armedButton)
        return {.hoveredButton = m_buttonAtPointer, .pressedButton = std::nullopt, .isOverGroup = m_isOverGroup};

    const auto ACTIVE_BUTTON = m_buttonAtPointer == m_armedButton ? m_armedButton : std::nullopt;
    return {.hoveredButton = ACTIVE_BUTTON, .pressedButton = ACTIVE_BUTTON, .isOverGroup = m_isOverGroup};
}

bool CButtonStateMachine::hasArmedButton() const {
    return m_armedButton.has_value();
}
