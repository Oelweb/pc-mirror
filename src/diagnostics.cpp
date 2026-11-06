// SPDX-License-Identifier: GPL-3.0-or-later
#include "diagnostics.h"
#include "receiver.h"
#include "i18n.h"
#include <QDateTime>
#include <QDir>
#include <QProcess>
#include <QSaveFile>
#include <windows.h>
#include <shellapi.h>

Diagnostics::Diagnostics(QObject *parent) : QObject(parent), file(logPath()) {
    QDir().mkpath(Settings::dataDirectory() + "/logs");
    if (file.size() > 5 * 1024 * 1024) {
        QFile::remove(logPath() + ".previous");
        QFile::rename(logPath(), logPath() + ".previous");
    }
    if (!file.open(QIODevice::Append | QIODevice::Text))
        emit lineAdded("FEHLER: Logdatei kann nicht geschrieben werden: " + file.errorString());
}
QString Diagnostics::logPath() const { return Settings::dataDirectory() + "/logs/pc-mirror.log"; }
void Diagnostics::log(const QString &line) {
    auto text = QDateTime::currentDateTime().toString(Qt::ISODateWithMs) + "  " + line;
    { QMutexLocker lock(&mutex); if (file.isOpen()) { file.write(text.toUtf8() + '\n'); file.flush(); } }
    emit lineAdded(text);
}
QString Diagnostics::networkSummary() const {
    QStringList adapters;
    for (const auto &adapter : QNetworkInterface::allInterfaces()) {
        if (!(adapter.flags() & QNetworkInterface::IsUp) || adapter.flags() & QNetworkInterface::IsLoopBack) continue;
        for (const auto &address : adapter.addressEntries()) {
            if (address.ip().protocol() == QAbstractSocket::IPv4Protocol)
                adapters << adapter.humanReadableName() + ": " + address.ip().toString();
        }
    }
    if (adapters.isEmpty()) return I18n::tr("Kein aktiver IPv4-Netzwerkadapter. Bitte den PC mit deinem Router verbinden.");
    return adapters.join('\n') + "\n" + I18n::tr("PC und iPhone/iPad müssen im selben lokalen Netz sein. Ethernet am PC ist erlaubt.\nGastnetz, VPN und AP-Isolation können AirPlay blockieren. Das WLAN des Apple-Geräts kann Windows nicht zuverlässig prüfen.");
}

void Diagnostics::checkFirewall() {
    auto process = new QProcess(this);
    // Native program path is escaped as a PowerShell literal, never shell code.
    auto program = QDir::toNativeSeparators(UxPlayReceiver::executable());
    program.replace("'", "''");
    QString script = "$ErrorActionPreference='Stop'; $p='" + program + "'; "
        "$r=@(Get-NetFirewallRule -DisplayName 'PC-Mirror*' -ErrorAction SilentlyContinue | Where-Object { $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' } | Get-NetFirewallApplicationFilter | Where-Object { $_.Program -eq $p }); "
        "$n=@(Get-NetConnectionProfile); $private=@($n | Where-Object { $_.NetworkCategory -eq 'Private' }); "
        "if($private.Count -eq 0){Write-Output 'PUBLIC';exit 2}; if($r.Count -ge 3){Write-Output 'OK';exit 0}; Write-Output 'MISSING';exit 1";
    connect(process, qOverload<int,QProcess::ExitStatus>(&QProcess::finished), this, [this, process](int code, QProcess::ExitStatus) {
        auto out = QString::fromUtf8(process->readAllStandardOutput());
        auto err = QString::fromUtf8(process->readAllStandardError());
        QString message;
        if (code == 0 && out.contains("OK")) message = "Firewall-Freigaben vorhanden · privates Netzwerk erkannt";
        else if (out.contains("PUBLIC")) message = "Kein privates Netzwerk erkannt. Windows-Einstellungen → Netzwerk → Profil Privat wählen, wenn dies dein vertrauenswürdiges Heimnetz ist.";
        else message = "Firewall-Freigabe fehlt oder ist nicht prüfbar. Über „Firewall einrichten“ freigeben. Windows fragt nach Administratorrechten.";
        log(message + (err.isEmpty() ? "" : "\n" + err));
        emit firewallResult(code == 0, message);
        process->deleteLater();
    });
    connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            log("Firewall-Prüfung konnte nicht starten: " + process->errorString());
            emit firewallResult(false, "Die Firewall-Prüfung konnte nicht starten. Bitte Diagnose öffnen.");
            process->deleteLater();
        }
    });
    const QString powershell = qEnvironmentVariable("SystemRoot") + "/System32/WindowsPowerShell/v1.0/powershell.exe";
    process->start(powershell, {"-NoProfile", "-NonInteractive", "-Command", script});
    QTimer::singleShot(15000, process, [process] { if (process->state() != QProcess::NotRunning) process->kill(); });
}

void Diagnostics::configureFirewall() {
    auto script = QDir::toNativeSeparators(UxPlayReceiver::packageRoot() + "/scripts/Configure-Firewall.ps1");
    if (!QFile::exists(script)) {
        log("FEHLER: Firewall-Helfer fehlt: " + script);
        emit firewallResult(false, "Firewall-Helfer fehlt. Bitte App vollständig neu installieren.");
        return;
    }
    auto args = QString("-NoProfile -ExecutionPolicy Bypass -File \"%1\" -ReceiverPath \"%2\"").arg(script, QDir::toNativeSeparators(UxPlayReceiver::executable()));
    SHELLEXECUTEINFOW info = {};
    info.cbSize = sizeof(info); info.fMask = SEE_MASK_NOCLOSEPROCESS;
    const QString powershell = qEnvironmentVariable("SystemRoot") + "/System32/WindowsPowerShell/v1.0/powershell.exe";
    info.lpVerb = L"runas"; info.lpFile = (LPCWSTR)powershell.utf16();
    info.lpParameters = (LPCWSTR)args.utf16(); info.nShow = SW_HIDE;
    if (!ShellExecuteExW(&info)) {
        auto message = GetLastError() == ERROR_CANCELLED ? "Firewall-Einrichtung wurde im Windows-Dialog abgebrochen." : "Windows konnte die Firewall-Einrichtung nicht starten.";
        log(message); emit firewallResult(false, message); return;
    }
    auto poll = new QTimer(this);
    auto handle = info.hProcess;
    connect(poll, &QTimer::timeout, this, [this, poll, handle] {
        if (WaitForSingleObject(handle, 0) != WAIT_OBJECT_0) return;
        DWORD code = 1; GetExitCodeProcess(handle, &code); CloseHandle(handle);
        poll->stop(); poll->deleteLater();
        log("Firewall-Helfer beendet, Code " + QString::number(code));
        checkFirewall();
    });
    poll->start(350);
}
