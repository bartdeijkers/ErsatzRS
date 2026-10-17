// Opt-in Linux/X11 calibration input for pinned QTeletextMaker 1.0.
// The external editor binary, decoder and TTI writer remain unchanged. This
// test-only preload drives actual widgets; it grants no ErsatzRS publication.
// Other platforms retain separate editor/client acceptance requirements.
#include <QApplication>
#include <QAction>
#include <QFileDialog>
#include <QFileInfo>
#include <QComboBox>
#include <QDockWidget>
#include <QTableView>
#include <QMainWindow>
#include <QMenu>
#include <QRadioButton>
#include <QLineEdit>
#include <QPushButton>
#include <QProcess>
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
    const bool extendedGlyph = mode == "single-page-local-extended-glyph";
    const bool localGlyph = mode == "single-page-local-glyph" || extendedGlyph;
    const bool suppliedGlyph = qEnvironmentVariableIsSet("ERSATZRS_QTM_GLYPH_MODE")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_FROM_GLYPH")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH")
        || qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE");
    if (!localGlyph && suppliedGlyph)
        qFatal("Glyph operands require explicit single-page-local-glyph mode");
    if (!extendedGlyph && qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE"))
        qFatal("Replacement glyph mode requires the fixed extended glyph trial");
    const int glyphMode = localGlyph ? boundedDecimal("ERSATZRS_QTM_GLYPH_MODE", extendedGlyph ? 31 : 9) : 0;
    const int toGlyphMode = extendedGlyph && qEnvironmentVariableIsSet("ERSATZRS_QTM_TO_GLYPH_MODE")
        ? boundedDecimal("ERSATZRS_QTM_TO_GLYPH_MODE", 31) : glyphMode;
    const int fromGlyph = localGlyph ? boundedDecimal("ERSATZRS_QTM_FROM_GLYPH", 127) : 0;
    const int toGlyph = localGlyph ? boundedDecimal("ERSATZRS_QTM_TO_GLYPH", 127) : 0;
    if (localGlyph && !extendedGlyph && (glyphMode != 9 && glyphMode != 1))
        qFatal("Fixed local glyph mode must retain raw G0 mode9 or G1 mode1");
    if (localGlyph && fromGlyph == toGlyph && glyphMode == toGlyphMode)
        qFatal("Local glyph trial requires a changed glyph");
    if (localGlyph && !extendedGlyph && ((glyphMode == 9 && (fromGlyph != 65 || toGlyph != 66))
        || (glyphMode == 1 && (fromGlyph != 33 || toGlyph != 34))))
        qFatal("Fixed local glyph trial requires G0 A-to-B or G1 0x21-to-0x22");
    if (extendedGlyph && !((glyphMode == 15 && toGlyphMode == 15 && fromGlyph == 35 && toGlyph == 66)
        || (glyphMode == 25 && toGlyphMode == 28 && fromGlyph == 65 && toGlyph == 65)))
        qFatal("Fixed extended trial requires G2 35-to-66 or A diacritic9-to12");
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
    if (!foreground && (suppliedForeground || (!localGlyph && suppliedTarget)))
        qFatal("Foreground operands require explicit single-page-foreground mode");
    const int triplet = (foreground || localGlyph) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_INDEX", 207) : 0;
    const int column = (foreground || localGlyph) ? boundedDecimal("ERSATZRS_QTM_TRIPLET_COLUMN", 39) : 0;
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
    if ((mode != "single-page-zone-q" && mode != "single-page-save" && !foreground && !drcsEdit && !drcsVerify && !panelProof && !localGlyph) || !safeTtiBasename(input) || !safeTtiBasename(output)
        || input == output) qFatal("Invalid explicit synthetic single-page driver context");
    const int left = panelColumns("ERSATZRS_QTM_LEFT_PANEL_COLUMNS");
    const int right = panelColumns("ERSATZRS_QTM_RIGHT_PANEL_COLUMNS");
    const QByteArray drcs = qgetenv("ERSATZRS_QTM_NORMAL_DRCS_BASENAME");
    if (!drcs.isEmpty() && (!safeTtiBasename(drcs) || drcs == input || drcs == output))
        qFatal("Invalid explicit protected Normal DRCS companion name");
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
    if (localGlyph && (left != 4 || right != 12 || drcs.isEmpty() || drcsSubpage != 0))
        qFatal("Fixed local glyph trial requires original left4/right12 and Normal S0 companion");
    return {true, mode == "single-page-zone-q" || panelDay, foreground, drcsEdit, drcsVerify, resourceSet, triplet, column, from, to, drcsSubpage, input, output, QString("/work/") + QString::fromLatin1(input),
        QString("/work/") + QString::fromLatin1(output), drcs, left, right, panelProof, panelNight, localGlyph, glyphMode, fromGlyph, toGlyph, toGlyphMode};
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
        capture(context.captureName("saved").toUtf8().constData());
        if (context.panelProof) capturePanelPaint(context, "saved");
        if (context.singlePage) {
            std::printf("QTM actual single-page widget/Save As complete: edited=%d input=%s output=%s\n",
                (context.editZone || context.editForeground || context.editDrcs || context.editLocalGlyph), context.inputName.constData(), context.outputName.constData());
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
