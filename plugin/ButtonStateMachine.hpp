#pragma once

#include "ButtonLayout.hpp"

#include <optional>

struct SButtonState {
    std::optional<eBarButton> hoveredButton;
    std::optional<eBarButton> pressedButton;
    bool                      isOverGroup = false;

    bool                      operator==(const SButtonState&) const = default;
};

struct SButtonStateUpdate {
    bool                      isStateChanged = false;
    std::optional<eBarButton> firedButton;

    bool                      operator==(const SButtonStateUpdate&) const = default;
};

class CButtonStateMachine {
  public:
    SButtonStateUpdate onPointerMove(const SBarLayout& layout, double x, double y);
    SButtonStateUpdate onPointerLeave();
    SButtonStateUpdate onPress();
    SButtonStateUpdate onRelease();

    SButtonState       state() const;
    bool               hasArmedButton() const;

  private:
    std::optional<eBarButton> m_buttonAtPointer;
    std::optional<eBarButton> m_armedButton;
    bool                      m_isOverGroup = false;
};
