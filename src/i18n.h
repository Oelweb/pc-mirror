// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QStringList>
namespace I18n {
QString tr(const QString &source);
QString language();
bool setLanguage(const QString &language);
void bind(QObject *object, const char *property, const QString &source, const QStringList &arguments = {});
void refresh(QObject *root);
}
