// Real Qt widgets, hidden preview: never starts AirPlay or changes autostart.
#include "i18n.h"
#include "mainwindow.h"
#include "settingsdialog.h"
#include <QApplication>
#include <QComboBox>
#include <QLineEdit>
#include <QSettings>
#include <QStyleFactory>
#include <QTemporaryDir>
#include <iostream>
#include <cstdlib>
static void require(bool ok, const char *message) {
    if (!ok) { std::cerr << message << '\n'; std::exit(1); }
}
static QPushButton *button(QWidget &widget, const QString &text) {
    for (auto value : widget.findChildren<QPushButton *>()) if (value->text() == text) return value;
    return nullptr;
}
static bool hasLabel(QWidget &widget, const QString &text) {
    for (auto value : widget.findChildren<QLabel *>()) if (value->text() == text) return true;
    return false;
}
static void events() { QApplication::processEvents(); QApplication::processEvents(); }
int main(int argc, char **argv) {
    QTemporaryDir data; require(data.isValid(), "Temporary data folder");
    qputenv("LOCALAPPDATA",data.path().toUtf8());
    QApplication app(argc,argv); app.setApplicationVersion("0.2.7"); app.setStyle(QStyleFactory::create("Fusion"));
    QSettings saved(Settings::dataDirectory()+"/settings.ini",QSettings::IniFormat);
    saved.setValue("ui/language","en"); saved.sync(); require(Settings::load().language == "en","Stored language load");
    saved.setValue("ui/language","unknown"); saved.sync(); require(Settings::load().language == "de","Invalid language fallback");
    saved.setValue("ui/language","de"); saved.setValue("receiver/name","Einstellungen"); saved.sync();
    MainWindow window(false,true); events();
    auto video = window.findChild<QWidget *>("videoSurface"); require(video,"Video surface"); const auto handle = video->winId();
    auto backend = window.findChild<UxPlayReceiver *>(); require(backend,"Receiver backend");
    Settings initial = Settings::load(); UpdateService updater;
    {
        SettingsDialog dialog(initial,updater,&window); auto language = dialog.findChild<QComboBox *>("languageSelector");
        require(language,"Language selector"); language->setCurrentIndex(1); events();
        require(I18n::language()=="en" && dialog.windowTitle()=="Settings","English live preview");
        require(button(dialog,"Save") && button(dialog,"Cancel"),"English buttons");
        require(hasLabel(dialog,"Language") && hasLabel(dialog,"Device name"),"English setting labels");
        require(dialog.findChild<QComboBox *>("qualitySelector")->itemText(2)=="4K · 60 FPS (experimental)","English quality menu");
        bool nameUnchanged=false;
        for (auto field : dialog.findChildren<QLineEdit *>()) if(field->text()=="Einstellungen") nameUnchanged=true;
        require(nameUnchanged,"User-entered device name must not be translated");
        require(window.findChild<QLabel *>("instruction")->text()=="Control Center → Screen Mirroring → Einstellungen","English instructions with untouched device name");
        backend->idle(); events(); require(window.findChild<QLabel *>("status")->text()=="Ready for a new connection","English idle status");
        backend->frame(1440,1000); events(); require(window.findChild<QLabel *>("status")->text()=="Connected","English connected status");
        language->setCurrentIndex(0); events(); require(window.findChild<QLabel *>("status")->text()=="Verbunden","Connected status retranslates");
        require(video->winId()==handle,"Language changes preserve the video window");
        language->setCurrentIndex(1); events();
        backend->error("uxplay.exe fehlt. Bitte die App vollständig neu installieren.","technical details");
        require(window.findChild<QLabel *>("errorText")->text()=="uxplay.exe is missing. Reinstall the complete app.","English receiver error");
        require(I18n::tr("Update wird installiert …")=="Installing update …","English update status");
        dialog.reject(); events(); require(I18n::language()=="de","Cancel restores original language");
        require(window.findChild<QLabel *>("errorText")->text()=="uxplay.exe fehlt. Bitte die App vollständig neu installieren.","Error retranslates on cancel");
    }
    {
        SettingsDialog dialog(initial,updater,&window); dialog.findChild<QComboBox *>("languageSelector")->setCurrentIndex(1); events();
        QLineEdit *name=nullptr;
        for (auto field:dialog.findChildren<QLineEdit *>()) if(field->text()=="Einstellungen") name=field;
        require(name,"Device name field"); name->clear(); button(dialog,"Save")->click();
        require(hasLabel(dialog,"Enter a device name without periods, slashes or control characters (maximum 48 UTF-8 bytes)."),"English validation");
        name->setText("My iPad"); button(dialog,"Save")->click();
        require(static_cast<QDialog &>(dialog).result()==QDialog::Accepted && dialog.result.language=="en","Saving selects persisted language");
    }
    I18n::setLanguage("de"); events();
    std::cout << "PASS language persistence loading, live EN/DE UI, cancel/save, menus, validation, errors, update text and stable video HWND\n";
}
