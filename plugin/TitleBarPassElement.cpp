#include "TitleBarPassElement.hpp"
#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include "TitleBarDecoration.hpp"

CTitleBarPassElement::CTitleBarPassElement(const CTitleBarPassElement::STitleBarData& data) : m_data(data) {
    ;
}

std::vector<UP<IPassElement>> CTitleBarPassElement::draw(Render::CRenderContext& ctx) {
    m_data.decoration->renderPass(ctx, ctx.m_data.pMonitor.lock(), m_data.alpha);
    return {};
}

bool CTitleBarPassElement::needsLiveBlur(Render::CRenderContext&) {
    return false;
}

std::optional<CBox> CTitleBarPassElement::boundingBox(Render::CRenderContext& ctx) {
    return m_data.decoration->assignedBoxGlobal().translate(-ctx.m_data.pMonitor->m_position).expand(10);
}

bool CTitleBarPassElement::needsPrecomputeBlur(Render::CRenderContext&) {
    return false;
}
