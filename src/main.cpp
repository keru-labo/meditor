#include "QtPlaybackBackend.h"

#include <QDir>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QtPlaybackBackend backend;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("playback"), &backend);
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty())
        return 1;

    if (app.arguments().size() > 1)
        backend.open(QUrl::fromLocalFile(QDir::current().absoluteFilePath(app.arguments().at(1))));
    return app.exec();
}
