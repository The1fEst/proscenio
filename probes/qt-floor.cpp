#include <LayerShellQt/Shell>
#include <LayerShellQt/Window>
#include <QApplication>
#include <QGuiApplication>
#include <QScreen>
#include <QWidget>
#include <QWindow>

int main(int argc, char **argv) {
    QApplication app(argc, argv);

    QList<QWidget *> floors;
    for (QScreen *screen : QGuiApplication::screens()) {
        auto *widget = new QWidget;
        widget->setAttribute(Qt::WA_TranslucentBackground);
        widget->setFixedHeight(50);
        widget->winId();

        QWindow *handle = widget->windowHandle();
        handle->setScreen(screen);

        auto *layer = LayerShellQt::Window::get(handle);
        layer->setScope(QStringLiteral("floor:qt"));
        layer->setLayer(LayerShellQt::Window::LayerTop);
        layer->setAnchors(LayerShellQt::Window::Anchors(LayerShellQt::Window::AnchorLeft |
                                                        LayerShellQt::Window::AnchorRight |
                                                        LayerShellQt::Window::AnchorTop));
        layer->setExclusiveZone(0);
        layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityNone);

        widget->show();
        floors.append(widget);
    }

    return app.exec();
}
