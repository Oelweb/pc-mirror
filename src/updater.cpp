// SPDX-License-Identifier: GPL-3.0-or-later
#include "updater.h"
#include "i18n.h"
#include <QCoreApplication>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <Velopack.hpp>
#include <iostream>
#include <thread>
#include <windows.h>

namespace {
void publish(QJsonObject object) {
    std::cout << QJsonDocument(object).toJson(QJsonDocument::Compact).toStdString() << std::endl;
}
bool strongHash(const Velopack::UpdateInfo &update) {
    return QRegularExpression("^[0-9a-fA-F]{64}$").match(QString::fromStdString(update.TargetFullRelease.SHA256)).hasMatch();
}
}

int runUpdateWorker(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    const auto arguments = app.arguments();
    const int ownerIndex = arguments.indexOf("--owner-pid");
    bool valid = false;
    const DWORD ownerPid = arguments.value(ownerIndex + 1).toUInt(&valid);
    if (ownerIndex < 0 || !valid || !ownerPid || ownerPid == GetCurrentProcessId()) {
        publish({{"event","checkFailed"},{"details","Invalid updater owner PID"}}); return 2;
    }
    HANDLE owner = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, ownerPid);
    if (!owner) { publish({{"event","checkFailed"},{"details","Cannot open updater owner: " + QString::number(GetLastError())}}); return 2; }
    wchar_t ownerPath[32768]; DWORD length = 32768;
    if (!QueryFullProcessImageNameW(owner, 0, ownerPath, &length)) {
        publish({{"event","checkFailed"},{"details","Cannot identify updater owner executable"}}); CloseHandle(owner); return 2;
    }
    const QString ownerExecutable = QString::fromWCharArray(ownerPath, length);
    const QString ownExecutable = QDir::toNativeSeparators(QCoreApplication::applicationFilePath());
    HANDLE a = CreateFileW((LPCWSTR)ownerExecutable.utf16(),0,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
    HANDLE b = CreateFileW((LPCWSTR)ownExecutable.utf16(),0,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
    BY_HANDLE_FILE_INFORMATION ai={}, bi={};
    bool same = a!=INVALID_HANDLE_VALUE && b!=INVALID_HANDLE_VALUE && GetFileInformationByHandle(a,&ai) && GetFileInformationByHandle(b,&bi)
        && ai.dwVolumeSerialNumber==bi.dwVolumeSerialNumber && ai.nFileIndexHigh==bi.nFileIndexHigh && ai.nFileIndexLow==bi.nFileIndexLow;
    if(a!=INVALID_HANDLE_VALUE) CloseHandle(a); if(b!=INVALID_HANDLE_VALUE) CloseHandle(b);
    if (!same) {
        publish({{"event","checkFailed"},{"details","Updater owner executable does not match: " + ownerExecutable + " / " + ownExecutable}});
        CloseHandle(owner); return 2;
    }
    // The helper never outlives its GUI, even if a remote request stalls.
    std::thread([owner] { WaitForSingleObject(owner, INFINITE); CloseHandle(owner); ExitProcess(0); }).detach();
    std::unique_ptr<Velopack::UpdateManager> manager;
    std::optional<Velopack::UpdateInfo> update;
    bool downloaded = false;
    std::string command;
    while (std::getline(std::cin, command)) {
        if (!command.empty() && command.back() == '\r') command.pop_back();
        if (command == "STOP") break;
        try {
            if (!manager) manager = std::make_unique<Velopack::UpdateManager>(
                std::make_unique<Velopack::HttpSource>(UpdateService::updateUrl));
            if (command == "CHECK") {
                update = manager->CheckForUpdates(); downloaded = false;
                if (update && !strongHash(*update)) throw std::runtime_error("Release has no valid SHA-256. Refusing update.");
                publish({{"event", "checked"}, {"version", update ? QString::fromStdString(update->TargetFullRelease.Version) : QString()}});
            } else if (command == "DOWNLOAD") {
                if (!update || !strongHash(*update)) throw std::runtime_error("No SHA-256 protected update selected");
                // Verification and installation are exclusively Velopack operations.
                manager->DownloadUpdates(*update, [](void *, size_t value) { publish({{"event","progress"},{"value",int(value)}}); });
                downloaded = true; publish({{"event", "downloaded"}});
            } else if (command == "APPLY") {
                if (!update || !downloaded) throw std::runtime_error("No verified download available");
                // The SDK's default WaitExitThenApply waits for THIS helper. Pass
                // the validated GUI PID explicitly so it waits for the real app.
                manager->UnsafeApplyUpdates(update->TargetFullRelease, true, ownerPid, true, {});
                publish({{"event", "applying"}});
            }
        } catch (const std::exception &error) {
            publish({{"event", command == "CHECK" ? "checkFailed" : "failed"}, {"details", QString::fromUtf8(error.what())}});
        }
    }
    return 0;
}

UpdateService::UpdateService(QObject *parent) : QObject(parent) {
    periodic.setInterval(6 * 60 * 60 * 1000);
    connect(&periodic, &QTimer::timeout, this, [this] { check(false); });
    watchdog.setSingleShot(true);
    connect(&watchdog, &QTimer::timeout, this, [this] {
        if (shuttingDown) return;
        const bool installation = policy.requested || policy.preparing || progress >= 0;
        worker.kill(); busy = false; progress = -1; status.clear();
        const bool active = policy.streaming; policy = {}; policy.streaming = active; version.clear(); bannerVisible = installation;
        if (installation) status = "Update-Zeitlimit erreicht. Später erneut versuchen.";
        emit diagnostic("Update-Vorgang hat das Zeitlimit erreicht; App läuft weiter."); emit changed();
    });
    connect(&worker, &QProcess::readyReadStandardOutput, this, &UpdateService::readOutput);
    connect(&worker, &QProcess::readyReadStandardError, this, [this] {
        emit diagnostic("Velopack: " + QString::fromUtf8(worker.readAllStandardError()));
    });
    connect(&worker, &QProcess::errorOccurred, this, [this](QProcess::ProcessError) {
        if (shuttingDown) return;
        const bool installation = policy.requested || policy.preparing || progress >= 0;
        watchdog.stop(); busy = false; status.clear();
        const bool active = policy.streaming; policy = {}; policy.streaming = active; version.clear(); bannerVisible = installation;
        if (installation) status = "Update konnte nicht installiert werden. Später erneut versuchen.";
        emit diagnostic("Update-Helfer: " + worker.errorString()); emit changed();
    });
    connect(&worker, qOverload<int,QProcess::ExitStatus>(&QProcess::finished), this, [this](int code, QProcess::ExitStatus) {
        if (shuttingDown) return;
        watchdog.stop();
        if (busy) { busy = false; progress = -1; status.clear(); const bool active = policy.streaming; policy = {}; policy.streaming = active; version.clear(); bannerVisible = false; emit diagnostic("Update-Helfer beendet: " + QString::number(code)); emit changed(); }
    });
    idleGrace.setSingleShot(true); idleGrace.setInterval(2000);
    connect(&idleGrace, &QTimer::timeout, this, [this] {
        if (!policy.canPrepare()) return;
        policy.preparing = true; status = "Update wird installiert …"; emit changed(); emit prepareInstall();
    });
}
UpdateService::~UpdateService() {
    shuttingDown = true;
    periodic.stop(); idleGrace.stop(); watchdog.stop();
    if (worker.state() != QProcess::NotRunning) {
        worker.write("STOP\n"); worker.closeWriteChannel();
        if (!worker.waitForFinished(150)) { worker.kill(); worker.waitForFinished(1000); }
    }
}
void UpdateService::send(const QByteArray &command) {
    if (worker.state() == QProcess::NotRunning) {
        output.clear(); worker.setProgram(QCoreApplication::applicationFilePath());
        worker.setArguments({"--update-worker", "--owner-pid", QString::number(QCoreApplication::applicationPid())}); worker.start();
    }
    worker.write(command + '\n');
}
void UpdateService::readOutput() {
    if (shuttingDown) return;
    output += worker.readAllStandardOutput();
    while (output.contains('\n')) {
        int newline = output.indexOf('\n'); auto line = output.left(newline); output.remove(0,newline+1);
        const auto event = QJsonDocument::fromJson(line).object();
        const auto kind = event.value("event").toString();
        if (kind == "checked") {
            watchdog.stop(); busy = false; version = event.value("version").toString();
            status = version.isEmpty() ? "Die App ist aktuell." : "Update verfügbar (v%1)";
            bannerVisible = !version.isEmpty();
        } else if (kind == "checkFailed") {
            watchdog.stop(); busy = false; status.clear();
            emit diagnostic("Update-Prüfung ohne Ergebnis: " + event.value("details").toString());
        } else if (kind == "progress") progress = event.value("value").toInt();
        else if (kind == "downloaded") {
            watchdog.stop(); busy = false; progress = -1; policy.downloaded = true;
            status = "Update heruntergeladen und geprüft."; maybePrepare();
        } else if (kind == "failed") {
            watchdog.stop(); busy = false; progress = -1; policy.preparing = false; policy.requested = false;
            status = "Update konnte nicht installiert werden. Später erneut versuchen."; bannerVisible = true;
            emit diagnostic("Update fehlgeschlagen: " + event.value("details").toString());
        } else if (kind == "applying") { watchdog.stop(); busy = false; emit quitForUpdate(); }
        if (!kind.isEmpty()) emit changed();
    }
    if (output.size() > 65536) output.clear();
}
void UpdateService::start(bool enabled) {
    setAutomatic(enabled);
    if (enabled) QTimer::singleShot(2000, this, [this] { if (automatic) check(false); });
}
void UpdateService::setAutomatic(bool enabled) { automatic = enabled; if (enabled) periodic.start(); else periodic.stop(); }
void UpdateService::check(bool) {
    if (busy || policy.preparing) return;
    if (policy.downloaded) { bannerVisible = true; emit changed(); return; }
    busy = true; status = "Nach Updates suchen …"; emit changed(); watchdog.start(30000); send("CHECK");
}
void UpdateService::setStreaming(bool value) {
    policy.streaming = value; if (value) idleGrace.stop(); else maybePrepare(); emit changed();
}
void UpdateService::install() {
    if (busy || version.isEmpty() || policy.preparing) return;
    bannerVisible = true; policy.requested = true;
    if (policy.downloaded) { maybePrepare(); emit changed(); return; }
    busy = true; progress = 0; emit changed(); watchdog.start(10 * 60 * 1000); send("DOWNLOAD");
}
void UpdateService::postpone() { policy.postpone(); idleGrace.stop(); bannerVisible = false; emit changed(); }
void UpdateService::maybePrepare() { if (policy.canPrepare()) idleGrace.start(); }
void UpdateService::applyAfterReceiverStopped() {
    if (!policy.preparing || policy.streaming) return;
    busy = true; watchdog.start(30000); send("APPLY");
}
QString UpdateService::message() const {
    if (policy.preparing) return I18n::tr("Update wird installiert …");
    if (progress >= 0) return I18n::tr("Update wird heruntergeladen … %1 %").arg(progress);
    if (policy.downloaded && policy.requested && policy.streaming) return I18n::tr("Update bereit. Installation nach dem Spiegeln.");
    if (policy.downloaded) return I18n::tr("Update bereit (v%1)").arg(version);
    return statusText();
}

QString UpdateService::statusText() const {
    return status.contains("%1") ? I18n::tr(status).arg(version) : I18n::tr(status);
}
