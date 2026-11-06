// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QString>
#include <QStringList>

struct Settings {
    QString name = "PC-Mirror";
    QString language = "de";
    int quality = 1;
    bool autostart = false;
    bool pinEnabled = false;
    QString pin;
    bool softwareDecoder = false;
    bool topmost = false;
    bool muted = false;
    bool automaticUpdates = true;
    bool ipadFill = false;
    static Settings load();
    bool save(QString *error) const;
    QStringList receiverArguments(const QString &configFile) const;
    static QString dataDirectory();
};
