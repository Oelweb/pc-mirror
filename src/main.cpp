// SPDX-License-Identifier: GPL-3.0-or-later
#include "mainwindow.h"
#include "ui.h"
#include "i18n.h"
#include "updater.h"
#include <QApplication>
#include <QDir>
#include <QFile>
#include <QIcon>
#include <QLockFile>
#include <QMessageBox>
#include <QPainter>
#include <QStyleFactory>
#include <Velopack.hpp>
#include <cstring>

int main(int argc, char *argv[]) {
    // Installer fast-exit hooks must run before Qt or any single-instance lock.
    // Applying an already downloaded update always requires our UI consent.
    Velopack::VelopackApp::Build().SetAutoApplyOnStartup(false).Run();
    if (argc > 1 && !std::strcmp(argv[1], "--update-worker")) return runUpdateWorker(argc, argv);
    QApplication app(argc, argv);
    app.setApplicationName("PC-Mirror"); app.setApplicationVersion(PCMIRROR_VERSION);
    app.setStyle(QStyleFactory::create("Fusion")); app.setFont(QFont("Segoe UI Variable", 10));
    I18n::setLanguage(Settings::load().language);
    QDir().mkpath(Settings::dataDirectory() + "/logs");
    QFile logProbe(Settings::dataDirectory() + "/logs/pc-mirror.log");
    if (!logProbe.open(QIODevice::Append)) {
        QMessageBox::critical(nullptr, "PC-Mirror", I18n::tr("Der Logordner ist nicht beschreibbar:\n%1\n%2").arg(logProbe.fileName(),logProbe.errorString())); return 1;
    }
    logProbe.close();
    bool synthetic = app.arguments().contains("--test-pattern");
    bool preview = app.arguments().contains("--ui-preview");
    QLockFile lock(Settings::dataDirectory() + (preview ? "/preview.lock" : synthetic ? "/test.lock" : "/instance.lock")); lock.setStaleLockTime(0);
    if (!lock.tryLock(0)) { QMessageBox::information(nullptr,"PC-Mirror",I18n::tr("PC-Mirror läuft bereits. Bitte das vorhandene Fenster über die Taskleiste öffnen.")); return 0; }
    app.setWindowIcon(Ui::appIcon());
    QFile style(":/theme.qss"); if (style.open(QIODevice::ReadOnly)) app.setStyleSheet(QString::fromUtf8(style.readAll()));
    MainWindow window(synthetic, preview); window.show();
    return app.exec();
}
