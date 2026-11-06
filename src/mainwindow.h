// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "receiver.h"
#include "diagnostics.h"
#include "updater.h"
#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QStackedWidget>
#include <QPlainTextEdit>
#include <QElapsedTimer>

class MainWindow final : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(bool testPattern = false, bool preview = false);
protected:
    bool eventFilter(QObject *, QEvent *) override;
    void closeEvent(QCloseEvent *) override;
    void changeEvent(QEvent *) override;
private:
    void launch();
    void restart();
    void showSettings();
    void showDiagnostics();
    void showError(const QString &, const QString &);
    void setStatus(const QString &, const QString &color);
    void setStreaming(bool);
    void revealControls();
    void toggleFullscreen();
    void screenshot();
    void finishScreenshot(bool);
    void savePreferences();
    void refreshInstructions();
    void updateNotice();
    void positionToolbar();
    Settings settings;
    Diagnostics diagnostics;
    UxPlayReceiver receiver;
    UpdateService updater;
    QWidget *header = nullptr, *toolbar = nullptr, *video = nullptr, *idlePage = nullptr, *banner = nullptr;
    QStackedWidget *stage = nullptr;
    QLabel *status = nullptr, *deviceName = nullptr, *bannerText = nullptr, *pinLabel = nullptr, *notice = nullptr;
    QPushButton *muteButton = nullptr, *topButton = nullptr, *shotButton = nullptr, *disconnectButton = nullptr;
    QPlainTextEdit *logView = nullptr;
    QWidget *updateBanner = nullptr;
    QLabel *updateText = nullptr;
    QPushButton *installUpdate = nullptr, *laterUpdate = nullptr;
    QTimer hideTimer, snapshotTimer;
    QElapsedTimer inactivity;
    QPoint previousCursor;
    QString snapshotTarget, snapshotTemporary;
    bool streaming = false, restarting = false, closing = false, synthetic = false, firewallBanner = false;
    bool preview = false, preparingUpdate = false;
};
