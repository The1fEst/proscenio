#include "TitleBarDecoration.hpp"

#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/desktop/state/LayerState.hpp>
#include <hyprland/src/desktop/state/ViewHitTester.hpp>
#include <hyprland/src/desktop/view/window/Window.hpp>
#include <hyprland/src/desktop/view/window/WindowPresentation.hpp>
#include <hyprland/src/desktop/view/LayerSurface.hpp>
#include <hyprland/src/helpers/MiscFunctions.hpp>
#include <hyprland/src/keybinds/Manager.hpp>
#include <hyprland/src/managers/SeatManager.hpp>
#include <hyprland/src/managers/SessionLockManager.hpp>
#include <hyprland/src/managers/fullscreen/FullscreenController.hpp>
#include <hyprland/src/managers/input/InputManager.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/config/ConfigManager.hpp>
#include <hyprland/src/config/shared/actions/ConfigActions.hpp>
#include <hyprland/src/protocols/LayerShell.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/layout/LayoutManager.hpp>
#include <hyprland/src/layout/supplementary/DragController.hpp>
#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/state/MonitorState.hpp>

#include <linux/input-event-codes.h>

#include <algorithm>
#include <climits>
#include <cmath>

#include "DragGeometry.hpp"
#include "TitleBarPassElement.hpp"

using namespace Render::GL;

constexpr double  DISC_SIZE                = 16.0;
constexpr double  DRAG_THRESHOLD           = 4.0;
constexpr int     BAR_PRIORITY             = 10005;
constexpr auto    DOUBLE_CLICK_INTERVAL_MS = std::chrono::milliseconds(400);

static CHyprColor toColor(Config::INTEGER color) {
    return CHyprColor{sc<uint64_t>(color)};
}

CTitleBarDecoration::CTitleBarDecoration(PHLWINDOW window) : IHyprWindowDecoration(window), m_window(window) {
    if (const auto MONITOR = window->m_monitor.lock())
        MONITOR->m_scheduledRecalc = true;

    m_isHidden = shouldHide();

    m_mouseMoveListener = Event::bus()->m_events.input.mouse.move.listen([this](Vector2D, Event::SCallbackInfo&) { onMouseMove(); });
}

CTitleBarDecoration::~CTitleBarDecoration() {
    std::erase(g_pGlobalState->bars, m_self);
}

SDecorationPositioningInfo CTitleBarDecoration::getPositioningInfo() {
    SDecorationPositioningInfo info;
    info.policy         = m_isHidden ? DECORATION_POSITION_ABSOLUTE : DECORATION_POSITION_STICKY;
    info.edges          = DECORATION_EDGE_TOP;
    info.priority       = BAR_PRIORITY;
    info.reserved       = true;
    info.desiredExtents = {{0, sc<int>(m_isHidden ? 0 : g_pGlobalState->config.barHeight->value())}, {0, 0}};
    return info;
}

void CTitleBarDecoration::onPositioningReply(const SDecorationPositioningReply& reply) {
    if (reply.assignedGeometry.size() != m_assignedBox.size())
        m_isSizeChanged = true;

    m_assignedBox = reply.assignedGeometry;
}

std::string CTitleBarDecoration::getDisplayName() {
    return BAR_DISPLAY_NAME;
}

bool CTitleBarDecoration::shouldHide() {
    const auto WINDOW = m_window.lock();

    if (!WINDOW->isFloating() || WINDOW->backend().traits().overrideRedirect || !WINDOW->m_ruleApplicator->decorate().valueOrDefault())
        return true;

    const auto& PROPS = WINDOW->m_ruleApplicator->m_otherProps.props;
    const auto  IT    = PROPS.find(g_pGlobalState->noBarRuleIdx);
    return IT != PROPS.end() && truthy(IT->second->effect);
}

bool CTitleBarDecoration::isBarInteractive() {
    if (m_isHidden || !validMapped(m_window) || g_pSessionLockManager->isSessionLocked())
        return false;

    return Fullscreen::controller()->getFullscreenModes(m_window.lock()).internal != Fullscreen::FSMODE_FULLSCREEN;
}

bool CTitleBarDecoration::inputIsValid() {
    if (!isBarInteractive())
        return false;

    if (g_pSeatManager->m_seatGrab && !g_pSeatManager->m_seatGrab->accepts(m_window->wlSurface()->resource()))
        return false;

    const auto MOUSE   = g_pInputManager->getMouseCoordsInternal();
    const auto MONITOR = State::monitorState()->query().vec(MOUSE).run();

    if (!MONITOR)
        return false;

    Desktop::CViewHitTester hitTester{*Desktop::viewState()};

    if (hitTester.windowAt(MOUSE, Desktop::View::RESERVED_EXTENTS | Desktop::View::INPUT_EXTENTS | Desktop::View::ALLOW_FLOATING) != m_window)
        return false;

    PHLLS    layerSurface = nullptr;
    Vector2D surfaceCoords;

    hitTester.layerSurfaceAt(MOUSE, &MONITOR->m_layerSurfaceLayers[ZWLR_LAYER_SHELL_V1_LAYER_TOP], &surfaceCoords, &layerSurface);
    if (layerSurface)
        return false;

    hitTester.layerSurfaceAt(MOUSE, &MONITOR->m_layerSurfaceLayers[ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY], &surfaceCoords, &layerSurface);
    return !layerSurface;
}

bool CTitleBarDecoration::isPointerOnBar() {
    const auto BOX    = assignedBoxGlobal();
    const auto COORDS = cursorRelativeToBar();

    if (COORDS.x < 0 || COORDS.y < 0 || COORDS.x >= BOX.w || COORDS.y >= BOX.h)
        return false;

    if (g_layoutManager->dragController()->target() || g_pInputManager->hasHeldButtons())
        return false;

    if (std::ranges::any_of(g_pGlobalState->bars, [this](const auto& bar) { return bar && bar != m_self && std::ranges::contains(bar->m_consumedMouseButtons, BTN_LEFT); }))
        return false;

    return inputIsValid() && !m_window->hasPopupAt(g_pInputManager->getMouseCoordsInternal());
}

SBarLayout CTitleBarDecoration::computeLayout() {
    const auto SIDE = g_pGlobalState->config.buttonSide->value() == "left" ? BUTTON_SIDE_LEFT : BUTTON_SIDE_RIGHT;
    return computeBarLayout(m_assignedBox.w, m_assignedBox.h, SIDE, DISC_SIZE);
}

void CTitleBarDecoration::updatePointer() {
    if (!isPointerOnBar()) {
        applyUpdate(m_buttonStateMachine.onPointerLeave());
        return;
    }

    const auto COORDS = cursorRelativeToBar();
    applyUpdate(m_buttonStateMachine.onPointerMove(computeLayout(), COORDS.x, COORDS.y));
}

void CTitleBarDecoration::applyUpdate(const SButtonStateUpdate& update) {
    if (update.isStateChanged)
        damageEntire();

    if (update.firedButton)
        fireButton(*update.firedButton);
}

void CTitleBarDecoration::onMouseButton(Event::SCallbackInfo& info, IPointer::SButtonEvent event) {
    if (event.state == WL_POINTER_BUTTON_STATE_PRESSED)
        handlePress(info, event);
    else
        handleRelease(info, event);
}

void CTitleBarDecoration::onMouseMove() {
    updatePointer();

    if (!m_isDragPending)
        return;

    if (!isBarInteractive()) {
        m_isDragPending = false;
        return;
    }

    if (g_pInputManager->getMouseCoordsInternal().distance(m_freeAreaPressPosition) < DRAG_THRESHOLD)
        return;

    m_isDragPending = false;
    m_isDragging    = true;
    m_lastFreeAreaPress.reset();

    if (Fullscreen::controller()->isFullscreen(m_window.lock(), Fullscreen::FSMODE_MAXIMIZED))
        restoreUnderPointer();

    g_layoutManager->beginDragTarget(m_window->layoutTarget(), MBIND_MOVE);
}

void CTitleBarDecoration::restoreUnderPointer() {
    const auto   WINDOW       = m_window.lock();
    const auto   TARGET       = WINDOW->layoutTarget();
    const auto   MOUSE        = g_pInputManager->getMouseCoordsInternal();
    const double BAR_OFFSET_Y = -m_assignedBox.y;

    Fullscreen::controller()->setFullscreenMode(WINDOW, Fullscreen::FSMODE_NONE);

    const auto SIZE     = TARGET->position().size();
    const auto POSITION = computeRestoredPosition({.pointerX      = MOUSE.x,
                                                   .pointerY      = MOUSE.y,
                                                   .grabX         = m_freeAreaGrabOffset.x,
                                                   .grabY         = m_freeAreaGrabOffset.y,
                                                   .barWidth      = m_freeAreaBarWidth,
                                                   .barOffsetY    = BAR_OFFSET_Y,
                                                   .restoredWidth = SIZE.x});

    TARGET->setPositionGlobal(CBox{{POSITION.x, POSITION.y}, SIZE});
    TARGET->warpPositionSize();
}

void CTitleBarDecoration::dropStalePress(uint32_t mouseButton) {
    if (std::erase(m_consumedMouseButtons, mouseButton) == 0 || mouseButton != BTN_LEFT)
        return;

    m_lastFreeAreaPress.reset();
    endDrag();
    applyUpdate(m_buttonStateMachine.onPointerLeave());
    applyUpdate(m_buttonStateMachine.onRelease());
}

void CTitleBarDecoration::endDrag() {
    m_isDragPending = false;

    if (m_isDragging && validMapped(m_window) && g_layoutManager->dragController()->target() == m_window->layoutTarget())
        g_layoutManager->endDragTarget();

    m_isDragging = false;
}

void CTitleBarDecoration::handlePress(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event) {
    if (info.cancelled || g_pInputManager->getModsFromAllKBs() || !isPointerOnBar())
        return;

    info.cancelled = true;
    m_consumedMouseButtons.push_back(event.button);

    if (event.button == BTN_LEFT) {
        updatePointer();
        applyUpdate(m_buttonStateMachine.onPress());
    }

    const auto WINDOW = m_window.lock();

    if (Desktop::focusState()->window() != WINDOW)
        Desktop::focusState()->fullWindowFocus(WINDOW, Desktop::FOCUS_REASON_CLICK);

    Desktop::windowState()->raise(WINDOW);

    if (event.button != BTN_LEFT)
        return;

    if (m_buttonStateMachine.hasArmedButton()) {
        m_lastFreeAreaPress.reset();
        return;
    }

    const auto MOUSE = g_pInputManager->getMouseCoordsInternal();

    if (m_lastFreeAreaPress && Time::steadyNow() - *m_lastFreeAreaPress < DOUBLE_CLICK_INTERVAL_MS && MOUSE.distance(m_freeAreaPressPosition) < DRAG_THRESHOLD) {
        m_lastFreeAreaPress.reset();
        toggleMaximized();
        return;
    }

    m_lastFreeAreaPress     = Time::steadyNow();
    m_freeAreaPressPosition = MOUSE;
    m_freeAreaGrabOffset    = cursorRelativeToBar();
    m_freeAreaBarWidth      = assignedBoxGlobal().w;
    m_isDragPending         = true;
}

void CTitleBarDecoration::handleRelease(Event::SCallbackInfo& info, const IPointer::SButtonEvent& event) {
    const auto IT = std::ranges::find(m_consumedMouseButtons, event.button);
    if (IT == m_consumedMouseButtons.end())
        return;

    m_consumedMouseButtons.erase(IT);
    info.cancelled = true;

    if (event.button != BTN_LEFT)
        return;

    endDrag();
    updatePointer();
    applyUpdate(m_buttonStateMachine.onRelease());
}

void CTitleBarDecoration::toggleMaximized() {
    (void)Config::Actions::fullscreenWindow(Fullscreen::FSMODE_MAXIMIZED, true, m_window.lock());
}

void CTitleBarDecoration::fireButton(eBarButton button) {
    if (!validMapped(m_window))
        return;

    switch (button) {
        case BAR_BUTTON_CLOSE: (void)Config::Actions::closeWindow(m_window.lock()); break;
        case BAR_BUTTON_MINIMIZE: (void)Config::Actions::moveToWorkspace("special:minimized", true, m_window.lock()); break;
        case BAR_BUTTON_MAXIMIZE: toggleMaximized(); break;
    }
}

void CTitleBarDecoration::renderTitle(Render::CRenderContext& ctx, const CBox& barBox, const Vector2D& bufferSize, float scale, float alpha) {
    const int  SCALED_SIZE = std::round(g_pGlobalState->config.titleSize->value() * scale);
    const auto TITLE       = m_window->metadata().title();

    if (m_lastTitle != TITLE || m_isSizeChanged || !m_titleTexture || m_titleTexture->m_texID == 0 || m_lastScaledTitleSize != SCALED_SIZE) {
        m_lastTitle           = TITLE;
        m_lastScaledTitleSize = SCALED_SIZE;

        const double GROUP_WIDTH = computeLayout().groupBox.w * scale;
        const int    MAX_WIDTH   = std::clamp(sc<int>(bufferSize.x - 2 * GROUP_WIDTH), 0, INT_MAX);

        m_titleTexture = TITLE.empty() || MAX_WIDTH < 1 ?
            nullptr :
            g_pHyprRenderer->renderText(TITLE, toColor(g_pGlobalState->config.titleColor->value()), SCALED_SIZE, false, g_pGlobalState->config.titleFont->value(), MAX_WIDTH,
                                        g_pGlobalState->config.titleWeight->value().m_value);
    }

    if (!m_titleTexture)
        return;

    const CBox TITLE_BOX = {barBox.x + std::round((bufferSize.x - m_titleTexture->m_size.x) / 2.0), barBox.y + std::round((bufferSize.y - m_titleTexture->m_size.y) / 2.0),
                            m_titleTexture->m_size.x, m_titleTexture->m_size.y};

    g_pHyprOpenGL->renderTexture(ctx, m_titleTexture, TITLE_BOX, {.a = alpha});
}

void CTitleBarDecoration::renderButtons(Render::CRenderContext& ctx, const CBox& barBox, float scale, float alpha) {
    const auto                                                          LAYOUT = computeLayout();
    const std::array<SP<Config::Values::CColorValue>, BAR_BUTTON_COUNT> COLORS = {g_pGlobalState->config.closeColor, g_pGlobalState->config.minimizeColor,
                                                                                  g_pGlobalState->config.maximizeColor};

    for (size_t i = 0; i < LAYOUT.buttons.size(); ++i) {
        const auto& DISC_BOX = LAYOUT.buttons[i].discBox;
        if (DISC_BOX.w <= 0)
            continue;

        CHyprColor color = toColor(COLORS[i]->value());
        color.a *= alpha;

        const double SIZE = std::round(DISC_BOX.w * scale);
        const CBox   DISC = {std::round(barBox.x + DISC_BOX.x * scale), std::round(barBox.y + DISC_BOX.y * scale), SIZE, SIZE};

        g_pHyprOpenGL->renderRect(ctx, DISC, color, {.round = sc<int>(std::round(SIZE / 2.0)), .roundingPower = 2.F});
    }
}

void CTitleBarDecoration::draw(Render::CRenderContext& ctx, PHLMONITOR, const float& alpha, const Render::SWindowRenderPresentation&) {
    if (m_isHidden || !validMapped(m_window) || !m_window->m_ruleApplicator->decorate().valueOrDefault())
        return;

    Render::IHyprRenderer::addPassElement(ctx, makeUnique<CTitleBarPassElement>(CTitleBarPassElement::STitleBarData{this, alpha}));
}

void CTitleBarDecoration::renderPass(Render::CRenderContext& ctx, PHLMONITOR monitor, const float& alpha) {
    const auto WINDOW = m_window.lock();
    const auto HEIGHT = g_pGlobalState->config.barHeight->value();

    const auto WORKSPACE       = WINDOW->m_workspace;
    const auto WORKSPACEOFFSET = WORKSPACE && !(WINDOW->m_state & Desktop::View::WINDOW_STATE_PINNED) ? WORKSPACE->m_renderOffset->value() : Vector2D();
    const auto ROUNDING        = WINDOW->presentation().rounding();
    const auto SCALEDROUNDING  = ROUNDING > 0 ? ROUNDING * monitor->m_scale - 2 : 0;
    const auto DECOBOX         = assignedBoxGlobal();
    const auto BARBUFFER       = DECOBOX.size() * monitor->m_scale;

    CBox       titleBarBox = {DECOBOX.x - monitor->m_position.x, DECOBOX.y - monitor->m_position.y, DECOBOX.w, DECOBOX.h + ROUNDING * 3};
    titleBarBox.translate(WINDOW->presentation().floatingOffset()).scale(monitor->m_scale).round();

    if (titleBarBox.w < 1 || titleBarBox.h < 1)
        return;

    CBox windowBox = {WINDOW->position(Desktop::View::IGeometric::GEOMETRIC_CURRENT).x + WINDOW->presentation().floatingOffset().x - monitor->m_position.x + 1,
                      WINDOW->position(Desktop::View::IGeometric::GEOMETRIC_CURRENT).y + WINDOW->presentation().floatingOffset().y - monitor->m_position.y + 1,
                      WINDOW->size(Desktop::View::IGeometric::GEOMETRIC_CURRENT).x - 2, WINDOW->size(Desktop::View::IGeometric::GEOMETRIC_CURRENT).y - 2};

    if (ROUNDING && (windowBox.w < 1 || windowBox.h < 1))
        return;

    CHyprColor color = toColor(g_pGlobalState->config.barColor->value());
    color.a *= alpha;

    g_pHyprOpenGL->scissor(ctx, titleBarBox);

    if (ROUNDING) {
        glClearStencil(0);
        glClear(GL_STENCIL_BUFFER_BIT);

        g_pHyprOpenGL->setCapStatus(GL_STENCIL_TEST, true);

        glStencilFunc(GL_ALWAYS, 1, -1);
        glStencilOp(GL_KEEP, GL_KEEP, GL_REPLACE);

        glColorMask(GL_FALSE, GL_FALSE, GL_FALSE, GL_FALSE);

        windowBox.translate(WORKSPACEOFFSET).scale(monitor->m_scale).round();
        g_pHyprOpenGL->renderRect(ctx, windowBox, CHyprColor(0, 0, 0, 0),
                                  CHyprOpenGLImpl::SRectRenderData{.round = sc<int>(SCALEDROUNDING), .roundingPower = WINDOW->presentation().roundingPower()});
        glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);

        glStencilFunc(GL_NOTEQUAL, 1, -1);
        glStencilOp(GL_KEEP, GL_KEEP, GL_REPLACE);
    }

    g_pHyprOpenGL->renderRect(ctx, titleBarBox, color, CHyprOpenGLImpl::SRectRenderData{.round = sc<int>(SCALEDROUNDING), .roundingPower = WINDOW->presentation().roundingPower()});

    if (ROUNDING) {
        glClearStencil(0);
        glClear(GL_STENCIL_BUFFER_BIT);
        g_pHyprOpenGL->setCapStatus(GL_STENCIL_TEST, false);
        glStencilMask(-1);
        glStencilFunc(GL_ALWAYS, 1, 0xFF);
    }

    const CBox BARBOX = {titleBarBox.x, titleBarBox.y, BARBUFFER.x, BARBUFFER.y};

    renderTitle(ctx, BARBOX, BARBUFFER, monitor->m_scale, alpha);
    renderButtons(ctx, BARBOX, monitor->m_scale, alpha);

    g_pHyprOpenGL->scissor(ctx, nullptr);

    m_isSizeChanged = false;

    if (m_lastHeight != HEIGHT) {
        WINDOW->layoutTarget()->recalc();
        m_lastHeight = HEIGHT;
    }
}

eDecorationType CTitleBarDecoration::getDecorationType() {
    return DECORATION_CUSTOM;
}

void CTitleBarDecoration::updateWindow(PHLWINDOW) {
    damageEntire();
}

void CTitleBarDecoration::onConfigReloaded() {
    m_titleTexture = nullptr;

    g_pDecorationPositioner->repositionDeco(this);
    updatePointer();
    damageEntire();
}

void CTitleBarDecoration::damageEntire() {
    g_pHyprRenderer->damageBox(assignedBoxGlobal());
}

Vector2D CTitleBarDecoration::cursorRelativeToBar() {
    return g_pInputManager->getMouseCoordsInternal() - assignedBoxGlobal().pos();
}

eDecorationLayer CTitleBarDecoration::getDecorationLayer() {
    return DECORATION_LAYER_UNDER;
}

uint64_t CTitleBarDecoration::getDecorationFlags() {
    return DECORATION_ALLOWS_MOUSE_INPUT | DECORATION_PART_OF_MAIN_WINDOW;
}

CBox CTitleBarDecoration::assignedBoxGlobal() {
    if (!validMapped(m_window))
        return {};

    CBox box = m_assignedBox;
    box.translate(g_pDecorationPositioner->getEdgeDefinedPoint(DECORATION_EDGE_TOP, m_window.lock()));

    const auto WORKSPACE       = m_window->m_workspace;
    const auto WORKSPACEOFFSET = WORKSPACE && !(m_window->m_state & Desktop::View::WINDOW_STATE_PINNED) ? WORKSPACE->m_renderOffset->value() : Vector2D();

    return box.translate(WORKSPACEOFFSET);
}

PHLWINDOW CTitleBarDecoration::window() {
    return m_window.lock();
}

void CTitleBarDecoration::updateRules() {
    const bool IS_HIDDEN = shouldHide();
    if (IS_HIDDEN == m_isHidden)
        return;

    m_isHidden = IS_HIDDEN;
    g_pDecorationPositioner->repositionDeco(this);
    updatePointer();
}
