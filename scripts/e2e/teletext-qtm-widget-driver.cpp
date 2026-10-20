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
#include <QMenuBar>
#include <QSignalSpy>
#include <QMenu>
#include <QMessageBox>
#include <QRadioButton>
#include <QSpinBox>
#include <QStyle>
#include <QStyleOptionSpinBox>
#include <QStyleOptionButton>
#include <QLineEdit>
#include <QPushButton>
#include <QProcess>
#include <QKeyEvent>
#include <QTimer>
#include <QWindow>
#include <QtTest/QTest>
#include <dlfcn.h>
#include <cstdio>
#include <array>
#include <vector>
#include <utility>

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
    bool editResourceCallOrigin = false;
    bool resourceCallContext = false;
    bool insertResourceCall = false;
    bool deleteResourceCall = false;
    bool mixedResourceCallGlyph = false;
    bool mixedResourceCallStructure = false;
    bool insertFragmentCall = false;
    bool deleteFragmentCall = false;
    int fragmentCallSelector = 0;
    bool addFirstDefinitionPosition = false;
    bool addInteriorDefinitionRow = false;
    bool editMainG2 = false;
    bool editNativeG0 = false;
    int nativeUnicode = 0;
    bool editBaseSpacing = false;
    bool createMain = false;
    bool createMainGeneratedBase = false;
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
    const bool createMainGeneratedBase = mode == "single-page-create-main-generated-base";
    const bool createMain = mode == "single-page-create-main" || createMainGeneratedBase;
    const bool glyphStructure = mode == "single-page-local-glyph-structure";
    const bool mixedGlyphStructure = mode == "single-page-local-mixed-glyph-structure";
    const bool styleStructure = mode == "single-page-local-style-structure";
    const bool resourceInsert = mode == "resource-object-g0-insert-copy";
    const bool resourceDelete = mode == "resource-object-g0-delete";
    const bool resourceStructure = resourceInsert || resourceDelete;
    const bool firstDefinitionPosition = mode == "new-empty-definition-position";
    const bool interiorDefinitionRow = mode == "added-definition-interior-row";
    const bool mixedResourceCallGlyph = mode == "resource-object-call-insert-glyph-context";
    const bool mixedResourceCallStructure = mode == "resource-object-call-insert-structure-context";
    const bool insertResourceCall = mode == "resource-object-call-insert-context" || mixedResourceCallGlyph || mixedResourceCallStructure;
    const bool deleteResourceCall = mode == "resource-object-call-delete-context";
    const bool callTopology = insertResourceCall || deleteResourceCall;
    const bool resourceCallContext = mode == "resource-object-call-origin-context" || callTopology;
    const bool resourceCallOrigin = mode == "resource-object-call-origin" || mode == "resource-object-call-origin-context";
    const bool insertFragmentCall = mode == "resource-fragment-call-insert";
    const bool deleteFragmentCall = mode == "resource-fragment-call-delete";
    const bool fragmentCall = insertFragmentCall || deleteFragmentCall;
    const bool suppliedFragmentSelector = qEnvironmentVariableIsSet("ERSATZRS_QTM_FRAGMENT_CALL_SELECTOR");
    if (fragmentCall != suppliedFragmentSelector)
        qFatal("Native fragment call selector requires exactly the explicit fragment trial");
    const int fragmentSelector = fragmentCall ? boundedDecimal("ERSATZRS_QTM_FRAGMENT_CALL_SELECTOR", 0x3ffff) : 0;
    if (fragmentCall && ((fragmentSelector & 0x38) != 0x30 || ((fragmentSelector >> 6) & 31) != 19))
        qFatal("Native fragment call selector must retain authenticated Public Passive scope/type");
    const bool resourceTrial = resourceStructure || resourceCallOrigin || callTopology || fragmentCall || firstDefinitionPosition || interiorDefinitionRow;
    const bool generatedBaseG2 = mode == "single-page-base-g2";
    const bool mainG2 = mode == "single-page-main-g2" || generatedBaseG2;
    const bool nativeG0 = mode == "single-page-native-g0";
    const bool baseSpacing = mode == "single-page-base-spacing"
        || mode == "single-page-enhanced-spacing";
    if (nativeG0 != qEnvironmentVariableIsSet("ERSATZRS_QTM_NATIVE_G0_UNICODE"))
        qFatal("Native G0 input requires exactly the fixed native trial");
    const int nativeUnicode = nativeG0 ? boundedDecimal("ERSATZRS_QTM_NATIVE_G0_UNICODE", 0xffff) : 0;
    if (nativeG0 && nativeUnicode != 0x0409 && nativeUnicode != 0x0416
        && nativeUnicode != 0x0406 && nativeUnicode != 0x05d0)
        qFatal("Native G0 trial supports only the four qualified keyboard inputs");
    const bool extendedGlyph = mode == "single-page-local-extended-glyph" || mainG2;
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
    if (extendedGlyph && !((glyphMode == 15 && toGlyphMode == 15 && fromGlyph == 35
            && (generatedBaseG2 ? (toGlyph == 36 || toGlyph == 37) : toGlyph == 66))
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
    if (!foreground && (suppliedForeground || (!localGlyph && !glyphStructure && !mixedGlyphStructure && !styleStructure && !resourceTrial && suppliedTarget)))
        qFatal("Foreground operands require explicit single-page-foreground mode");
    const int triplet = (foreground || localGlyph || glyphStructure || mixedGlyphStructure || styleStructure || resourceTrial) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_INDEX", 207) : 0;
    const int column = (foreground || localGlyph || glyphStructure || mixedGlyphStructure || styleStructure || resourceTrial) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_COLUMN", (drcsSelection || resourceCallOrigin || callTopology || fragmentCall) ? 40 : 39) : 0;
    if (drcsSelection && (column != 40 || !(triplet < glyph0 && glyph0 < glyph1)))
        qFatal("Fixed DRCS prefix address40 and ordered distinct glyph indices are required");
    if ((glyphStructure || mixedGlyphStructure) && (triplet != 12 || column != 0))
        qFatal("Fixed Local structure trial requires body word12 and column0");
    if (styleStructure && (triplet != 17 || column != 1))
        qFatal("Fixed Local style structure requires original mosaic word17 and column1");
    if (resourceStructure && (triplet != 7 || column != 1))
        qFatal("Fixed resource structure requires original G0P word7 and column1");
    if ((resourceCallOrigin || callTopology) && (triplet != 7 || column != 40))
        qFatal("Fixed resource call requires original OriginModifier word7 and raw address40");
    if (fragmentCall && (triplet != 22 || column != 40))
        qFatal("Fixed native fragment requires original OriginModifier row22 and address40");
    if (firstDefinitionPosition && (triplet != 3 || column != 8))
        qFatal("Fixed empty definition requires first word3 and new glyph column8");
    if (interiorDefinitionRow && (triplet != 5 || column != 6))
        qFatal("Fixed interior definition requires retained row3 word5 and new glyph column6");
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
    if ((mode != "single-page-zone-q" && mode != "single-page-save" && !nativeG0 && !baseSpacing && !foreground && !createMain && !drcsEdit && !drcsVerify && !panelProof && !localGlyph && !glyphStructure && !mixedGlyphStructure && !styleStructure && !resourceTrial) || !safeTtiBasename(input) || !safeTtiBasename(output)
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
    if ((localGlyph || glyphStructure || mixedGlyphStructure || styleStructure) && !mainG2 && (left != 4 || right != 12 || drcs.isEmpty() || drcsSubpage != 0))
        qFatal("Fixed local glyph trial requires original left4/right12 and Normal S0 companion");
    if (mainG2 && (left != 0 || right != 0 || !drcs.isEmpty() || !globalDrcs.isEmpty()
        || glyphMode != 15 || toGlyphMode != 15 || fromGlyph != 35
        || (generatedBaseG2 ? (triplet != 1 || column != 8 || (toGlyph != 36 && toGlyph != 37))
            : (triplet != 206 || column != 0 || toGlyph != 66))))
        qFatal("Fixed standalone Main/Base G2 requires its exact word/data, zero panels and no banks");
    if ((nativeG0 || baseSpacing) && (left != 0 || right != 0 || !drcs.isEmpty() || !globalDrcs.isEmpty()))
        qFatal("Fixed native G0 trial requires zero panels and no banks");
    if (createMain && (qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_BASENAME")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE")))
        qFatal("Fixed absent-Main trial requires no DRCS companion context");
    if (resourceTrial && (left != 0 || right != 0
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_BASENAME")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE")))
        qFatal("Fixed resource structure requires zero panels and no DRCS companion context");
    return {true, mode == "single-page-zone-q" || panelDay, foreground, drcsEdit, drcsVerify, resourceSet, triplet, column, from, to, drcsSubpage, input, output, QString("/work/") + QString::fromLatin1(input),
        QString("/work/") + QString::fromLatin1(output), drcs, left, right, panelProof, panelNight, localGlyph, glyphMode, fromGlyph, toGlyph, toGlyphMode, drcsSlot, flash, charset, drcsSubtable, drcsScope, glyph0, glyph1, globalDrcs, glyphStructure, mixedGlyphStructure, styleStructure, resourceInsert, resourceDelete, resourceCallOrigin, resourceCallContext, insertResourceCall, deleteResourceCall, mixedResourceCallGlyph, mixedResourceCallStructure, insertFragmentCall, deleteFragmentCall, fragmentSelector, firstDefinitionPosition, interiorDefinitionRow, mainG2, nativeG0, nativeUnicode, baseSpacing, createMain, createMainGeneratedBase};
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
        || !view->isVisible()
        || (context.editMainG2 ? (view->width() < 480 || view->height() < 250)
                               : view->size() != QSize(672, 250)))
        qFatal("Actual glyph widget must be exposed at its fixed native geometry");
    const QImage painted = view->grab().toImage();
    if ((!context.editMainG2 && painted.size() != QSize(672, 250))
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
    // Selecting a row updates the cooked operand controls through Qt signals.
    // Wait for that existing UI transition; never assign model/control values.
    if (context.editMainG2 && !QTest::qWaitFor([dock, main, expectedFrom, offset, comboSize] {
        int matching = 0;
        for (QComboBox *candidate : dock->findChildren<QComboBox *>())
            if (candidate->isVisible() && candidate->isEnabled()
                && candidate->count() == comboSize
                && candidate->currentIndex() == expectedFrom - offset) ++matching;
        return matching == 1 && !main->isWindowModified();
    }, 3000))
        qFatal("Actual operand controls did not settle on the original tuple");
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
// Qt6.10 QMenu Home only scrolls scrollable menus. Down establishes an empty
// keyboard selection; bounded Up/Down reaches the named action without assigning
// active actions, triggering QAction directly or mutating the editor model.
static void interiorDefinitionMenuSelect(QMenu *menu, QAction *target) {
    if (!menu || !target || !menu->isVisible() || !target->isVisible() || !target->isEnabled()
        || menu->actions().size() > 128 || !menu->actions().contains(target))
        qFatal("Invalid fixed interior-definition menu target");
    if (!menu->activeAction()) QTest::keyClick(menu, Qt::Key_Down);
    if (!QTest::qWaitFor([menu] { return menu->isVisible() && menu->activeAction(); }, 3000))
        qFatal("Actual interior-definition menu Down did not select an action");
    const int targetIndex = menu->actions().indexOf(target);
    for (int step = 0; menu->activeAction() != target && step < menu->actions().size(); ++step) {
        auto before = menu->activeAction();
        const int currentIndex = menu->actions().indexOf(before);
        if (currentIndex < 0) qFatal("Actual interior-definition current menu action is foreign");
        QTest::keyClick(menu, currentIndex < targetIndex ? Qt::Key_Down : Qt::Key_Up);
        if (!QTest::qWaitFor([menu, before] {
            return menu->isVisible() && menu->activeAction() && menu->activeAction() != before;
        }, 3000)) qFatal("Actual interior-definition menu arrow did not move once");
    }
    if (!menu->isVisible() || menu->activeAction() != target)
        qFatal("Actual interior-definition menu did not select the exact named action");
}
// Pinned mainwindow.cpp873..880: ordinary Insert/Mosaics hold menu actions.
// Drive visible menus; neither QAction nor a document/model setter is invoked.
static void editBaseSpacing(QMainWindow *main, const DriverContext &context) {
    auto view = editor();
    if (!view || main->isWindowModified()
        || !view->grab().save(QString("/work/") + context.captureName("base-spacing-before-widget")))
        qFatal("Fixed Base spacing edit requires a pristine visible page");
    auto named = [](const QList<QAction *> &actions, const QString &name) {
        QAction *found = nullptr;
        for (auto action : actions) {
            QString text = action->text(); text.remove('&');
            if (text != name || !action->isVisible() || !action->isEnabled()) continue;
            if (found) qFatal("Duplicate fixed Base menu action");
            found = action;
        }
        if (!found) qFatal("Fixed Base menu action absent");
        return found;
    };
    auto insertAction = named(main->menuBar()->actions(), "Insert");
    auto insert = insertAction->menu();
    if (!insert) qFatal("Fixed Insert menu absent");
    if (qgetenv("ERSATZRS_QTM_DRIVER_MODE") == "single-page-enhanced-spacing") {
        // Populate ordinary text first. The unchanged editor then inserts all
        // four enhanced controls and their resets through its visible menus.
        QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(6, 45));
        QTest::keyClicks(view, " A  f  B   C    Z", Qt::NoModifier, 20);
        struct Edit { int column; const char *group; const char *action; };
        for (const auto &edit : std::array<Edit, 9>{{
            {0, "Alphanumeric colour", "Black"}, {2, "Alphanumeric colour", "White"},
            {3, "Mosaic colour", "Black"}, {5, "Alphanumeric colour", "White"},
            {6, "Text size", "Double width"}, {9, "Text size", "Normal size"},
            {10, "Text size", "Double size"}, {13, "Text size", "Normal size"},
            {14, "Alphanumeric colour", "White"}
        }}) {
            auto groupAction = named(insert->actions(), edit.group);
            auto group = groupAction->menu();
            if (!group) qFatal("Fixed enhanced spacing submenu absent");
            auto action = named(group->actions(), edit.action);
            QSignalSpy triggered(action, &QAction::triggered);
            if (!triggered.isValid()) qFatal("Fixed enhanced spacing signal unavailable");
            QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier,
                QPoint(edit.column * 12 + 6, 45));
            QTest::mouseClick(main->menuBar(), Qt::LeftButton, Qt::NoModifier,
                main->menuBar()->actionGeometry(insertAction).center());
            if (!QTest::qWaitFor([insert] { return insert->isVisible(); }, 3000))
                qFatal("Actual enhanced spacing Insert menu did not expose");
            interiorDefinitionMenuSelect(insert, groupAction);
            QTest::keyClick(insert, Qt::Key_Right);
            if (!QTest::qWaitFor([group] { return group->isVisible(); }, 3000))
                qFatal("Actual enhanced spacing submenu did not expose");
            interiorDefinitionMenuSelect(group, action);
            const QString stage = QString("enhanced-spacing-menu-%1").arg(edit.column);
            if (!group->grab().save(QString("/work/")
                + context.captureName(stage.toUtf8().constData())))
                qFatal("Actual enhanced spacing menu capture failed");
            QTest::keyClick(group, Qt::Key_Return);
            if (!QTest::qWaitFor([&triggered, main] {
                return triggered.count() == 1 && main->isWindowModified();
            }, 3000)) qFatal("Actual enhanced spacing action did not trigger exactly once");
        }
        if (!view->grab().save(QString("/work/")
            + context.captureName("enhanced-spacing-edited-widget")))
            qFatal("Actual enhanced spacing widget capture failed");
        std::puts("QTM actual enhanced spacing: row4 AlphaBlack0 MosaicBlack3 DoubleWidth6 DoubleSize10 and exact resets via native menus");
        return;
    }
    auto holdMenuAction = named(insert->actions(), "Mosaics hold");
    auto holdMenu = holdMenuAction->menu();
    if (!insert || !holdMenu) qFatal("Fixed Base spacing menu chain absent");
    for (const auto &edit : std::array<std::pair<int, const char *>, 2>{{
        {9, "Hold mosaics"}, {14, "Release mosaics"}
    }}) {
        auto action = named(holdMenu->actions(), edit.second);
        QSignalSpy triggered(action, &QAction::triggered);
        if (!triggered.isValid()) qFatal("Fixed Base QAction signal unavailable");
        QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(edit.first * 12 + 6, 15));
        QTest::mouseClick(main->menuBar(), Qt::LeftButton, Qt::NoModifier,
            main->menuBar()->actionGeometry(insertAction).center());
        if (!QTest::qWaitFor([insert] { return insert->isVisible(); }, 3000))
            qFatal("Actual Insert menu did not expose");
        interiorDefinitionMenuSelect(insert, holdMenuAction);
        QTest::keyClick(insert, Qt::Key_Right);
        if (!QTest::qWaitFor([holdMenu] { return holdMenu->isVisible(); }, 3000))
            qFatal("Actual Mosaics hold submenu did not expose");
        interiorDefinitionMenuSelect(holdMenu, action);
        const char *stage = edit.first == 9 ? "base-spacing-hold-menu" : "base-spacing-release-menu";
        if (!holdMenu->grab().save(QString("/work/") + context.captureName(stage)))
            qFatal("Actual Base attribute menu capture failed");
        QTest::keyClick(holdMenu, Qt::Key_Return);
        if (!QTest::qWaitFor([&triggered, main] {
            return triggered.count() == 1 && main->isWindowModified();
        }, 3000)) qFatal("Actual Base attribute menu did not trigger exactly once");
    }
    if (!view->grab().save(QString("/work/") + context.captureName("base-spacing-edited-widget")))
        qFatal("Fixed Base spacing edited widget capture failed");
    std::puts("QTM actual Base spacing: Insert/Mosaics hold menus, Hold row1col9 and Release row1col14");
}

// Pinned x26menus.h names and x26dockwidget.cpp insertion defaults. Both
// additions go through the real Insert before menu; no tuple/model is assigned.
static void insertFirstDefinitionWord(QDockWidget *dock, const DriverContext &context, bool glyph) {
    QPushButton *button = nullptr;
    for (auto candidate : dock->findChildren<QPushButton *>()) {
        QString label = candidate->text(); label.remove('&');
        if (label != "Insert before" || !candidate->isVisible() || !candidate->isEnabled()
            || !candidate->menu()) continue;
        if (button) qFatal("Ambiguous first-definition Insert before button");
        button = candidate;
    }
    if (!button) qFatal("Actual first-definition Insert before menu absent");
    auto menu = button->menu();
    QTimer::singleShot(150, qApp, [menu, context, glyph] {
        const auto named = [](QMenu *parent, const char *label) {
            QAction *found = nullptr;
            for (auto action : parent->actions()) {
                if (action->text() != label || !action->isVisible() || !action->isEnabled()) continue;
                if (found) qFatal("Ambiguous first-definition insertion action");
                found = action;
            }
            if (!found) qFatal("Actual first-definition insertion action absent");
            return found;
        };
        if (!QTest::qWaitFor([menu] { return menu->isVisible(); }, 3000))
            qFatal("Actual first-definition insertion menu did not open");
        if (context.addInteriorDefinitionRow) {
            QMenu *targetMenu = menu;
            if (glyph) {
                auto column = named(menu, "Column triplet");
                auto child = column->menu();
                if (!child) qFatal("Actual interior-definition Column submenu absent");
                interiorDefinitionMenuSelect(menu, column);
                QTest::keyClick(menu, Qt::Key_Right);
                if (!QTest::qWaitFor([menu, column, child] {
                    return menu->activeAction() == column && child->isVisible();
                }, 3000)) qFatal("Actual interior-definition Right did not open Column submenu");
                targetMenu = child;
            }
            auto action = named(targetMenu, glyph ? "G0 character" : "Set Active Position");
            if (action->menu()) qFatal("Actual interior-definition target must be a leaf action");
            interiorDefinitionMenuSelect(targetMenu, action);
            if (!targetMenu->grab().save(QString("/work/") + context.captureName(
                glyph ? "first-definition-glyph-menu" : "first-definition-position-menu")))
                qFatal("Actual interior-definition keyboard selection capture failed");
            if (targetMenu->activeAction() != action || !targetMenu->isVisible())
                qFatal("Actual interior-definition target changed before Return");
            QTest::keyClick(targetMenu, Qt::Key_Return);
            return;
        }
        QMenu *targetMenu = menu;
        if (glyph) {
            auto column = named(menu, "Column triplet");
            auto child = column->menu();
            const QPoint point = menu->actionGeometry(column).center();
            if (!child || !menu->rect().contains(point)) qFatal("Actual column submenu is not clickable");
            QTest::mouseMove(menu, point);
            if (!QTest::qWaitFor([menu, column, child] {
                return menu->activeAction() == column && child->isVisible();
            }, 3000)) qFatal("Actual first-definition column submenu did not receive hover");
            targetMenu = child;
        }
        auto action = named(targetMenu, glyph ? "G0 character" : "Set Active Position");
        const QPoint target = targetMenu->actionGeometry(action).center();
        if (action->menu() || !targetMenu->rect().contains(target))
            qFatal("Actual first-definition insertion target is not clickable");
        QTest::mouseMove(targetMenu, target);
        if (!QTest::qWaitFor([targetMenu, action] {
            return targetMenu->isVisible() && targetMenu->activeAction() == action;
        }, 3000)) qFatal("Actual first-definition insertion target did not receive hover");
        if (!targetMenu->grab().save(QString("/work/") + context.captureName(
            glyph ? "first-definition-glyph-menu" : "first-definition-position-menu")))
            qFatal("Actual first-definition insertion menu capture failed");
        QTest::mouseClick(targetMenu, Qt::LeftButton, Qt::NoModifier, target);
    });
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
static void editFirstDefinitionPosition(QMainWindow *main, const DriverContext &context) {
    if (!context.addFirstDefinitionPosition || context.tripletIndex != 3
        || context.tripletColumn != 8 || main->isWindowModified())
        qFatal("Unexpected fixed empty-definition baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual first-definition dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual first-definition dock not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual first-definition X26 table is ambiguous");
    auto table = tables.front();
    auto model = table->model();
    std::vector<StructureTriplet> expected{
        {40,19,2}, {63,31,6}, {40,23,2}, {63,31,5}
    };
    structureCheck(model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("first-definition-before")))
        qFatal("Actual empty definition baseline capture failed");
    structureSelect(table, 3);
    insertFirstDefinitionWord(dock, context, false);
    expected.insert(expected.begin() + 3, StructureTriplet{41,4,0});
    structureCheck(model, expected);
    structureSelect(table, 4);
    insertFirstDefinitionWord(dock, context, true);
    expected.insert(expected.begin() + 4, StructureTriplet{0,9,32});
    structureCheck(model, expected);
    structureSelect(table, 4);
    QSpinBox *column = nullptr;
    for (auto candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 39) continue;
        if (column) qFatal("Ambiguous first-definition cooked Column control");
        column = candidate;
    }
    if (!column || column->value() != 0) qFatal("Actual first-definition G0 column must begin zero");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const QRect step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxUp, column);
    if (!step.isValid() || !column->rect().contains(step.center()))
        qFatal("Actual first-definition Column step is not clickable");
    for (int next = 1; next <= 8; ++next) {
        QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
        if (!QTest::qWaitFor([column, next] { return column->value() == next; }, 3000))
            qFatal("Actual first-definition Column did not advance once");
        expected[4][0] = next;
        structureCheck(model, expected);
    }
    structureCode(dock, context, 4, 32, 80);
    expected[4][2] = 80;
    structureCheck(model, expected);
    if (!main->isWindowModified()) qFatal("Actual first-definition edits did not mark document modified");
    if (!dock->grab().save(QString("/work/") + context.captureName("first-definition-edited")))
        qFatal("Actual first-definition edited capture failed");
    savedGlyphStructure = expected;
    std::puts("QTM actual first definition: InsertBefore SetActivePosition41/4/0, InsertBefore G0space, Column0-to8, actual character popup P80; full six-row model checked");
}
// Only the cooked Row/Column spinbox is clicked; the raw triplet controls and
// the model are read-only oracles. Every step is exactly one visible increment.
static void interiorDefinitionCoordinateStep(QDockWidget *dock, int minimum, int maximum, int from, int to) {
    QSpinBox *field = nullptr;
    for (auto candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || !candidate->prefix().isEmpty()
            || candidate->minimum() != minimum || candidate->maximum() != maximum) continue;
        if (field) qFatal("Ambiguous interior-definition cooked coordinate");
        field = candidate;
    }
    if (!field || field->value() != from || to != from + 1)
        qFatal("Actual interior-definition coordinate source differs");
    QStyleOptionSpinBox style;
    style.initFrom(field);
    style.rect = field->rect();
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = field->hasFrame();
    style.buttonSymbols = field->buttonSymbols();
    const QRect step = field->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxUp, field);
    if (!step.isValid() || !field->rect().contains(step.center())
        || field->style()->hitTestComplexControl(QStyle::CC_SpinBox, &style, step.center(), field) != QStyle::SC_SpinBoxUp)
        qFatal("Actual interior-definition coordinate step is not clickable");
    QTest::mouseClick(field, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([field, to] { return field->value() == to; }, 3000))
        qFatal("Actual interior-definition coordinate did not advance once");
}
static void interiorDefinitionCheck(QDockWidget *dock, const DriverContext &context,
    QAbstractItemModel *model, const std::vector<StructureTriplet> &expected) {
    if (QTest::qWaitFor([model, &expected] { return structureMatches(model, expected); }, 3000)) return;
    std::fprintf(stderr, "QTM interior-definition mismatch: actual_rows=%d expected_rows=%zu columns=%d\n",
        model ? model->rowCount() : -1, expected.size(), model ? model->columnCount() : -1);
    if (model && model->columnCount() == 4 && model->rowCount() <= 208) {
        for (int row = 0; row < model->rowCount(); ++row) {
            const auto actual = structureTriplet(model, row);
            const auto wanted = row < int(expected.size()) ? expected[row] : StructureTriplet{-1,-1,-1};
            std::fprintf(stderr, "QTM interior-definition row%d actual=(%d,%d,%d) expected=(%d,%d,%d)\n",
                row, actual[0], actual[1], actual[2], wanted[0], wanted[1], wanted[2]);
        }
    }
    std::fflush(stderr);
    dock->grab().save(QString("/work/") + context.captureName("interior-definition-mismatch"));
    qFatal("Interior definition differs from its complete fixed model");
}
static void editInteriorDefinitionRow(QMainWindow *main, const DriverContext &context) {
    if (!context.addInteriorDefinitionRow || context.tripletIndex != 5
        || context.tripletColumn != 6 || context.leftPanelColumns != 0
        || context.rightPanelColumns != 0 || !context.drcsName.isEmpty() || main->isWindowModified())
        qFatal("Unexpected fixed populated-definition baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual interior-definition dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual interior-definition dock not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual interior-definition X26 table is ambiguous");
    auto table = tables.front();
    auto model = table->model();
    const std::vector<StructureTriplet> original{
        {40,19,2}, {63,31,6}, {40,23,2}, {41,4,0}, {8,9,80}, {43,4,0}, {4,9,81}, {63,31,5}
    };
    auto expected = original;
    interiorDefinitionCheck(dock, context, model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("interior-definition-before")))
        qFatal("Actual populated definition baseline capture failed");
    structureSelect(table, 5);
    insertFirstDefinitionWord(dock, context, false);
    // cac2167 x26dockwidget.cpp: inserting AP scans preceding row setters;
    // after the retained AProw1 it already defaults to row2, not row1.
    expected.insert(expected.begin() + 5, StructureTriplet{42,4,0});
    interiorDefinitionCheck(dock, context, model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("interior-definition-position")))
        qFatal("Actual interior-definition row2 capture failed");
    structureSelect(table, 6);
    insertFirstDefinitionWord(dock, context, true);
    expected.insert(expected.begin() + 6, StructureTriplet{0,9,32});
    interiorDefinitionCheck(dock, context, model, expected);
    structureSelect(table, 6);
    for (int next = 1; next <= 6; ++next) {
        interiorDefinitionCoordinateStep(dock, 0, 39, next - 1, next);
        expected[6][0] = next;
        interiorDefinitionCheck(dock, context, model, expected);
    }
    structureCode(dock, context, 6, 32, 82);
    expected[6][2] = 82;
    interiorDefinitionCheck(dock, context, model, expected);
    auto retained = expected;
    retained.erase(retained.begin() + 5, retained.begin() + 7);
    if (retained != original || !main->isWindowModified() || expected.size() != 10)
        qFatal("Interior insertion changed a retained row, pointer, marker or terminator");
    if (!dock->grab().save(QString("/work/") + context.captureName("interior-definition-edited")))
        qFatal("Actual interior-definition edited capture failed");
    savedGlyphStructure = expected;
    std::puts("QTM actual interior definition: InsertBefore AP42/4/0 and G0R6/9/82 before retained row3; complete8-to10 model and original row chunks exact");
}
// Observed Qt paint only, never decoder/model input. A no-panel QTM widget can
// retain additional layout proxy space; this captures its explicit480x250 page
// viewport. The independent native raster remains the full-service authority.
static std::vector<StructureTriplet> resourceCallContextTriplets(bool added) {
    std::vector<StructureTriplet> expected{
        {40,17,2}, {63,31,6}, {40,21,2}, {41,4,8}, {8,12,65},
        {8,9,65}, {10,12,0}, {40,16,0}, {40,19,17}, {40,22,9},
        {41,4,7}, {7,12,64}, {9,12,0}, {9,9,88}, {40,23,17},
        {44,4,1}, {1,12,0}, {1,0,2}, {1,3,0}, {1,9,80}, {63,31,5}
    };
    if (added) {
        expected.insert(expected.begin() + 9, StructureTriplet{40,16,1});
        expected.insert(expected.begin() + 10, StructureTriplet{40,19,17});
        structureRelocate(expected, 9, 2);
    }
    return expected;
}
// The combined fixture omits only the original Adaptive size writer. Keep the
// old call-only catalogue exact; all aliases are relocated by the same rules.
static std::vector<StructureTriplet> mixedResourceCallTriplets(bool added) {
    auto expected = resourceCallContextTriplets(false);
    if (expected[11] != StructureTriplet{7,12,64})
        qFatal("Fixed mixed Adaptive size writer differs");
    expected.erase(expected.begin() + 11);
    structureRelocate(expected, 11, -1);
    if (added) {
        expected.insert(expected.begin() + 9, StructureTriplet{40,16,1});
        expected.insert(expected.begin() + 10, StructureTriplet{40,19,16});
        structureRelocate(expected, 9, 2);
    }
    return expected;
}
static QImage resourceCallOriginalPaint;
static QImage resourceCallViewport() {
    auto main = window();
    auto view = editor();
    if (!main || !main->isVisible() || !main->windowHandle() || !main->windowHandle()->isExposed()
        || !view || !view->isVisible() || view->width() < 480 || view->height() < 250)
        qFatal("Actual resource context viewport must be visible at480x250");
    const QImage painted = view->grab(QRect(0, 0, 480, 250)).toImage();
    if (painted.size() != QSize(480, 250)) qFatal("Actual resource context viewport capture failed");
    return painted;
}
static int resourceCallGreen(const QImage &painted, int column) {
    int count = 0;
    for (int y = 50; y < 60; ++y)
        for (int x = column * 12; x < (column + 1) * 12; ++x)
            count += painted.pixelColor(x, y) == QColor(0, 255, 0);
    return count;
}
static void captureResourceCallPaint(const DriverContext &context, const char *stage, bool edited) {
    if (!context.resourceCallContext) qFatal("Resource paint requires the explicit context mode");
    const int destination = edited ? 12 : 11;
    if (!QTest::qWaitFor([destination] {
        const QImage painted = resourceCallViewport();
        return resourceCallGreen(painted, destination) > 0
            && resourceCallGreen(painted, destination == 11 ? 12 : 11) == 0;
    }, 3000)) qFatal("Actual nested green P did not occupy the expected origin cell");
    const QImage painted = resourceCallViewport();
    if (!edited) {
        int white = 0;
        for (int y = 10; y < 30; ++y)
            for (int x = 8 * 12; x < 10 * 12; ++x)
                white += painted.pixelColor(x, y) == QColor(255, 255, 255);
        if (white == 0) qFatal("Actual context parent enlarged white A is absent");
        resourceCallOriginalPaint = painted;
    } else {
        if (resourceCallOriginalPaint.size() != QSize(480, 250))
            qFatal("Actual original context viewport oracle absent");
        for (int y = 0; y < 250; ++y)
            for (int x = 0; x < 480; ++x) {
                QColor expected = resourceCallOriginalPaint.pixelColor(x, y);
                if (y >= 50 && y < 60) {
                    if (x >= 11 * 12 && x < 12 * 12) expected = QColor(0, 0, 0);
                    if (x >= 12 * 12 && x < 13 * 12)
                        expected = resourceCallOriginalPaint.pixelColor(x - 12, y);
                }
                if (painted.pixelColor(x, y) != expected)
                    qFatal("Actual context paint changed outside exact one-cell P translation");
            }
    }
    const QByteArray label = QByteArray("resource-call-") + stage + "-widget";
    if (!painted.save(QString("/work/") + context.captureName(label.constData())))
        qFatal("Actual resource context viewport evidence capture failed");
    std::printf("QTM actual resource context viewport480x250: stage=%s greenPcolumn=%d greenpixels=%d comparison=%s\n",
        stage, destination, resourceCallGreen(painted, destination),
        edited ? "exact translated P; every other pixel unchanged" : "original P and enlarged white A observed");
}
// Pinned cac2167 x26dockwidget.cpp920-928 exposes the OriginModifier's cooked
// +Column spinbox0..71. x26model.cpp605-611 commits this as undoable ETdata,
// preserving raw address40, mode16 and the following allocated call selector.
static void editResourceCallOrigin(QMainWindow *main, const DriverContext &context) {
    if (!context.editResourceCallOrigin || context.tripletIndex != 7 || context.tripletColumn != 40
        || context.leftPanelColumns != 0 || context.rightPanelColumns != 0
        || !context.drcsName.isEmpty() || main->isWindowModified())
        qFatal("Unexpected fixed resource call origin baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets")
        qFatal("Actual resource call X/26 dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000))
        qFatal("Actual resource call dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual resource call requires one X/26 table");
    auto table = tables.front();
    auto model = table->model();
    // Exact Active20 editor-only Local wrapper and native body. Public Passive21
    // is allocated pointer0; the editor wrapper never changes its native identity.
    std::vector<StructureTriplet> expected;
    if (context.resourceCallContext) {
        // Root-first original catalogue order is Active20, Adaptive22, Passive21.
        // Disposable Local aliases at markers9/14 retain both original targets,
        // including the currently uninvoked Adaptive22. Native IDs stay original.
        expected = resourceCallContextTriplets(false);
    } else {
        expected = {
            {40,17,2}, {63,31,6}, {40,21,2}, {41,4,8}, {8,12,65},
            {8,9,65}, {10,12,0}, {40,16,0}, {48,19,0}, {63,31,5}
        };
    }
    structureCheck(model, expected);
    if (context.resourceCallContext) captureResourceCallPaint(context, "before", false);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-before")))
        qFatal("Actual resource call baseline capture failed");
    structureSelect(table, 7);
    QCheckBox *raw = nullptr;
    for (auto candidate : dock->findChildren<QCheckBox *>()) {
        if (!candidate->isVisible() || candidate->text() != "Raw values") continue;
        if (raw) qFatal("Ambiguous resource call Raw values checkbox");
        raw = candidate;
    }
    if (!raw || raw->isChecked()) qFatal("Resource call must use original cooked controls");
    QSpinBox *column = nullptr;
    for (auto candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 71
            || candidate->prefix() != "+") continue;
        if (column) qFatal("Ambiguous actual OriginModifier +Column spinbox");
        column = candidate;
    }
    if (!column || column->value() != 0)
        qFatal("Original OriginModifier +Column0 control absent");
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-selected")))
        qFatal("Actual resource call selected control capture failed");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.rect = column->rect();
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const QRect step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxUp, column);
    if (!step.isValid() || !column->rect().contains(step.center())
        || column->style()->hitTestComplexControl(QStyle::CC_SpinBox, &style, step.center(), column)
            != QStyle::SC_SpinBoxUp)
        qFatal("Actual OriginModifier +Column step-up must be clickable");
    QTest::mouseMove(column, step.center());
    QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
    expected[7] = StructureTriplet{40,16,1};
    if (!QTest::qWaitFor([column, main] { return column->value() == 1 && main->isWindowModified(); }, 3000))
        qFatal("Actual OriginModifier +Column did not step0-to1");
    structureCheck(model, expected);
    if (context.resourceCallContext) captureResourceCallPaint(context, "edited", true);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-edited")))
        qFatal("Actual resource call edited control capture failed");
    savedGlyphStructure = expected;
    if (context.resourceCallContext)
        std::puts("QTM actual resource call context: cooked +Column step-up (40,16,0)-to-(40,16,1); Local alias selector17 and complete21-row protected catalogue retained");
    else
        std::puts("QTM actual resource call origin: cooked +Column step-up (40,16,0)-to-(40,16,1); exact allocated invocation word8 and complete10-row wrapper retained");
}
static QImage resourceCallTopologyOriginalPaint;
static void captureResourceCallTopologyPaint(const DriverContext &context, const char *stage, bool added, bool edited) {
    if (context.insertResourceCall == context.deleteResourceCall)
        qFatal("Call topology paint requires exactly one fixed action");
    if (!QTest::qWaitFor([added] {
        const auto painted = resourceCallViewport();
        return resourceCallGreen(painted, 11) > 0 && (resourceCallGreen(painted, 12) > 0) == added;
    }, 3000)) qFatal("Actual topology P cells do not match the fixed call count");
    const auto painted = resourceCallViewport();
    if (!edited) {
        int white = 0;
        for (int y = 10; y < 30; ++y)
            for (int x = 96; x < 120; ++x)
                white += painted.pixelColor(x, y) == QColor(255, 255, 255);
        if (white == 0) qFatal("Actual topology original parent A is absent");
        resourceCallTopologyOriginalPaint = painted;
    } else {
        if (resourceCallTopologyOriginalPaint.size() != QSize(480, 250))
            qFatal("Actual topology original viewport oracle absent");
        for (int y = 0; y < 250; ++y)
            for (int x = 0; x < 480; ++x) {
                QColor expected = resourceCallTopologyOriginalPaint.pixelColor(x, y);
                if (y >= 50 && y < 60 && x >= 144 && x < 156)
                    expected = added ? resourceCallTopologyOriginalPaint.pixelColor(x - 12, y) : QColor(0, 0, 0);
                if (painted.pixelColor(x, y) != expected)
                    qFatal("Actual topology viewport changed outside the added/removed P cell");
            }
    }
    const QByteArray label = QByteArray("resource-call-pair-") + stage + "-widget";
    if (!painted.save(QString("/work/") + context.captureName(label.constData())))
        qFatal("Actual call topology viewport capture failed");
}
// Actual widget paint, not a decoder or model patch. The remaining Adaptive
// reset occupies the right quarter, so only the unobscured left quarter has a
// literal enlarged-B oracle. Every pixel outside B and the added P stays exact.
static void captureMixedResourceCallPaint(const DriverContext &context, const char *stage) {
    if (!context.mixedResourceCallGlyph || !context.insertResourceCall
        || resourceCallTopologyOriginalPaint.size() != QSize(480, 250))
        qFatal("Mixed call paint requires the fixed original viewport");
    const unsigned short rows[10] = {0,0x3fc,0x306,0x306,0x3fc,0x306,0x306,0x3fc,0,0};
    const auto matches = [&rows](const QImage &painted) {
        bool changed[2] = {false, false};
        for (int y = 0; y < 250; ++y)
            for (int x = 0; x < 480; ++x) {
                QColor expected = resourceCallTopologyOriginalPaint.pixelColor(x, y);
                if (y >= 10 && y < 30 && x >= 96 && x < 120) {
                    if (x >= 108) continue;
                    expected = rows[(y - 10) / 2] & (1 << (11 - (x - 96) / 2))
                        ? QColor(255, 255, 255) : QColor(0, 0, 0);
                    changed[(y - 10) / 10] |= painted.pixelColor(x, y)
                        != resourceCallTopologyOriginalPaint.pixelColor(x, y);
                }
                if (y >= 50 && y < 60 && x >= 144 && x < 156)
                    expected = resourceCallTopologyOriginalPaint.pixelColor(x - 12, y);
                if (painted.pixelColor(x, y) != expected) return false;
            }
        return changed[0] && changed[1];
    };
    if (!QTest::qWaitFor([&matches] { return matches(resourceCallViewport()); }, 3000))
        qFatal("Actual mixed viewport differs from fixed B/P and unchanged outside pixels");
    const auto painted = resourceCallViewport();
    const QByteArray label = QByteArray("resource-call-mixed-") + stage + "-widget";
    if (!painted.save(QString("/work/") + context.captureName(label.constData())))
        qFatal("Actual mixed call viewport evidence capture failed");
    std::puts("QTM mixed actual viewport: literal B left quarter in both enlarged halves; exact added P and unchanged outside B/P");
}
static void resourceOriginStep(QDockWidget *dock, int maximum, int from, int to, const DriverContext &context, int expectedRow = 9) {
    QSpinBox *field = nullptr;
    for (auto candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->minimum() != 0
            || candidate->maximum() != maximum || candidate->prefix() != "+") continue;
        if (field) qFatal("Ambiguous actual OriginModifier cooked coordinate");
        field = candidate;
    }
    if (!field || field->value() != from || (to - from != 1 && from - to != 1))
        qFatal("Actual OriginModifier coordinate source differs");
    const auto observe = [dock, field, maximum, from, to, context](const char *stage) {
        auto focus = QApplication::focusWidget();
        auto active = QApplication::activeWindow();
        auto popup = QApplication::activePopupWidget();
        auto modal = QApplication::activeModalWidget();
        const auto tables = dock->findChildren<QTableView *>();
        const int row = tables.size() == 1 ? tables.front()->currentIndex().row() : -1;
        StructureTriplet tuple{-1,-1,-1};
        if (tables.size() == 1 && row >= 0) tuple = structureTriplet(tables.front()->model(), row);
        const QByteArray label = QByteArray("resource-call-pair-coordinate-")
            + QByteArray::number(maximum) + "-" + stage;
        const bool captured = dock->isVisible()
            && dock->grab().save(QString("/work/") + context.captureName(label.constData()));
        std::fprintf(stderr,
            "QTM OriginModifier step stage=%s maximum=%d from=%d to=%d current=%d min=%d max=%d prefix=%s visible=%d enabled=%d dock_active=%d field_focus=%d focus_class=%s active_class=%s popup_class=%s modal_class=%s selected_row=%d raw=(%d,%d,%d) capture=%d\n",
            stage, maximum, from, to, field->value(), field->minimum(), field->maximum(), field->prefix().toUtf8().constData(),
            field->isVisible(), field->isEnabled(), dock->isActiveWindow(), field->hasFocus(),
            focus ? focus->metaObject()->className() : "none", active ? active->metaObject()->className() : "none",
            popup ? popup->metaObject()->className() : "none", modal ? modal->metaObject()->className() : "none",
            row, tuple[0], tuple[1], tuple[2], captured);
        std::fflush(stderr);
    };
    observe("before-focus");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1 || tables.front()->currentIndex().row() != expectedRow) {
        observe("selection-failed");
        qFatal("Actual inserted OriginModifier row%d is no longer selected", expectedRow);
    }
    // The menu's model update is synchronous. Wait for the actual popup grab to
    // disappear before restoring native activation and clicking the real input.
    if (!QTest::qWaitFor([] {
        return QApplication::activePopupWidget() == nullptr && QApplication::activeModalWidget() == nullptr;
    }, 3000)) {
        observe("popup-failed");
        qFatal("OriginModifier insertion popup did not release its grab");
    }
    dock->raise();
    dock->activateWindow();
    if (auto handle = dock->windowHandle()) handle->requestActivate();
    if (!QTest::qWaitForWindowActive(dock, 5000)) {
        observe("activation-failed");
        qFatal("Actual OriginModifier coordinate dock did not become active");
    }
    auto input = field->findChild<QLineEdit *>();
    if (!input || !input->isVisible() || !input->isEnabled())
        qFatal("Actual OriginModifier coordinate text input absent");
    QTest::mouseMove(input, input->rect().center());
    QTest::mouseClick(input, Qt::LeftButton, Qt::NoModifier, input->rect().center());
    if (!QTest::qWaitFor([field] {
        auto focus = QApplication::focusWidget();
        return focus && (focus == field || field->isAncestorOf(focus));
    }, 3000)) {
        observe("focus-failed");
        qFatal("Actual OriginModifier coordinate input did not receive focus");
    }
    observe("focused");
    if (field->value() != from) qFatal("Actual coordinate focus click changed its original value");
    QStyleOptionSpinBox style;
    style.initFrom(field);
    style.rect = field->rect();
    style.frame = field->hasFrame();
    style.buttonSymbols = field->buttonSymbols();
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    const auto control = to > from ? QStyle::SC_SpinBoxUp : QStyle::SC_SpinBoxDown;
    const auto step = field->style()->subControlRect(QStyle::CC_SpinBox, &style, control, field);
    if (!step.isValid() || !field->rect().contains(step.center())
        || field->style()->hitTestComplexControl(QStyle::CC_SpinBox, &style, step.center(), field) != control)
        qFatal("Actual OriginModifier coordinate step is not clickable");
    QTest::mouseMove(field, step.center());
    QTest::mouseClick(field, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([field, to] { return field->value() == to; }, 3000)) {
        observe("step-failed");
        qFatal("Actual OriginModifier coordinate did not step exactly once");
    }
    observe("stepped");
}
static void insertResourceOrigin(QDockWidget *dock, const DriverContext &context) {
    QPushButton *button = nullptr;
    for (auto candidate : dock->findChildren<QPushButton *>()) {
        QString text = candidate->text(); text.remove('&');
        if (text != "Insert before" || !candidate->isVisible() || !candidate->isEnabled() || !candidate->menu()) continue;
        if (button) qFatal("Ambiguous actual Insert before button");
        button = candidate;
    }
    if (!button) qFatal("Actual Insert before menu button absent");
    auto menu = button->menu();
    QTimer::singleShot(150, qApp, [menu, context] {
        const auto named = [](QMenu *parent, const char *name) {
            QAction *found = nullptr;
            for (auto action : parent->actions()) {
                if (action->text() != name || !action->isVisible() || !action->isEnabled()) continue;
                if (found) qFatal("Ambiguous actual OriginModifier insertion action");
                found = action;
            }
            if (!found) qFatal("Actual OriginModifier insertion action absent");
            return found;
        };
        if (!QTest::qWaitFor([menu] { return menu->isVisible(); }, 3000))
            qFatal("Actual Insert before menu did not open");
        auto object = named(menu, "Object");
        auto child = object->menu();
        const auto point = menu->actionGeometry(object).center();
        if (!child || !menu->rect().contains(point)) qFatal("Actual Object submenu is not clickable");
        QTest::mouseMove(menu, point);
        if (!QTest::qWaitFor([menu, object, child] { return menu->activeAction() == object && child->isVisible(); }, 3000))
            qFatal("Actual Object submenu did not receive hover");
        auto origin = named(child, "Origin modifier");
        const auto target = child->actionGeometry(origin).center();
        if (origin->menu() || !child->rect().contains(target)) qFatal("Actual OriginModifier action is not clickable");
        QTest::mouseMove(child, target);
        if (!QTest::qWaitFor([child, origin] { return child->isVisible() && child->activeAction() == origin; }, 3000))
            qFatal("Actual OriginModifier action did not receive hover");
        if (!child->grab().save(QString("/work/") + context.captureName("resource-call-pair-insert-menu")))
            qFatal("Actual OriginModifier insertion menu capture failed");
        QTest::mouseClick(child, Qt::LeftButton, Qt::NoModifier, target);
    });
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
// The structural fixture also retains Adaptive22's original Passive21 call.
// Keep that protected edge and relocate every complete alias/pointer normally.
static std::vector<StructureTriplet> resourceStructureTriplets(bool added) {
    auto expected = mixedResourceCallTriplets(false);
    if (expected[13] != StructureTriplet{40,23,16})
        qFatal("Original structural Passive definition differs");
    expected.insert(expected.begin() + 13, StructureTriplet{40,16,0});
    expected.insert(expected.begin() + 14, StructureTriplet{40,19,16});
    structureRelocate(expected, 13, 2);
    if (added) {
        expected.insert(expected.begin() + 9, StructureTriplet{40,16,1});
        expected.insert(expected.begin() + 10, StructureTriplet{40,19,18});
        structureRelocate(expected, 9, 2);
    }
    return expected;
}
// One fixed real-control resource structure trial, after the existing call pair.
static void captureResourceStructurePaint(const DriverContext &context, const char *stage) {
    if (!context.mixedResourceCallStructure || resourceCallTopologyOriginalPaint.size() != QSize(480, 250))
        qFatal("Resource structure paint requires the captured original viewport");
    const unsigned short rows[10] = {0,0x3fc,0x306,0x306,0x3fc,0x306,0x306,0x3fc,0,0};
    const auto matches = [&rows](const QImage &painted) {
        bool changed[2] = {false, false};
        for (int y = 0; y < 250; ++y)
            for (int x = 0; x < 480; ++x) {
                QColor expected = resourceCallTopologyOriginalPaint.pixelColor(x, y);
                if (y >= 10 && y < 30 && x >= 96 && x < 132) {
                    if (x >= 108) continue; // Moved A and the existing Adaptive reset.
                    expected = rows[(y - 10) / 2] & (1 << (11 - (x - 96) / 2))
                        ? QColor(255, 255, 255) : QColor(0, 0, 0);
                    changed[(y - 10) / 10] |= painted.pixelColor(x, y)
                        != resourceCallTopologyOriginalPaint.pixelColor(x, y);
                }
                if (y >= 50 && y < 60 && x >= 144 && x < 156)
                    expected = resourceCallTopologyOriginalPaint.pixelColor(x - 12, y);
                if (painted.pixelColor(x, y) != expected) return false;
            }
        return changed[0] && changed[1];
    };
    if (!QTest::qWaitFor([&matches] { return matches(resourceCallViewport()); }, 3000))
        qFatal("Actual resource structure paint differs from B/P or protected outside pixels");
    const QByteArray label = QByteArray("resource-call-structure-") + stage + "-widget";
    if (!resourceCallViewport().save(QString("/work/") + context.captureName(label.constData())))
        qFatal("Actual resource structure paint capture failed");
}
static void editResourceCallStructure(QDockWidget *dock, QTableView *table,
        QAbstractItemModel *model, const DriverContext &context,
        std::vector<StructureTriplet> &expected) {
    if (!context.mixedResourceCallStructure || !context.insertResourceCall
        || expected != resourceStructureTriplets(true))
        qFatal("Fixed resource structure requires the exact original22-to24 call insertion");
    structureSelect(table, 4);
    structureButton(dock, "Insert copy");
    expected.insert(expected.begin() + 4, StructureTriplet{8,12,65});
    structureRelocate(expected, 4, 1);
    structureCheck(model, expected);
    structureSelect(table, 5);
    QCheckBox *boxed = nullptr;
    for (auto candidate : dock->findChildren<QCheckBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->text() != "Box/Win") continue;
        if (boxed) qFatal("Ambiguous real Box/Win control");
        boxed = candidate;
    }
    if (!boxed || boxed->isChecked()) qFatal("Copied original Attributes must have Box/Win off");
    // A stretched checkbox includes blank layout space outside its hit area.
    // Use the same style-derived real indicator click as the existing radios.
    QStyleOptionButton boxStyle;
    boxStyle.initFrom(boxed);
    boxStyle.text = boxed->text();
    const QRect boxIndicator = boxed->style()->subElementRect(
        QStyle::SE_CheckBoxIndicator, &boxStyle, boxed);
    if (!boxIndicator.isValid() || !boxed->rect().contains(boxIndicator.center()))
        qFatal("Actual Box/Win indicator is not clickable");
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-structure-boxed-before")))
        qFatal("Actual original Box/Win control capture failed");
    QTest::mouseMove(boxed, boxIndicator.center());
    QTest::mouseClick(boxed, Qt::LeftButton, Qt::NoModifier, boxIndicator.center());
    if (!QTest::qWaitFor([boxed] { return boxed->isChecked(); }, 3000))
        qFatal("Actual Box/Win click did not set the copied writer");
    expected[5] = StructureTriplet{8,12,67};
    structureCheck(model, expected);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-structure-boxed")))
        qFatal("Actual boxed writer capture failed");
    structureSelect(table, 6);
    structureButton(dock, "Insert copy");
    expected.insert(expected.begin() + 6, StructureTriplet{8,9,65});
    structureRelocate(expected, 6, 1);
    structureCheck(model, expected);
    structureSelect(table, 6);
    structureCode(dock, context, 6, 65, 66);
    expected[6] = StructureTriplet{8,9,66};
    structureCheck(model, expected);
    structureSelect(table, 7);
    QSpinBox *column = nullptr;
    for (auto candidate : dock->findChildren<QSpinBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled()
            || candidate->minimum() != 0 || candidate->maximum() != 39) continue;
        if (column) qFatal("Ambiguous actual resource structure Column control");
        column = candidate;
    }
    if (!column || column->value() != 8) qFatal("Original shifted A must remain at column8");
    QStyleOptionSpinBox style;
    style.initFrom(column);
    style.subControls = QStyle::SC_All;
    style.stepEnabled = QAbstractSpinBox::StepDownEnabled | QAbstractSpinBox::StepUpEnabled;
    style.frame = column->hasFrame();
    style.buttonSymbols = column->buttonSymbols();
    const auto step = column->style()->subControlRect(QStyle::CC_SpinBox, &style,
        QStyle::SC_SpinBoxUp, column);
    if (!step.isValid() || !column->rect().contains(step.center())
        || column->style()->hitTestComplexControl(QStyle::CC_SpinBox, &style, step.center(), column)
            != QStyle::SC_SpinBoxUp)
        qFatal("Actual resource structure Column step is not clickable");
    QTest::mouseClick(column, Qt::LeftButton, Qt::NoModifier, step.center());
    if (!QTest::qWaitFor([column] { return column->value() == 9; }, 3000))
        qFatal("Actual resource structure Column did not step8-to9");
    expected[7] = StructureTriplet{9,9,65};
    structureCheck(model, expected);
    const std::vector<StructureTriplet> final{
        {40,17,2},{63,31,6},{40,21,2},{41,4,8},{8,12,65},{8,12,67},
        {8,9,66},{9,9,65},{10,12,0},{40,16,0},{40,19,22},{40,16,1},
        {40,19,22},{40,22,16},{41,4,7},{9,12,0},{9,9,88},{40,16,0},
        {40,19,22},{40,23,22},
        {44,4,1},{1,12,0},{1,0,2},{1,3,0},{1,9,80},{63,31,5}
    };
    if (expected != final) qFatal("Fixed complete26-word resource structure differs");
    captureResourceStructurePaint(context, "edited");
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-structure-edited")))
        qFatal("Actual resource structure edited dock capture failed");
}

static void editResourceCallTopology(QMainWindow *main, const DriverContext &context) {
    if (context.insertResourceCall == context.deleteResourceCall || !context.resourceCallContext
        || context.tripletIndex != 7 || context.tripletColumn != 40 || main->isWindowModified())
        qFatal("Unexpected fixed call topology baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual call topology dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual call topology dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual call topology requires one table");
    auto table = tables.front();
    auto model = table->model();
    auto expected = context.mixedResourceCallStructure ? resourceStructureTriplets(false)
        : context.mixedResourceCallGlyph ? mixedResourceCallTriplets(false)
        : resourceCallContextTriplets(context.deleteResourceCall);
    structureCheck(model, expected);
    captureResourceCallTopologyPaint(context, "before", context.deleteResourceCall, false);
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-pair-before")))
        qFatal("Actual topology baseline dock capture failed");
    if (context.insertResourceCall) {
        structureSelect(table, 8);
        if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-pair-selected")))
            qFatal("Actual original invocation selection capture failed");
        structureButton(dock, "Insert copy");
        expected.insert(expected.begin() + 8, expected[8]);
        structureRelocate(expected, 8, 1);
        structureCheck(model, expected);
        if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-pair-copied")))
            qFatal("Actual copied call capture failed");
        structureSelect(table, 9);
        insertResourceOrigin(dock, context);
        expected.insert(expected.begin() + 9, StructureTriplet{41,16,0});
        structureRelocate(expected, 9, 1);
        structureCheck(model, expected);
        resourceOriginStep(dock, 23, 1, 0, context);
        expected[9] = StructureTriplet{40,16,0};
        structureCheck(model, expected);
        resourceOriginStep(dock, 71, 0, 1, context);
        expected[9] = StructureTriplet{40,16,1};
    } else {
        // Single-selection Delete twice avoids pinned QTM's multirow undo bug.
        for (int deletion = 0; deletion < 2; ++deletion) {
            structureSelect(table, 9);
            if (deletion == 0 && !dock->grab().save(QString("/work/") + context.captureName("resource-call-pair-selected")))
                qFatal("Actual added pair selection capture failed");
            structureButton(dock, "Delete");
            expected.erase(expected.begin() + 9);
            structureRelocate(expected, 9, -1);
            structureCheck(model, expected);
        }
    }
    if (expected != (context.mixedResourceCallStructure ? resourceStructureTriplets(true)
        : context.mixedResourceCallGlyph ? mixedResourceCallTriplets(true)
        : resourceCallContextTriplets(context.insertResourceCall)))
        qFatal("Actual topology final expected catalogue differs");
    structureCheck(model, expected);
    if (!main->isWindowModified()) qFatal("Actual call topology did not modify the document");
    captureResourceCallTopologyPaint(context, "edited", context.insertResourceCall, true);
    if (context.mixedResourceCallGlyph) {
        structureSelect(table, 5);
        structureCode(dock, context, 5, 65, 66);
        expected[5] = StructureTriplet{8,9,66};
        structureCheck(model, expected);
        captureMixedResourceCallPaint(context, "edited");
    }
    if (!dock->grab().save(QString("/work/") + context.captureName("resource-call-pair-edited")))
        qFatal("Actual topology edited dock capture failed");
    if (context.mixedResourceCallStructure)
        editResourceCallStructure(dock, table, model, context, expected);
    savedGlyphStructure = expected;
    std::printf("QTM actual resource call topology: action=%s modelrows=%zu; complete alias catalogue and pointers checked\n",
        context.insertResourceCall ? "InsertCopy invocation then InsertBefore OriginModifier and cooked row/column clicks" : "two single Delete clicks, not GUIUndo", expected.size());
}
// V7 fragments retain native Public selectors, not V8 renderable Local aliases.
// Observe unchanged actual widget pixels without claiming external-resource decoding.
static QImage nativeFragmentOriginalPaint;
static std::vector<StructureTriplet> nativeFragmentTriplets(const DriverContext &context, int calls) {
    if (calls < 0 || calls > 2) qFatal("Fixed native fragment call count differs");
    std::vector<StructureTriplet> result{{40,17,2},{63,31,6},{40,21,2}};
    for (int index = 0; index < 18; ++index) result.push_back({8,9,65});
    result.push_back({10,12,0});
    for (int call = 0; call < calls; ++call) {
        result.push_back({40,16,call});
        result.push_back({context.fragmentCallSelector & 63, 19, context.fragmentCallSelector >> 11});
    }
    result.push_back({63,31,5});
    return result;
}
static void captureNativeFragmentPaint(const DriverContext &context, const char *stage, bool original) {
    const auto paint = resourceCallViewport();
    if (paint.size() != QSize(480,250)) qFatal("Native fragment viewport geometry differs");
    if (original) nativeFragmentOriginalPaint = paint;
    else if (nativeFragmentOriginalPaint.size() != paint.size() || paint != nativeFragmentOriginalPaint)
        qFatal("Native Public call edits unexpectedly changed pinned editor viewport");
    const QByteArray label = QByteArray("native-fragment-") + stage + "-widget";
    if (!paint.save(QString("/work/") + context.captureName(label.constData())))
        qFatal("Native fragment actual viewport capture failed");
}
static void editNativeFragmentCalls(QMainWindow *main, const DriverContext &context) {
    if (context.insertFragmentCall == context.deleteFragmentCall || context.insertResourceCall
        || context.deleteResourceCall || context.resourceCallContext || main->isWindowModified()
        || context.tripletIndex != 22 || context.tripletColumn != 40)
        qFatal("Unexpected native fragment call baseline");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual native fragment dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800,680);
    dock->move(main->pos() + QPoint(32,32));
    if (!QTest::qWaitForWindowExposed(dock,5000)) qFatal("Actual native fragment dock was not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual native fragment requires one table");
    auto table = tables.front();
    auto model = table->model();
    auto expected = nativeFragmentTriplets(context,context.insertFragmentCall ? 1 : 2);
    structureCheck(model,expected);
    captureNativeFragmentPaint(context,"before",true);
    if (!dock->grab().save(QString("/work/") + context.captureName("native-fragment-before")))
        qFatal("Actual native fragment baseline dock capture failed");
    if (context.insertFragmentCall) {
        structureSelect(table,23);
        structureButton(dock,"Insert copy");
        expected.insert(expected.begin()+23,expected[23]);
        structureRelocate(expected,23,1);
        structureCheck(model,expected);
        structureSelect(table,24);
        insertResourceOrigin(dock,context);
        expected.insert(expected.begin()+24,StructureTriplet{41,16,0});
        structureRelocate(expected,24,1);
        structureCheck(model,expected);
        resourceOriginStep(dock,23,1,0,context,24);
        expected[24] = StructureTriplet{40,16,0};
        structureCheck(model,expected);
        resourceOriginStep(dock,71,0,1,context,24);
        expected[24] = StructureTriplet{40,16,1};
        structureCheck(model,expected);
    } else {
        // Delete the two complete native pairs through four single selections.
        for (int row : {25,24,23,22}) {
            structureSelect(table,row);
            structureButton(dock,"Delete");
            expected.erase(expected.begin()+row);
            structureRelocate(expected,row,-1);
            structureCheck(model,expected);
        }
    }
    if (expected != nativeFragmentTriplets(context,context.insertFragmentCall ? 2 : 0)
        || !main->isWindowModified()) qFatal("Actual native fragment final model differs");
    captureNativeFragmentPaint(context,"edited",false);
    if (!dock->grab().save(QString("/work/") + context.captureName("native-fragment-edited")))
        qFatal("Actual native fragment edited dock capture failed");
    savedGlyphStructure = expected;
    std::printf("QTM actual V7 native fragment calls action=%s modelrows=%zu: complete Public selector model checked; viewport unchanged, no external target rendering claimed\n",
        context.insertFragmentCall ? "InsertCopy then InsertBeforeOrigin with cooked Row1-to0 Column0-to1" : "four single Delete clicks",expected.size());
}

// Pinned x26dockwidget.cpp1103: Insert copy on an empty list creates the
// termination. Subsequent additions use only the actual Insert before menus.
static void insertAbsentMainWord(QDockWidget *dock, const DriverContext &context,
    const char *label, const char *stage) {
    QPushButton *button = nullptr;
    for (auto candidate : dock->findChildren<QPushButton *>()) {
        QString text = candidate->text(); text.remove('&');
        if (text != "Insert before" || !candidate->isVisible() || !candidate->isEnabled()
            || !candidate->menu()) continue;
        if (button) qFatal("Ambiguous absent-Main Insert before button");
        button = candidate;
    }
    if (!button) qFatal("Actual absent-Main Insert before button absent");
    auto menu = button->menu();
    QTimer::singleShot(150, qApp, [menu, context, label, stage] {
        const auto named = [](QMenu *parent, const char *name) {
            QAction *found = nullptr;
            for (auto action : parent->actions()) {
                if (action->text() != name || !action->isVisible() || !action->isEnabled()) continue;
                if (found) qFatal("Ambiguous absent-Main insertion action");
                found = action;
            }
            if (!found) qFatal("Actual absent-Main insertion action absent");
            return found;
        };
        if (!QTest::qWaitFor([menu] { return menu->isVisible(); }, 3000))
            qFatal("Actual absent-Main insertion menu did not open");
        auto targetMenu = menu;
        if (QByteArray(label) != "Set Active Position") {
            auto column = named(menu, "Column triplet");
            auto child = column->menu();
            if (!child) qFatal("Actual absent-Main Column submenu absent");
            interiorDefinitionMenuSelect(menu, column);
            QTest::keyClick(menu, Qt::Key_Right);
            if (!QTest::qWaitFor([menu, column, child] {
                return menu->activeAction() == column && child->isVisible();
            }, 3000)) qFatal("Actual absent-Main Column submenu did not open");
            targetMenu = child;
        }
        auto action = named(targetMenu, label);
        if (action->menu()) qFatal("Actual absent-Main insertion target is not a leaf");
        interiorDefinitionMenuSelect(targetMenu, action);
        if (!targetMenu->grab().save(QString("/work/") + context.captureName(stage)))
            qFatal("Actual absent-Main insertion menu capture failed");
        if (targetMenu->activeAction() != action || !targetMenu->isVisible())
            qFatal("Actual absent-Main insertion target changed before Return");
        QTest::keyClick(targetMenu, Qt::Key_Return);
    });
    QTest::mouseClick(button, Qt::LeftButton, Qt::NoModifier, button->rect().center());
}
static void absentMainColour(QDockWidget *dock, const DriverContext &context, int to,
    const char *stage) {
    QComboBox *colour = nullptr;
    for (auto candidate : dock->findChildren<QComboBox *>()) {
        if (!candidate->isVisible() || !candidate->isEnabled() || candidate->count() != 32) continue;
        if (colour) qFatal("Ambiguous absent-Main foreground selector");
        colour = candidate;
    }
    if (!colour || colour->currentIndex() != 0 || (to != 18 && to != 7))
        qFatal("Actual absent-Main foreground source or fixed target differs");
    QTest::mouseClick(colour, Qt::LeftButton, Qt::NoModifier, colour->rect().center());
    auto popup = colour->view();
    if (!QTest::qWaitFor([popup] { return popup->isVisible(); }, 3000))
        qFatal("Actual absent-Main foreground popup did not open");
    const auto choice = popup->model()->index(to, colour->modelColumn(), colour->rootModelIndex());
    popup->scrollTo(choice, QAbstractItemView::PositionAtCenter);
    if (!QTest::qWaitFor([popup, choice] {
        return choice.isValid() && popup->viewport()->rect().contains(popup->visualRect(choice).center());
    }, 3000)) qFatal("Actual absent-Main foreground choice is not clickable");
    const auto point = popup->visualRect(choice).center();
    QTest::mouseMove(popup->viewport(), point);
    if (!QTest::qWaitFor([popup, choice, point] {
        return popup->isVisible() && popup->indexAt(point) == choice && popup->currentIndex() == choice;
    }, 3000)) qFatal("Actual absent-Main foreground choice did not receive hover");
    if (!popup->window()->grab().save(QString("/work/") + context.captureName(stage)))
        qFatal("Actual absent-Main foreground popup capture failed");
    QTest::mouseClick(popup->viewport(), Qt::LeftButton, Qt::NoModifier, point);
    if (!QTest::qWaitFor([colour, to] { return colour->currentIndex() == to; }, 3000))
        qFatal("Actual absent-Main foreground choice was not activated");
}
static void editAbsentMain(QMainWindow *main, const DriverContext &context) {
    if (!context.createMain || main->isWindowModified())
        qFatal("Absent-Main creation requires a pristine fixed input");
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || dock->windowTitle() != "X/26 triplets") qFatal("Actual absent-Main dock absent");
    if (!dock->isVisible()) dock->toggleViewAction()->trigger();
    dock->setFloating(true);
    dock->resize(800, 680);
    dock->move(main->pos() + QPoint(32, 32));
    if (!QTest::qWaitForWindowExposed(dock, 5000)) qFatal("Actual absent-Main dock not exposed");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Actual absent-Main table is ambiguous");
    auto table = tables.front();
    auto model = table->model();
    std::vector<StructureTriplet> expected;
    const int first = context.createMainGeneratedBase ? 2 : 0;
    // structureMatches intentionally accepts only nonempty fixed programs.
    // Pinned LevelOnePage reserves an empty list; absent OL26 imports no words.
    if (context.createMainGeneratedBase) {
        expected = {{41,4,8},{8,15,104},{63,31,7}};
        structureCheck(model, expected);
    } else if (!QTest::qWaitFor([model] {
        return model && model->columnCount() == 4 && model->rowCount() == 0;
    }, 3000)) qFatal("Actual absent-Main baseline must have exactly zero X26 rows");
    if ((!context.createMainGeneratedBase && table->currentIndex().isValid()) || main->isWindowModified())
        qFatal("Actual absent-Main baseline must have no selected triplet or edits");
    if (!dock->grab().save(QString("/work/") + context.captureName("absent-main-before")))
        qFatal("Actual absent-Main baseline capture failed");
    if (!context.createMainGeneratedBase) {
        structureButton(dock, "Insert copy");
        expected.push_back({63,31,7});
        structureCheck(model, expected);
    }
    structureSelect(table, first);
    insertAbsentMainWord(dock, context, "Set Active Position", "absent-main-position-menu");
    // Pinned insertion scans the preceding AP and initializes its next row.
    expected.insert(expected.begin() + first, StructureTriplet{context.createMainGeneratedBase ? 42 : 41,4,0});
    structureCheck(model, expected);
    structureSelect(table, first);
    // Pinned cooked AP Row uses range1..24, unlike Origin modifier range0..23.
    if (!context.createMainGeneratedBase) interiorDefinitionCoordinateStep(dock, 1, 24, 1, 2);
    expected[first][0] = 42;
    structureCheck(model, expected);
    for (int column = 0; column < 8; ++column) {
        interiorDefinitionCoordinateStep(dock, 0, 39, column, column + 1);
        expected[first][2] = column + 1;
        structureCheck(model, expected);
    }
    for (int word = 1; word <= 3; ++word) {
        const int row = first + word;
        structureSelect(table, row);
        insertAbsentMainWord(dock, context, word == 2 ? "G0 character" : "Foreground colour",
            word == 1 ? "absent-main-foreground-menu" : word == 2
                ? "absent-main-glyph-menu" : "absent-main-reset-menu");
        expected.insert(expected.begin() + row, StructureTriplet{0,word == 2 ? 9 : 0,word == 2 ? 32 : 0});
        structureCheck(model, expected);
        structureSelect(table, row);
        const int targetColumn = word == 3 ? 9 : 8;
        for (int column = 0; column < targetColumn; ++column) {
            interiorDefinitionCoordinateStep(dock, 0, 39, column, column + 1);
            expected[row][0] = column + 1;
            structureCheck(model, expected);
        }
        if (word == 2) structureCode(dock, context, row, 32, 81);
        else absentMainColour(dock, context, word == 1 ? 18 : 7,
            word == 1 ? "absent-main-foreground-popup" : "absent-main-reset-popup");
        expected[row][2] = word == 1 ? 18 : word == 2 ? 81 : 7;
        structureCheck(model, expected);
    }
    std::vector<StructureTriplet> final{{42,4,8},{8,0,18},{8,9,81},{9,0,7},{63,31,7}};
    if (context.createMainGeneratedBase) final.insert(final.begin(), {{41,4,8},{8,15,104}});
    structureCheck(model, final);
    if (expected != final || !main->isWindowModified()) qFatal("Actual absent-Main final model differs");
    if (!dock->grab().save(QString("/work/") + context.captureName("absent-main-edited")))
        qFatal("Actual absent-Main edited capture failed");
    savedGlyphStructure = final;
    std::printf("QTM actual absent Main: retained generated-Base words=%d; AP2/8, FG18@8, G0Q81@8, FG7@9, termination7; full %zu-row model checked\n", first, final.size());
}
static void verifySavedGlyphStructure(QMainWindow *main, int expectedCount = 32) {
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || savedGlyphStructure.size() != std::size_t(expectedCount)) qFatal("Fixed saved structure oracle absent");
    const auto tables = dock->findChildren<QTableView *>();
    if (tables.size() != 1) qFatal("Saved structure table absent");
    structureCheck(tables.front()->model(), savedGlyphStructure);
}

// Pinned mainwindow.cpp162: F5 reloads the saved current file through the
// original loader after its real confirmation dialog; no model is replaced here.
static void reloadAbsentMain(QMainWindow *main, const DriverContext &context) {
    if (!context.createMain || main->isWindowModified() || main->windowFilePath() != context.outputPath)
        qFatal("Absent-Main reload requires the clean actual Save As output");
    main->raise();
    main->activateWindow();
    if (auto handle = main->windowHandle()) handle->requestActivate();
    if (!QTest::qWaitForWindowActive(main, 5000)) qFatal("Actual absent-Main reload window not active");
    bool confirmed = false;
    QTimer::singleShot(150, qApp, [main, context, &confirmed] {
        QMessageBox *dialog = nullptr;
        for (auto widget : QApplication::topLevelWidgets()) {
            auto candidate = qobject_cast<QMessageBox *>(widget);
            if (!candidate || !candidate->isVisible() || candidate->parentWidget() != main) continue;
            if (dialog) qFatal("Ambiguous actual absent-Main reload confirmation");
            dialog = candidate;
        }
        if (!dialog || dialog->standardButtons() != (QMessageBox::Yes | QMessageBox::No)
            || dialog->text() != QString("Do you want to reload the document \"%1\" from disk?")
                .arg(QFileInfo(context.outputPath).fileName()))
            qFatal("Actual absent-Main reload confirmation differs");
        auto yes = dialog->button(QMessageBox::Yes);
        if (!yes || !yes->isVisible() || !yes->isEnabled()) qFatal("Actual reload Yes button absent");
        if (!dialog->grab().save(QString("/work/") + context.captureName("absent-main-reload-dialog")))
            qFatal("Actual absent-Main reload dialog capture failed");
        confirmed = true;
        QTest::mouseClick(yes, Qt::LeftButton, Qt::NoModifier, yes->rect().center());
    });
    QTest::keyClick(main, Qt::Key_F5);
    if (!confirmed || main->isWindowModified() || main->windowFilePath() != context.outputPath)
        qFatal("Actual absent-Main disk reload did not complete");
    verifySavedGlyphStructure(main, context.createMainGeneratedBase ? 7 : 5);
    auto dock = main->findChild<QDockWidget *>("x26DockWidget");
    if (!dock || !dock->grab().save(QString("/work/") + context.captureName("absent-main-reloaded")))
        qFatal("Actual absent-Main reloaded dock capture failed");
    std::printf("QTM actual absent Main: Save As followed by confirmed F5 disk reload; full %d-row model retained\n", context.createMainGeneratedBase ? 7 : 5);
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
        if (context.addFirstDefinitionPosition) verifySavedGlyphStructure(saved, 6);
        if (context.addInteriorDefinitionRow) verifySavedGlyphStructure(saved, 10);
        if (context.createMain) {
            verifySavedGlyphStructure(saved, context.createMainGeneratedBase ? 7 : 5);
            reloadAbsentMain(saved, context);
        }
        if (context.insertResourceGlyph || context.deleteResourceGlyph)
            verifySavedGlyphStructure(saved, context.insertResourceGlyph ? 10 : 8);
        if (context.editResourceCallOrigin) verifySavedGlyphStructure(saved, context.resourceCallContext ? 21 : 10);
        if (context.insertResourceCall || context.deleteResourceCall) {
            verifySavedGlyphStructure(saved, context.mixedResourceCallStructure ? 26 : context.mixedResourceCallGlyph ? 22
                : context.insertResourceCall ? 23 : 21);
            if (context.mixedResourceCallStructure) captureResourceStructurePaint(context, "saved");
            else if (context.mixedResourceCallGlyph) captureMixedResourceCallPaint(context, "saved");
            else captureResourceCallTopologyPaint(context, "saved", context.insertResourceCall, true);
        } else if (context.resourceCallContext) captureResourceCallPaint(context, "saved", true);
        if (context.insertFragmentCall || context.deleteFragmentCall) {
            verifySavedGlyphStructure(saved, context.insertFragmentCall ? 27 : 23);
            captureNativeFragmentPaint(context,"saved",false);
        }
        capture(context.captureName("saved").toUtf8().constData());
        if (context.panelProof) capturePanelPaint(context, "saved");
        if (context.singlePage) {
            std::printf("QTM actual single-page widget/Save As complete: edited=%d input=%s output=%s\n",
                (context.editBaseSpacing || context.editNativeG0 || context.editZone || context.editForeground || context.editDrcs || context.editLocalGlyph || context.editLocalGlyphStructure || context.editLocalMixedGlyphStructure || context.editLocalStyleStructure || context.insertResourceGlyph || context.deleteResourceGlyph || context.editResourceCallOrigin || context.insertResourceCall || context.deleteResourceCall || context.insertFragmentCall || context.deleteFragmentCall || context.addFirstDefinitionPosition || context.addInteriorDefinitionRow || context.createMain), context.inputName.constData(), context.outputName.constData());
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
        const bool level15 = context.editMainG2 || context.editNativeG0
            || (context.editBaseSpacing
                && qgetenv("ERSATZRS_QTM_DRIVER_MODE") == "single-page-base-spacing");
        const QString level = level15 ? "1.5" : "2.5";
        for (QRadioButton *radio : main->findChildren<QRadioButton *>())
            if (radio->text() == level) { radio->click(); selected = radio->isChecked(); }
        if (!selected) qFatal("Required fixed presentation-level radio absent");
        capture(context.captureName(level15 ? "level15" : "level25").toUtf8().constData());
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
            if (context.editResourceCallOrigin) editResourceCallOrigin(main, context);
            if (context.insertResourceCall || context.deleteResourceCall) editResourceCallTopology(main, context);
            if (context.insertFragmentCall || context.deleteFragmentCall) editNativeFragmentCalls(main,context);
            if (context.addFirstDefinitionPosition) editFirstDefinitionPosition(main, context);
            if (context.addInteriorDefinitionRow) editInteriorDefinitionRow(main, context);
            if (context.createMain) editAbsentMain(main, context);
            if (context.editBaseSpacing) editBaseSpacing(main, context);
            if (context.editNativeG0) {
                if (main->isWindowModified()
                    || !view->grab().save(QString("/work/") + context.captureName("native-g0-before-widget")))
                    qFatal("Native G0 keyboard trial requires a pristine visible page");
                QTest::mouseClick(view, Qt::LeftButton, Qt::NoModifier, QPoint(102, 15));
                // Send a real Unicode text key to the unchanged editor handler.
                // Its own charset mapping and undo command perform the edit.
                const QString text(QChar(context.nativeUnicode));
                QKeyEvent press(QEvent::KeyPress, 0, Qt::NoModifier, text);
                QKeyEvent release(QEvent::KeyRelease, 0, Qt::NoModifier, text);
                QApplication::sendEvent(view, &press);
                QApplication::sendEvent(view, &release);
                if (!QTest::qWaitFor([main] { return main->isWindowModified(); }, 3000)
                    || !view->grab().save(QString("/work/") + context.captureName("native-g0-edited-widget")))
                    qFatal("Native G0 text key did not produce a visible modified page");
                std::printf("QTM actual native G0 keyboard: row1 column8 UnicodeU+%04x\n", context.nativeUnicode);
            }
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
