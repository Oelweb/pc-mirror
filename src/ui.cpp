// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui.h"
#include "i18n.h"
#include <QPainter>
#include <QPainterPath>
#include <QShowEvent>
#include <windows.h>
#include <dwmapi.h>

namespace Ui {
QIcon icon(Symbol symbol, const QColor &color) {
    QPixmap pixmap(48, 48); pixmap.fill(Qt::transparent);
    QPainter p(&pixmap); p.setRenderHint(QPainter::Antialiasing); p.scale(2, 2);
    p.setPen(QPen(color, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    p.setBrush(Qt::NoBrush);
    switch (symbol) {
    case Symbol::Mirror:
        p.drawRoundedRect(QRectF(3,4,13,11),2,2); p.drawRoundedRect(QRectF(8,9,13,11),2,2); break;
    case Symbol::Fullscreen:
        p.drawPolyline(QPolygonF{QPointF(4,9),QPointF(4,4),QPointF(9,4)});
        p.drawPolyline(QPolygonF{QPointF(15,4),QPointF(20,4),QPointF(20,9)});
        p.drawPolyline(QPolygonF{QPointF(20,15),QPointF(20,20),QPointF(15,20)});
        p.drawPolyline(QPolygonF{QPointF(9,20),QPointF(4,20),QPointF(4,15)}); break;
    case Symbol::Screenshot:
        p.drawRoundedRect(QRectF(3,7,18,13),2,2); p.drawEllipse(QPointF(12,13.5),3.5,3.5);
        p.drawPolyline(QPolygonF{QPointF(8,7),QPointF(9,4),QPointF(15,4),QPointF(16,7)}); break;
    case Symbol::Volume: case Symbol::Muted:
        p.drawPolygon(QPolygonF{QPointF(3,9),QPointF(7,9),QPointF(12,5),QPointF(12,19),QPointF(7,15),QPointF(3,15)});
        if(symbol == Symbol::Muted) { p.drawLine(QPointF(16,9),QPointF(21,15)); p.drawLine(QPointF(21,9),QPointF(16,15)); }
        else { p.drawArc(QRectF(12,7,6,10),-70*16,140*16); p.drawArc(QRectF(11,3,12,18),-60*16,120*16); } break;
    case Symbol::Disconnect: p.drawRoundedRect(QRectF(6,6,12,12),2,2); break;
    case Symbol::Settings:
        p.drawEllipse(QPointF(12,12),6.5,6.5); p.drawEllipse(QPointF(12,12),2.5,2.5);
        for(int i=0;i<8;++i) { p.save(); p.translate(12,12); p.rotate(i*45); p.drawLine(QPointF(0,-6.5),QPointF(0,-9)); p.restore(); } break;
    case Symbol::Help:
        p.drawEllipse(QPointF(12,12),9,9); p.drawPoint(QPointF(12,7)); p.drawLine(QPointF(12,11),QPointF(12,17)); break;
    case Symbol::Close: p.drawLine(6,6,18,18); p.drawLine(18,6,6,18); break;
    }
    return QIcon(pixmap);
}
QIcon appIcon() {
    QPixmap image(64,64); image.fill(Qt::transparent);
    QPainter p(&image); p.setRenderHint(QPainter::Antialiasing); p.setPen(Qt::NoPen);
    p.setBrush(QColor("#2F7BFF")); p.drawRoundedRect(QRectF(0,0,64,64),8,8);
    icon(Symbol::Mirror).paint(&p, QRect(8,8,48,48)); return QIcon(image);
}
QPushButton *iconButton(Symbol symbol, const QString &name, QWidget *parent) {
    auto button = new QPushButton(parent); button->setIcon(icon(symbol)); button->setIconSize(QSize(20,20));
    button->setObjectName("iconButton"); I18n::bind(button,"toolTip",name); I18n::bind(button,"accessibleName",name);
    button->setFixedSize(40,40); button->setCursor(Qt::PointingHandCursor); return button;
}
void nativeWindow(QWidget *window) {
    const HWND handle = (HWND)window->winId();
    BOOL dark = TRUE; DwmSetWindowAttribute(handle, 20, &dark, sizeof(dark));
    COLORREF caption = RGB(28,28,28), text = RGB(255,255,255);
    DwmSetWindowAttribute(handle, 35, &caption, sizeof(caption));
    DwmSetWindowAttribute(handle, 36, &text, sizeof(text));
    DWORD corners = 2, backdrop = 2;
    DwmSetWindowAttribute(handle, 33, &corners, sizeof(corners));
    DwmSetWindowAttribute(handle, 38, &backdrop, sizeof(backdrop));
}
Switch::Switch(const QString &name, QWidget *parent) : QCheckBox(parent), animation(this) {
    I18n::bind(this,"accessibleName",name); setFixedSize(40,24); setCursor(Qt::PointingHandCursor);
    animation.setDuration(150);
    animation.setEasingCurve(QEasingCurve::InOutCubic);
    connect(&animation, &QVariantAnimation::valueChanged, this, [this](const QVariant &value) {
        position = value.toReal(); update();
    });
    connect(this, &QCheckBox::toggled, this, [this](bool checked) {
        animation.stop();
        const qreal target = checked ? 1.0 : 0.0;
        BOOL animationsEnabled = TRUE;
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animationsEnabled, 0);
        if (!isVisible() || !animationsEnabled) { position = target; update(); return; }
        animation.setStartValue(position);
        animation.setEndValue(target);
        animation.start();
    });
}
void Switch::showEvent(QShowEvent *event) {
    animation.stop(); position = isChecked() ? 1.0 : 0.0;
    QCheckBox::showEvent(event);
}
void Switch::paintEvent(QPaintEvent *) {
    QPainter p(this); p.setRenderHint(QPainter::Antialiasing);
    const QColor off("#333333"), on("#2F7BFF");
    const QColor track = QColor::fromRgbF(
        off.redF() + (on.redF() - off.redF()) * position,
        off.greenF() + (on.greenF() - off.greenF()) * position,
        off.blueF() + (on.blueF() - off.blueF()) * position);
    p.setPen(QPen(QColor("#333333"),1)); p.setBrush(track);
    p.drawRoundedRect(QRectF(.5,2.5,39,19),8,8);
    p.setPen(Qt::NoPen); p.setBrush(QColor(isEnabled() ? "#FFFFFF" : "#A0A0A0"));
    p.drawEllipse(QRectF(3 + 20 * position,5,14,14));
}
}
