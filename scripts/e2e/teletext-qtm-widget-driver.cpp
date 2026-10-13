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

// Environment options only opt this test driver into the native synthetic
// single-page trial. Basenames are confined to /work and never read as code.
struct DriverContext {
    bool singlePage;
    QByteArray inputName;
    QByteArray outputName;
    QString inputPath;
    QString outputPath;
    QByteArray drcsName;
    int leftPanelColumns;
    int rightPanelColumns;
    QString captureName(const char *stage) const {
        if (!singlePage) return QString("qtm-driver-") + stage + ".png";
        return QString::fromLatin1(outputName.chopped(4)) + "-qtm-" + stage + ".png";
    }
};
static bool safeTtiBasename(const QByteArray &name) {
    if (name.isEmpty() || name.size() > 96 || !name.endsWith(".tti")
        || name.startsWith('.') || name.contains("..")) return false;
    for (char value : name)
        if (!((value >= 'A' && value <= 'Z') || (value >= 'a' && value <= 'z')
            || (value >= '0' && value <= '9') || value == '-' || value == '_' || value == '.'))
            return false;
    return true;
}
// Effective Level 2.5 widths come from the native original's typed display
// geometry in ready.json, not from the edited TTI or an uploaded sidecar.
static int panelColumns(const char *name) {
    const QByteArray supplied = qgetenv(name);
    bool valid = false;
    const int value = supplied.toInt(&valid, 10);
    if (!valid || value < 0 || value > 16 || supplied != QByteArray::number(value))
        qFatal("Explicit native Level 2.5 panel count must be canonical decimal 0..16");
    return value;
}
static DriverContext driverContext() {
    const QByteArray mode = qgetenv("ERSATZRS_QTM_DRIVER_MODE");
    const QByteArray input = qgetenv("ERSATZRS_QTM_INPUT_BASENAME");
    const QByteArray output = qgetenv("ERSATZRS_QTM_OUTPUT_BASENAME");
    if (mode.isEmpty()) {
        if (!input.isEmpty() || !output.isEmpty()
            || !qgetenv("ERSATZRS_QTM_LEFT_PANEL_COLUMNS").isEmpty()
            || !qgetenv("ERSATZRS_QTM_RIGHT_PANEL_COLUMNS").isEmpty()
            || !qgetenv("ERSATZRS_QTM_NORMAL_DRCS_BASENAME").isEmpty())
            qFatal("Native basenames/panels require explicit single-page driver mode");
        return {false, "synthetic-editor-audit.tti", "edited-editor-audit.tti",
            "/work/synthetic-editor-audit.tti", "/work/edited-editor-audit.tti", {}, 0, 0};
    }
    if (mode != "single-page-zone-q" || !safeTtiBasename(input) || !safeTtiBasename(output)
        || input == output) qFatal("Invalid explicit synthetic single-page driver context");
    const int left = panelColumns("ERSATZRS_QTM_LEFT_PANEL_COLUMNS");
    const int right = panelColumns("ERSATZRS_QTM_RIGHT_PANEL_COLUMNS");
    const QByteArray drcs = qgetenv("ERSATZRS_QTM_NORMAL_DRCS_BASENAME");
    if (!drcs.isEmpty() && (!safeTtiBasename(drcs) || drcs == input || drcs == output))
        qFatal("Invalid explicit protected Normal DRCS companion name");
    if (left + right > 16) qFatal("Native Level 2.5 panel counts exceed sixteen columns");
    return {true, input, output, QString("/work/") + QString::fromLatin1(input),
        QString("/work/") + QString::fromLatin1(output), drcs, left, right};
}
static void loadNormalDrcs(QMainWindow *main, const DriverContext &context) {
    if (context.drcsName.isEmpty()) return;
    const QString path = QString("/work/") + QString::fromLatin1(context.drcsName);
    const QFileInfo file(path);
    if (!file.isFile() || file.isSymLink() || file.size() <= 0)
        qFatal("Protected Normal DRCS companion absent");
    QAction *load = nullptr;
    for (QAction *action : main->findChildren<QAction *>())
        if (action->statusTip() == "Load a file to use for Normal DRCS definitions") load = action;
    if (!load) qFatal("Actual Normal DRCS Load file action absent");
    QTimer::singleShot(250, qApp, [main, context, path] {
        for (QWidget *widget : QApplication::topLevelWidgets())
            if (auto dialog = qobject_cast<QFileDialog *>(widget)) {
                if (!dialog->isVisible() || dialog->parentWidget() != main
                    || dialog->acceptMode() != QFileDialog::AcceptOpen
                    || dialog->windowTitle() != "Select Normal DRCS file") continue;
                auto name = dialog->findChild<QLineEdit *>("fileNameEdit");
                if (!name) qFatal("Actual DRCS filename input absent");
                name->selectAll();
                QTest::keyClicks(name, path, Qt::NoModifier, 5);
                QTimer::singleShot(200, qApp, [dialog, context, path] {
                    if (!dialog->grab().save(QString("/work/") + context.captureName("drcs-open-dialog"))
                        || dialog->selectedFiles() != QStringList{path})
                        qFatal("Actual DRCS dialog selection/capture failed");
                    for (QPushButton *button : dialog->findChildren<QPushButton *>()) {
                        QString text = button->text(); text.remove('&');
                        if (text == "Open" && button->isEnabled()) { button->click(); return; }
                    }
                    qFatal("Actual enabled DRCS Open button absent");
                });
                return;
            }
        qFatal("Actual Normal DRCS open dialog absent");
    });
    load->trigger();
    bool attached = false;
    for (QAction *action : main->findChildren<QAction *>())
        if (action->text() == QString("Normal DRCS: ") + QString::fromLatin1(context.drcsName)) attached = true;
    if (!attached) qFatal("Actual Normal DRCS attachment did not complete");
    capture(context.captureName("drcs-loaded").toUtf8().constData());
    // Fixed opt-in synthetic fixture: Normal S1=0/slot0 at Main(1,8),
    // asymmetric literal bitmap, day CLUT2 foreground red/background black.
    // An attachment label alone would also succeed for an empty/wrong file.
    auto view = editor();
    if (!view) qFatal("Actual DRCS editor view absent");
    const QImage bitmap = view->grab().toImage();
    if (bitmap.size() != view->size()) qFatal("Expected one-to-one DRCS pixel capture");
    const unsigned short rows[10] = {0x801, 0x800, 0x400, 0x200, 0x100,
        0x080, 0x040, 0x020, 0x010, 0x009};
    for (int y = 0; y < 10; ++y)
        for (int x = 0; x < 12; ++x) {
            const QColor expected = ((rows[y] >> (11 - x)) & 1) ? QColor(255, 0, 0) : QColor(0, 0, 0);
            if (bitmap.pixelColor((context.leftPanelColumns + 8) * 12 + x, 10 + y) != expected)
                qFatal("Actual attached DRCS pixels/palette differ from synthetic literal");
        }
    std::puts("QTM actual Normal DRCS glyph: all 120 literal bitmap/day palette pixels match");
    std::printf("QTM actual Normal DRCS Load file complete: %s\n", context.drcsName.constData());
}
static void saveAs(QMainWindow *main, const DriverContext &context) {
    QAction *save = nullptr;
    for (QAction *action : main->findChildren<QAction *>()) {
        QString text = action->text(); text.remove('&');
        if (text.startsWith("Save As")) save = action;
    }
    if (!save) qFatal("Save As action absent");
    QTimer::singleShot(250, qApp, [main, context] {
        for (QWidget *widget : QApplication::topLevelWidgets())
            if (auto dialog = qobject_cast<QFileDialog *>(widget)) {
                if (!dialog->isVisible() || dialog->parentWidget() != main
                    || dialog->acceptMode() != QFileDialog::AcceptSave
                    || dialog->windowTitle() != "Save As") continue;
                auto name = dialog->findChild<QLineEdit *>("fileNameEdit");
                if (!name) qFatal("Actual filename input absent");
                name->selectAll();
                QTest::keyClicks(name, context.outputPath, Qt::NoModifier, 5);
                QTimer::singleShot(200, qApp, [dialog, context] {
                    if (!dialog->grab().save(QString("/work/") + context.captureName("save-dialog")))
                        qFatal("Save dialog capture failed");
                    if (dialog->selectedFiles() != QStringList{context.outputPath})
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
    QTimer::singleShot(200, qApp, [context] {
        auto saved = window();
        const QFileInfo savedFile(context.outputPath);
        if (!saved || saved->windowFilePath() != context.outputPath
            || saved->isWindowModified() || !savedFile.isFile()
            || savedFile.isSymLink() || savedFile.size() <= 0)
            qFatal("Actual Save As did not complete");
        capture(context.captureName("saved").toUtf8().constData());
        if (context.singlePage) {
            std::printf("QTM actual single-page widget edit/Save As complete: row=2 column=8 key=Q input=%s output=%s\n",
                context.inputName.constData(), context.outputName.constData());
        } else std::puts("QTM actual widget edit/navigation/Save As complete");
        std::fflush(stdout);
        qApp->quit();
    });
}
extern "C" int _ZN16QCoreApplication4execEv() {
    const DriverContext context = driverContext();
    QTimer::singleShot(500, qApp, [context] {
        auto main = window();
        auto view = editor();
        if (!main || !view) qFatal("Editor widgets absent");
        if (QGuiApplication::platformName() != "xcb"
            || main->windowFilePath() != context.inputPath)
            qFatal("Expected visible X11 synthetic editor context");
        const QFileInfo source(context.inputPath);
        if (!source.isFile() || source.isSymLink()) qFatal("Expected synthetic regular input file");
        const QFileInfo destination(context.outputPath);
        if (destination.exists() || destination.isSymLink())
            qFatal("Audit destination must not exist or be a symlink");
        bool selected = false;
        for (QRadioButton *radio : main->findChildren<QRadioButton *>())
            if (radio->text() == "2.5") { radio->click(); selected = radio->isChecked(); }
        if (!selected) qFatal("Level 2.5 radio absent");
        capture(context.captureName("level25").toUtf8().constData());
        loadNormalDrcs(main, context);
        if (context.singlePage) {
            // cac2167 mainwidget.cpp:240,567-581: x/12 minus the
            // decoder's left count addresses Main; y/10 addresses the row.
            // With panels, changeSize fixes the exact widget dimensions.
            // No-panel ctorresize is not fixed and may retain proxy space.
            const int panelTotal = context.leftPanelColumns + context.rightPanelColumns;
            const QSize expected(480 + panelTotal * 12, 250);
            if (view->width() < expected.width() || view->height() < expected.height()
                || (panelTotal != 0 && view->size() != expected))
                qFatal("Actual widget geometry disagrees with the native Level 2.5 panel counts");
            std::printf("QTM actual widget geometry: width=%d height=%d left=%d right=%d; target row=2 column=8\n",
                view->width(), view->height(), context.leftPanelColumns, context.rightPanelColumns);
            QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier,
                QPoint((context.leftPanelColumns + 8) * 12 + 6, 2 * 10 + 5));
            QTest::keyClicks(view, "Q", Qt::NoModifier, 20);
            QTimer::singleShot(100, qApp, [main, context] {
                capture(context.captureName("edited").toUtf8().constData());
                saveAs(main, context);
            });
            return;
        }
        QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(6, 25));
        QTest::keyClicks(view, "SAVED QTM ROW!", Qt::NoModifier, 20);
        QTimer::singleShot(100, qApp, [view, main, context] {
            capture("qtm-driver-edited.png");
            QTest::keyClick(view, Qt::Key_PageUp);
            QTimer::singleShot(100, qApp, [view, main, context] {
                capture("qtm-driver-next.png");
                QTest::keyClick(view, Qt::Key_PageDown);
                QTimer::singleShot(100, qApp, [main, context] {
                    capture("qtm-driver-return.png");
                    saveAs(main, context);
                });
            });
        });
    });
    auto original = reinterpret_cast<int (*)()>(dlsym(RTLD_NEXT, "_ZN16QCoreApplication4execEv"));
    if (!original) qFatal("Original Qt event loop absent");
    return original();
}
