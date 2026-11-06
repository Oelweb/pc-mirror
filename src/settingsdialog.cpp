// SPDX-License-Identifier: GPL-3.0-or-later
#include "settingsdialog.h"
#include "ui.h"
#include "i18n.h"
#include <QEvent>
#include <QApplication>
#include <QBoxLayout>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFrame>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
#include <QStyledItemDelegate>
#include <QRegularExpressionValidator>
#include <QScrollArea>

namespace {
QWidget *row(const QString &text, QWidget *control, const QString &hint = {}) {
    auto frame = new QFrame; frame->setObjectName("settingRow"); frame->setMinimumHeight(56);
    auto layout = new QHBoxLayout(frame); layout->setContentsMargins(16, 8, 16, 8); layout->setSpacing(16);
    auto captions = new QVBoxLayout; captions->setSpacing(4);
    auto label = new QLabel; I18n::bind(label,"text",text); label->setBuddy(control); captions->addWidget(label);
    if (!hint.isEmpty()) { auto secondary = new QLabel; I18n::bind(secondary,"text",hint); secondary->setObjectName("secondary"); secondary->setWordWrap(true); captions->addWidget(secondary); }
    layout->addLayout(captions, 1); layout->addWidget(control, 0, Qt::AlignRight|Qt::AlignVCenter);
    return frame;
}
}
SettingsDialog::SettingsDialog(const Settings &settings, UpdateService &updater, QWidget *parent)
    : QDialog(parent), result(settings) {
    I18n::bind(this,"windowTitle","Einstellungen"); setObjectName("settingsWindow"); resize(664, 752); setMinimumSize(600, 560);
    auto root = new QVBoxLayout(this); root->setContentsMargins(24,24,24,24); root->setSpacing(16);
    auto scroll = new QScrollArea; scroll->setWidgetResizable(true); scroll->setFrameShape(QFrame::NoFrame);
    auto content = new QWidget; auto list = new QVBoxLayout(content); list->setContentsMargins(0,0,0,0); list->setSpacing(8);
    auto language = new QComboBox; language->setObjectName("languageSelector"); language->setFixedWidth(256);
    I18n::bind(language,"accessibleName","Sprache");
    auto languageMenu = new QListView(language); languageMenu->setFrameShape(QFrame::NoFrame);
    languageMenu->setUniformItemSizes(true); languageMenu->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    language->setView(languageMenu); language->setItemDelegate(new QStyledItemDelegate(languageMenu));
    language->addItem("Deutsch","de"); language->addItem("English","en");
    language->setCurrentIndex(settings.language == "en" ? 1 : 0);
    list->addWidget(row("Sprache",language));
    const QString originalLanguage = I18n::language();
    connect(language,qOverload<int>(&QComboBox::currentIndexChanged),this,[language] {
        I18n::setLanguage(language->currentData().toString());
    });
    connect(this,&QDialog::finished,this,[originalLanguage](int code) {
        if (code == QDialog::Rejected) I18n::setLanguage(originalLanguage);
    });
    auto name = new QLineEdit(settings.name); I18n::bind(name,"accessibleName","Gerätename"); name->setMaxLength(48); name->setFixedWidth(256);
    list->addWidget(row("Gerätename", name));
    auto quality = new QComboBox; I18n::bind(quality,"accessibleName","Qualität"); quality->setObjectName("qualitySelector"); quality->setFixedWidth(256);
    auto qualityMenu = new QListView(quality);
    qualityMenu->setFrameShape(QFrame::NoFrame);
    qualityMenu->setUniformItemSizes(true);
    qualityMenu->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    quality->setView(qualityMenu);
    quality->setItemDelegate(new QStyledItemDelegate(qualityMenu));
    quality->addItems({"720p · 30 FPS", "1080p · 60 FPS", I18n::tr("4K · 60 FPS (experimentell)")}); quality->setCurrentIndex(settings.quality);
    list->addWidget(row("Qualität", quality));
    auto ipadFill = new Ui::Switch("iPad: Fenster ausfüllen"); ipadFill->setChecked(settings.ipadFill);
    list->addWidget(row("iPad: Fenster ausfüllen", ipadFill,
        "Nur beim iPad. Ohne Balken, dafür können Bildränder fehlen. Aus: ganzes Bild."));
    auto top = new Ui::Switch("Immer im Vordergrund"); top->setChecked(settings.topmost); list->addWidget(row("Immer im Vordergrund", top));
    auto autostart = new Ui::Switch("Mit Windows starten"); autostart->setChecked(settings.autostart); list->addWidget(row("Mit Windows starten", autostart));
    auto pin = new Ui::Switch("PIN-Schutz"); pin->setChecked(settings.pinEnabled); list->addWidget(row("PIN-Schutz", pin));
    auto code = new QLineEdit(settings.pin); I18n::bind(code,"accessibleName","PIN"); code->setFixedWidth(256); code->setMaxLength(4); I18n::bind(code,"placeholderText","Leer = zufälliger Code");
    code->setValidator(new QRegularExpressionValidator(QRegularExpression("[0-9]{0,4}"), code));
    auto pinRow = row("PIN", code); pinRow->setVisible(settings.pinEnabled);
    connect(pin, &QCheckBox::toggled, pinRow, &QWidget::setVisible); list->addWidget(pinRow);
    auto software = new Ui::Switch("Software-Decodierung"); software->setChecked(settings.softwareDecoder); list->addWidget(row("Software-Decodierung", software));
    auto notificationsButton = new QPushButton; notificationsButton->setCheckable(true);
    I18n::bind(notificationsButton,"text","Anleitung anzeigen");
    list->addWidget(row("Mitteilungen beim Spiegeln",notificationsButton,"Am iPhone ein- oder ausschalten."));
    auto notificationsHelp = new QLabel; notificationsHelp->setObjectName("secondary");
    notificationsHelp->setWordWrap(true); notificationsHelp->setContentsMargins(16,8,16,8);
    I18n::bind(notificationsHelp,"text","Am iPhone: Einstellungen → Mitteilungen → Bildschirmfreigabe → Mitteilungen erlauben.\n\nDort Mitteilungen ein- oder ausschalten. Der Menüpunkt hängt von der iOS-Version ab. Fokus und die Mitteilungseinstellungen der jeweiligen App gelten weiterhin.\n\nPC-Mirror kann diese iPhone-Einstellung nicht ändern.");
    notificationsHelp->hide(); list->addWidget(notificationsHelp);
    connect(notificationsButton,&QPushButton::toggled,this,[notificationsButton,notificationsHelp](bool visible) {
        notificationsHelp->setVisible(visible);
        I18n::bind(notificationsButton,"text",visible ? "Anleitung ausblenden" : "Anleitung anzeigen");
    });
    list->addSpacing(8);
    auto check = new QPushButton; I18n::bind(check,"text","Nach Updates suchen"); check->setFixedWidth(208);
    auto versionRow = row("Version " + QCoreApplication::applicationVersion(), check); list->addWidget(versionRow);
    auto automatic = new Ui::Switch("Automatisch nach Updates suchen"); automatic->setChecked(settings.automaticUpdates);
    list->addWidget(row("Automatisch nach Updates suchen", automatic));
    auto updateStatus = new QLabel; updateStatus->setObjectName("secondary"); updateStatus->setWordWrap(true); list->addWidget(updateStatus);
    refreshUpdates = [&updater, check, updateStatus] {
        check->setEnabled(!updater.isBusy()); updateStatus->setText(updater.statusText()); updateStatus->setVisible(!updater.statusText().isEmpty());
    };
    connect(&updater, &UpdateService::changed, this, refreshUpdates); refreshUpdates();
    connect(check, &QPushButton::clicked, this, [&updater] { updater.check(true); });
    list->addStretch(); scroll->setWidget(content); root->addWidget(scroll, 1);
    auto validation = new QLabel; validation->setObjectName("errorText"); validation->setWordWrap(true); validation->hide(); root->addWidget(validation);
    auto footer = new QHBoxLayout; footer->setSpacing(8);
    auto help = Ui::iconButton(Ui::Symbol::Help, "Diagnose"); footer->addWidget(help); footer->addStretch();
    connect(help, &QPushButton::clicked, this, [this] { if (openDiagnostics) openDiagnostics(); });
    auto cancel = new QPushButton; I18n::bind(cancel,"text","Abbrechen"); auto save = new QPushButton; I18n::bind(save,"text","Speichern"); save->setObjectName("primaryButton");
    footer->addWidget(cancel); footer->addWidget(save); root->addLayout(footer);
    connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
    connect(save, &QPushButton::clicked, this, [=] {
        QString candidate = name->text().trimmed(), message;
        if (candidate.isEmpty() || candidate.toUtf8().size() > 48 || candidate.contains(QRegularExpression("[\\x00-\\x1F.\\\\/|]")))
            message = "Bitte einen Gerätenamen ohne Punkte, Schrägstriche oder Steuerzeichen eingeben (maximal 48 UTF-8-Bytes).";
        if (pin->isChecked() && !code->text().isEmpty() && (code->text().size() != 4 || code->text() == "0000"))
            message = "PIN: vier Ziffern von 0001 bis 9999, oder für einen zufälligen Code leer lassen.";
        if (!message.isEmpty()) { I18n::bind(validation,"text",message); validation->show(); return; }
        result.language = language->currentData().toString(); result.name = candidate; result.quality = quality->currentIndex(); result.topmost = top->isChecked();
        result.autostart = autostart->isChecked(); result.pinEnabled = pin->isChecked(); result.pin = code->text();
        result.softwareDecoder = software->isChecked(); result.automaticUpdates = automatic->isChecked();
        result.ipadFill = ipadFill->isChecked();
        accept();
    });
    Ui::nativeWindow(this);
}

void SettingsDialog::changeEvent(QEvent *event) {
    QDialog::changeEvent(event);
    if (event->type() != QEvent::LanguageChange) return;
    I18n::refresh(this);
    if (auto quality = findChild<QComboBox *>("qualitySelector"))
        quality->setItemText(2,I18n::tr("4K · 60 FPS (experimentell)"));
    if (refreshUpdates) refreshUpdates();
}
