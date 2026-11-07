#pragma once

#define WLR_USE_UNSTABLE

#include <hyprland/src/render/decorations/IHyprWindowDecoration.hpp>
#include <hyprland/src/render/Texture.hpp>
#include <hyprland/src/devices/IPointer.hpp>
#include <hyprland/src/helpers/time/Time.hpp>
#include <hyprland/src/helpers/signal/Signal.hpp>
#include <hyprland/src/helpers/AnimatedVariable.hpp>
#include "globals.hpp"
#include "ButtonLayout.hpp"
#include "ButtonStateMachine.hpp"

#include <optional>

namespace Event {
    struct SCallbackInfo;
}

namespace Layout {
    class ITarget;
}

struct SButtonAnimation {
    PHLANIMVAR<CHyprColor> discColor;
    PHLANIMVAR<float>      iconAlpha;
    PHLANIMVAR<float>      stateLayerAlpha;
};

class CTitleBarDecoration : public IHyprWindowDecoration {
  public:
    CTitleBarDecoration(PHLWINDOW window);
    virtual ~CTitleBarDecoration();

    virtual SDecorationPositioningInfo getPositioningInfo();
    virtual void                       onPositioningReply(const SDecorationPositioningReply& reply);
    virtual void                       draw(Render::CRenderContext& ctx, PHLMONITOR monitor, const float& alpha, const Render::SWindowRenderPresentation& presentation);
    virtual eDecorationType            getDecorationType();
    virtual void                       updateWindow(PHLWINDOW);
    virtual void                       damageEntire();
    virtual eDecorationLayer           getDecorationLayer();
    virtual uint64_t                   getDecorationFlags();
    virtual std::string                getDisplayName();

    PHLWINDOW                          window();
    bool                               isBarVisible();
    void                               updateRules();
    void                               onConfigReloaded();
    void                               onMouseButton(Event::SCallbackInfo& info, IPointer::SButtonEvent event);
    void                               dropStalePress(uint32_t mouseButton);
    void                               updatePointer();
    void                               updateFocus();

    WP<CTitleBarDecoration>            m_self;

  private:
    PHLWINDOWREF                   m_window;
    CBox                           m_assignedBox;
    SP<Render::ITexture>           m_titleTexture;
    SP<Render::ITexture>           m_inactiveTitleTexture;
    std::string                    m_lastTitle;
    int                            m_lastScaledTitleSize = 0;
    int                            m_lastHeight          = 0;
    bool                           m_isHidden            = false;
    bool                           m_isSizeChanged       = false;
    bool                           m_isWindowFocused     = false;

    bool                           m_isDragPending      = false;
    bool                           m_isDragging         = false;
    bool                           m_isTiledDragAligned = false;
    Vector2D                       m_freeAreaPressPosition;
    Vector2D                       m_freeAreaGrabOffset;
    double                         m_freeAreaBarWidth = 0;
    std::vector<uint32_t>          m_consumedMouseButtons;
    CButtonStateMachine            m_buttonStateMachine;

    std::optional<Time::steady_tp> m_lastFreeAreaPress;

    CHyprSignalListener            m_mouseMoveListener;
    CHyprSignalListener            m_dragMotionListener;

    PHLANIMVAR<CHyprColor>         m_barColor;
    PHLANIMVAR<float>              m_titleFocusAlpha;
    SButtonAnimation               m_buttonAnimations[BAR_BUTTON_COUNT];

    void                           syncAnimations(bool isInstant);

    bool                           shouldHide();
    bool                           isBarInteractive();
    bool                           inputIsValid();
    bool                           isPointerOnBar();
    Vector2D                       cursorRelativeToBar();
    CBox                           assignedBoxGlobal();
    CBox                           computePaintedBox();

    SBarLayout                     computeLayout();
    void                           applyUpdate(const SButtonStateUpdate& update);
    void                           endDrag();
    Vector2D                       placeUnderPointer(const SP<Layout::ITarget>& target);
    void                           alignTiledDrag();

    void                           onMouseMove();
    void                           handlePress(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event);
    void                           handleRelease(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event);
    void                           toggleMaximized();
    bool                           isMaximized();
    void                           fireButton(eBarButton button);

    void                           renderPass(Render::CRenderContext& ctx, PHLMONITOR monitor, const float& alpha);
    void                           renderTitle(Render::CRenderContext& ctx, const CBox& barBox, const Vector2D& bufferSize, float scale, float alpha);
    void renderTitleTexture(Render::CRenderContext& ctx, SP<Render::ITexture>& texture, Config::INTEGER color, const CBox& barBox, const Vector2D& bufferSize, float scale,
                            float alpha);
    void renderButtons(Render::CRenderContext& ctx, const CBox& barBox, float scale, float alpha);

    template <typename T>
    void createAnimation(PHLANIMVAR<T>& variable, const T& value);

    friend class CTitleBarPassElement;
};
