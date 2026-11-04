#pragma once

#include <hyprland/src/render/pass/PassElement.hpp>

class CTitleBarDecoration;

class CTitleBarPassElement : public IPassElement {
  public:
    struct STitleBarData {
        CTitleBarDecoration* decoration = nullptr;
        float                alpha      = 1.F;
    };

    CTitleBarPassElement(const STitleBarData& data);
    virtual ~CTitleBarPassElement() = default;

    virtual std::vector<UP<IPassElement>> draw(Render::CRenderContext& ctx) override;
    virtual bool                          needsLiveBlur(Render::CRenderContext&) override;
    virtual bool                          needsPrecomputeBlur(Render::CRenderContext&) override;
    virtual std::optional<CBox>           boundingBox(Render::CRenderContext& ctx) override;

    virtual const char*                   passName() override {
        return "CTitleBarPassElement";
    }

    virtual ePassElementType type() override {
        return EK_CUSTOM;
    }

  private:
    STitleBarData m_data;
};
