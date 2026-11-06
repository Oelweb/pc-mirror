// SPDX-License-Identifier: GPL-3.0-or-later
#include "i18n.h"
#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocale>
#include <QTranslator>
#include <QVariant>
#include <memory>

namespace {
class EnglishTranslator final : public QTranslator {
public:
    EnglishTranslator() {
        QFile file(":/translations/en.json");
        if (file.open(QIODevice::ReadOnly)) entries = QJsonDocument::fromJson(file.readAll()).object();
    }
    bool isEmpty() const override { return entries.isEmpty(); }
    QString translate(const char *context, const char *source, const char *, int) const override {
        if (qstrcmp(context, "PCMirror")) return {};
        const auto entry = entries.value(QString::fromUtf8(source));
        return entry.isString() ? entry.toString() : QString();
    }
private:
    QJsonObject entries;
};
std::unique_ptr<EnglishTranslator> english;
QString selected = "de";
constexpr char prefix[] = "_pcmirror_text_";
void apply(QObject *object, const QByteArray &property) {
    QString translated = I18n::tr(object->property((prefix + property).constData()).toString());
    const auto arguments = object->property(("_pcmirror_args_" + property).constData()).toStringList();
    for (const auto &argument : arguments) translated = translated.arg(argument);
    object->setProperty(property.constData(), translated);
}
}
QString I18n::tr(const QString &source) {
    return QCoreApplication::translate("PCMirror", source.toUtf8().constData());
}
QString I18n::language() { return selected; }
bool I18n::setLanguage(const QString &value) {
    const auto next = value == "en" ? QString("en") : QString("de");
    if (next == selected) return true;
    if (next == "en") {
        if (!english) english = std::make_unique<EnglishTranslator>();
        if (english->isEmpty()) return false;
        selected = next;
        QCoreApplication::installTranslator(english.get());
    } else {
        selected = next;
        QCoreApplication::removeTranslator(english.get());
    }
    QLocale::setDefault(QLocale(next));
    return true;
}
void I18n::bind(QObject *object, const char *property, const QString &source, const QStringList &arguments) {
    const QByteArray name(property);
    object->setProperty((prefix + name).constData(), source);
    object->setProperty(("_pcmirror_args_" + name).constData(), arguments);
    apply(object, name);
}
void I18n::refresh(QObject *root) {
    for (const auto &property : root->dynamicPropertyNames()) {
        if (property.startsWith(prefix)) apply(root, property.mid(sizeof(prefix)-1));
    }
    for (auto child : root->children()) refresh(child);
}
