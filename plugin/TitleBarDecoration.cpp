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
#include <hyprland/src/animation/AnimationManager.hpp>

#include <linux/input-event-codes.h>

#include <algorithm>
#include <climits>
#include <cmath>

#include "ButtonAppearance.hpp"
#include "DragGeometry.hpp"
#include "IconRasterizer.hpp"
#include "TitleBarPassElement.hpp"

using namespace Render::GL;

constexpr double  DISC_SIZE                = 16.0;
constexpr double  DRAG_THRESHOLD           = 4.0;
constexpr int     BAR_PRIORITY             = 10005;
constexpr auto    DOUBLE_CLICK_INTERVAL_MS = std::chrono::milliseconds(400);

static CHyprColor toColor(Config::INTEGER color) {
    return CHyprColor{sc<uint64_t>(color)};
}

static Config::INTEGER selectRoleColor(eBarButton button) {
    const auto& CONFIG = g_pGlobalState->config;

    switch (button) {
        case BAR_BUTTON_CLOSE: return CONFIG.closeColor->value();
        case BAR_BUTTON_MINIMIZE: return CONFIG.minimizeColor->value();
        case BAR_BUTTON_MAXIMIZE: return CONFIG.maximizeColor->value();
    }

    return 0;
}

static Config::INTEGER selectIconColor(eBarButton button) {
    const auto& CONFIG = g_pGlobalState->config;

    switch (button) {
        case BAR_BUTTON_CLOSE: return CONFIG.closeIconColor->value();
        case BAR_BUTTON_MINIMIZE: return CONFIG.minimizeIconColor->value();
        case BAR_BUTTON_MAXIMIZE: return CONFIG.maximizeIconColor->value();
    }

    return 0;
}

CTitleBarDecoration::CTitleBarDecoration(PHLWINDOW window) : IHyprWindowDecoration(window), m_window(window) {
    if (const auto MONITOR = window->m_monitor.lock())
        MONITOR->m_scheduledRecalc = true;

    m_isHidden        = shouldHide();
    m_isWindowFocused = Desktop::focusState()->window() == window;

    createAnimation(m_barColor, CHyprColor{});
    createAnimation(m_titleFocusAlpha, 0.F);

    for (auto& animation : m_buttonAnimations) {
        createAnimation(animation.discColor, CHyprColor{});
        createAnimation(animation.iconAlpha, 0.F);
        createAnimation(animation.stateLayerAlpha, 0.F);
    }

    syncAnimations(true);

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
    const bool IS_FREE_AREA_PRESS_HELD = std::ranges::contains(m_consumedMouseButtons, BTN_LEFT) && !m_buttonStateMachine.hasArmedButton();

    if (IS_FREE_AREA_PRESS_HELD || !isPointerOnBar()) {
        applyUpdate(m_buttonStateMachine.onPointerLeave());
        return;
    }

    const auto COORDS = cursorRelativeToBar();
    applyUpdate(m_buttonStateMachine.onPointerMove(computeLayout(), COORDS.x, COORDS.y));
}

void CTitleBarDecoration::updateFocus() {
    const bool IS_FOCUSED = Desktop::focusState()->window() == m_window.lock();
    if (IS_FOCUSED == m_isWindowFocused)
        return;

    m_isWindowFocused = IS_FOCUSED;
    syncAnimations(false);
}

template <typename T>
void CTitleBarDecoration::createAnimation(PHLANIMVAR<T>& variable, const T& value) {
    Animation::mgr()->createAnimation(value, variable, g_pGlobalState->animationConfig, AVARDAMAGE_NONE);
    variable->setUpdateCallback([this](auto) { damageEntire(); });
}

void CTitleBarDecoration::syncAnimations(bool isInstant) {
    if (m_isHidden || m_window.expired())
        return;

    const auto setGoal = [isInstant = isInstant || m_window->m_ruleApplicator->noAnim().valueOrDefault()](auto& variable, const auto& goal) {
        if (isInstant)
            variable->setValueAndWarp(goal);
        else
            *variable = goal;
    };

    const auto& CONFIG = g_pGlobalState->config;

    setGoal(m_barColor, toColor(m_isWindowFocused ? CONFIG.barColor->value() : CONFIG.inactiveBarColor->value()));
    setGoal(m_titleFocusAlpha, m_isWindowFocused ? 1.F : 0.F);

    const auto STATE = m_buttonStateMachine.state();

    for (size_t i = 0; i < BAR_BUTTON_COUNT; ++i) {
        const auto BUTTON     = sc<eBarButton>(i);
        const auto APPEARANCE = computeButtonAppearance(BUTTON, STATE, m_isWindowFocused);
        auto&      animation  = m_buttonAnimations[i];

        setGoal(animation.discColor, toColor(APPEARANCE.usesRoleColor ? selectRoleColor(BUTTON) : CONFIG.inactiveButtonColor->value()));
        setGoal(animation.iconAlpha, APPEARANCE.iconAlpha);
        setGoal(animation.stateLayerAlpha, APPEARANCE.stateLayerAlpha);
    }
}

void CTitleBarDecoration::applyUpdate(const SButtonStateUpdate& update) {
    if (update.isStateChanged)
        syncAnimations(false);

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

    if (event.button == BTN_LEFT) {
        updatePointer();
        applyUpdate(m_buttonStateMachine.onPress());
    }

    m_consumedMouseButtons.push_back(event.button);

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

    if (m_lastTitle != TITLE || m_isSizeChanged || m_lastScaledTitleSize != SCALED_SIZE) {
        m_lastTitle            = TITLE;
        m_lastScaledTitleSize  = SCALED_SIZE;
        m_titleTexture         = nullptr;
        m_inactiveTitleTexture = nullptr;
    }

    const float FOCUS_ALPHA = m_titleFocusAlpha->value();

    if (FOCUS_ALPHA < 1)
        renderTitleTexture(ctx, m_inactiveTitleTexture, g_pGlobalState->config.inactiveTitleColor->value(), barBox, bufferSize, scale, alpha);

    if (FOCUS_ALPHA > 0)
        renderTitleTexture(ctx, m_titleTexture, g_pGlobalState->config.titleColor->value(), barBox, bufferSize, scale, FOCUS_ALPHA * alpha);
}

void CTitleBarDecoration::renderTitleTexture(Render::CRenderContext& ctx, SP<Render::ITexture>& texture, Config::INTEGER color, const CBox& barBox, const Vector2D& bufferSize,
                                             float scale, float alpha) {
    if (!texture || texture->m_texID == 0) {
        const double GROUP_WIDTH = computeLayout().groupBox.w * scale;
        const int    MAX_WIDTH   = std::clamp(sc<int>(bufferSize.x - 2 * GROUP_WIDTH), 0, INT_MAX);

        if (m_lastTitle.empty() || MAX_WIDTH < 1)
            return;

        texture = g_pHyprRenderer->renderText(m_lastTitle, toColor(color), m_lastScaledTitleSize, false, g_pGlobalState->config.titleFont->value(), MAX_WIDTH,
                                              g_pGlobalState->config.titleWeight->value().m_value);
    }

    if (!texture)
        return;

    const CBox TITLE_BOX = {barBox.x + std::round((bufferSize.x - texture->m_size.x) / 2.0), barBox.y + std::round((bufferSize.y - texture->m_size.y) / 2.0), texture->m_size.x,
                            texture->m_size.y};

    g_pHyprOpenGL->renderTexture(ctx, texture, TITLE_BOX, {.a = alpha});
}

static SP<Render::ITexture> resolveIconTexture(eBarButton button, eBarIcon icon, Config::INTEGER color, int deviceSize) {
    const SIconKey KEY = {.button = button, .icon = icon, .color = color, .deviceSize = deviceSize};

    if (const auto IT = g_pGlobalState->iconTextures.find(KEY); IT != g_pGlobalState->iconTextures.end())
        return IT->second;

    const auto ICON_COLOR = toColor(color);
    const auto SURFACE    = rasterizeIcon(icon, {.red = ICON_COLOR.r, .green = ICON_COLOR.g, .blue = ICON_COLOR.b, .alpha = ICON_COLOR.a}, deviceSize);
    auto       texture    = g_pHyprRenderer->createTexture(SURFACE.get());

    g_pGlobalState->iconTextures.emplace(KEY, texture);

    return texture;
}

void CTitleBarDecoration::renderButtons(Render::CRenderContext& ctx, const CBox& barBox, float scale, float alpha) {
    const auto LAYOUT = computeLayout();

    for (size_t i = 0; i < LAYOUT.buttons.size(); ++i) {
        const auto& DISC_BOX = LAYOUT.buttons[i].discBox;
        if (DISC_BOX.w <= 0)
            continue;

        const auto   BUTTON      = sc<eBarButton>(i);
        const auto&  ANIMATION   = m_buttonAnimations[i];
        const auto   ICON_COLOR  = selectIconColor(BUTTON);
        const double DEVICE_SIZE = std::max(2.0, std::round(DISC_BOX.w * scale / 2.0) * 2.0);
        const double RADIUS      = DEVICE_SIZE / 2.0;
        const CBox   DISC = {std::round(barBox.x + (DISC_BOX.x + DISC_BOX.w / 2.0) * scale - RADIUS), std::round(barBox.y + (DISC_BOX.y + DISC_BOX.h / 2.0) * scale - RADIUS),
                             DEVICE_SIZE, DEVICE_SIZE};

        CHyprColor   discColor = ANIMATION.discColor->value();
        discColor.a *= alpha;

        g_pHyprOpenGL->renderRect(ctx, DISC, discColor, {.round = sc<int>(RADIUS), .roundingPower = 2.F});

        const float STATE_LAYER_ALPHA = ANIMATION.stateLayerAlpha->value();

        if (STATE_LAYER_ALPHA > 0)
            g_pHyprOpenGL->renderRect(ctx, DISC, toColor(ICON_COLOR).modifyA(STATE_LAYER_ALPHA * alpha), {.round = sc<int>(RADIUS), .roundingPower = 2.F});

        const float ICON_ALPHA = ANIMATION.iconAlpha->value();

        if (ICON_ALPHA <= 0)
            continue;

        const bool IS_MAXIMIZED = Fullscreen::controller()->isFullscreen(m_window.lock(), Fullscreen::FSMODE_MAXIMIZED);
        g_pHyprOpenGL->renderTexture(ctx, resolveIconTexture(BUTTON, selectIcon(BUTTON, IS_MAXIMIZED), ICON_COLOR, sc<int>(DEVICE_SIZE)), DISC, {.a = ICON_ALPHA * alpha});
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

    CHyprColor color = m_barColor->value();
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
    updatePointer();
}

void CTitleBarDecoration::onConfigReloaded() {
    m_titleTexture         = nullptr;
    m_inactiveTitleTexture = nullptr;

    g_pDecorationPositioner->repositionDeco(this);
    updatePointer();
    syncAnimations(true);
    damageEntire();
}

void CTitleBarDecoration::damageEntire() {
    g_pHyprRenderer->damageBox(computePaintedBox());
}

CBox CTitleBarDecoration::computePaintedBox() {
    if (!validMapped(m_window))
        return {};

    CBox box = assignedBoxGlobal();
    box.h += m_window->presentation().rounding() * 3;

    return box.translate(m_window->presentation().floatingOffset());
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
    syncAnimations(true);
}
