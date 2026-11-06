// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QCheckBox>
#include <QIcon>
#include <QPushButton>
#include <QVariantAnimation>

namespace Ui {
enum class Symbol { Mirror, Fullscreen, Screenshot, Volume, Muted, Disconnect, Settings, Help, Close };
QIcon icon(Symbol symbol, const QColor &color = QColor("#FFFFFF"));
QIcon appIcon();
QPushButton *iconButton(Symbol symbol, const QString &name, QWidget *parent = nullptr);
void nativeWindow(QWidget *window);
class Switch final : public QCheckBox {
public:
    explicit Switch(const QString &name, QWidget *parent = nullptr);
    QSize sizeHint() const override { return QSize(40, 24); }
protected:
    void paintEvent(QPaintEvent *) override;
    void showEvent(QShowEvent *) override;
    bool hitButton(const QPoint &point) const override { return rect().contains(point); }
private:
    QVariantAnimation animation;
    qreal position = 0.0;
};
}
