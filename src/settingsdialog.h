// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "settings.h"
#include "updater.h"
#include <QDialog>
#include <functional>

class SettingsDialog final : public QDialog {
public:
    SettingsDialog(const Settings &settings, UpdateService &updater, QWidget *parent = nullptr);
    Settings result;
    std::function<void()> openDiagnostics;
protected:
    void changeEvent(QEvent *) override;
private:
    std::function<void()> refreshUpdates;
};
