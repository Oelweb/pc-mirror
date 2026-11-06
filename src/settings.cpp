// SPDX-License-Identifier: GPL-3.0-or-later
#include "settings.h"
#include <QCoreApplication>
#include <QDir>
#include <QSettings>

QString Settings::dataDirectory() {
    return qEnvironmentVariable("LOCALAPPDATA") + "/PC-Mirror";
}

Settings Settings::load() {
    QSettings file(dataDirectory() + "/settings.ini", QSettings::IniFormat);
    Settings value;
    value.name = file.value("receiver/name", value.name).toString();
    value.language = file.value("ui/language", "de").toString();
    if (value.language != "de" && value.language != "en") value.language = "de";
    value.quality = qBound(0, file.value("receiver/quality", 1).toInt(), 2);
    value.pinEnabled = file.value("receiver/pinEnabled", false).toBool();
    value.pin = file.value("receiver/pin").toString();
    value.softwareDecoder = file.value("receiver/softwareDecoder", false).toBool();
    value.topmost = file.value("window/topmost", false).toBool();
    value.muted = file.value("audio/muted", false).toBool();
    value.automaticUpdates = file.value("updates/automatic", true).toBool();
    value.ipadFill = file.value("video/ipadFill", false).toBool();
    QSettings startup("HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", QSettings::NativeFormat);
    value.autostart = startup.contains("PC-Mirror");
    return value;
}

bool Settings::save(QString *error) const {
    QDir().mkpath(dataDirectory());
    QSettings file(dataDirectory() + "/settings.ini", QSettings::IniFormat);
    file.setValue("receiver/name", name);
    file.setValue("ui/language", language);
    file.setValue("receiver/quality", quality);
    file.setValue("receiver/pinEnabled", pinEnabled);
    file.setValue("receiver/pin", pin);
    file.setValue("receiver/softwareDecoder", softwareDecoder);
    file.setValue("window/topmost", topmost);
    file.setValue("audio/muted", muted);
    file.setValue("updates/automatic", automaticUpdates);
    file.setValue("video/ipadFill", ipadFill);
    file.sync();
    if (file.status() != QSettings::NoError) {
        *error = "Die Einstellungen konnten nicht gespeichert werden. Bitte Schreibrechte prüfen.";
        return false;
    }
    QSettings startup("HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", QSettings::NativeFormat);
    if (autostart) startup.setValue("PC-Mirror", '"' + QDir::toNativeSeparators(QCoreApplication::applicationFilePath()) + '"');
    else startup.remove("PC-Mirror");
    startup.sync();
    if (startup.status() != QSettings::NoError) {
        *error = "Der Windows-Autostart konnte nicht geändert werden.";
        return false;
    }
    return true;
}

QStringList Settings::receiverArguments(const QString &configFile) const {
    const QStringList resolutions = {"1280x720@30", "1920x1080@60", "3840x2160@60"};
    QStringList args = {"-rc", configFile, "-n", name, "-nh", "-p", "48000", "-s", resolutions.at(quality),
        "-fps", quality == 0 ? "30" : "60", "-vs", "d3d11videosink force-aspect-ratio=true", "-as", "wasapisink",
        "-key", dataDirectory() + "/receiver.pem"};
    if (quality == 2) args << "-h265";
    if (softwareDecoder) args << "-avdec";
    if (pinEnabled) {
        args << "-pin";
        if (!pin.isEmpty()) args << pin;
    }
    return args;
}
