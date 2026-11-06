// SPDX-License-Identifier: GPL-3.0-or-later
#include "mainwindow.h"
#include "settingsdialog.h"
#include "ui.h"
#include "i18n.h"
#include <QApplication>
#include <QBoxLayout>
#include <QCloseEvent>
#include <QDateTime>
#include <QDesktopServices>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QImage>
#include <QMouseEvent>
#include <QSettings>
#include <QShortcut>
#include <QStandardPaths>
#include <QUrl>
#include <QUuid>
#include <windows.h>

namespace {
QPushButton *button(const QString &text, const QString &name, QWidget *parent = nullptr) {
    auto value = new QPushButton(parent); I18n::bind(value,"text",text); value->setObjectName(name);
    value->setCursor(Qt::PointingHandCursor); value->setMinimumHeight(40);
    I18n::bind(value,"accessibleName",text); return value;
}
QLabel *label(const QString &text, const QString &name, QWidget *parent = nullptr) {
    auto value = new QLabel(parent); I18n::bind(value,"text",text); value->setObjectName(name); value->setWordWrap(true); return value;
}
}

MainWindow::MainWindow(bool testPattern, bool isPreview)
    : settings(Settings::load()), diagnostics(this), receiver(this), updater(this), synthetic(testPattern), preview(isPreview) {
    setObjectName("mainWindow"); setWindowTitle("PC-Mirror"); resize(1008, 672); setMinimumSize(720, 480);
    auto root = new QWidget; setCentralWidget(root);
    auto layout = new QVBoxLayout(root); layout->setContentsMargins(0,0,0,0); layout->setSpacing(0);

    banner = new QFrame; banner->setObjectName("errorBanner");
    auto bannerLayout = new QHBoxLayout(banner); bannerLayout->setContentsMargins(16,8,16,8); bannerLayout->setSpacing(8);
    bannerText = label("", "errorText"); bannerLayout->addWidget(bannerText,1);
    auto details = button("Diagnose", "subtleButton"), retry = button("Neu starten", "subtleButton");
    auto dismiss = Ui::iconButton(Ui::Symbol::Close, "Hinweis schließen");
    bannerLayout->addWidget(details); bannerLayout->addWidget(retry); bannerLayout->addWidget(dismiss);
    connect(details,&QPushButton::clicked,this,&MainWindow::showDiagnostics);
    connect(retry,&QPushButton::clicked,this,&MainWindow::restart);
    connect(dismiss,&QPushButton::clicked,banner,&QWidget::hide);
    layout->addWidget(banner); banner->hide();

    updateBanner = new QFrame; updateBanner->setObjectName("updateBanner");
    auto updates = new QHBoxLayout(updateBanner); updates->setContentsMargins(16,8,16,8); updates->setSpacing(8);
    updateText = label("", "updateText"); updates->addWidget(updateText,1);
    installUpdate = button("Jetzt installieren", "primaryButton"); laterUpdate = button("Später", "subtleButton");
    updates->addWidget(installUpdate); updates->addWidget(laterUpdate);
    connect(installUpdate,&QPushButton::clicked,&updater,&UpdateService::install);
    connect(laterUpdate,&QPushButton::clicked,&updater,&UpdateService::postpone);
    layout->addWidget(updateBanner); updateBanner->hide();

    stage = new QStackedWidget; stage->setObjectName("stage");
    video = new QWidget; video->setObjectName("videoSurface");
    video->setAttribute(Qt::WA_NativeWindow); video->setAttribute(Qt::WA_DontCreateNativeAncestors);
    video->setAttribute(Qt::WA_NoSystemBackground); video->setMouseTracking(true); video->installEventFilter(this);
    idlePage = new QWidget; idlePage->setObjectName("idlePage");
    auto idle = new QVBoxLayout(idlePage); idle->setContentsMargins(32,16,32,32); idle->setSpacing(16);
    header = new QWidget; auto chrome = new QHBoxLayout(header); chrome->setContentsMargins(0,0,0,0); chrome->addStretch();
    auto settingsButton = Ui::iconButton(Ui::Symbol::Settings, "Einstellungen"); chrome->addWidget(settingsButton);
    connect(settingsButton,&QPushButton::clicked,this,&MainWindow::showSettings); idle->addWidget(header);
    idle->addStretch(1);
    auto mark = new QLabel; mark->setPixmap(Ui::icon(Ui::Symbol::Mirror,QColor("#A0A0A0")).pixmap(48,48));
    mark->setAlignment(Qt::AlignCenter); idle->addWidget(mark);
    deviceName = label("", "instruction"); deviceName->setAlignment(Qt::AlignCenter); idle->addWidget(deviceName);
    status = label("Startet", "status"); status->setAlignment(Qt::AlignCenter); idle->addWidget(status);
    pinLabel = label("", "pin"); pinLabel->setAlignment(Qt::AlignCenter); idle->addWidget(pinLabel); pinLabel->hide();
    idle->addStretch(1); idle->addSpacing(40);
    stage->addWidget(idlePage); stage->addWidget(video); layout->addWidget(stage,1);
    notice = label(synthetic ? "Synthetischer Bildtest" : "", "notice"); notice->setAlignment(Qt::AlignCenter);
    layout->addWidget(notice); notice->setVisible(synthetic);

    // An owned native tool window stays above the cross-process D3D child.
    // Showing/hiding it never resizes or restarts the video pipeline.
    toolbar = new QFrame(this, Qt::Tool | Qt::FramelessWindowHint | Qt::WindowDoesNotAcceptFocus);
    toolbar->setObjectName("playbackToolbar"); toolbar->setAttribute(Qt::WA_ShowWithoutActivating);
    auto controls = new QHBoxLayout(toolbar); controls->setContentsMargins(8,8,8,8); controls->setSpacing(8);
    auto full = Ui::iconButton(Ui::Symbol::Fullscreen,"Vollbild (F11)");
    shotButton = Ui::iconButton(Ui::Symbol::Screenshot,"Screenshot speichern");
    muteButton = Ui::iconButton(Ui::Symbol::Volume,"Ton an/aus"); muteButton->setCheckable(true); muteButton->setChecked(settings.muted);
    disconnectButton = Ui::iconButton(Ui::Symbol::Disconnect,"Verbindung trennen");
    controls->addWidget(full); controls->addWidget(shotButton); controls->addWidget(muteButton); controls->addWidget(disconnectButton);
    toolbar->adjustSize(); toolbar->hide();
    connect(full,&QPushButton::clicked,this,&MainWindow::toggleFullscreen);
    connect(shotButton,&QPushButton::clicked,this,&MainWindow::screenshot);
    connect(disconnectButton,&QPushButton::clicked,this,&MainWindow::restart);
    auto refreshMute = [this] {
        muteButton->setIcon(Ui::icon(settings.muted ? Ui::Symbol::Muted : Ui::Symbol::Volume));
        I18n::bind(muteButton,"toolTip",settings.muted ? "Ton einschalten" : "Ton ausschalten");
    };
    connect(muteButton,&QPushButton::toggled,this,[this,refreshMute](bool checked) {
        settings.muted = checked; receiver.mute(checked); refreshMute(); savePreferences();
    }); refreshMute();
    connect(new QShortcut(QKeySequence(Qt::Key_F11),this),&QShortcut::activated,this,&MainWindow::toggleFullscreen);
    connect(new QShortcut(QKeySequence(Qt::Key_Escape),this),&QShortcut::activated,this,[this] { if(isFullScreen()) showNormal(); });
    connect(new QShortcut(QKeySequence("Ctrl+,"),this),&QShortcut::activated,this,&MainWindow::showSettings);

    connect(&receiver,&ReceiverBackend::ready,this,[this] { setStatus("Bereit", "#C1A55B"); });
    connect(&receiver,&ReceiverBackend::frame,this,[this](int width,int height) {
        setStreaming(true); pinLabel->hide(); setStatus("Verbunden", "#6FA77B");
        diagnostics.log(QString("Videoframe empfangen: %1 × %2").arg(width).arg(height));
    });
    connect(&receiver,&ReceiverBackend::idle,this,[this] { setStreaming(false); setStatus("Bereit für eine neue Verbindung", "#C1A55B"); });
    connect(&receiver,&ReceiverBackend::stopped,this,[this] {
        setStreaming(false);
        if (preparingUpdate) { updater.applyAfterReceiverStopped(); return; }
        if(restarting && !closing) { restarting=false; QTimer::singleShot(300,this,&MainWindow::launch); }
    });
    connect(&receiver,&ReceiverBackend::pinRequired,this,[this](const QString &pin) {
        setStreaming(false); I18n::bind(pinLabel,"text","Code: %1",{pin}); pinLabel->show(); setStatus("PIN am Gerät eingeben", "#C1A55B");
    });
    connect(&receiver,&ReceiverBackend::snapshotFinished,this,&MainWindow::finishScreenshot);
    connect(&receiver,&ReceiverBackend::error,this,&MainWindow::showError);
    connect(&receiver,&ReceiverBackend::diagnostic,&diagnostics,&Diagnostics::log);
    connect(&diagnostics,&Diagnostics::lineAdded,this,[this](const QString &line) { if(logView) logView->appendPlainText(line); });
    connect(&diagnostics,&Diagnostics::firewallResult,this,[this](bool ok,const QString &message) {
        if(!ok && !synthetic && !preview) { firewallBanner=true; I18n::bind(bannerText,"text",message); banner->show(); }
        else if(ok && firewallBanner) { firewallBanner=false; banner->hide(); }
    });
    connect(&updater,&UpdateService::changed,this,&MainWindow::updateNotice);
    connect(&updater,&UpdateService::diagnostic,&diagnostics,&Diagnostics::log);
    connect(&updater,&UpdateService::prepareInstall,this,[this] {
        if(streaming) return;
        preparingUpdate=true; restarting=false; receiver.stop();
    });
    connect(&updater,&UpdateService::quitForUpdate,qApp,&QApplication::quit);

    snapshotTimer.setSingleShot(true); snapshotTimer.setInterval(6000);
    connect(&snapshotTimer,&QTimer::timeout,this,[this] { finishScreenshot(false); });
    hideTimer.setInterval(150); inactivity.start(); previousCursor=QCursor::pos();
    connect(&hideTimer,&QTimer::timeout,this,[this] {
        const QPoint cursor=QCursor::pos();
        if(cursor!=previousCursor && frameGeometry().contains(cursor)) revealControls();
        previousCursor=cursor;
        if(streaming) positionToolbar();
        bool overControls=toolbar->isVisible() && toolbar->frameGeometry().contains(cursor);
        if(streaming && inactivity.elapsed()>2600 && !overControls && !QApplication::activeModalWidget()) toolbar->hide();
    });
    hideTimer.start(); refreshInstructions(); setStreaming(false);
    QSettings state(Settings::dataDirectory()+"/settings.ini",QSettings::IniFormat);
    if(!preview) restoreGeometry(state.value("window/geometry").toByteArray());
    Ui::nativeWindow(this);
    QTimer::singleShot(0,this,[this] {
        if(settings.topmost) SetWindowPos((HWND)winId(),HWND_TOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);
        if(preview) { setStatus("Bereit", "#C1A55B"); return; }
        launch(); if(!synthetic) { diagnostics.checkFirewall(); updater.start(settings.automaticUpdates); }
    });
}

void MainWindow::refreshInstructions() { I18n::bind(deviceName,"text","Kontrollzentrum → Bildschirmsynchronisierung → %1",{settings.name}); }
void MainWindow::setStatus(const QString &text,const QString &color) { I18n::bind(status,"text",text); status->setStyleSheet("color:"+color); }
void MainWindow::setStreaming(bool value) {
    streaming=value; stage->setCurrentWidget(value ? video : idlePage); updater.setStreaming(value);
    shotButton->setEnabled(value); disconnectButton->setEnabled(value);
    if(value) revealControls(); else toolbar->hide();
}
void MainWindow::positionToolbar() {
    const QPoint bottom=mapToGlobal(QPoint(width()/2,height()-24));
    toolbar->move(bottom.x()-toolbar->width()/2,bottom.y()-toolbar->height());
    if(isMinimized() || !isVisible()) toolbar->hide();
}
void MainWindow::revealControls() {
    inactivity.restart();
    if(streaming && !isMinimized() && isVisible()) { positionToolbar(); toolbar->show(); toolbar->raise(); }
}
void MainWindow::launch() {
    if(preparingUpdate || preview) return;
    setStatus("Startet", "#A0A0A0"); receiver.start(settings,quintptr(video->winId()),synthetic);
}
void MainWindow::restart() {
    if(restarting || preparingUpdate) return;
    banner->hide(); pinLabel->hide(); restarting=true; setStatus("Startet neu", "#A0A0A0"); receiver.stop();
}
void MainWindow::showError(const QString &message,const QString &details) {
    firewallBanner=false; diagnostics.log("FEHLER: "+message+'\n'+details);
    I18n::bind(bannerText,"text",message); banner->show(); setStatus("Fehler", "#C87878");
}
void MainWindow::toggleFullscreen() { if(isFullScreen()) showNormal(); else showFullScreen(); revealControls(); }
bool MainWindow::eventFilter(QObject *object,QEvent *event) {
    if(object==video && event->type()==QEvent::MouseButtonDblClick) { toggleFullscreen(); return true; }
    if(event->type()==QEvent::MouseMove) revealControls();
    return QMainWindow::eventFilter(object,event);
}
void MainWindow::savePreferences() { QString error; if(!settings.save(&error)) showError(error,"Einstellungen speichern"); }
void MainWindow::closeEvent(QCloseEvent *event) {
    closing=true; restarting=false; toolbar->hide();
    if(!preview) { QSettings state(Settings::dataDirectory()+"/settings.ini",QSettings::IniFormat); state.setValue("window/geometry",saveGeometry()); }
    receiver.stop(); event->accept();
}
void MainWindow::updateNotice() {
    updateText->setText(updater.message()); installUpdate->setEnabled(updater.canInstall()); laterUpdate->setEnabled(!updater.applying()); updateBanner->setVisible(updater.visible());
    if(!updater.applying() && preparingUpdate) { preparingUpdate=false; if(!closing) launch(); }
}
void MainWindow::showSettings() {
    SettingsDialog dialog(settings,updater,this); dialog.openDiagnostics=[this] { showDiagnostics(); };
    if(dialog.exec()!=QDialog::Accepted) return;
    auto next=dialog.result; QString error;
    if(!next.save(&error)) { I18n::setLanguage(settings.language); showError(error,"Einstellungen speichern"); return; }
    bool receiverChanged=next.name!=settings.name || next.quality!=settings.quality || next.autostart!=settings.autostart
        || next.pinEnabled!=settings.pinEnabled || next.pin!=settings.pin || next.softwareDecoder!=settings.softwareDecoder;
    settings=next; I18n::setLanguage(settings.language); refreshInstructions(); updater.setAutomatic(settings.automaticUpdates);
    if (!preview) receiver.setIpadFill(settings.ipadFill);
    SetWindowPos((HWND)winId(),settings.topmost ? HWND_TOPMOST : HWND_NOTOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);
    if(receiverChanged && !preview) restart();
}
void MainWindow::screenshot() {
    if (!streaming || snapshotTimer.isActive()) return;
    QString folder = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    QString file = QFileDialog::getSaveFileName(this, I18n::tr("Screenshot speichern"), folder + "/PC-Mirror-" + QDateTime::currentDateTime().toString("yyyyMMdd-HHmmss") + ".png", I18n::tr("PNG-Bild (*.png)"));
    if (file.isEmpty()) return;
    if (!file.endsWith(".png", Qt::CaseInsensitive)) file += ".png";
    snapshotTarget = file;
    snapshotTemporary = QDir::tempPath() + "/pc-mirror-" + QUuid::createUuid().toString(QUuid::WithoutBraces) + ".ppm";
    shotButton->setEnabled(false); snapshotTimer.start(); receiver.snapshot(snapshotTemporary);
}
void MainWindow::finishScreenshot(bool ok) {
    if (snapshotTemporary.isEmpty()) return;
    snapshotTimer.stop();
    QImage image(snapshotTemporary);
    if (!ok || image.isNull() || !image.save(snapshotTarget, "PNG")) showError("Der Screenshot konnte nicht gespeichert werden. Bitte Bildempfang und Schreibrechte prüfen.", snapshotTarget);
    else {
        diagnostics.log("Screenshot gespeichert: " + snapshotTarget);
        I18n::bind(notice,"text","Screenshot gespeichert"); notice->show();
        QTimer::singleShot(3000, this, [this] { if (!synthetic) notice->hide(); });
    }
    QFile::remove(snapshotTemporary); snapshotTemporary.clear(); snapshotTarget.clear(); shotButton->setEnabled(streaming);
}


void MainWindow::showDiagnostics() {
    revealControls(); QDialog dialog(this); I18n::bind(&dialog,"windowTitle","Diagnose & Hilfe · PC-Mirror"); dialog.resize(800, 640);
    auto layout = new QVBoxLayout(&dialog); layout->setContentsMargins(24,24,24,24); layout->setSpacing(16);
    layout->addWidget(label(diagnostics.networkSummary(), "secondary"));
    layout->addWidget(label("mDNS: in UxPlay integriert · Bonjour ist nicht erforderlich\nAirPlay: TCP/UDP 48000–48002 · Discovery: UDP 5353\nFirewall-Freigaben gelten nur im privaten lokalen Netz.", "secondary"));
    auto actions = new QHBoxLayout;
    auto firewall = button("Firewall einrichten", "primaryButton"); auto recheck = button("Erneut prüfen", "toolButton"); auto logs = button("Logordner öffnen", "toolButton");
    actions->addWidget(firewall); actions->addWidget(recheck); actions->addWidget(logs); actions->addStretch(); layout->addLayout(actions);
    connect(firewall, &QPushButton::clicked, &diagnostics, &Diagnostics::configureFirewall);
    connect(recheck, &QPushButton::clicked, &diagnostics, &Diagnostics::checkFirewall);
    connect(logs, &QPushButton::clicked, this, [this] {
        if (!QDesktopServices::openUrl(QUrl::fromLocalFile(QFileInfo(diagnostics.logPath()).absolutePath())))
            showError("Der Logordner konnte nicht geöffnet werden.", diagnostics.logPath());
    });
    auto firewallLabel = label("Windows fragt zum Einrichten nach Administratorrechten.", "secondary"); layout->addWidget(firewallLabel);
    connect(&diagnostics, &Diagnostics::firewallResult, firewallLabel, [firewallLabel](bool, const QString &text) { I18n::bind(firewallLabel,"text",text); });
    layout->addWidget(label("Nicht sichtbar: gleiches Netz, Profil Privat, VPN/Gastnetz/AP-Isolation prüfen.\nSchwarzes Bild: Home-Bildschirm testen; geschützte Inhalte können gesperrt sein.\nKein Ton: Windows-Ausgabe und Lautstärkemixer prüfen.\nRuckeln: 720p/30 FPS wählen, PC per Ethernet anbinden.\nMehr Hilfe: README im Installationsordner.", "secondary"));
    logView = new QPlainTextEdit; logView->setReadOnly(true); logView->setMaximumBlockCount(500);
    QFile current(diagnostics.logPath()); if (current.open(QIODevice::ReadOnly)) { if (current.size() > 40000) current.seek(current.size() - 40000); logView->setPlainText(QString::fromUtf8(current.readAll())); }
    layout->addWidget(logView, 1);
    auto close = new QDialogButtonBox(QDialogButtonBox::Close); I18n::bind(close->button(QDialogButtonBox::Close),"text","Schließen"); connect(close, &QDialogButtonBox::rejected, &dialog, &QDialog::reject); layout->addWidget(close);
    Ui::nativeWindow(&dialog); diagnostics.checkFirewall(); dialog.exec(); logView = nullptr;
}


void MainWindow::changeEvent(QEvent *event) {
    QMainWindow::changeEvent(event);
    if (event->type() == QEvent::LanguageChange && deviceName) {
        I18n::refresh(this); refreshInstructions(); updateNotice();
    }
}
