#pragma once

#define WLR_USE_UNSTABLE

#include <hyprland/src/render/decorations/IHyprWindowDecoration.hpp>
#include <hyprland/src/render/Texture.hpp>
#include <hyprland/src/devices/IPointer.hpp>
#include <hyprland/src/helpers/time/Time.hpp>
#include <hyprland/src/helpers/signal/Signal.hpp>
#include "globals.hpp"
#include "ButtonLayout.hpp"
#include "ButtonStateMachine.hpp"

#include <optional>

namespace Event {
    struct SCallbackInfo;
}

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
    void                               updateRules();
    void                               onConfigReloaded();
    void                               onMouseButton(Event::SCallbackInfo& info, IPointer::SButtonEvent event);
    void                               dropStalePress(uint32_t mouseButton);

    WP<CTitleBarDecoration>            m_self;

  private:
    PHLWINDOWREF                   m_window;
    CBox                           m_assignedBox;
    SP<Render::ITexture>           m_titleTexture;
    std::string                    m_lastTitle;
    int                            m_lastScaledTitleSize = 0;
    int                            m_lastHeight          = 0;
    bool                           m_isHidden            = false;
    bool                           m_isSizeChanged       = false;

    bool                           m_isDragPending = false;
    bool                           m_isDragging    = false;
    Vector2D                       m_freeAreaPressPosition;
    Vector2D                       m_freeAreaGrabOffset;
    double                         m_freeAreaBarWidth = 0;
    std::vector<uint32_t>          m_consumedMouseButtons;
    CButtonStateMachine            m_buttonStateMachine;

    std::optional<Time::steady_tp> m_lastFreeAreaPress;

    CHyprSignalListener            m_mouseMoveListener;

    bool                           shouldHide();
    bool                           isBarInteractive();
    bool                           inputIsValid();
    bool                           isPointerOnBar();
    Vector2D                       cursorRelativeToBar();
    CBox                           assignedBoxGlobal();

    SBarLayout                     computeLayout();
    void                           updatePointer();
    void                           applyUpdate(const SButtonStateUpdate& update);
    void                           endDrag();
    void                           restoreUnderPointer();

    void                           onMouseMove();
    void                           handlePress(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event);
    void                           handleRelease(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event);
    void                           toggleMaximized();
    void                           fireButton(eBarButton button);

    void                           renderPass(Render::CRenderContext& ctx, PHLMONITOR monitor, const float& alpha);
    void                           renderTitle(Render::CRenderContext& ctx, const CBox& barBox, const Vector2D& bufferSize, float scale, float alpha);
    void                           renderButtons(Render::CRenderContext& ctx, const CBox& barBox, float scale, float alpha);

    friend class CTitleBarPassElement;
};
