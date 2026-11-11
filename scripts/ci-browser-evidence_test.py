#!/usr/bin/env python3
"""Regression tests for the browser spec evidence (scripts/ci-browser-evidence.py)."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ci_browser_evidence", ROOT / "scripts/ci-browser-evidence.py")
evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evidence)

ADMIN = evidence.ADMIN
PAGES = evidence.PAGES


def tree(files):
    """A throwaway checkout holding `files` (repository path -> content)."""
    folder = tempfile.TemporaryDirectory()
    for name, text in files.items():
        target = Path(folder.name) / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    return folder


ROUTES = '''
use crate::admin::home::HomePage;
use crate::admin::pages::things::{editor::ThingEditorPage, list::ThingsListPage};
use crate::admin::pages::other::OtherPage;
<Route path=StaticSegment("") view=HomePage/>
<Route path=StaticSegment("things") view=ThingsListPage/>
<Route
    path=(StaticSegment("things"), StaticSegment("add"))
    view=ThingEditorPage
/>
<Route path=(StaticSegment("things"), ParamSegment("id")) view=ThingEditorPage/>
<Route path=StaticSegment("other") view=OtherPage/>
'''
SOURCES = {
    ADMIN + "routes.rs": ROUTES,
    ADMIN + "home.rs": "pub fn HomePage() {}\n",
    ADMIN + "app.rs": "fn shell() { Shell() }\n",
    ADMIN + "layout/mod.rs": "pub fn Shell() { Banner() }\n",
    ADMIN + "ui/mod.rs": "pub mod primitives;\n",
    ADMIN + "ui/primitives/mod.rs": "pub mod banner;\npub mod chip;\npub use banner::Banner;\npub use chip::{Chip};\n",
    ADMIN + "ui/primitives/banner.rs": 'import_crate_style!(style, "src/admin/ui/primitives/banner.module.css");\npub fn Banner() {}\n',
    ADMIN + "ui/primitives/banner.module.css": ".banner {}\n",
    ADMIN + "ui/primitives/chip.rs": "pub fn Chip() {}\nimpl Chip {\n    pub fn title(self) {}\n}\n",
    ADMIN + "ui/primitives/orphan.module.css": ".orphan {}\n",
    PAGES + "mod.rs": "pub mod things;\npub mod other;\n",
    PAGES + "things/mod.rs": "pub mod editor;\npub mod list;\npub mod shared;\n",
    PAGES + "things/list.rs": "use super::shared::row;\npub fn ThingsListPage() { Chip(); row() }\n",
    PAGES + "things/editor.rs": "pub mod fields;\npub fn ThingEditorPage() { fields::title() }\npub fn preview() {}\n",
    PAGES + "things/editor/fields.rs": "pub fn title() {}\n",
    PAGES + "things/shared.rs": "pub fn row() {}\n",
    PAGES + "things/unused.rs": "pub fn nothing() {}\n",
    PAGES + "other.rs": "use crate::admin::pages::things::editor::preview;\npub fn OtherPage() { preview(); title() }\n",
    ADMIN + "server_fns/things.rs": "pub fn load() { Chip() }\n",
    "tests/e2e/things.spec.ts": "import { open } from './helpers/open';\ntest('x', async ({ page }) => { await open(page); await expect(page).toHaveURL(/\\/things\\/add$/); });\n",
    "tests/e2e/helpers/open.ts": "export const open = (page) => page.goto('/things');\n",
    "tests/e2e/detail.spec.ts": "test('x', async ({ page }) => { await page.goto(`/things/${id}`); await page.goto('/'); });\n",
    "tests/e2e/host.spec.ts": "test('x', async () => { await fetch('/api/things'); });\n",
}


class BrowserEvidence(unittest.TestCase):
    def setUp(self):
        self.folder = tree(SOURCES)
        self.addCleanup(self.folder.cleanup)
        self.facts = evidence.Evidence(self.folder.name)

    def test_routes_resolve_to_their_page_modules(self):
        self.assertTrue(self.facts.available())
        self.assertEqual(self.facts.resolve("/things"), PAGES + "things/list.rs")
        self.assertEqual(self.facts.resolve("/things/add?x=1"), PAGES + "things/editor.rs")
        self.assertEqual(self.facts.resolve("/things/:x"), PAGES + "things/editor.rs")
        self.assertEqual(self.facts.resolve("/"), ADMIN + "home.rs")
        self.assertIsNone(self.facts.resolve("/things/1/2"))

    def test_specs_own_the_pages_they_or_their_helpers_name(self):
        pages = self.facts.spec_pages()
        self.assertEqual(pages["things"], {PAGES + "things/list.rs", PAGES + "things/editor.rs"})
        self.assertEqual(pages["detail"], {PAGES + "things/editor.rs", ADMIN + "home.rs"})
        self.assertEqual(pages["host"], set())
        self.assertEqual(self.facts.page_specs(PAGES + "things/list.rs"), {"things"})

    def test_missing_route_table_is_unavailable(self):
        with tree({ADMIN + "home.rs": ""}) as folder:
            self.assertFalse(evidence.Evidence(folder).available())
        with tree({ADMIN + "routes.rs": "use crate::admin::pages::gone::GonePage;\n"
                                         '<Route path=StaticSegment("gone") view=GonePage/>'}) as folder:
            self.assertFalse(evidence.Evidence(folder).available())

    def test_changed_page_reaches_itself_and_the_users_of_its_items(self):
        pages, server, known = self.facts.reach((PAGES + "things/editor.rs",))
        self.assertEqual((pages, server, known),
                         ({PAGES + "things/editor.rs", PAGES + "other.rs"}, set(), True))
        # A page that only uses an item is a leaf, and a method name is not an item.
        self.assertEqual(self.facts.reach((PAGES + "things/list.rs",))[0], {PAGES + "things/list.rs"})

    def test_page_helpers_reach_their_users_or_their_containing_page(self):
        self.assertEqual(self.facts.reach((PAGES + "things/shared.rs",)),
                         ({PAGES + "things/list.rs"}, set(), True))
        # Used only through the page that declares it.
        self.assertEqual(self.facts.reach((PAGES + "things/editor/fields.rs",)),
                         ({PAGES + "things/editor.rs"}, set(), True))
        _, _, known = self.facts.reach((PAGES + "things/unused.rs",))
        self.assertFalse(known)
        self.assertIn("no visible user", self.facts.unknown)

    def test_primitives_reach_pages_and_the_shell(self):
        self.assertEqual(self.facts.reach((ADMIN + "ui/primitives/chip.rs",)),
                         ({PAGES + "things/list.rs"}, set(), True))
        # The shell is a leaf: its own specs prove a primitive it renders.
        for path in (ADMIN + "ui/primitives/banner.rs", ADMIN + "ui/primitives/banner.module.css",
                     ADMIN + "layout/mod.rs"):
            with self.subTest(path=path):
                self.assertEqual(self.facts.reach((path,)), ({ADMIN + "layout/mod.rs"}, set(), True))
        self.assertTrue(evidence.shell(ADMIN + "hydration.js"))
        self.assertFalse(evidence.shell(ADMIN + "routes.rs"))
        for path in (ADMIN + "ui/primitives/orphan.module.css", ADMIN + "routes.rs", ADMIN + "missing.rs"):
            with self.subTest(path=path):
                self.assertFalse(self.facts.reach((path,))[2])
                self.assertTrue(self.facts.unknown)

    def test_shell_specs_name_elements_only_the_shell_renders(self):
        with tree({**SOURCES,
                   ADMIN + "layout/mod.rs": 'pub fn Shell() { <aside id="drawer" data-drawer data-state=state/> }\n',
                   PAGES + "other.rs": "pub fn OtherPage() { <div data-state=state data-drawer-open/> }\n",
                   "tests/e2e/helpers/open.ts": "export const open = (page) => page.locator('#drawer');\n",
                   "tests/e2e/detail.spec.ts": "test('x', async ({ page }) => page.locator('[data-state]'));\n",
                   "tests/e2e/host.spec.ts": "test('x', async ({ page }) => page.locator('[data-drawer-open]'));\n",
                   }) as folder:
            facts = evidence.Evidence(folder)
            self.assertEqual(facts.shell_hooks(), {"drawer", "data-drawer"})
            # Named through an imported helper; a longer attribute is another hook.
            self.assertEqual(facts.shell_specs(), {"things"})
        self.assertEqual(self.facts.shell_specs(), set())

    def test_used_server_functions_are_reported_not_followed(self):
        with tree({**SOURCES, ADMIN + "format.rs": "pub fn stamp() {}\n",
                   ADMIN + "server_fns/things.rs": "use crate::admin::format::stamp;\npub fn load() { stamp() }\n"}) as folder:
            facts = evidence.Evidence(folder)
            self.assertEqual(facts.reach((ADMIN + "format.rs",)),
                             (set(), {ADMIN + "server_fns/things.rs"}, True))

    def test_changed_server_functions_reach_their_callers(self):
        things = ADMIN + "server_fns/things.rs"
        with tree({**SOURCES,
                   PAGES + "things/list.rs": "use crate::admin::server_fns::things::load;\npub fn ThingsListPage() { load() }\n",
                   ADMIN + "layout/mod.rs": "pub fn Shell() { crate::admin::server_fns::things::load() }\n",
                   ADMIN + "server_fns/other.rs": "pub fn count() { super::things::load() }\n",
                   # Shares the stem, but names the page module, not the server one.
                   PAGES + "other.rs": "pub fn OtherPage() { crate::admin::pages::things::load() }\n",
                   }) as folder:
            facts = evidence.Evidence(folder)
            self.assertEqual(facts.reach((things,)),
                             ({PAGES + "things/list.rs", ADMIN + "layout/mod.rs"},
                              {things, ADMIN + "server_fns/other.rs"}, True))
        # Without a caller the reach is unknown.
        self.assertFalse(self.facts.reach((things,))[2])
        self.assertIn("no visible user", self.facts.unknown)

    def test_a_shared_module_stem_needs_its_parent(self):
        with tree({**SOURCES,
                   PAGES + "things/editor/fields.rs": "pub fn title() {}\n",
                   PAGES + "things/editor.rs": "pub mod fields;\npub fn ThingEditorPage() { self::fields::title() }\n",
                   ADMIN + "server_fns/fields.rs": "pub fn title() {}\n",
                   PAGES + "other.rs": "pub fn OtherPage() { crate::admin::server_fns::fields::title() }\n",
                   }) as folder:
            facts = evidence.Evidence(folder)
            self.assertEqual(facts.reach((PAGES + "things/editor/fields.rs",)),
                             ({PAGES + "things/editor.rs"}, set(), True))
            self.assertEqual(facts.reach((ADMIN + "server_fns/fields.rs",)),
                             ({PAGES + "other.rs"}, {ADMIN + "server_fns/fields.rs"}, True))

    def test_changed_items_name_the_public_items_a_change_touches(self):
        before = ("use a::b;\n\nfn helper() -> u32 {\n    1\n}\n\n/// Lists.\npub async fn list() -> u32 {\n"
                  "    helper()\n}\n\npub fn other() {}\n\nimpl Thing {\n    fn x(&self) {}\n}\n\n"
                  "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n")
        # A private helper is replaced by the public items calling it.
        self.assertEqual(evidence.changed_items(before, before.replace("    1\n", "    2\n")), {"list"})
        self.assertEqual(evidence.changed_items(before, before.replace("/// Lists.", "/// All rows.")), {"list"})
        self.assertEqual(evidence.changed_items(before, before.replace("fn x(&self) {}", "fn x(&self) { y() }")),
                         {"Thing"})
        # Imports and test modules have no consumers of their own.
        self.assertEqual(evidence.changed_items(before, before.replace("use a::b;", "use a::{b, c};")), set())
        self.assertEqual(evidence.changed_items(before, before.replace("fn t() {}", "fn t() { u() }")), set())
        self.assertIsNone(evidence.changed_items(before, before + "mod extra;\n"))

    def test_workspace_users_follow_module_paths_and_the_crate_root(self):
        app = "crates/ersatzrs-app/src/"
        with tree({app + "queries.rs": "pub fn list_things() {}\npub fn count_things() {}\n",
                   app + "lib.rs": "pub mod queries;\nimpl App {\n    pub fn list_all_things(&self) { queries::list_things() }\n}\n",
                   app + "commands.rs": "fn list_things() {}\n",
                   app + "operations.rs": "fn go() { queries::count_things() }\n",
                   app + "queries_tests.rs": "fn t() { queries::list_things() }\n",
                   ADMIN + "server_fns/things.rs": "fn load(app: &App) { app.list_all_things() }\n",
                   ADMIN + "pages/other.rs": "fn x() { ersatzrs_app::queries::list_things() }\n",
                   "crates/ersatzrs-web/src/host/router.rs": "get(api::things::list_things)\n",
                   "crates/ersatzrs-web/src/api/things.rs": "pub async fn list_things() {}\n",
                   }) as folder:
            self.assertEqual(evidence.workspace_users(folder, app + "queries.rs", {"list_things"}),
                             {ADMIN + "server_fns/things.rs", ADMIN + "pages/other.rs"})
            self.assertEqual(evidence.workspace_users(folder, app + "queries.rs", {"count_things"}),
                             {app + "operations.rs"})

    def test_declaration_only_modules_are_transparent(self):
        self.assertTrue(self.facts.declaration_only(ADMIN + "ui/primitives/mod.rs"))
        self.assertFalse(self.facts.declaration_only(ADMIN + "ui/primitives/chip.rs"))
        before = SOURCES[ADMIN + "ui/primitives/mod.rs"]
        after = before.replace("pub use chip::{Chip};", "pub mod tag;\npub use chip::{Chip, Dot};\npub use tag::Tag;")
        self.assertEqual(evidence.changed_names(before, after), {"Dot", "tag", "Tag"})
        self.assertEqual(evidence.changed_names(before, before), set())

    def test_route_changes_name_their_components(self):
        known = {"HomePage", "ThingsListPage", "ThingEditorPage", "OtherPage"}
        after = ROUTES.replace('(StaticSegment("things"), StaticSegment("add"))',
                               '(StaticSegment("things"), StaticSegment("new"))')
        self.assertEqual(evidence.changed_names(ROUTES, after, known), {"ThingEditorPage"})
        after = ROUTES.replace("view=OtherPage", "view=HomePage")
        self.assertEqual(evidence.changed_names(ROUTES, after, known), {"OtherPage", "HomePage"})

    def test_translation_changes_name_keys_and_helpers(self):
        before = (
            "pub enum TextKey {\n    Save,\n    Cancel,\n}\n"
            "pub struct Strings;\n"
            "fn text(key: TextKey) -> &'static str {\n    match key {\n"
            "        TextKey::Save => \"Save\",\n"
            "        TextKey::Cancel => {\n            \"Cancel\"\n        }\n    }\n}\n"
            "pub fn count(total: u32) -> String {\n    format!(\"{total} items\")\n}\n"
            "pub fn label() -> String {\n    tr(TextKey::Save)\n}\n")
        after = before.replace("    Cancel,\n", "    Cancel,\n    Close,\n").replace(
            '"Cancel"', '"Annuleren"').replace("{total} items", "{total} rows")
        self.assertEqual(evidence.text_changes(before, after), ({"Cancel", "Close"}, {"count"}, True))
        self.assertEqual(evidence.text_changes(before, before), (set(), set(), True))
        self.assertEqual(evidence.text_changes(before, before + "// note\n\n"), (set(), set(), True))
        _, _, known = evidence.text_changes(before, before.replace("pub struct Strings;", "pub struct Strings {}"))
        self.assertFalse(known)
        self.assertEqual(evidence.key_helpers(before, {"Save"}), {"text", "label"})
        self.assertEqual(evidence.key_helpers(before, set()), set())


if __name__ == "__main__":
    unittest.main()
