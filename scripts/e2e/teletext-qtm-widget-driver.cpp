// Opt-in Linux/X11 calibration input for pinned QTeletextMaker 1.0.
// The external editor binary, decoder and TTI writer remain unchanged. This
// test-only preload drives actual widgets; it grants no ErsatzRS publication.
// Other platforms retain separate editor/client acceptance requirements.
#include <QApplication>
#include <QAction>
#include <QFileDialog>
#include <QFileInfo>
#include <QComboBox>
#include <QCheckBox>
#include <QDockWidget>
#include <QTableView>
#include <QMainWindow>
#include <QMenu>
#include <QRadioButton>
#include <QSpinBox>
#include <QStyle>
#include <QStyleOptionSpinBox>
#include <QStyleOptionButton>
#include <QLineEdit>
#include <QPushButton>
#include <QProcess>
#include <QTimer>
#include <QWindow>
#include <QtTest/QTest>
#include <dlfcn.h>
#include <cstdio>
#include <array>
#include <vector>

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
    bool editZone;
    bool editForeground;
    bool editDrcs;
    bool verifyEditedDrcs;
    bool resourceSet;
    int tripletIndex;
    int tripletColumn;
    int fromColour;
    int toColour;
    int drcsSubpage;
    QByteArray inputName;
    QByteArray outputName;
    QString inputPath;
    QString outputPath;
    QByteArray drcsName;
    int leftPanelColumns;
    int rightPanelColumns;
    bool panelProof = false;
    bool panelNight = false;
    bool editLocalGlyph = false;
    int glyphMode = 0;
    int fromGlyph = 0;
    int toGlyph = 0;
    int toGlyphMode = 0;
    bool editLocalDrcsSlot = false;
    bool editLocalFlash = false;
    bool editLocalCharset = false;
    bool editLocalDrcsSubtable = false;
    bool editLocalDrcsScope = false;
    int drcsGlyph0Index = 0;
    int drcsGlyph1Index = 0;
    QByteArray globalDrcsName;
    bool editLocalGlyphStructure = false;
    bool editLocalMixedGlyphStructure = false;
    bool editLocalStyleStructure = false;
    bool insertResourceGlyph = false;
    bool deleteResourceGlyph = false;
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
static int boundedDecimal(const char *name, int maximum) {
    const QByteArray supplied = qgetenv(name);
    bool valid = false;
    const int value = supplied.toInt(&valid, 10);
    if (!valid || value < 0 || value > maximum || supplied != QByteArray::number(value))
        qFatal("Required foreground operand must be a canonical bounded decimal");
    return value;
}
static DriverContext driverContext() {
    const QByteArray mode = qgetenv("ERSATZRS_QTM_DRIVER_MODE");
    const QByteArray input = qgetenv("ERSATZRS_QTM_INPUT_BASENAME");
    const QByteArray output = qgetenv("ERSATZRS_QTM_OUTPUT_BASENAME");
    const bool foreground = mode == "single-page-foreground";
    const bool glyphStructure = mode == "single-page-local-glyph-structure";
    const bool mixedGlyphStructure = mode == "single-page-local-mixed-glyph-structure";
    const bool styleStructure = mode == "single-page-local-style-structure";
    const bool resourceInsert = mode == "resource-object-g0-insert-copy";
    const bool resourceDelete = mode == "resource-object-g0-delete";
    const bool resourceStructure = resourceInsert || resourceDelete;
    const bool extendedGlyph = mode == "single-page-local-extended-glyph";
    const bool remainingGlyph = mode == "single-page-local-remaining-glyph";
    const bool drcsSlot = mode == "single-page-local-drcs-slot";
    const bool flash = mode == "single-page-local-flash";
    const bool charset = mode == "single-page-local-charset";
    const bool attribute = flash || charset;
    const bool drcsSubtable = mode == "single-page-local-drcs-subtable";
    const bool drcsScope = mode == "single-page-local-drcs-scope";
    const bool drcsSelection = drcsSubtable || drcsScope;
    const bool suppliedDrcsIndices = qEnvironmentVariableIsSet("ERSATZRS_QTM_DRCS_GLYPH0_INDEX")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_DRCS_GLYPH1_INDEX");
    const QByteArray globalDrcs = qgetenv("ERSATZRS_QTM_GLOBAL_DRCS_BASENAME");
    if ((!drcsSelection && !mixedGlyphStructure && (suppliedDrcsIndices
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_GLOBAL_DRCS_BASENAME")))
        || (mixedGlyphStructure && suppliedDrcsIndices))
        qFatal("DRCS selection indices/Global companion require a fixed selection trial");
    const int glyph0 = drcsSelection ? boundedDecimal("ERSATZRS_QTM_DRCS_GLYPH0_INDEX", 207) : 0;
    const int glyph1 = drcsSelection ? boundedDecimal("ERSATZRS_QTM_DRCS_GLYPH1_INDEX", 207) : 0;
    const bool localGlyph = mode == "single-page-local-glyph" || extendedGlyph || remainingGlyph || drcsSlot || attribute || drcsSelection;
    const bool suppliedGlyph = qEnvironmentVariableIsSet("ERSATZRS_QTM_GLYPH_MODE")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_FROM_GLYPH")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE");
    if (!localGlyph && suppliedGlyph)
        qFatal("Glyph operands require explicit single-page-local-glyph mode");
    if (!extendedGlyph && qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE"))
        qFatal("Replacement glyph mode requires the fixed extended glyph trial");
    const int glyphMode = localGlyph ? boundedDecimal("ERSATZRS_QTM_GLYPH_MODE", (extendedGlyph || remainingGlyph || drcsSlot || attribute || drcsSelection) ? 31 : 9) : 0;
    const int toGlyphMode = extendedGlyph && qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE")
        ? boundedDecimal("ERSATZRS_QTM_TO_GLYPH_MODE", 31) : glyphMode;
    const int fromGlyph = localGlyph ? boundedDecimal("ERSATZRS_QTM_FROM_GLYPH", 127) : 0;
    const int toGlyph = localGlyph ? boundedDecimal("ERSATZRS_QTM_TO_GLYPH", 127) : 0;
    if (localGlyph && !extendedGlyph && !remainingGlyph && !drcsSlot && !attribute && !drcsSelection && (glyphMode != 9 && glyphMode != 1))
        qFatal("Fixed local glyph mode must retain raw G0 mode9 or G1 mode1");
    if (localGlyph && fromGlyph == toGlyph && glyphMode == toGlyphMode)
        qFatal("Local glyph trial requires a changed glyph");
    if (localGlyph && !extendedGlyph && !remainingGlyph && !drcsSlot && !attribute && !drcsSelection && ((glyphMode == 9 && (fromGlyph != 65 || toGlyph != 66))
        || (glyphMode == 1 && (fromGlyph != 33 || toGlyph != 34))))
        qFatal("Fixed local glyph trial requires G0 A-to-B or G1 0x21-to-0x22");
    if (extendedGlyph && !((glyphMode == 15 && toGlyphMode == 15 && fromGlyph == 35 && toGlyph == 66)
        || (glyphMode == 25 && toGlyphMode == 28 && fromGlyph == 65 && toGlyph == 65)))
        qFatal("Fixed extended trial requires G2 35-to-66 or A diacritic9-to12");
    if (remainingGlyph && !((glyphMode == 1 && fromGlyph == 65 && toGlyph == 64)
        || (glyphMode == 11 && fromGlyph == 47 && toGlyph == 95)))
        qFatal("Fixed remaining trial requires G1 alphabetic65-to64 or G3 Level25 47-to95");
    if (drcsSlot && (glyphMode != 13 || fromGlyph != 64 || toGlyph != 65))
        qFatal("Fixed DRCS reference trial requires Normal mode13 slot0-to1");
    if (flash && (glyphMode != 7 || fromGlyph != 0 || toGlyph != 11))
        qFatal("Fixed Flash trial requires mode7 raw0-to11");
    if (charset && (glyphMode != 8 || fromGlyph != 0 || toGlyph != 55))
        qFatal("Fixed charset trial requires mode8 raw0-to55");
    if (drcsSelection && (glyphMode != 24 || fromGlyph != 80
        || toGlyph != (drcsSubtable ? 81 : 16)))
        qFatal("Fixed DRCS selection requires mode24 NormalS0 raw80-to81 or80-to16");
    const bool panelDay = mode == "single-page-panel-zone-q";
    const bool panelNight = mode == "single-page-panel-save";
    const bool panelProof = panelDay || panelNight;
    const QByteArray panelTheme = qgetenv("ERSATZRS_QTM_PANEL_THEME");
    if ((!panelProof && qEnvironmentVariableIsSet("ERSATZRS_QTM_PANEL_THEME"))
        || (panelDay && panelTheme != "day") || (panelNight && panelTheme != "night"))
        qFatal("Panel theme requires the exact fixed Day-edit or Night-save mode");
    const bool suppliedTarget = qEnvironmentVariableIsSet("ERSATZRS_QTM_TRIPLET_INDEX")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TRIPLET_COLUMN");
    const bool suppliedForeground = qEnvironmentVariableIsSet("ERSATZRS_QTM_FROM_COLOUR")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_COLOUR");
    if (!foreground && (suppliedForeground || (!localGlyph && !glyphStructure && !mixedGlyphStructure && !styleStructure && !resourceStructure && suppliedTarget)))
        qFatal("Foreground operands require explicit single-page-foreground mode");
    const int triplet = (foreground || localGlyph || glyphStructure || mixedGlyphStructure || styleStructure || resourceStructure) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_INDEX", 207) : 0;
    const int column = (foreground || localGlyph || glyphStructure || mixedGlyphStructure || styleStructure || resourceStructure) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_COLUMN", drcsSelection ? 40 : 39) : 0;
    if (drcsSelection && (column != 40 || !(triplet < glyph0 && glyph0 < glyph1)))
        qFatal("Fixed DRCS prefix address40 and ordered distinct glyph indices are required");
    if ((glyphStructure || mixedGlyphStructure) && (triplet != 12 || column != 0))
        qFatal("Fixed Local structure trial requires body word12 and column0");
    if (styleStructure && (triplet != 17 || column != 1))
        qFatal("Fixed Local style structure requires original mosaic word17 and column1");
    if (resourceStructure && (triplet != 7 || column != 1))
        qFatal("Fixed resource structure requires original G0P word7 and column1");
    const int from = foreground ? boundedDecimal("ERSATZRS_QTM_FROM_COLOUR", 31) : 0;
    const int to = foreground ? boundedDecimal("ERSATZRS_QTM_TO_COLOUR", 31) : 0;
    if (foreground && from == to) qFatal("Foreground trial requires a changed colour");
    if (mode.isEmpty()) {
        if (!input.isEmpty() || !output.isEmpty()
            || !qgetenv("ERSATZRS_QTM_LEFT_PANEL_COLUMNS").isEmpty()
            || !qgetenv("ERSATZRS_QTM_RIGHT_PANEL_COLUMNS").isEmpty()
            || !qgetenv("ERSATZRS_QTM_NORMAL_DRCS_BASENAME").isEmpty()
            || !qgetenv("ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE").isEmpty())
            qFatal("Native basenames/panels require explicit single-page driver mode");
        return {false, true, false, false, false, false, 0, 0, 0, 0, 0, "synthetic-editor-audit.tti", "edited-editor-audit.tti",
            "/work/synthetic-editor-audit.tti", "/work/edited-editor-audit.tti", {}, 0, 0};
    }
    const bool resourceSet = mode == "resource-set-drcs-bit";
    const bool drcsEdit = mode == "single-page-drcs-bit" || resourceSet;
    const bool drcsVerify = mode == "single-page-drcs-verify";
    if ((mode != "single-page-zone-q" && mode != "single-page-save" && !foreground && !drcsEdit && !drcsVerify && !panelProof && !localGlyph && !glyphStructure && !mixedGlyphStructure && !styleStructure && !resourceStructure) || !safeTtiBasename(input) || !safeTtiBasename(output)
        || input == output) qFatal("Invalid explicit synthetic single-page driver context");
    const int left = panelColumns("ERSATZRS_QTM_LEFT_PANEL_COLUMNS");
    const int right = panelColumns("ERSATZRS_QTM_RIGHT_PANEL_COLUMNS");
    const QByteArray drcs = qgetenv("ERSATZRS_QTM_NORMAL_DRCS_BASENAME");
    if (!drcs.isEmpty() && (!safeTtiBasename(drcs) || drcs == input || drcs == output))
        qFatal("Invalid explicit protected Normal DRCS companion name");
    if ((drcsSelection || mixedGlyphStructure) && (!safeTtiBasename(globalDrcs) || globalDrcs == input
        || globalDrcs == output || globalDrcs == drcs))
        qFatal("Fixed selection requires a distinct safe Global DRCS companion basename");
    if (left + right > 16) qFatal("Native Level 2.5 panel counts exceed sixteen columns");
    if (drcsEdit && (left != 0 || right != 0 || !drcs.isEmpty()))
        qFatal("Synthetic DRCS bitmap edit requires a standalone no-panel resource document");
    int drcsSubpage = 0;
    const QByteArray suppliedSubpage = qgetenv("ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE");
    if (!suppliedSubpage.isEmpty()) {
        bool valid = false;
        drcsSubpage = suppliedSubpage.toInt(&valid, 10);
        if (!valid || drcsSubpage < 0 || drcsSubpage > 1
            || suppliedSubpage != QByteArray::number(drcsSubpage) || drcs.isEmpty())
            qFatal("Synthetic DRCS pixel oracle supports only explicit S1=0 or S1=1");
    }
    if (drcsVerify && (drcs.isEmpty() || drcsSubpage != 0))
        qFatal("Edited DRCS verification requires the synthetic S1=0 companion");
    if (panelProof && (left != 4 || right != 12 || drcs.isEmpty() || drcsSubpage != 0))
        qFatal("Fixed panel proof requires left4/right12 and actual Normal S1=0 companion");
    if ((localGlyph || glyphStructure || mixedGlyphStructure || styleStructure) && (left != 4 || right != 12 || drcs.isEmpty() || drcsSubpage != 0))
        qFatal("Fixed local glyph trial requires original left4/right12 and Normal S0 companion");
    if (resourceStructure && (left != 0 || right != 0
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_BASENAME")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE")))
        qFatal("Fixed resource structure requires zero panels and no DRCS companion context");
    return {true, mode == "single-page-zone-q" || panelDay, foreground, drcsEdit, drcsVerify, resourceSet, triplet, column, from, to, drcsSubpage, input, output, QString("/work/") + QString::fromLatin1(input),
        QString("/work/") + QString::fromLatin1(output), drcs, left, right, panelProof, panelNight, localGlyph, glyphMode, fromGlyph, toGlyph, toGlyphMode, drcsSlot, flash, charset, drcsSubtable, drcsScope, glyph0, glyph1, globalDrcs, glyphStructure, mixedGlyphStructure, styleStructure, resourceInsert, resourceDelete};
}
// Actual Qt widget paint only; no document, decoder or model field is assigned.
static void capturePanelPaint(const DriverContext &context, const char *stage) {
    auto main = window();
    auto view = editor();
    if (!context.panelProof || !main || !view || !main->isVisible()
        || !main->windowHandle() || !main->windowHandle()->isExposed()
        || !view->isVisible() || view->size() != QSize(672, 250))
        qFatal("Fixed actual panel widget must be exposed at672x250");
    const QImage painted = view->grab().toImage();
    if (painted.size() != QSize(672, 250))
        qFatal("Panel widget capture must have one-to-one pixels");
    const unsigned short rows[10] = {
        0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009
    };
    const QColor foreground = context.panelNight ? QColor(0, 0, 255) : QColor(255, 0, 0);
    for (int column : {12, 44})
        for (int y = 0; y < 10; ++y)
            for (int x = 0; x < 12; ++x) {
                const QColor expected = ((rows[y] >> (11 - x)) & 1) ? foreground : QColor(0, 0, 0);
                if (painted.pixelColor(column * 12 + x, 10 + y) != expected)
                    qFatal("Actual Main/RightPanel literal801 slot0/theme pixels differ");
            }
    const QString widgetStage = QString("panel-%1-widget").arg(QString::fromLatin1(stage));
    if (!painted.save(QString("/work/") + context.captureName(widgetStage.toUtf8().constData())))
        qFatal("Actual panel widget crop capture failed");
    const QString windowStage = QString("panel-%1-window").arg(QString::fromLatin1(stage));
    capture(context.captureName(windowStage.toUtf8().constData()).toUtf8().constData());
    std::printf("QTM actual panel paint stage=%s theme=%s geometry672x250 visual12/44 NormalS0slot0 literal801 240RGBpixels match\n",
        stage, context.panelNight ? "night" : "day");
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
    if (context.panelProof) {
        if (main->isWindowModified()) qFatal("Actual panel document is dirty before any edit");
        capturePanelPaint(context, "original");
        return;
    }
    // Fixed opt-in synthetic fixture: Normal S1=0 or S1=1/slot0 at Main(1,8),
    // asymmetric literal bitmap, day CLUT2 foreground red/background black.
    // An attachment label alone would also succeed for an empty/wrong file.
    auto view = editor();
    if (!view) qFatal("Actual DRCS editor view absent");
    const QImage bitmap = view->grab().toImage();
    if (bitmap.size() != view->size()) qFatal("Expected one-to-one DRCS pixel capture");
    const unsigned short patterns[2][10] = {
        {0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009},
        {0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900}
    };
    const auto &rows = patterns[context.drcsSubpage];
    for (int y = 0; y < 10; ++y)
        for (int x = 0; x < 12; ++x) {
            const unsigned short row = (context.verifyEditedDrcs && y == 0) ? 0x803 : rows[y];
            const QColor expected = ((row >> (11 - x)) & 1) ? QColor(255, 0, 0) : QColor(0, 0, 0);
            if (bitmap.pixelColor((context.leftPanelColumns + 8) * 12 + x, 10 + y) != expected)
                qFatal("Actual attached DRCS pixels/palette differ from synthetic literal");
        }
    std::printf("QTM actual Normal DRCS glyph: S1=%d edited=%d all 120 literal bitmap/day palette pixels match\n",
        context.drcsSubpage, context.verifyEditedDrcs);
    std::printf("QTM actual Normal DRCS Load file complete: %s\n", context.drcsName.constData());
}
static void loadGlobalDrcs(QMainWindow *main, const DriverContext &context) {
    if (context.globalDrcsName.isEmpty()) return;
    const QString path = QString("/work/") + QString::fromLatin1(context.globalDrcsName);
    const QFileInfo file(path);
    if (!file.isFile() || file.isSymLink() || file.size() <= 0)
        qFatal("Protected Global DRCS companion absent");
    QAction *load = nullptr;
    for (QAction *action : main->findChildren<QAction *>())
        if (action->statusTip() == "Load a file to use for Global DRCS definitions") load = action;
    if (!load) qFatal("Actual Global DRCS Load file action absent");
    QTimer::singleShot(250, qApp, [main, context, path] {
        for (QWidget *widget : QApplication::topLevelWidgets())
            if (auto dialog = qobject_cast<QFileDialog *>(widget)) {
                if (!dialog->isVisible() || dialog->parentWidget() != main
                    || dialog->acceptMode() != QFileDialog::AcceptOpen
                    || dialog->windowTitle() != "Select Global DRCS file") continue;
                auto name = dialog->findChild<QLineEdit *>("fileNameEdit");
                if (!name) qFatal("Actual DRCS filename input absent");
                // Reuse Save As's observed native modal/input focus gate.
                dialog->raise();
                dialog->activateWindow();
                if (auto handle = dialog->windowHandle()) handle->requestActivate();
                if (!QTest::qWaitForWindowActive(dialog, 5000))
                    qFatal("Actual Global DRCS dialog did not become active");
                const QPoint target = name->mapToGlobal(name->rect().center());
                QProcess pointer;
                pointer.start("/usr/bin/xdotool", {"mousemove",
                    QString::number(target.x()), QString::number(target.y()), "click", "1"});
                if (!pointer.waitForFinished(3000)) {
                    pointer.kill();
                    pointer.waitForFinished(1000);
                    qFatal("Actual Global DRCS filename pointer timed out");
                }
                if (pointer.exitStatus() != QProcess::NormalExit || pointer.exitCode() != 0)
                    qFatal("Actual Global DRCS filename pointer failed");
                if (!QTest::qWaitFor([name] { return name->hasFocus(); }, 3000))
                    QTest::keyClick(dialog, Qt::Key_N, Qt::AltModifier);
                if (!QTest::qWaitFor([name] { return name->hasFocus(); }, 3000))
                    qFatal("Actual Global DRCS filename input did not receive focus");
                QTest::qWait(100);
                QTest::keyClick(name, Qt::Key_A, Qt::ControlModifier);
                QTest::keyClicks(name, path, Qt::NoModifier, 5);
                QTest::keyClick(name, Qt::Key_Tab);
                if (!QTest::qWaitFor([dialog, path] { return dialog->selectedFiles() == QStringList{path}; }, 3000))
                    qFatal("Actual Global DRCS filename did not select its exact companion");
                QTimer::singleShot(200, qApp, [dialog, context, path] {
                    if (!dialog->grab().save(QString("/work/") + context.captureName("global-drcs-open-dialog"))
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
        qFatal("Actual Global DRCS open dialog absent");
    });
    load->trigger();
    bool attached = false;
    for (QAction *action : main->findChildren<QAction *>())
        if (action->text() == QString("Global DRCS: ") + QString::fromLatin1(context.globalDrcsName)) attached = true;
    if (!attached) qFatal("Actual Global DRCS attachment did not complete");
    capture(context.captureName("global-drcs-loaded").toUtf8().constData());
    if (main->isWindowModified()) qFatal("Actual Global companion attachment changed the original document");
}

// Captures actual painted widgets; native import remains the edit authority.
static void captureLocalGlyphWidget(const DriverContext &context, const char *stage) {
    auto main = window();
    auto view = editor();
    if (!context.editLocalGlyph || !main || !view || !main->isVisible()
        || !main->windowHandle() || !main->windowHandle()->isExposed()
        || !view->isVisible() || view->size() != QSize(672, 250))
        qFatal("Actual local glyph widget must be exposed at672x250");
    const QImage painted = view->grab().toImage();
    if (painted.size() != QSize(672, 250)
        || !painted.save(QString("/work/") + context.captureName(
            (QByteArray("local-glyph-") + stage + "-widget").constData())))
        qFatal("Actual local glyph widget capture failed");
}
// Select the existing cooked-mode button's real nested menu. The timer is
// queued before the click because QPushButton::setMenu may enter a nested loop.
// Every transition is an observed mouse hover/click, never QAction::trigger or
// a document/model assignment.
static void editDiacriticMode(QDockWidget *dock, const DriverContext &context) {
    if (context.glyphMode != 25 || context.toGlyphMode != 28
        || context.fromGlyph != 65 || context.toGlyph != 65)
        qFatal("Unexpected fixed diacritic mode transition");
    QPushButton *button = nullptr;
    for (QPushButton *candidate : dock->findChildren<QPushButton *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || !candidate->menu()
            || candidate->text() != "G0 character diacritical 9") continue;
        if (button) qFatal("Ambiguous actual cooked diacritic mode button");
        button = candidate;
    }
    if (!button) qFatal("Actual cooked diacritic mode button absent");
    auto menu = button->menu();
    QTimer::singleShot(150, qApp, [menu, context] {
        const auto actionNamed = [](QMenu *parent, const char *text) {
            QAction *found = nullptr;
            for (QAction *action : parent->actions()) {
                if (action->text() != text || !action->isVisible() || !action->isEnabled()) continue;
                if (found) qFatal("Ambiguous fixed actual mode menu action");
                found = action;
            }
            if (!found) qFatal("Fixed actual mode menu action absent");
            return found;
        };
        const auto hoverSubmenu = [context, actionNamed](QMenu *parent, const char *name, const char *stage) {
            if (!QTest::qWaitFor([parent] { return parent->isVisible(); }, 3000))
                qFatal("Actual mode menu did not open");
            QAction *action = actionNamed(parent, name);
            QMenu *child = action->menu();
            const QPoint point = parent->actionGeometry(action).center();
            if (!child || !parent->rect().contains(point))
                qFatal("Actual mode submenu is not mouse-addressable");
            QTest::mouseMove(parent, point);
            if (!QTest::qWaitFor([parent, child, action] {
                return parent->activeAction() == action && child->isVisible();
            }, 3000)) qFatal("Actual mode submenu did not receive mouse hover");
            if (!parent->grab().save(QString("/work/") + context.captureName(stage)))
                qFatal("Actual mode menu capture failed");
            return child;
        };
        QMenu *column = hoverSubmenu(menu, "Column triplet", "local-glyph-mode-top");
        QMenu *diacritic = hoverSubmenu(column, "G0 diacritical", "local-glyph-mode-column");
        QAction *target = actionNamed(diacritic, "G0 character diacritical C");
        if (target->menu()) qFatal("Fixed final diacritic action is unexpectedly a submenu");
        const QPoint point = diacritic->actionGeometry(target).center();
        if (!diacritic->rect().contains(point)) qFatal("Actual diacritic choice is not mouse-addressable");
        QTest::mouseMove(diacritic, point);
        if (!QTest::qWaitFor([diacritic, target] {
            return diacritic->isVisible() && diacritic->activeAction() == target;
        }, 3000)) qFatal("Actual diacritic choice did not receive mouse hover");
        if (!diacritic->grab().save(QString("/work/") + context.captureName("local-glyph-mode-popup")))
            qFatal("Actual diacritic choice capture failed");
        QTest::mouseClick(diacritic, Qt::LeftButton, Qt::NoModifier, point);
    });
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
// QTM's existing mode13 PTU editor writes only the low six data bits. Click
// its visible step-up button, retaining Normal scope and every selection word.
static void editDrcsSlot(QMainWindow *main, QDockWidget *dock,
    QAbstractItemModel *model, const DriverContext &context) {
    if (!context.editLocalDrcsSlot || context.glyphMode != 13
        || context.fromGlyph != 64 || context.toGlyph != 65 || main->isWindowModified())
        qFatal("Unexpected fixed original DRCS slot trial");
    QSpinBox *slot = nullptr;
    for (QSpinBox *candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 47) continue;
        if (slot) qFatal("Ambiguous actual DRCS PTU spinbox");
        slot = candidate;
    }
    if (!slot || slot->value() != 0) qFatal("Actual original DRCS PTU0 editor absent");
    if (slot->isReadOnly() || slot->buttonSymbols() == QAbstractSpinBox::NoButtons)
        qFatal("Actual DRCS PTU step buttons are unavailable");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-before")))
        qFatal("Actual DRCS slot baseline dock capture failed");
    captureLocalGlyphWidget(context, "before");
    // Use the existing visible step-up control, not keyboard focus or a setter.
    QStyleOptionSpinBox option;
    option.initFrom(slot);
    option.frame = slot->hasFrame();
    option.buttonSymbols = slot->buttonSymbols();
    option.stepEnabled = QAbstractSpinBox::StepUpEnabled | QAbstractSpinBox::StepDownEnabled;
    option.subControls = QStyle::SC_SpinBoxEditField | QStyle::SC_SpinBoxUp | QStyle::SC_SpinBoxDown;
    if (slot->style()->styleHint(QStyle::SH_SpinBox_ButtonsInsideFrame, nullptr, slot))
        option.subControls |= QStyle::SC_SpinBoxFrame;
    const QRect up = slot->style()->subControlRect(QStyle::CC_SpinBox, &option, QStyle::SC_SpinBoxUp, slot);
    if (!up.isValid() || !slot->rect().contains(up.center())
        || slot->style()->hitTestComplexControl(QStyle::CC_SpinBox, &option, up.center(), slot) != QStyle::SC_SpinBoxUp)
        qFatal("Actual visible DRCS PTU step-up button is not clickable");
    if (!slot->grab().save(QString("/work/") + context.captureName("local-glyph-step-up")))
        qFatal("Actual DRCS PTU button capture failed");
    QTest::mouseMove(slot, up.center());
    QTest::mouseClick(slot, Qt::LeftButton, Qt::NoModifier, up.center());
    if (!QTest::qWaitFor([slot, main, model, context] {
        return slot->value() == 1 && main->isWindowModified()
            && model->data(model->index(context.tripletIndex, 0), Qt::UserRole).toInt() == context.tripletColumn
            && model->data(model->index(context.tripletIndex, 1), Qt::UserRole).toInt() == 13
            && model->data(model->index(context.tripletIndex, 2), Qt::UserRole).toInt() == 65;
    }, 3000)) qFatal("Actual DRCS PTU edit did not retain Normal mode13 and select slot1");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-edited")))
        qFatal("Actual DRCS slot edited dock capture failed");
    captureLocalGlyphWidget(context, "edited");
    std::printf("QTM actual Local DRCS PTU visible step-up click: triplet=%d column=%d mode13 Normal slot0-to1 raw64-to65\n",
        context.tripletIndex, context.tripletColumn);
}
// Existing cooked Flash selectors and raw Data spinbox; real mouse events only.
static void pickFlashCombo(QDockWidget *dock, int count, int choice) {
    QComboBox *combo = nullptr;
    for (auto candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != count) continue;
        if (combo) qFatal("Ambiguous actual Flash selector");
        combo = candidate;
    }
    if (!combo || combo->currentIndex() != 0) qFatal("Original Flash selector must be zero");
    QTest::mouseClick(combo, Qt::LeftButton, Qt::NoModifier, combo->rect().center());
    auto popup = combo->view();
    if (!QTest::qWaitFor([popup] { return popup->isVisible(); }, 3000)) qFatal("Actual Flash popup absent");
    const QModelIndex target = popup->model()->index(choice, combo->modelColumn(), combo->rootModelIndex());
    popup->scrollTo(target, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([popup, target] { return popup->viewport()->rect().contains(popup->visualRect(target).center()); }, 3000))
        qFatal("Actual Flash choice not clickable");
    // Reuse the existing glyph/foreground popup's real hover requirement.
    const QPoint point = popup->visualRect(target).center();
    QTest::mouseMove(popup->viewport(), point);
    if (!QTest::qWaitFor([popup, target, point] {
        return popup->isVisible() && popup->indexAt(point) == target && popup->currentIndex() == target;
    }, 3000)) qFatal("Actual Flash choice did not receive mouse hover");
    if (!popup->window()->grab().save(QString("/work/actual-flash-choice-%1.png").arg(count)))
        qFatal("Actual Flash popup capture failed");
    QTest::mouseClick(popup->viewport(), Qt::LeftButton, Qt::NoModifier, point);
    if (!QTest::qWaitFor([combo, choice] { return combo->currentIndex() == choice; }, 3000))
        qFatal("Actual Flash choice did not apply");
}
static void editFlashCharset(QMainWindow *main, QDockWidget *dock, QAbstractItemModel *model,
    const DriverContext &context) {
    if (main->isWindowModified()) qFatal("Original Flash/charset trial must be unmodified");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-before")))
        qFatal("Actual Flash/charset baseline dock capture failed");
    captureLocalGlyphWidget(context, "before");
    if (context.editLocalFlash) {
        pickFlashCombo(dock, 4, 3);
        pickFlashCombo(dock, 6, 2);
    } else {
        QCheckBox *raw = nullptr;
        for (auto candidate : dock->findChildren<QCheckBox *>()) {
            if (!candidate->isVisible() || candidate->text() != "Raw values") continue;
            if (raw) qFatal("Ambiguous actual Raw values checkbox");
            raw = candidate;
        }
        if (!raw || raw->isChecked()) qFatal("Original raw selector state differs");
        QTest::mouseClick(raw, Qt::LeftButton, Qt::NoModifier, QPoint(8, raw->rect().center().y()));
        if (!raw->isChecked()) qFatal("Actual Raw values click did not select raw controls");
        QSpinBox *data = nullptr;
        for (auto candidate : dock->findChildren<QSpinBox *>()) {
            if (!candidate->isVisible() || !candidate->isEnabled() || candidate->minimum() != 0 || candidate->maximum() != 127) continue;
            // The TODO charset parameter fallback also exposes a reserved/PDC
            // spinbox0..127. Only the actual raw Address/Mode/Data group qualifies.
            const auto fields = candidate->parentWidget()->findChildren<QSpinBox *>();
            bool address = false, mode = false;
            for (auto field : fields) {
                address = address || (field->maximum() == 63 && field->value() == context.tripletColumn);
                mode = mode || (field->maximum() == 31 && field->value() == 8);
            }
            if (fields.size() != 3 || !address || !mode) continue;
            if (data) qFatal("Ambiguous actual raw Data spinbox");
            data = candidate;
        }
        if (!data || data->value() != 0) qFatal("Original raw Data value differs");
        QStyleOptionSpinBox option;
        option.initFrom(data);
        option.rect = data->rect();
        option.frame = data->hasFrame();
        option.buttonSymbols = data->buttonSymbols();
        option.stepEnabled = QAbstractSpinBox::StepUpEnabled;
        option.subControls = QStyle::SC_All;
        const QRect up = data->style()->subControlRect(QStyle::CC_SpinBox, &option, QStyle::SC_SpinBoxUp, data);
        if (!data->rect().contains(up.center()) || data->style()->hitTestComplexControl(QStyle::CC_SpinBox, &option, up.center(), data) != QStyle::SC_SpinBoxUp)
            qFatal("Actual raw Data step-up must be visible");
        for (int value = 1; value <= 55; ++value) {
            QTest::mouseClick(data, Qt::LeftButton, Qt::NoModifier, up.center());
            if (!QTest::qWaitFor([data, value] { return data->value() == value; }, 3000))
                qFatal("Actual raw Data step-up did not advance exactly one");
        }
        if (!data->grab().save(QString("/work/") + context.captureName("local-glyph-raw-data")))
            qFatal("Actual raw Data capture failed");
    }
    if (!QTest::qWaitFor([main, model, context] {
        return main->isWindowModified()
            && model->data(model->index(context.tripletIndex, 0), Qt::UserRole).toInt() == context.tripletColumn
            && model->data(model->index(context.tripletIndex, 1), Qt::UserRole).toInt() == context.glyphMode
            && model->data(model->index(context.tripletIndex, 2), Qt::UserRole).toInt() == context.toGlyph;
    }, 3000)) qFatal("Actual Flash/charset changed the wrong raw tuple");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-edited")))
        qFatal("Actual Flash/charset edited dock capture failed");
    captureLocalGlyphWidget(context, "edited");
    std::printf("QTM actual Local Flash/charset mouse edit: triplet=%d column=%d mode=%d raw0-to%d\n",
        context.tripletIndex, context.tripletColumn, context.glyphMode, context.toGlyph);
}
// Real cooked Global radio indicator, used for the fixed mode24/mode13 rows.
static void clickGlobalDrcs(QDockWidget *dock, const DriverContext &context, const char *stage) {
    QRadioButton *global = nullptr, *normal = nullptr;
    for (auto radio : dock->findChildren<QRadioButton *>()) {
        if (!radio->isVisible() || !radio->isEnabled()) continue;
        if (radio->text() == "Global") {
            if (global) qFatal("Ambiguous visible Global DRCS radio");
            global = radio;
        } else if (radio->text() == "Normal") {
            if (normal) qFatal("Ambiguous visible Normal DRCS radio");
            normal = radio;
        }
    }
    if (!global || !normal || global->isChecked() || !normal->isChecked())
        qFatal("Actual cooked DRCS scope must start Normal");
    QStyleOptionButton option;
    option.initFrom(global);
    option.text = global->text();
    const QRect indicator = global->style()->subElementRect(QStyle::SE_RadioButtonIndicator, &option, global);
    if (!indicator.isValid() || !global->rect().contains(indicator.center()))
        qFatal("Actual visible Global DRCS radio indicator is not clickable");
    if (!dock->grab().save(QString("/work/") + context.captureName(stage)))
        qFatal("Actual Global DRCS radio capture failed");
    QTest::mouseMove(global, indicator.center());
    QTest::mouseClick(global, Qt::LeftButton, Qt::NoModifier, indicator.center());
    if (!QTest::qWaitFor([global, normal] { return global->isChecked() && !normal->isChecked(); }, 3000))
        qFatal("Actual Global DRCS radio click did not select Global");
    const QByteArray selectedStage = QByteArray(stage) + "-selected";
    if (!dock->grab().save(QString("/work/") + context.captureName(selectedStage.constData())))
        qFatal("Actual selected Global DRCS radio capture failed");
}
static void editDrcsSelection(QMainWindow *main, QDockWidget *dock, QTableView *table,
    QAbstractItemModel *model, const DriverContext &context) {
    const int rows = model->rowCount();
    if (!(context.editLocalDrcsSubtable || context.editLocalDrcsScope) || rows <= 0 || rows > 208
        || context.drcsGlyph1Index >= rows || main->isWindowModified())
        qFatal("Unexpected fixed original DRCS selection context");
    const auto raw = [model](int row, int column) {
        bool valid = false;
        const int value = model->data(model->index(row, column), Qt::UserRole).toInt(&valid);
        if (!valid) qFatal("Actual DRCS raw tuple absent");
        return value;
    };
    std::array<std::array<int, 3>, 208> expected{};
    for (int row = 0; row < rows; ++row)
        for (int column = 0; column < 3; ++column) expected[row][column] = raw(row, column);
    if (expected[context.tripletIndex] != std::array<int, 3>{40, 24, 80}
        || expected[context.drcsGlyph0Index] != std::array<int, 3>{0, 13, 64}
        || expected[context.drcsGlyph1Index] != std::array<int, 3>{1, 13, 65})
        qFatal("Original DRCS prefix and two NormalS0 PTU0/1 tuples differ");
    QComboBox *levels = nullptr;
    for (auto candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != 3) continue;
        if (levels) qFatal("Ambiguous actual DRCS required-level selector");
        levels = candidate;
    }
    if (!levels || levels->currentIndex() != 0)
        qFatal("Original DRCS prefix must remain L2.5-only");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-before")))
        qFatal("Actual DRCS selection baseline dock capture failed");
    captureLocalGlyphWidget(context, "before");
    if (context.editLocalDrcsSubtable) {
        QSpinBox *subtable = nullptr;
        for (auto candidate : dock->findChildren<QSpinBox *>()) {
            if (!candidate->isVisible() || !candidate->isEnabled()
                || candidate->minimum() != 0 || candidate->maximum() != 15) continue;
            if (subtable) qFatal("Ambiguous actual DRCS subtable spinbox");
            subtable = candidate;
        }
        if (!subtable || subtable->value() != 0 || subtable->isReadOnly()
            || subtable->buttonSymbols() == QAbstractSpinBox::NoButtons)
            qFatal("Actual original DRCS subtable0 step-up control absent");
        QStyleOptionSpinBox option;
        option.initFrom(subtable);
        option.rect = subtable->rect();
        option.frame = subtable->hasFrame();
        option.buttonSymbols = subtable->buttonSymbols();
        option.stepEnabled = QAbstractSpinBox::StepUpEnabled | QAbstractSpinBox::StepDownEnabled;
        option.subControls = QStyle::SC_SpinBoxEditField | QStyle::SC_SpinBoxUp | QStyle::SC_SpinBoxDown;
        if (subtable->style()->styleHint(QStyle::SH_SpinBox_ButtonsInsideFrame, nullptr, subtable))
            option.subControls |= QStyle::SC_SpinBoxFrame;
        const QRect up = subtable->style()->subControlRect(QStyle::CC_SpinBox, &option, QStyle::SC_SpinBoxUp, subtable);
        if (!up.isValid() || !subtable->rect().contains(up.center())
            || subtable->style()->hitTestComplexControl(QStyle::CC_SpinBox, &option, up.center(), subtable) != QStyle::SC_SpinBoxUp)
            qFatal("Actual DRCS subtable step-up button is not clickable");
        if (!subtable->grab().save(QString("/work/") + context.captureName("local-drcs-subtable-step-up")))
            qFatal("Actual DRCS subtable step-up capture failed");
        QTest::mouseMove(subtable, up.center());
        QTest::mouseClick(subtable, Qt::LeftButton, Qt::NoModifier, up.center());
        if (!QTest::qWaitFor([subtable, raw, context] {
            return subtable->value() == 1 && raw(context.tripletIndex, 2) == 81;
        }, 3000)) qFatal("Actual DRCS subtable step-up did not select1");
    } else {
        clickGlobalDrcs(dock, context, "local-drcs-prefix-global-radio");
        if (!QTest::qWaitFor([raw, context] { return raw(context.tripletIndex, 2) == 16; }, 3000))
            qFatal("Actual DRCS prefix did not retain usage/S0 while selecting Global");
        captureLocalGlyphWidget(context, "prefix-global");
        for (int glyph = 0; glyph < 2; ++glyph) {
            const int index = glyph == 0 ? context.drcsGlyph0Index : context.drcsGlyph1Index;
            const QModelIndex target = model->index(index, 0);
            table->scrollTo(target, QAbstractItemView::PositionAtCenter);
            if (!QTest::qWaitFor([table, target] {
                return table->isVisible() && table->isEnabled()
                    && table->viewport()->rect().contains(table->visualRect(target).center());
            }, 3000)) qFatal("Actual DRCS glyph row is not clickable");
            QTest::mouseClick(table->viewport(), Qt::LeftButton, Qt::NoModifier, table->visualRect(target).center());
            if (table->currentIndex().row() != index || raw(index, 0) != glyph
                || raw(index, 1) != 13 || raw(index, 2) != 64 + glyph)
                qFatal("Actual DRCS glyph source row no longer matches original Normal slot");
            clickGlobalDrcs(dock, context, glyph == 0 ? "local-drcs-glyph0-global-radio" : "local-drcs-glyph1-global-radio");
            if (!QTest::qWaitFor([raw, index, glyph] { return raw(index, 2) == glyph; }, 3000))
                qFatal("Actual DRCS glyph Global click changed the slot");
            captureLocalGlyphWidget(context, glyph == 0 ? "glyph0-global" : "glyph1-global");
        }
        expected[context.drcsGlyph0Index][2] = 0;
        expected[context.drcsGlyph1Index][2] = 1;
    }
    expected[context.tripletIndex][2] = context.toGlyph;
    if (model->rowCount() != rows || !main->isWindowModified())
        qFatal("Actual DRCS selection must change only existing tuples");
    for (int row = 0; row < rows; ++row)
        for (int column = 0; column < 3; ++column)
            if (raw(row, column) != expected[row][column])
                qFatal("Actual DRCS selection changed an unapproved tuple/address/mode/slot");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-glyph-edited")))
        qFatal("Actual DRCS selection edited dock capture failed");
    captureLocalGlyphWidget(context, "edited");
    std::printf("QTM actual cooked DRCS selection: prefix=%d mode24 address40 raw80-to%d glyphs=%d/%d PTU0/1 retained scope=%s\n",
        context.tripletIndex, context.toGlyph, context.drcsGlyph0Index, context.drcsGlyph1Index,
        context.editLocalDrcsScope ? "Global" : "Normal");
}
static void editForeground(QMainWindow *main, const DriverContext &context) {
    const int expectedMode = context.editLocalGlyph ? context.glyphMode : 0;
    const int expectedFrom = context.editLocalGlyph ? context.fromGlyph : context.fromColour;
    const int expectedTo = context.editLocalGlyph ? context.toGlyph : context.toColour;
    const int offset = context.editLocalGlyph ? 32 : 0;
    const int comboSize = context.editLocalGlyph ? 96 : 32;
    const auto operandCapture = [context](const char *stage) {
        return context.captureName(((context.editLocalGlyph ? QByteArray("local-glyph-")
            : QByteArray("foreground-")) + stage).constData());
    };
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets")
        qFatal("Pinned X/26 triplets dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    // Arrange the actual Tools dock for a separately inspectable headed capture.
    // No document/model mutation is performed by this display arrangement.
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000))
        qFatal("Actual floating X/26 dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Expected exactly one actual X/26 table");
    auto table = tables.front();
    auto model = table->model();
    if (!model || context.tripletIndex >= model->rowCount() || model->columnCount() != 4)
        qFatal("Native semantic word is outside the actual X/26 table");
    const auto raw = [model, context](int column) {
        bool valid = false;
        const int value = model->data(model->index(context.tripletIndex, column), Qt::UserRole).toInt(&valid);
        if (!valid) qFatal("Actual X/26 raw operand absent");
        return value;
    };
    if (raw(0) != context.tripletColumn || raw(1) != expectedMode || raw(2) != expectedFrom)
        qFatal("Actual X/26 row does not match native foreground tuple");
    const QModelIndex target = model->index(context.tripletIndex, 0);
    table->scrollTo(target, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([table, target] {
        return table->isVisible() && table->isEnabled()
            && table->viewport()->rect().contains(table->visualRect(target).center());
    }, 3000)) qFatal("Actual X/26 target row is not clickable");
    QTest::mouseClick(table->viewport(), Qt::LeftButton, Qt::NoModifier, table->visualRect(target).center());
    if (table->currentIndex().row() != context.tripletIndex)
        qFatal("Actual X/26 row selection failed");
    if (context.editLocalDrcsSubtable || context.editLocalDrcsScope) {
        editDrcsSelection(main, dock, table, model, context);
        return;
    }
    if (context.editLocalDrcsSlot) {
        editDrcsSlot(main, dock, model, context);
        return;
    }
    if (context.editLocalFlash || context.editLocalCharset) {
        editFlashCharset(main, dock, model, context);
        return;
    }
    QComboBox *colour = nullptr;
    for (QComboBox *candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != comboSize) continue;
        if (colour) qFatal("Ambiguous visible X/26 colour selector");
        colour = candidate;
    }
    if (!colour || colour->currentIndex() != expectedFrom - offset || main->isWindowModified())
        qFatal("Actual foreground colour selector/current document state differs from native baseline");
    if (!dock->grab().save(QString("/work/") + operandCapture("before")))
        qFatal("Actual foreground dock baseline capture failed");
    if (context.editLocalGlyph) captureLocalGlyphWidget(context, "before");
    if (context.editLocalGlyph && context.toGlyphMode != context.glyphMode) {
        editDiacriticMode(dock, context);
        if (!QTest::qWaitFor([colour, main, context, raw] {
            return colour->currentIndex() == context.toGlyph - 32 && main->isWindowModified()
                && raw(0) == context.tripletColumn && raw(1) == context.toGlyphMode
                && raw(2) == context.toGlyph;
        }, 3000)) qFatal("Actual diacritic menu did not update only the fixed mode");
        if (!dock->grab().save(QString("/work/") + operandCapture("edited")))
            qFatal("Actual diacritic edited dock capture failed");
        captureLocalGlyphWidget(context, "edited");
        std::printf("QTM actual local diacritic menu edit: triplet=%d column=%d mode=%d-to-%d code=%d\n",
            context.tripletIndex, context.tripletColumn, context.glyphMode, context.toGlyphMode, context.toGlyph);
        return;
    }
    QTest::mouseClick(colour, Qt::LeftButton, Qt::NoModifier, colour->rect().center());
    auto popup = colour->view();
    if (!QTest::qWaitFor([popup] { return popup->isVisible(); }, 3000))
        qFatal("Actual colour popup did not open");
    const QModelIndex choice = popup->model()->index(expectedTo - offset, colour->modelColumn(), colour->rootModelIndex());
    popup->scrollTo(choice, QAbstractItemView::PositionAtCenter);
    const auto observeFailure = [popup, table, colour, dock, main, context, raw, expectedTo, operandCapture](const char *stage) {
        auto focus = QApplication::focusWidget();
        std::fprintf(stderr,
            "QTM foreground failure stage=%s target=%d table_row=%d raw=%d/%d/%d combo=%d expected=%d popup_visible=%d popup_current=%d focus=%s active_popup=%s\n",
            stage, context.tripletIndex, table->currentIndex().row(), raw(0), raw(1), raw(2),
            colour->currentIndex(), expectedTo, popup->isVisible(), popup->currentIndex().row(),
            focus ? focus->metaObject()->className() : "none",
            QApplication::activePopupWidget() ? QApplication::activePopupWidget()->metaObject()->className() : "none");
        std::fprintf(stderr, "QTM foreground failure modified=%d title=%s\n", main->isWindowModified(),
            main->windowTitle().toUtf8().constData());
        if (!dock->grab().save(QString("/work/") + operandCapture("failed-dock")))
            qFatal("Foreground failure dock capture failed");
        capture(operandCapture("failed-main").toUtf8().constData());
        if (popup->isVisible()
            && !popup->window()->grab().save(QString("/work/") + operandCapture("failed-popup")))
            qFatal("Foreground failure popup capture failed");
        std::fflush(stderr);
    };
    if (!QTest::qWaitFor([popup, choice] {
        const QPoint point = popup->visualRect(choice).center();
        return choice.isValid() && popup->viewport()->rect().contains(point)
            && popup->indexAt(point) == choice;
    }, 3000)) {
        observeFailure("choice-layout");
        qFatal("Actual replacement colour item is not clickable");
    }
    // Qt's combo popup activates its current hovered item. A synthetic click
    // alone has no preceding mouse movement; scrollTo does not hover the item.
    const QPoint point = popup->visualRect(choice).center();
    QTest::mouseMove(popup->viewport(), point);
    if (!QTest::qWaitFor([popup, choice, point] {
        return popup->isVisible() && popup->indexAt(point) == choice
            && popup->currentIndex() == choice;
    }, 3000)) {
        observeFailure("choice-hover");
        qFatal("Actual replacement colour item did not receive mouse hover");
    }
    if (!popup->window()->grab().save(QString("/work/") + operandCapture("popup")))
        qFatal("Actual foreground popup capture failed");
    QTest::mouseClick(popup->viewport(), Qt::LeftButton, Qt::NoModifier, point);
    if (!QTest::qWaitFor([colour, main, context, raw, expectedMode, expectedTo, offset] {
        return colour->currentIndex() == expectedTo - offset && main->isWindowModified()
            && raw(0) == context.tripletColumn && raw(1) == expectedMode && raw(2) == expectedTo;
    }, 3000)) {
        observeFailure("activation");
        qFatal("Actual foreground widget edit did not update the expected X/26 tuple");
    }
    if (!dock->grab().save(QString("/work/") + operandCapture("edited")))
        qFatal("Actual foreground dock edited capture failed");
    if (context.editLocalGlyph) {
        captureLocalGlyphWidget(context, "edited");
        std::printf("QTM actual local glyph popup edit: triplet=%d column=%d mode=%d from=%d to=%d\n",
            context.tripletIndex, context.tripletColumn, expectedMode, expectedFrom, expectedTo);
    } else std::printf("QTM actual foreground popup edit: triplet=%d column=%d mode=0 from=%d to=%d\n",
        context.tripletIndex, context.tripletColumn, context.fromColour, context.toColour);
}
// One fixed structural trial. The model is read only; changes below are made
// through the existing Insert copy/Delete buttons, cooked column spinbox and
// character combo popup. The bounded snapshot is an oracle, never editor input.
using StructureTriplet = std::array<int, 3>;
static std::vector<StructureTriplet> savedGlyphStructure;
static StructureTriplet structureTriplet(QAbstractItemModel *model, int row) {
    StructureTriplet tuple;
    for (int column = 0; column < 3; ++column) {
        bool valid = false;
        tuple[column] = model->data(model->index(row, column), Qt::UserRole).toInt(&valid);
        if (!valid) qFatal("Fixed structure raw triplet absent");
    }
    return tuple;
}
static bool structureMatches(QAbstractItemModel *model, const std::vector<StructureTriplet> &expected) {
    if (!model || expected.empty() || expected.size() > 208
        || model->columnCount() != 4 || model->rowCount() != int(expected.size())) return false;
    for (int row = 0; row < int(expected.size()); ++row)
        if (structureTriplet(model, row) != expected[row]) return false;
    return true;
}
static void structureCheck(QAbstractItemModel *model, const std::vector<StructureTriplet> &expected) {
    if (!QTest::qWaitFor([model, &expected] { return structureMatches(model, expected); }, 3000))
        qFatal("Fixed structure changed an unexpected triplet, pointer, marker or row count");
}
// cac2167 x26commands.cpp retains existing Local invocation pointers, and
// x26triplets.cpp regenerates definition-number fields after actual GUI edits.
static void structureRelocate(std::vector<StructureTriplet> &expected, int boundary, int delta) {
    for (int row = 0; row < int(expected.size()); ++row) {
        auto &tuple = expected[row];
        int target = -1;
        if (tuple[0] >= 40 && tuple[1] >= 0x11 && tuple[1] <= 0x13
            && (tuple[0] & 0x18) == 8) {
            target = (((tuple[0] & 1) << 3) | (tuple[2] >> 4)) * 13 + (tuple[2] & 15);
            if (target >= boundary) target += delta;
        } else if (tuple[0] >= 40 && tuple[1] >= 0x15 && tuple[1] <= 0x17) target = row;
        if (target >= 0) {
            if (target >= 208) qFatal("Fixed structure relocated target exceeds208 words");
            tuple[0] = (tuple[0] & 0x38) | int(target >= 104);
            tuple[2] = (((target / 13) & 7) << 4) | (target % 13);
        }
    }
}
static void structureSelect(QTableView *table, int row) {
    auto target = table->model()->index(row, 0);
    table->scrollTo(target, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([table, target] {
        return target.isValid() && table->viewport()->rect().contains(table->visualRect(target).center());
    }, 3000)) qFatal("Fixed structure triplet is not clickable");
    QTest::mouseClick(table->viewport(), Qt::LeftButton, Qt::NoModifier, table->visualRect(target).center());
    if (table->currentIndex().row() != row) qFatal("Fixed structure triplet selection failed");
}
static void structureButton(QDockWidget *dock, const char *text) {
    QPushButton *button = nullptr;
    for (QPushButton *candidate : dock->findChildren<QPushButton *>()) {
        QString label = candidate->text(); label.remove('&');
        if (label != text || !candidate->isVisible() || !candidate->isEnabled() || candidate->menu()) continue;
        if (button) qFatal("Ambiguous fixed structure button");
        button = candidate;
    }
    if (!button) qFatal("Actual fixed structure Insert copy/Delete button absent");
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
static void structureCode(QDockWidget *dock, const DriverContext &context, int row, int from, int to) {
    QComboBox *combo = nullptr;
    for (QComboBox *candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != 96) continue;
        if (combo) qFatal("Ambiguous fixed structure character combo");
        combo = candidate;
    }
    if (!combo || from < 32 || from > 127 || to < 32 || to > 127
        || combo->currentIndex() != from - 32) qFatal("Fixed structure character source differs");
    QTest::mouseClick(combo, Qt::LeftButton, Qt::NoModifier, combo->rect().center());
    auto popup = combo->view();
    if (!QTest::qWaitFor([popup] { return popup->isVisible(); }, 3000))
        qFatal("Actual structure character popup did not open");
    auto choice = popup->model()->index(to - 32, combo->modelColumn(), combo->rootModelIndex());
    popup->scrollTo(choice, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([popup, choice] {
        return choice.isValid() && popup->viewport()->rect().contains(popup->visualRect(choice).center());
    }, 3000)) qFatal("Actual structure character choice is not clickable");
    const QPoint point = popup->visualRect(choice).center();
    QTest::mouseMove(popup->viewport(), point);
    if (!QTest::qWaitFor([popup, choice, point] {
        return popup->isVisible() && popup->indexAt(point) == choice && popup->currentIndex() == choice;
    }, 3000)) qFatal("Actual structure character choice did not receive hover");
    const QByteArray stage = QByteArray("local-structure-popup-") + QByteArray::number(row);
    if (!popup->window()->grab().save(QString("/work/") + context.captureName(stage.constData())))
        qFatal("Actual structure character popup capture failed");
    QTest::mouseClick(popup->viewport(), Qt::LeftButton, Qt::NoModifier, point);
    if (!QTest::qWaitFor([combo, to] { return combo->currentIndex() == to - 32; }, 3000))
        qFatal("Actual structure character choice was not activated");
}
static void structureColumnDown(QDockWidget *dock) {
    QSpinBox *column = nullptr;
    for (QSpinBox *candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 39) continue;
        if (column) qFatal("Ambiguous fixed structure cooked Column spinbox");
        column = candidate;
    }
    if (!column || column->value() != 3) qFatal("Fixed structure original column3 differs");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const QRect step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxDown, column);
    if (!step.isValid() || !column->rect().contains(step.center()))
        qFatal("Actual structure Column step-down is not clickable");
    QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([column] { return column->value() == 2; }, 3000))
        qFatal("Actual structure Column did not step3-to2");
}
static void editLocalGlyphStructure(QMainWindow *main, const DriverContext &context) {
    if (!context.editLocalGlyphStructure || context.tripletIndex != 12 || context.tripletColumn != 0
        || main->isWindowModified()) qFatal("Unexpected fixed Local structure baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual structure X/26 dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual structure dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual structure requires exactly one X/26 table");
    auto table = tables.front();
    auto model = table->model();
    if (!model || model->rowCount() != 26 || model->columnCount() != 4)
        qFatal("Fixed structure requires exactly26 original model triplets");
    const std::array<StructureTriplet, 9> original{{
        {0,0,18}, {0,3,0}, {0,8,0}, {0,9,65}, {0,9,65}, {1,1,33}, {2,12,32}, {2,9,66}, {3,9,67}
    }};
    const std::array<StructureTriplet, 15> final{{
        {0,0,18}, {0,3,0}, {0,8,0}, {0,9,65}, {1,1,34}, {1,1,35}, {1,1,36}, {1,1,37},
        {1,1,33}, {1,1,38}, {1,1,39}, {2,12,32}, {2,9,66}, {2,9,90}, {2,9,67}
    }};
    std::vector<StructureTriplet> expected;
    for (int row = 0; row < 26; ++row) expected.push_back(structureTriplet(model, row));
    for (int offset = 0; offset < 9; ++offset)
        if (expected[12 + offset] != original[offset]) qFatal("Original nine Local commands differ");
    if (expected[11] != StructureTriplet{40,23,11} || expected[21] != StructureTriplet{40,23,24}
        || expected[22] != StructureTriplet{0,8,0} || expected[23] != StructureTriplet{0,12,16}
        || expected[24] != StructureTriplet{0,9,80} || expected[25] != StructureTriplet{63,31,5})
        qFatal("Original target/following markers or protected following program differ");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-structure-before")))
        qFatal("Actual structure before dock capture failed");
    DriverContext paint = context; paint.editLocalGlyph = true;
    captureLocalGlyphWidget(paint, "before");
    structureSelect(table, 15);
    structureButton(dock, "Delete");
    expected.erase(expected.begin() + 15);
    structureRelocate(expected, 15, -1);
    structureCheck(model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("local-structure-deleted")))
        qFatal("Actual structure Delete capture failed");
    for (int copy = 0; copy < 6; ++copy) {
        structureSelect(table, 16);
        structureButton(dock, "Insert copy");
        expected.insert(expected.begin() + 16, StructureTriplet{1,1,33});
        structureRelocate(expected, 16, 1);
        structureCheck(model, expected);
    }
    const int codes[7] = {34,35,36,37,33,38,39};
    for (int offset = 0; offset < 7; ++offset) {
        structureSelect(table, 16 + offset);
        if (codes[offset] == 33) continue;
        structureCode(dock, context, 16 + offset, 33, codes[offset]);
        expected[16 + offset][2] = codes[offset];
        structureCheck(model, expected);
    }
    structureSelect(table, 25);
    structureButton(dock, "Insert copy");
    expected.insert(expected.begin() + 25, StructureTriplet{3,9,67});
    structureRelocate(expected, 25, 1);
    structureCheck(model, expected);
    structureSelect(table, 25);
    structureColumnDown(dock);
    expected[25][0] = 2;
    structureCheck(model, expected);
    structureCode(dock, context, 25, 67, 90);
    expected[25][2] = 90;
    structureCheck(model, expected);
    structureSelect(table, 26);
    structureColumnDown(dock);
    expected[26][0] = 2;
    structureCheck(model, expected);
    for (int offset = 0; offset < 15; ++offset)
        if (expected[12 + offset] != final[offset]) qFatal("Fixed final15 Local commands differ");
    if (!main->isWindowModified() || expected[27] != StructureTriplet{40,23,33})
        qFatal("Actual structure modified state or following marker21-to27 differs");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-structure-edited")))
        qFatal("Actual structure edited dock capture failed");
    captureLocalGlyphWidget(paint, "edited");
    savedGlyphStructure = expected;
    std::puts("QTM actual Local glyph structure: Delete1 InsertCopy7 G0/G1 popup edits column3-to2 original9 final15 followingmarker21-to27 fullmodelchecked");
}
// This second fixed trial retains the exact original DRCS selection prefix
// after deleting its glyph. Its unused-prefix normalization belongs to the
// native importer, never this driver or the actual editor's document model.
static void structureColumnOneToZero(QDockWidget *dock) {
    QSpinBox *column = nullptr;
    for (QSpinBox *candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 39) continue;
        if (column) qFatal("Ambiguous fixed mixed structure Column spinbox");
        column = candidate;
    }
    if (!column || column->value() != 1) qFatal("Fixed mixed structure original column1 differs");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const QRect step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxDown, column);
    if (!step.isValid() || !column->rect().contains(step.center()))
        qFatal("Actual mixed structure Column step-down is not clickable");
    QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([column] { return column->value() == 0; }, 3000))
        qFatal("Actual mixed structure Column did not step1-to0");
}
static void editLocalMixedGlyphStructure(QMainWindow *main, const DriverContext &context) {
    if (!context.editLocalMixedGlyphStructure || context.tripletIndex != 12
        || context.tripletColumn != 0 || main->isWindowModified()
        || context.globalDrcsName.isEmpty()) qFatal("Unexpected fixed mixed Local structure baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual mixed structure X/26 dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual mixed structure dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual mixed structure requires exactly one X/26 table");
    auto table = tables.front();
    auto model = table->model();
    // Pinned PageX26Base trims repeated final padding on load: the TTI has
    // three packets/39 physical words, while this model has32 meaningful rows.
    if (!model || model->rowCount() != 32 || model->columnCount() != 4)
        qFatal("Fixed mixed structure requires exactly32 meaningful model triplets");
    const std::array<StructureTriplet, 15> original{{
        {0,0,18}, {0,3,0}, {0,8,0}, {0,1,65}, {1,15,35}, {2,2,47}, {3,11,47},
        {4,17,65}, {40,24,80}, {5,13,64}, {40,24,16}, {6,13,0}, {7,9,66}, {7,12,32}, {7,9,67}
    }};
    const std::array<StructureTriplet, 15> final{{
        {0,0,18}, {0,3,0}, {0,8,0}, {0,1,65}, {0,15,35}, {1,15,35}, {2,2,47},
        {3,11,47}, {4,17,65}, {40,24,80}, {40,24,16}, {6,13,0}, {7,9,66}, {7,12,32}, {7,9,67}
    }};
    std::vector<StructureTriplet> expected;
    for (int row = 0; row < 32; ++row) expected.push_back(structureTriplet(model, row));
    for (int offset = 0; offset < 15; ++offset)
        if (expected[12 + offset] != original[offset]) qFatal("Original mixed Local body differs");
    if (expected[11] != StructureTriplet{40,23,11} || expected[27] != StructureTriplet{40,23,33}
        || expected[28] != StructureTriplet{0,8,0} || expected[29] != StructureTriplet{0,12,16}
        || expected[30] != StructureTriplet{0,9,80} || expected[31] != StructureTriplet{63,31,5})
        qFatal("Original mixed target/following markers or protected program differ");
    int targetCalls = 0, followingCalls = 0;
    for (int row = 0; row < 11; ++row) {
        if (expected[row] == StructureTriplet{40,19,11}) ++targetCalls;
        if (expected[row] == StructureTriplet{40,19,33}) ++followingCalls;
    }
    if (expected[10] != StructureTriplet{63,31,6} || targetCalls != 2 || followingCalls != 1)
        qFatal("Original mixed Main termination and exact Local invocation counts differ");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-mixed-structure-before")))
        qFatal("Actual mixed structure before dock capture failed");
    DriverContext paint = context; paint.editLocalGlyph = true;
    captureLocalGlyphWidget(paint, "before");
    structureSelect(table, 21);
    structureButton(dock, "Delete");
    expected.erase(expected.begin() + 21);
    structureRelocate(expected, 21, -1);
    structureCheck(model, expected);
    if (expected[20] != StructureTriplet{40,24,80} || expected[26] != StructureTriplet{40,23,32})
        qFatal("Actual mixed Delete did not preserve the original unused selector");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-mixed-structure-deleted")))
        qFatal("Actual mixed structure Delete capture failed");
    structureSelect(table, 16);
    structureButton(dock, "Insert copy");
    expected.insert(expected.begin() + 16, StructureTriplet{1,15,35});
    structureRelocate(expected, 16, 1);
    structureCheck(model, expected);
    structureSelect(table, 16);
    structureColumnOneToZero(dock);
    expected[16][0] = 0;
    structureCheck(model, expected);
    for (int offset = 0; offset < 15; ++offset)
        if (expected[12 + offset] != final[offset]) qFatal("Fixed final mixed Local body differs");
    if (!main->isWindowModified() || expected[21] != StructureTriplet{40,24,80}
        || expected[27] != StructureTriplet{40,23,33})
        qFatal("Actual mixed structure selector, modified state or following marker differs");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-mixed-structure-edited")))
        qFatal("Actual mixed structure edited dock capture failed");
    captureLocalGlyphWidget(paint, "edited");
    savedGlyphStructure = expected;
    std::puts("QTM actual mixed Local glyph structure: DeleteNormalGlyph21 keepOriginalSelector InsertCopyG2at16 Column1-to0 full32-row model checked; raw39-word padding is checked externally");
}
// Fixed style insertion: the selected original mosaic is copied through the
// real button, then the copy's cooked mode and colour are chosen by mouse.
static void styleForegroundMode(QDockWidget *dock, const DriverContext &context) {
    QPushButton *button = nullptr;
    for (QPushButton *candidate : dock->findChildren<QPushButton *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || !candidate->menu()
            || candidate->text() != "G1 block mosaic") continue;
        if (button) qFatal("Ambiguous fixed style cooked-mode button");
        button = candidate;
    }
    if (!button) qFatal("Actual fixed style mosaic mode button absent");
    QMenu *menu = button->menu();
    QTimer::singleShot(150, qApp, [menu, context] {
        const auto actionNamed = [](QMenu *parent, const char *text) {
            QAction *found = nullptr;
            for (QAction *action : parent->actions()) {
                if (action->text() != text || !action->isVisible() || !action->isEnabled()) continue;
                if (found) qFatal("Ambiguous fixed style mode action");
                found = action;
            }
            if (!found) qFatal("Actual fixed style mode action absent");
            return found;
        };
        if (!QTest::qWaitFor([menu] { return menu->isVisible(); }, 3000))
            qFatal("Actual fixed style mode menu did not open");
        QAction *column = actionNamed(menu, "Column triplet");
        QMenu *child = column->menu();
        const QPoint columnPoint = menu->actionGeometry(column).center();
        if (!child || !menu->rect().contains(columnPoint))
            qFatal("Actual fixed style column submenu is not clickable");
        QTest::mouseMove(menu, columnPoint);
        if (!QTest::qWaitFor([menu, column, child] {
            return menu->activeAction() == column && child->isVisible();
        }, 3000)) qFatal("Actual fixed style column submenu did not receive hover");
        QAction *foreground = actionNamed(child, "Foreground colour");
        const QPoint point = child->actionGeometry(foreground).center();
        if (foreground->menu() || !child->rect().contains(point))
            qFatal("Actual fixed style foreground action is not clickable");
        QTest::mouseMove(child, point);
        if (!QTest::qWaitFor([child, foreground] {
            return child->isVisible() && child->activeAction() == foreground;
        }, 3000)) qFatal("Actual fixed style foreground action did not receive hover");
        if (!child->grab().save(QString("/work/") + context.captureName("local-style-mode-popup")))
            qFatal("Actual fixed style mode popup capture failed");
        QTest::mouseClick(child, Qt::LeftButton, Qt::NoModifier, point);
    });
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
static void styleColour19(QDockWidget *dock, const DriverContext &context) {
    QComboBox *colour = nullptr;
    for (QComboBox *candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != 32) continue;
        if (colour) qFatal("Ambiguous fixed style colour selector");
        colour = candidate;
    }
    if (!colour || colour->currentIndex() != 1)
        qFatal("Actual fixed style copied mosaic did not become foreground1");
    QTest::mouseClick(colour, Qt::LeftButton, Qt::NoModifier, colour->rect().center());
    auto popup = colour->view();
    if (!QTest::qWaitFor([popup] { return popup->isVisible(); }, 3000))
        qFatal("Actual fixed style colour popup did not open");
    const QModelIndex choice = popup->model()->index(19, colour->modelColumn(), colour->rootModelIndex());
    popup->scrollTo(choice, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([popup, choice] {
        return choice.isValid() && popup->viewport()->rect().contains(popup->visualRect(choice).center());
    }, 3000)) qFatal("Actual fixed style colour19 is not clickable");
    const QPoint point = popup->visualRect(choice).center();
    QTest::mouseMove(popup->viewport(), point);
    if (!QTest::qWaitFor([popup, choice, point] {
        return popup->isVisible() && popup->indexAt(point) == choice && popup->currentIndex() == choice;
    }, 3000)) qFatal("Actual fixed style colour19 did not receive hover");
    if (!popup->window()->grab().save(QString("/work/") + context.captureName("local-style-colour-popup")))
        qFatal("Actual fixed style colour popup capture failed");
    QTest::mouseClick(popup->viewport(), Qt::LeftButton, Qt::NoModifier, point);
    if (!QTest::qWaitFor([colour] { return colour->currentIndex() == 19; }, 3000))
        qFatal("Actual fixed style foreground19 was not activated");
}
static void editLocalStyleStructure(QMainWindow *main, const DriverContext &context) {
    if (!context.editLocalStyleStructure || context.tripletIndex != 17 || context.tripletColumn != 1
        || main->isWindowModified()) qFatal("Unexpected fixed style structure baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual style X/26 dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual style dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual style trial requires one X/26 table");
    auto table = tables.front();
    auto model = table->model();
    // The two complete input packets contain26 meaningful words. One actual
    // insertion grows the model to27; saved TTI packet padding is checked by
    // the native handoff, never synthesized into this editor document.
    if (!model || model->rowCount() != 26 || model->columnCount() != 4)
        qFatal("Fixed style original must contain26 model triplets");
    const std::array<StructureTriplet, 9> original{{
        {0,0,18}, {0,3,0}, {0,8,0}, {0,9,65}, {0,9,65}, {1,1,33}, {2,12,32}, {2,9,66}, {3,9,67}
    }};
    std::vector<StructureTriplet> expected;
    for (int row = 0; row < 26; ++row) expected.push_back(structureTriplet(model, row));
    for (int offset = 0; offset < 9; ++offset)
        if (expected[12 + offset] != original[offset]) qFatal("Original fixed style body differs");
    if (expected[11] != StructureTriplet{40,23,11} || expected[21] != StructureTriplet{40,23,24}
        || expected[22] != StructureTriplet{0,8,0} || expected[23] != StructureTriplet{0,12,16}
        || expected[24] != StructureTriplet{0,9,80} || expected[25] != StructureTriplet{63,31,5})
        qFatal("Fixed style original markers/protected following program differ");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-style-before")))
        qFatal("Actual style baseline dock capture failed");
    DriverContext paint = context; paint.editLocalGlyph = true;
    captureLocalGlyphWidget(paint, "before");
    structureSelect(table, 17);
    structureButton(dock, "Insert copy");
    expected.insert(expected.begin() + 17, StructureTriplet{1,1,33});
    structureRelocate(expected, 17, 1);
    structureCheck(model, expected);
    structureSelect(table, 17);
    styleForegroundMode(dock, context);
    // Pinned x26model.cpp's real cooked-mode transition clears data0x60,
    // so G1 data33 becomes valid foreground1 while preserving address1.
    expected[17] = StructureTriplet{1,0,1};
    structureCheck(model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("local-style-mode-edited")))
        qFatal("Actual style cooked mode dock capture failed");
    styleColour19(dock, context);
    expected[17] = StructureTriplet{1,0,19};
    structureCheck(model, expected);
    if (!main->isWindowModified() || expected.size() != 27
        || expected[18] != StructureTriplet{1,1,33} || expected[22] != StructureTriplet{40,23,25})
        qFatal("Fixed style final insertion/modified state/following marker differs");
    if (!dock->grab().save(QString("/work/") + context.captureName("local-style-edited")))
        qFatal("Actual style edited dock capture failed");
    captureLocalGlyphWidget(paint, "edited");
    savedGlyphStructure = expected;
    std::puts("QTM actual Local style structure: InsertCopy17 mosaic1/1/33, cooked Foreground1, colour19; exact26-to27 model and marker21-to22 checked");
}
// The genuine small POP companion uses an editor-only Local wrapper. These
// fixed raw tuples authenticate that wrapper without granting native ownership.
static void resourceColumnOneToTwo(QDockWidget *dock) {
    QSpinBox *column = nullptr;
    for (QSpinBox *candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 39) continue;
        if (column) qFatal("Ambiguous resource structure cooked Column spinbox");
        column = candidate;
    }
    if (!column || column->value() != 1)
        qFatal("Fixed resource copied glyph column1 differs");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const QRect step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxUp, column);
    if (!step.isValid() || !column->rect().contains(step.center()))
        qFatal("Actual resource Column step-up is not clickable");
    QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([column] { return column->value() == 2; }, 3000))
        qFatal("Actual resource Column did not step1-to2");
}
static void editResourceGlyphStructure(QMainWindow *main, const DriverContext &context) {
    if (context.insertResourceGlyph == context.deleteResourceGlyph
        || context.tripletIndex != 7 || context.tripletColumn != 1
        || context.leftPanelColumns != 0 || context.rightPanelColumns != 0
        || !context.drcsName.isEmpty() || main->isWindowModified())
        qFatal("Unexpected fixed resource-object structure baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets")
        qFatal("Actual resource structure X/26 dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000))
        qFatal("Actual resource structure dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual resource structure requires one X/26 table");
    auto table = tables.front();
    auto model = table->model();
    // PageX26Base removes repeated final5 packet padding on load, leaving nine
    // meaningful original rows from the immutable thirteen-word TTI packet.
    std::vector<StructureTriplet> expected{
        {40,19,2}, {63,31,6}, {40,23,2}, {44,4,1}, {1,12,0},
        {1,0,2}, {1,3,0}, {1,9,80}, {63,31,5}
    };
    structureCheck(model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-structure-before")))
        qFatal("Actual resource structure baseline dock capture failed");
    structureSelect(table, 7);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-structure-selected")))
        qFatal("Actual resource structure selected G0P capture failed");
    if (context.insertResourceGlyph) {
        structureButton(dock, "Insert copy");
        expected.insert(expected.begin() + 7, StructureTriplet{1,9,80});
        structureRelocate(expected, 7, 1);
        structureCheck(model, expected);
        if (!dock->grab().save(QString("/work/") + context.captureName("resource-structure-copied")))
            qFatal("Actual resource Insert copy capture failed");
        // Both rows initially contain P/column1. Change the second row through
        // the real cooked Column control; the final semantic object is P then P.
        structureSelect(table, 8);
        resourceColumnOneToTwo(dock);
        expected[8] = StructureTriplet{2,9,80};
    } else {
        structureButton(dock, "Delete");
        expected.erase(expected.begin() + 7);
        structureRelocate(expected, 7, -1);
    }
    structureCheck(model, expected);
    if (!main->isWindowModified() || expected.back() != StructureTriplet{63,31,5}
        || expected.size() != std::size_t(context.insertResourceGlyph ? 10 : 8))
        qFatal("Fixed resource structure result or modified state differs");
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-structure-edited")))
        qFatal("Actual resource structure edited dock capture failed");
    savedGlyphStructure = expected;
    std::printf("QTM actual resource-object G0P %s: original9 model rows, final%zu, original command4/editor word7, complete wrapper retained\n",
        context.insertResourceGlyph ? "InsertCopy then second glyph Column1-to2" : "Delete", expected.size());
}
static void verifySavedGlyphStructure(QMainWindow *main, int expectedCount = 32) {
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || savedGlyphStructure.size() != std::size_t(expectedCount)) qFatal("Fixed saved structure oracle absent");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Saved structure table absent");
    structureCheck(tables.front()->model(), savedGlyphStructure);
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
                const auto focusFailure = [dialog, main, name, context](const char *stage) {
                    auto active = QApplication::activeWindow();
                    auto focus = QApplication::focusWidget();
                    auto nativeFocus = QGuiApplication::focusWindow();
                    std::fprintf(stderr,
                        "QTM Save As focus failure stage=%s dialog_title=%s dialog_active=%d dialog_visible=%d input_enabled=%d input_visible=%d input_focus=%d focus_class=%s active_class=%s active_title=%s native_focus_title=%s\n",
                        stage, dialog->windowTitle().toUtf8().constData(), dialog->isActiveWindow(),
                        dialog->isVisible(), name->isEnabled(), name->isVisible(), name->hasFocus(),
                        focus ? focus->metaObject()->className() : "none",
                        active ? active->metaObject()->className() : "none",
                        active ? active->windowTitle().toUtf8().constData() : "none",
                        nativeFocus ? nativeFocus->title().toUtf8().constData() : "none");
                    const bool dialogCaptured = dialog->isVisible()
                        && dialog->grab().save(QString("/work/") + context.captureName("save-focus-failed-dialog"));
                    const bool mainCaptured = main->isVisible()
                        && main->grab().save(QString("/work/") + context.captureName("save-focus-failed-main"));
                    std::fprintf(stderr, "QTM Save As focus failure captures: dialog=%d main=%d\n",
                        dialogCaptured, mainCaptured);
                    std::fflush(stderr);
                };
                // The exposed floating Tools dock or another headed application
                // may still own X11 activation when the modal Save As opens.
                // Arrange the actual dialog, then require OS-observed activation
                // before the real filename click; do not assign input focus.
                dialog->raise();
                dialog->activateWindow();
                if (auto handle = dialog->windowHandle()) handle->requestActivate();
                if (!QTest::qWaitForWindowActive(dialog, 5000)) {
                    focusFailure("dialog-activation");
                    qFatal("Actual Save As dialog did not become active");
                }
                // QtTest's direct widget click can leave the native modal
                // window behind QGraphicsView. Send a real X11 pointer click
                // to the visible input, keeping the observed-focus guard.
                const QPoint target = name->mapToGlobal(name->rect().center());
                QProcess pointer;
                pointer.start("/usr/bin/xdotool", {"mousemove",
                    QString::number(target.x()), QString::number(target.y()), "click", "1"});
                if (!pointer.waitForFinished(3000)) {
                    pointer.kill();
                    pointer.waitForFinished(1000);
                    focusFailure("native-pointer-timeout");
                    qFatal("Actual X11 filename pointer input did not finish");
                }
                if (pointer.exitStatus() != QProcess::NormalExit || pointer.exitCode() != 0) {
                    focusFailure("native-pointer-exit");
                    qFatal("Actual X11 filename pointer input failed");
                }
                // WSLg can deliver the native click to the file tree after a
                // modal reposition. Use the dialog's normal File name mnemonic
                // as a bounded fallback, retaining the observed-focus guard.
                if (!QTest::qWaitFor([name] { return name->hasFocus(); }, 3000))
                    QTest::keyClick(dialog, Qt::Key_N, Qt::AltModifier);
                if (!QTest::qWaitFor([name] { return name->hasFocus(); }, 3000)) {
                    focusFailure("filename-click");
                    qFatal("Actual Save As filename input did not receive focus");
                }
                // Consume the owned native click before selecting text: the
                // dialog can already report focus while its click is queued.
                QTest::qWait(100);
                QTest::keyClick(name, Qt::Key_A, Qt::ControlModifier);
                if (!QTest::qWaitFor([name] {
                    return !name->text().isEmpty() && name->selectedText() == name->text();
                }, 3000)) {
                    focusFailure("filename-selection");
                    qFatal("Actual Save As filename was not fully selected");
                }
                QTest::keyClicks(name, context.outputPath, Qt::NoModifier, 5);
                // Commit the actual filename field before selecting Save. A
                // directory-completion candidate can otherwise keep Open active.
                QTest::keyClick(name, Qt::Key_Tab);
                QTimer::singleShot(200, qApp, [dialog, context] {
                    const bool selected = QTest::qWaitFor([dialog, context] {
                        if (dialog->selectedFiles() != QStringList{context.outputPath}) return false;
                        for (QPushButton *button : dialog->findChildren<QPushButton *>()) {
                            QString text = button->text(); text.remove('&');
                            if (text == "Save" && button->isEnabled()) return true;
                        }
                        return false;
                    }, 3000);
                    if (!dialog->grab().save(QString("/work/") + context.captureName("save-dialog")))
                        qFatal("Save dialog capture failed");
                    if (!selected) {
                        std::fprintf(stderr, "Save context title=%s mode=%d filemode=%d expected=%s selected=%s\n",
                            dialog->windowTitle().toUtf8().constData(), int(dialog->acceptMode()), int(dialog->fileMode()),
                            context.outputPath.toUtf8().constData(), dialog->selectedFiles().join("|").toUtf8().constData());
                        qFatal("Unexpected selected audit destination");
                    }
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
        if (context.editLocalGlyphStructure || context.editLocalMixedGlyphStructure) verifySavedGlyphStructure(saved);
        if (context.editLocalStyleStructure) verifySavedGlyphStructure(saved, 27);
        if (context.insertResourceGlyph || context.deleteResourceGlyph)
            verifySavedGlyphStructure(saved, context.insertResourceGlyph ? 10 : 8);
        capture(context.captureName("saved").toUtf8().constData());
        if (context.panelProof) capturePanelPaint(context, "saved");
        if (context.singlePage) {
            std::printf("QTM actual single-page widget/Save As complete: edited=%d input=%s output=%s\n",
                (context.editZone || context.editForeground || context.editDrcs || context.editLocalGlyph || context.editLocalGlyphStructure || context.editLocalMixedGlyphStructure || context.editLocalStyleStructure || context.insertResourceGlyph || context.deleteResourceGlyph), context.inputName.constData(), context.outputName.constData());
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
        // A second real X11 process may expose later than the fixed timer.
        // Wait on Qt's actual window state; retain capture's strict headed gate.
        if (!QTest::qWaitForWindowExposed(main, 5000))
            qFatal("Actual editor window was not exposed before bounded deadline");
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
        loadGlobalDrcs(main, context);
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
            std::printf("QTM actual widget geometry: width=%d height=%d left=%d right=%d\n",
                view->width(), view->height(), context.leftPanelColumns, context.rightPanelColumns);
            if (context.editForeground || context.editLocalGlyph) editForeground(main, context);
            if (context.editLocalGlyphStructure) editLocalGlyphStructure(main, context);
            if (context.editLocalMixedGlyphStructure) editLocalMixedGlyphStructure(main, context);
            if (context.editLocalStyleStructure) editLocalStyleStructure(main, context);
            if (context.insertResourceGlyph || context.deleteResourceGlyph) editResourceGlyphStructure(main, context);
            if (context.editZone) {
                QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier,
                    QPoint((context.leftPanelColumns + 8) * 12 + 6, 2 * 10 + 5));
                QTest::keyClicks(view, "Q", Qt::NoModifier, 20);
            }
            if (context.editDrcs) {
                // Fixed singleton/set synthetic DRCS carrier: OL1 column1 is A.
                // Ordinary overwrite A->C adds bit1 to PTU0's first bitmap row.
                // The controller must verify the original and every saved byte;
                // the widget driver grants no resource-import permission.
                QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(18, 15));
                QTest::keyClicks(view, "C", Qt::NoModifier, 20);
                if (!QTest::qWaitFor([main] { return main->isWindowModified(); }, 3000))
                    qFatal("Actual DRCS carrier overwrite did not modify the document");
                std::puts("QTM actual DRCS carrier keyboard overwrite: row=1 column=1 A->C");
            }
            QTimer::singleShot(100, qApp, [main, context] {
                if (context.panelProof) {
                    if (main->isWindowModified() != context.editZone)
                        qFatal("Fixed panel Day Q/Night no-edit modified state differs");
                    capturePanelPaint(context, "edited");
                }
                capture(context.captureName("edited").toUtf8().constData());
                if (context.resourceSet) {
                    auto view = editor();
                    if (!view) qFatal("Resource-set navigation editor absent");
                    QTest::keyClick(view, Qt::Key_PageUp);
                    QTimer::singleShot(100, qApp, [view, main, context] {
                        capture(context.captureName("resource-next").toUtf8().constData());
                        QTest::keyClick(view, Qt::Key_PageDown);
                        QTimer::singleShot(100, qApp, [main, context] {
                            capture(context.captureName("resource-return").toUtf8().constData());
                            std::puts("QTM actual DRCS resource-set PageUp/PageDown navigation complete");
                            saveAs(main, context);
                        });
                    });
                    return;
                }
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
