// SPDX-License-Identifier: GPL-3.0-or-later
#include "receiver.h"
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QProcessEnvironment>
#include <windows.h>

QString UxPlayReceiver::packageRoot() {
    const QDir app(QCoreApplication::applicationDirPath());
    return app.exists("lib/gstreamer-1.0") ? app.absolutePath() : app.absoluteFilePath("..");
}
QString UxPlayReceiver::executable() { return QCoreApplication::applicationDirPath() + "/uxplay.exe"; }

UxPlayReceiver::UxPlayReceiver(QObject *parent) : ReceiverBackend(parent) {
    startupTimer.setSingleShot(true);
    startupTimer.setInterval(25000);
    killTimer.setSingleShot(true);
    killTimer.setInterval(3000);
    connect(&startupTimer, &QTimer::timeout, this, [this] {
        emit error("Der Empfänger startet nicht rechtzeitig. Bitte Diagnose öffnen und erneut starten.", lastDetails);
        stop();
    });
    connect(&killTimer, &QTimer::timeout, &process, &QProcess::kill);
    connect(&process, &QProcess::readyReadStandardOutput, this, [this] { consume(stdoutBuffer, process.readAllStandardOutput()); });
    connect(&process, &QProcess::readyReadStandardError, this, [this] { consume(stderrBuffer, process.readAllStandardError()); });
    connect(&process, &QProcess::started, this, [this] {
        HANDLE handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, FALSE, DWORD(process.processId()));
        if (handle && job && !AssignProcessToJobObject((HANDLE)job, handle))
            emit diagnostic("WARN: Windows konnte den Empfänger nicht dem Prozess-Job zuordnen, Code " + QString::number(GetLastError()));
        if (handle) CloseHandle(handle);
    });
    connect(&process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError reason) {
        if (!expectedStop && reason == QProcess::FailedToStart) {
            startupTimer.stop();
            emit error("Der AirPlay-Empfänger konnte nicht gestartet werden. Bitte die App vollständig neu installieren.", process.errorString());
        }
    });
    connect(&process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this, [this](int code, QProcess::ExitStatus status) {
        startupTimer.stop(); killTimer.stop();
        consume(stdoutBuffer, process.readAllStandardOutput() + '\n');
        consume(stderrBuffer, process.readAllStandardError() + '\n');
        if (!expectedStop) emit error("Der Empfänger wurde unerwartet beendet. Bitte Diagnose prüfen oder neu starten.",
            QString("Exit %1 / %2\n%3").arg(code).arg(status == QProcess::CrashExit ? "Absturz" : "Beendet", lastDetails));
        runningReady = false;
        emit stopped();
    });
    job = CreateJobObjectW(nullptr, nullptr);
    if (job) {
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION info = {};
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if (!SetInformationJobObject((HANDLE)job, JobObjectExtendedLimitInformation, &info, sizeof(info))) {
            CloseHandle((HANDLE)job); job = nullptr;
            emit diagnostic("WARN: Prozessüberwachung konnte nicht initialisiert werden.");
        }
    }
}

UxPlayReceiver::~UxPlayReceiver() {
    expectedStop = true;
    if (process.state() != QProcess::NotRunning) {
        command("STOP"); process.closeWriteChannel();
        if (!process.waitForFinished(1200)) { process.kill(); process.waitForFinished(1000); }
    }
    if (job) CloseHandle((HANDLE)job);
}

void UxPlayReceiver::start(const Settings &settings, quintptr window, bool testPattern) {
    if (process.state() != QProcess::NotRunning) return;
    stdoutBuffer.clear(); stderrBuffer.clear(); lastDetails.clear();
    expectedStop = false; runningReady = false; desiredMute = settings.muted;
    if (!QFile::exists(executable())) {
        emit error("uxplay.exe fehlt. Bitte die App vollständig neu installieren.", executable());
        return;
    }
    QDir().mkpath(Settings::dataDirectory());
    QFile config(Settings::dataDirectory() + "/receiver.conf");
    if (!config.open(QIODevice::WriteOnly) || config.write("# PC-Mirror manages receiver options.\n") < 0) {
        emit error("Die Empfänger-Konfiguration konnte nicht erstellt werden.", config.errorString());
        return;
    }
    config.close();
    auto env = QProcessEnvironment::systemEnvironment();
    const auto root = packageRoot();
    env.insert("PATH", QCoreApplication::applicationDirPath() + ";" + env.value("SystemRoot") + "/System32;" + env.value("SystemRoot"));
    env.insert("GST_PLUGIN_SYSTEM_PATH_1_0", root + "/lib/gstreamer-1.0");
    env.insert("GST_PLUGIN_PATH_1_0", "");
    env.insert("GST_PLUGIN_SCANNER_1_0", root + "/libexec/gstreamer-1.0/gst-plugin-scanner.exe");
    env.insert("GST_REGISTRY_1_0", Settings::dataDirectory() + "/gstreamer-registry.bin");
    env.insert("PCMIRROR_EMBEDDED", "1");
    env.insert("PCMIRROR_HWND", QString::number(window));
    env.insert("PCMIRROR_IPAD_FILL", settings.ipadFill ? "1" : "0");
    process.setProcessEnvironment(env);
    process.setWorkingDirectory(Settings::dataDirectory());
    process.setProgram(executable());
    process.setArguments(testPattern ? QStringList{"--pcmirror-selftest"} : settings.receiverArguments(config.fileName()));
    process.start(); startupTimer.start();
    emit diagnostic(testPattern ? "Synthetischer Bildtest gestartet; kein AirPlay-Test." : "AirPlay-Empfänger startet: " + settings.name);
}

void UxPlayReceiver::command(const QByteArray &line) {
    if (process.state() == QProcess::Running && process.write(line + '\n') < 0)
        emit error("Der Empfänger reagiert nicht auf die Steuerung. Bitte neu starten.", process.errorString());
}
void UxPlayReceiver::stop() {
    expectedStop = true; startupTimer.stop();
    if (process.state() == QProcess::NotRunning) { emit stopped(); return; }
    command("STOP"); killTimer.start();
}
void UxPlayReceiver::mute(bool muted) { desiredMute = muted; command(muted ? "MUTE 1" : "MUTE 0"); }
void UxPlayReceiver::snapshot(const QString &path) { command("SNAPSHOT " + path.toUtf8()); }
void UxPlayReceiver::setIpadFill(bool fill) { command(fill ? "IPAD_FILL 1" : "IPAD_FILL 0"); }

void UxPlayReceiver::consume(QByteArray &buffer, QByteArray bytes) {
    buffer += bytes;
    while (buffer.contains('\n')) {
        int newline = buffer.indexOf('\n');
        QString line = QString::fromUtf8(buffer.left(newline)).trimmed();
        buffer.remove(0, newline + 1);
        if (line.isEmpty()) continue;
        int marker = line.indexOf("PCMIRROR|");
        if (marker >= 0) {
            auto parts = line.mid(marker).split('|');
            if (parts.value(1) == "READY") {
                runningReady = true; startupTimer.stop(); mute(desiredMute); emit ready();
            } else if (parts.value(1) == "FRAME") emit frame(parts.value(2).toInt(), parts.value(3).toInt());
            else if (parts.value(1) == "IDLE") emit idle();
            else if (parts.value(1) == "PIN") emit pinRequired(parts.value(2));
            else if (parts.value(1) == "MUTED") emit muted(parts.value(2) == "1");
            else if (parts.value(1) == "SNAPSHOT") emit snapshotFinished(parts.value(2) == "OK");
            else if (parts.value(1) == "IPAD_FILL") emit diagnostic("iPad-Darstellung: " + QString(parts.value(2) == "1" ? "Fenster ausfüllen" : "Ganzes Bild"));
            continue;
        }
        lastDetails = (lastDetails + '\n' + line).right(12000);
        emit diagnostic(line);
        if (line.contains("error", Qt::CaseInsensitive) || line.contains("failed", Qt::CaseInsensitive) || line.contains("WARNING")) {
            emit error("Der Empfänger meldet ein Problem. Details und Hilfen stehen in der Diagnose.", line);
        }
    }
    if (buffer.size() > 65536) { emit diagnostic(QString::fromUtf8(buffer.left(65536))); buffer.clear(); }
}
