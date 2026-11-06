// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "settings.h"
#include <QObject>
#include <QProcess>
#include <QTimer>

// Protocol-neutral boundary: a later backend can implement these operations.
class ReceiverBackend : public QObject {
    Q_OBJECT
public:
    explicit ReceiverBackend(QObject *parent = nullptr) : QObject(parent) {}
    virtual void start(const Settings &, quintptr window, bool testPattern = false) = 0;
    virtual void stop() = 0;
    virtual void mute(bool) = 0;
    virtual void snapshot(const QString &) = 0;
    virtual void setIpadFill(bool) = 0;
signals:
    void ready();
    void frame(int width, int height);
    void idle();
    void pinRequired(QString pin);
    void muted(bool muted);
    void snapshotFinished(bool ok);
    void error(QString message, QString details);
    void diagnostic(QString line);
    void stopped();
};

class UxPlayReceiver final : public ReceiverBackend {
    Q_OBJECT
public:
    explicit UxPlayReceiver(QObject *parent = nullptr);
    ~UxPlayReceiver() override;
    void start(const Settings &, quintptr window, bool testPattern = false) override;
    void stop() override;
    void mute(bool) override;
    void snapshot(const QString &) override;
    void setIpadFill(bool) override;
    static QString packageRoot();
    static QString executable();
private:
    void consume(QByteArray &buffer, QByteArray bytes);
    void command(const QByteArray &line);
    QProcess process;
    QTimer startupTimer, killTimer;
    QByteArray stdoutBuffer, stderrBuffer;
    QString lastDetails;
    bool expectedStop = false;
    bool runningReady = false;
    bool desiredMute = false;
    void *job = nullptr;
};
