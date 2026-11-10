#!/usr/bin/env python3
"""Static evidence that ties admin source files to the Playwright specs that
drive them, so a pull request runs the browser tests of the pages it changes.

Three facts are read from the checked-out sources, never from a hand table:

* the admin route table (`admin/routes.rs`): URL pattern -> page module;
* the routes each spec (and the helpers it imports) names in `goto`, `href`
  and URL assertions: spec -> page modules;
* which admin modules mention the public items of a changed shared module:
  shared module -> page modules; a server-function module is traced to its
  callers the same way;
* the element ids and `data-*` attributes only the application shell renders,
  and the specs that name them: shell -> specs.

The evidence is static. A spec that reaches a page only by clicking, without
ever naming its URL, is invisible here; the weekly full run on `main` owns
that residual risk. Every caller therefore treats "unknown" as "run all".
"""

import posixpath
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ADMIN = "crates/ersatzrs-web/src/admin/"
PAGES = ADMIN + "pages/"
ROUTES = ADMIN + "routes.rs"
TELETEXT_PAGES = PAGES + "channels/teletext"
I18N = ADMIN + "i18n.rs"
LANGUAGES = ADMIN + "languages/"
SERVER_FNS = ADMIN + "server_fns/"
# The application shell wraps every page. It is proven by the hydration smoke
# set and by the specs naming the hooks only the shell renders.
SHELL = (ADMIN + "app.rs", ADMIN + "hydration.rs", ADMIN + "hydration.js")
SHELL_DIRS = (ADMIN + "layout/", ADMIN + "style/")
_HOOK = re.compile(r"""\bid="([\w-]+)"|(?<![\w-])(data-[a-z][a-z-]*)""")


def shell(path):
    return path in SHELL or path.startswith(SHELL_DIRS)

# Top-level items only: methods and fields are reached through their type.
_EXPORT = re.compile(
    r"^pub(?:\([^)]*\))?\s+(?:async\s+)?(?:fn|struct|enum|trait|type|const|static)\s+(\w+)", re.M)
_DECLARATION = re.compile(r"\s*(?://.*|#!?\[.*\]|(?:pub(?:\([^)]*\))?\s+)?(?:mod|use)\b[^;]*;)?\s*")
_ITEM = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")
_BOUNDARY = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?(?:impl|trait|struct|enum|const|static|mod|type)\b")
_KEY = re.compile(r"\bTextKey::(\w+)")


class Evidence:
    """Lazily read source facts for one checkout."""

    def __init__(self, root=ROOT):
        self.root = Path(root)
        self._sources = None
        self._words = None
        self._paths = None
        self._definitions = None
        self.unknown = None
        self._routes = None
        self._spec_pages = None
        self._spec_text = None

    def available(self):
        """False when the route table cannot be read: callers then run all."""
        try:
            return bool(self.routes())
        except (KeyError, ValueError):
            return False

    # -- sources -----------------------------------------------------------
    def sources(self):
        """Admin Rust sources by repository path."""
        if self._sources is None:
            base = self.root / ADMIN
            self._sources = {p.relative_to(self.root).as_posix(): p.read_text()
                             for p in sorted(base.rglob("*.rs"))}
        return self._sources

    def words(self):
        """Identifiers each admin source mentions."""
        if self._words is None:
            self._words = {name: set(re.findall(r"\b[A-Za-z_]\w*\b", text))
                           for name, text in self.sources().items()}
        return self._words

    def paths(self):
        """Module names each admin source uses as a path segment (`name::`),
        each with the segments written directly before it ("" when bare)."""
        if self._paths is None:
            self._paths = {}
            for name, text in self.sources().items():
                found, previous = {}, None
                for match in re.finditer(r"\b(\w+)\s*::\s*", text):
                    joined = previous is not None and previous.end() == match.start()
                    found.setdefault(match[1], set()).add(previous[1] if joined else "")
                    previous = match
                self._paths[name] = found
        return self._paths

    def names_module(self, user, origin):
        """Whether `user` writes a path that can mean the module `origin`.

        Several modules share a stem such as `settings`; the segment before
        it then decides. A bare or grouped use stays a possible user.
        """
        module = origin.removesuffix("/mod.rs").removesuffix(".rs")
        stem, parent = posixpath.basename(module), posixpath.dirname(module)
        before = self.paths()[user].get(stem)
        if not before:
            return False
        near = user.startswith(parent + "/") or user == parent + ".rs"
        return bool(before & {"", posixpath.basename(parent)}) or near and bool(before & {"super", "self"})

    def definitions(self):
        """Public item name -> the admin modules defining it."""
        if self._definitions is None:
            self._definitions = {}
            for name, text in self.sources().items():
                for item in set(_EXPORT.findall(text)):
                    self._definitions.setdefault(item, set()).add(name)
        return self._definitions

    # -- route table -------------------------------------------------------
    def routes(self):
        """[(segments, component, module path)]; a `None` segment is a parameter."""
        if self._routes is not None:
            return self._routes
        source = self.sources()[ROUTES]
        components = {}

        def expand(prefix, body):
            body = body.strip()
            if body.startswith("{"):
                depth, current, parts = 0, "", []
                for char in body[1:-1]:
                    depth += (char == "{") - (char == "}")
                    if char == "," and depth == 0:
                        parts.append(current)
                        current = ""
                    else:
                        current += char
                for part in parts + [current]:
                    if part.strip():
                        expand(prefix, part)
            elif "::" in body:
                head, rest = body.split("::", 1)
                expand(prefix + [head.strip()], rest)
            else:
                components[body] = prefix

        for match in re.finditer(r"use crate::admin::((?:\w+::)*)(\{.*?\}|\w+);", source, re.S):
            expand([part for part in match[1].split("::") if part], match[2])
        routes = []
        for match in re.finditer(r"<Route\s+path=(.*?)\s+view=(\w+)", source, re.S):
            pattern = []
            for kind, value in re.findall(r"(StaticSegment|ParamSegment|WildcardSegment)\(\"([^\"]*)\"\)", match[1]):
                if kind == "StaticSegment":
                    pattern += [segment for segment in value.split("/") if segment]
                else:
                    pattern.append(None)
            module = self._module(components.get(match[2]))
            if module is None:
                raise ValueError(f"route component {match[2]} has no resolvable module")
            routes.append((pattern, match[2], module))
        if not routes:
            raise ValueError("admin route table is empty; evidence cannot be trusted")
        self._routes = routes
        return routes

    def _module(self, parts):
        if not parts:
            return None
        for length in range(len(parts), 0, -1):
            stem = ADMIN + "/".join(parts[:length])
            for candidate in (stem + ".rs", stem + "/mod.rs"):
                if candidate in self.sources():
                    return candidate
        return None

    def route_modules(self):
        return {module for _, _, module in self.routes()}

    def resolve(self, url):
        """Page module a literal admin URL renders, preferring static segments."""
        segments = [s for s in re.split(r"[?#]", url)[0].split("/") if s]
        best = None
        for pattern, _, module in self.routes():
            if len(pattern) != len(segments):
                continue
            score = 0
            for expected, actual in zip(pattern, segments):
                if expected is None:
                    continue
                if expected != actual:
                    break
                score += 1
            else:
                if any(a == ":x" and e is not None for e, a in zip(pattern, segments)):
                    continue
                if best is None or score > best[0]:
                    best = (score, module)
        return best[1] if best else None

    # -- spec evidence -----------------------------------------------------
    def spec_text(self):
        """spec name -> the sources of the spec and of every helper it imports."""
        if self._spec_text is not None:
            return self._spec_text
        e2e = self.root / "tests/e2e"
        cache = {}

        def facts(path):
            if path not in cache:
                text = path.read_text().replace("\\/", "/")
                imports = []
                for module in re.findall(r"(?:\bfrom\s*|\bimport\s*\(?\s*)['\"](\.[^'\"]+)['\"]", text):
                    stem = posixpath.normpath(posixpath.join(path.parent.as_posix(), module))
                    for candidate in (stem, stem + ".ts", stem + "/index.ts", stem.removesuffix(".js") + ".ts"):
                        if Path(candidate).is_file():
                            imports.append(Path(candidate))
                            break
                cache[path] = (text, imports)
            return cache[path]

        result = {}
        for spec in sorted(e2e.glob("*.spec.ts")):
            seen, pending, texts = set(), [spec], []
            while pending:
                current = pending.pop()
                if current in seen:
                    continue
                seen.add(current)
                text, imports = facts(current)
                pending += imports
                texts.append(text)
            result[spec.name.removesuffix(".spec.ts")] = texts
        self._spec_text = result
        return result

    def spec_pages(self):
        """spec name -> page modules whose routes the spec or its helpers name."""
        if self._spec_pages is not None:
            return self._spec_pages
        first = sorted({pattern[0] for pattern, _, _ in self.routes() if pattern and pattern[0]})
        literal = re.compile(r"""['"`/^](/(?:%s)\b[^'"`\s),]*)""" % "|".join(map(re.escape, first)))
        result = {}
        for spec, texts in self.spec_text().items():
            urls = set()
            for text in texts:
                if re.search(r"""goto\(\s*['"`]/(?:[?#][^'"`]*)?['"`]""", text):
                    urls.add("/")
                for match in literal.finditer(text):
                    url = re.sub(r"\$\{[^}]*\}", ":x", match[1])
                    urls.add(re.sub(r"[$\\(\[*+].*$", "", url))
            result[spec] = {module for module in map(self.resolve, urls) if module}
        self._spec_pages = result
        return result

    def shell_hooks(self):
        """Element ids and `data-*` attributes that only the shell renders."""
        def hooks(texts):
            return {a or b for text in texts for a, b in _HOOK.findall(text)}
        inside = hooks(text for name, text in self.sources().items() if shell(name))
        return inside - hooks(text for name, text in self.sources().items() if not shell(name))

    def shell_specs(self):
        """Specs that name a shell hook; empty when the shell exposes none."""
        hooks = self.shell_hooks()
        if not hooks:
            return set()
        wanted = re.compile(r"(?<![\w-])(?:" + "|".join(sorted(map(re.escape, hooks))) + r")(?![\w-])")
        return {spec for spec, texts in self.spec_text().items() if any(map(wanted.search, texts))}

    def page_specs(self, module):
        return {spec for spec, pages in self.spec_pages().items() if module in pages}

    # -- consumers ---------------------------------------------------------
    def declaration_only(self, path):
        """A `mod.rs`-style file holding only module declarations and re-exports."""
        text = self.sources().get(path)
        return text is not None and all(_DECLARATION.fullmatch(line) for line in
                                        re.sub(r"(?s)\{[^{}]*\}", "{}", text).splitlines())

    def style_owners(self, path):
        """Rust modules importing a CSS module; None when nothing imports it."""
        marker = '"' + path.removeprefix("crates/ersatzrs-web/") + '"'
        owners = {name for name, text in self.sources().items() if marker in text}
        return owners or None

    def containing_page(self, path):
        """Nearest route module whose module tree holds a page helper."""
        if not path.startswith(PAGES):
            return None
        routes = self.route_modules()
        folder = posixpath.dirname(path)
        while folder.startswith(PAGES.rstrip("/")) and folder != PAGES.rstrip("/"):
            for candidate in (folder + ".rs", folder + "/mod.rs"):
                if candidate in routes and candidate != path:
                    return candidate
            folder = posixpath.dirname(folder)
        return None

    def reach(self, paths=(), names=()):
        """Follow public item names to the modules that mention them.

        Returns (page route modules, server-function modules, known). The
        pages may hold application shell modules (see `shell()`), which the
        caller proves with the shell specs. A changed server-function module
        is followed to its callers; one that merely uses a changed item is
        recorded for the caller to classify. `known` is False when an
        unreadable file or a changed module without any visible user is
        involved, so the caller runs all.
        """
        sources = self.sources()
        routes = self.route_modules()
        pages, server, known = set(), set(), True
        self.unknown = None
        changed = set(paths)
        seen, pending = set(), [("path", path) for path in paths] + [("name", name) for name in names]
        while pending and known:
            kind, value = pending.pop()
            if (kind, value) in seen:
                continue
            seen.add((kind, value))
            origin = owner = None
            if kind == "name":
                exported = {value}
            else:
                origin = value
                if origin.endswith(".module.css"):
                    owners = self.style_owners(origin)
                    if owners is None:
                        known, self.unknown = False, f"{origin} is imported by no module"
                    changed |= owners or set()
                    pending += [("path", owner) for owner in owners or ()]
                    continue
                if shell(origin):
                    pages.add(origin)
                    continue
                if origin == ROUTES or origin not in sources:
                    known, self.unknown = False, f"{origin} has unknown reach"
                    continue
                if origin.startswith(SERVER_FNS):
                    server.add(origin)
                    if origin not in changed:
                        continue
                if origin.startswith(TELETEXT_PAGES):
                    # The channel teletext page and its submodules are owned by
                    # the teletext spec families, not by route evidence.
                    pages.add(origin)
                    continue
                if origin in routes:
                    pages.add(origin)
                    if origin not in changed:
                        # A page that merely uses a changed item is a leaf; only
                        # a changed page's own public items are followed.
                        continue
                if self.declaration_only(origin):
                    continue
                owner = self.containing_page(origin)
                if owner:
                    pages.add(owner)
                exported = set(_EXPORT.findall(sources[origin]))
            # Interface primitives are re-exported, so their names stand alone.
            # Any other module is reached by path: its user names the module.
            # A name several modules define always needs that qualification.
            interface = origin is not None and origin.startswith(ADMIN + "ui/")
            distinct = {name for name in exported if len(self.definitions().get(name, ())) <= 1}
            subtree = None
            if origin:
                subtree = (origin.removesuffix("mod.rs") if origin.endswith("/mod.rs")
                           else origin.removesuffix(".rs") + "/")
            users = 0
            for name, words in self.words().items():
                # Translation tables name keys, not interface items, and the
                # route table only mounts page components. Server functions
                # use interface types as data; their behavior has its own rule.
                # A module's own submodules are part of it, not users of it.
                if name in (origin, ROUTES, I18N) or name.startswith(LANGUAGES) \
                        or interface and name.startswith(SERVER_FNS) \
                        or subtree and name.startswith(subtree):
                    continue
                if exported.isdisjoint(words):
                    continue
                if origin is None or self.names_module(name, origin) or (
                        interface and not distinct.isdisjoint(words)):
                    pending.append(("path", name))
                    users += 1
            if origin in changed and not users and not owner and origin not in routes:
                known, self.unknown = False, f"{origin} has no visible user"
        return pages, server, known


def changed_lines(before, after):
    """[(is the new side, lines of that side, index of a changed line)].

    `git diff` keeps this linear on the multi-thousand-line translation tables.
    """
    old, new = before.splitlines(), after.splitlines()
    if old == new:
        return []
    with tempfile.TemporaryDirectory() as folder:
        names = []
        for name, text in (("before", before), ("after", after)):
            names.append(str(Path(folder) / name))
            Path(names[-1]).write_text(text)
        done = subprocess.run(["git", "diff", "--no-index", "--no-color", "--unified=0", "--"] + names,
                              capture_output=True, text=True, check=False)
    if done.returncode != 1:
        raise ValueError("could not compare the two revisions of a changed file")
    changes = []
    for start_old, count_old, start_new, count_new in re.findall(
            r"(?m)^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", done.stdout):
        changes += [(False, old, index) for index in
                    range(int(start_old) - 1, int(start_old) - 1 + int(count_old or 1))]
        changes += [(True, new, index) for index in
                    range(int(start_new) - 1, int(start_new) - 1 + int(count_new or 1))]
    return changes


def text_changes(before, after):
    """Classify a translation-table change.

    Returns (keys, functions, known): the `TextKey` variants whose entries
    changed, the helper functions whose bodies changed, and False when a
    changed line belongs to neither.
    """
    keys, functions, known = set(), set(), True
    for _, lines, index in changed_lines(before, after):
        line = lines[index]
        stripped = line.strip()
        if not stripped or stripped.startswith(("//", "use ", "#[")):
            continue
        found = _KEY.findall(line)
        if found:
            keys.update(found)
            continue
        for above in range(index, -1, -1):
            candidate = lines[above]
            found = _KEY.findall(candidate)
            if found and above != index:
                keys.update(found)
                break
            item = _ITEM.match(candidate)
            if item:
                functions.add(item[1])
                break
            if re.match(r"^pub enum TextKey\b", candidate):
                variant = re.fullmatch(r"(\w+),?", stripped)
                if variant:
                    keys.add(variant[1])
                else:
                    known = False
                break
            if _BOUNDARY.match(candidate):
                known = False
                break
        else:
            known = False
    return keys, functions, known


def key_helpers(text, keys):
    """Functions of a translation module that read one of `keys` themselves."""
    helpers = set()
    lines = text.splitlines()
    wanted = re.compile(r"\bTextKey::(?:" + "|".join(sorted(map(re.escape, keys))) + r")\b") if keys else None
    for index, line in enumerate(lines):
        if not wanted or not wanted.search(line):
            continue
        for above in range(index, -1, -1):
            item = _ITEM.match(lines[above])
            if item:
                helpers.add(item[1])
                break
            if _BOUNDARY.match(lines[above]):
                break
    return helpers


def changed_names(before, after, known_names=None):
    """Identifiers a change adds to or removes from a declaration-only file.

    With `known_names` (the route table's components) every known name on a
    changed line counts, because a route can change without its component.
    """
    found = {True: set(), False: set()}
    for added, lines, index in changed_lines(before, after):
        window = lines[index]
        if known_names is not None and "view=" not in window:
            # A multi-line <Route>: its component follows within the element.
            for below in range(index + 1, min(index + 8, len(lines))):
                window += " " + lines[below]
                if "view=" in lines[below] or "/>" in lines[below]:
                    break
        if known_names is None and re.fullmatch(
                r"\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+;\s*", window):
            # The declared module file is classified by its own change.
            continue
        found[added].update(re.findall(r"\b[A-Za-z_]\w*\b", window))
    if known_names is not None:
        return (found[True] | found[False]) & known_names
    keywords = {"pub", "mod", "use", "crate", "self", "super", "as", "admin", "cfg", "feature", "allow"}
    return (found[True] ^ found[False]) - keywords
