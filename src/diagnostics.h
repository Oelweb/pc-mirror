// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QFile>
#include <QMutex>
#include <QNetworkInterface>

class Diagnostics : public QObject {
    Q_OBJECT
public:
    explicit Diagnostics(QObject *parent = nullptr);
    void log(const QString &line);
    QString logPath() const;
    QString networkSummary() const;
    void checkFirewall();
    void configureFirewall();
signals:
    void lineAdded(QString line);
    void firewallResult(bool configured, QString message);
private:
    QFile file;
    QMutex mutex;
};
