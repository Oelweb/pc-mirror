// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "updatepolicy.h"
#include <QObject>
#include <QProcess>
#include <QTimer>
#include <memory>

int runUpdateWorker(int argc, char **argv);
class UpdateService final : public QObject {
    Q_OBJECT
public:
    explicit UpdateService(QObject *parent = nullptr);
    ~UpdateService() override;
    void start(bool automatic);
    void setAutomatic(bool enabled);
    void setStreaming(bool value);
    void check(bool manual = true);
    void install();
    void postpone();
    void applyAfterReceiverStopped();
    bool visible() const { return bannerVisible; }
    bool canInstall() const { return !busy && !policy.preparing && !version.isEmpty(); }
    bool applying() const { return policy.preparing; }
    QString message() const;
    QString statusText() const;
    bool isBusy() const { return busy; }
    static constexpr const char *updateUrl = "https://pcmirror.oel.best";
signals:
    void changed();
    void prepareInstall();
    void quitForUpdate();
    void diagnostic(QString message);
private:
    void maybePrepare();
    void send(const QByteArray &command);
    void readOutput();
    QProcess worker;
    QByteArray output;
    QTimer periodic, idleGrace, watchdog;
    UpdatePolicy policy;
    QString version, status = "";
    bool busy = false, bannerVisible = false, automatic = true;
    bool shuttingDown = false;
    int progress = -1;
};
