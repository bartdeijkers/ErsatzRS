// Opt-in Linux/X11 calibration input for pinned QTeletextMaker 1.0.
// The external editor binary, decoder and TTI writer remain unchanged. This
// test-only preload drives actual widgets; it grants no ErsatzRS publication.
// Other platforms retain separate editor/client acceptance requirements.
#include <QApplication>
#include <QAction>
#include <QFileDialog>
#include <QFileInfo>
#include <QMainWindow>
#include <QRadioButton>
#include <QLineEdit>
#include <QPushButton>
#include <QTimer>
#include <QWindow>
#include <QtTest/QTest>
#include <dlfcn.h>
#include <cstdio>

static QWidget *editor() {
    for (QWidget *widget : QApplication::allWidgets())
        if (QByteArray(widget->metaObject()->className()) == "TeletextWidget") return widget;
    return nullptr;
}
static QMainWindow *window() {
    for (QWidget *widget : QApplication::topLevelWidgets())
        if (auto main = qobject_cast<QMainWindow *>(widget)) return main;
    return nullptr;
}
static void capture(const char *name) {
    if (auto main = window()) {
        if (!main->isVisible() || !main->windowHandle() || !main->windowHandle()->isExposed()
            || !main->grab().save(QString("/work/") + name))
            qFatal("Visible editor capture failed");
    } else qFatal("Editor window absent");
}
extern "C" int _ZN16QCoreApplication4execEv() {
    QTimer::singleShot(500, qApp, [] {
        auto main = window();
        auto view = editor();
        if (!main || !view) qFatal("Editor widgets absent");
        if (QGuiApplication::platformName() != "xcb"
            || main->windowFilePath() != "/work/synthetic-editor-audit.tti")
            qFatal("Expected visible X11 synthetic editor context");
        const QFileInfo destination("/work/edited-editor-audit.tti");
        if (destination.exists() || destination.isSymLink())
            qFatal("Audit destination must not exist or be a symlink");
        bool selected = false;
        for (QRadioButton *radio : main->findChildren<QRadioButton *>())
            if (radio->text() == "2.5") { radio->click(); selected = radio->isChecked(); }
        if (!selected) qFatal("Level 2.5 radio absent");
        capture("qtm-driver-level25.png");
        QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(6, 25));
        QTest::keyClicks(view, "SAVED QTM ROW!", Qt::NoModifier, 20);
        QTimer::singleShot(100, qApp, [view, main] {
            capture("qtm-driver-edited.png");
            QTest::keyClick(view, Qt::Key_PageUp);
            QTimer::singleShot(100, qApp, [view, main] {
                capture("qtm-driver-next.png");
                QTest::keyClick(view, Qt::Key_PageDown);
                QTimer::singleShot(100, qApp, [main] {
                    capture("qtm-driver-return.png");
                    QAction *save = nullptr;
                    for (QAction *action : main->findChildren<QAction *>()) {
                        QString text = action->text(); text.remove('&');
                        if (text.startsWith("Save As")) save = action;
                    }
                    if (!save) qFatal("Save As action absent");
                    QTimer::singleShot(250, qApp, [] {
                        for (QWidget *widget : QApplication::topLevelWidgets())
                            if (auto dialog = qobject_cast<QFileDialog *>(widget)) {
                                if (!dialog->isVisible()) continue;
                                auto name = dialog->findChild<QLineEdit *>("fileNameEdit");
                                if (!name) qFatal("Actual filename input absent");
                                name->selectAll();
                                QTest::keyClicks(name, "/work/edited-editor-audit.tti", Qt::NoModifier, 5);
                                QTimer::singleShot(200, qApp, [dialog] {
                                    if (!dialog->grab().save("/work/qtm-driver-save-dialog.png"))
                                        qFatal("Save dialog capture failed");
                                    if (dialog->selectedFiles() != QStringList{"/work/edited-editor-audit.tti"})
                                        qFatal("Unexpected selected audit destination");
                                    for (QPushButton *button : dialog->findChildren<QPushButton *>()) {
                                        QString text = button->text(); text.remove('&');
                                        if (text == "Save" && button->isEnabled()) { button->click(); return; }
                                    }
                                    qFatal("Actual enabled Save button absent");
                                });
                                return;
                            }
                        qFatal("Actual Save As dialog absent");
                    });
                    save->trigger();
                    QTimer::singleShot(200, qApp, [] {
                        auto saved = window();
                        if (!saved || saved->windowFilePath() != "/work/edited-editor-audit.tti"
                            || saved->isWindowModified())
                            qFatal("Actual Save As did not complete");
                        capture("qtm-driver-saved.png");
                        std::puts("QTM actual widget edit/navigation/Save As complete");
                        std::fflush(stdout);
                        qApp->quit();
                    });
                });
            });
        });
    });
    auto original = reinterpret_cast<int (*)()>(dlsym(RTLD_NEXT, "_ZN16QCoreApplication4execEv"));
    if (!original) qFatal("Original Qt event loop absent");
    return original();
}
