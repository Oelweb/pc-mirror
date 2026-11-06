// Exercise the production UpdateService/helper without a GUI or user input.
#include "updater.h"
#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>
#include <Velopack.hpp>
#include <cstring>
#include <iostream>
int main(int argc,char **argv) {
    Velopack::VelopackApp::Build().SetAutoApplyOnStartup(false).Run();
    if(argc>1 && !std::strcmp(argv[1],"--update-worker")) return runUpdateWorker(argc,argv);
    QCoreApplication app(argc,argv); UpdateService service; QString diagnostics;
    const bool download = app.arguments().contains("--download"); bool requested = false;
    QObject::connect(&service,&UpdateService::diagnostic,&app,[&](const QString &line) { diagnostics += line+'\n'; });
    QObject::connect(&service,&UpdateService::changed,&app,[&] {
        if(service.isBusy()) return;
        if(download && !requested && service.canInstall()) { requested = true; service.install(); return; }
        QJsonObject result{{"completed",true},{"banner",service.visible()},{"status",service.statusText()},{"diagnostics",diagnostics}};
        std::cout << QJsonDocument(result).toJson(QJsonDocument::Compact).toStdString() << std::endl; app.quit();
    });
    QTimer::singleShot(download ? 180000 : 35000,&app,[&] { app.exit(2); });
    QTimer::singleShot(0,&app,[&] { service.check(true); });
    return app.exec();
}
