"""Sphinx configuration for the consensus project site (Shibuya theme)."""

from __future__ import annotations

from pathlib import Path

_DOCS = Path(__file__).resolve().parent
_ROOT = _DOCS.parent.parent

project = "consensus"
copyright = "2026, Rohit Goswami"
author = "Rohit Goswami"
release = "0.1.0"
version = "0.1"

extensions = [
    "sphinx.ext.mathjax",
    "sphinx_copybutton",
]

templates_path = ["_templates"]
exclude_patterns: list[str] = []

html_theme = "shibuya"
html_static_path = ["_static"]
html_favicon = "_static/favicon.svg"
html_logo = "_static/mark.svg"
html_title = "consensus"
html_css_files = ["custom.css"]
# The menu's "view source" link is the copy Sphinx writes under _sources/.
# Without a base URL the theme points that link at raw.githubusercontent
# with a .rst.txt suffix, and that file is not in the repo.
html_baseurl = "https://leidarljos.github.io/consensus/"
html_copy_source = True

html_context = {
    "source_type": "github",
    "source_user": "leidarljos",
    "source_repo": "consensus",
    "source_version": "main",
    "source_docs_path": "/docs/source/",
    "og_image": "https://leidarljos.github.io/consensus/_static/og.png",
    # The page's own opening words, or its own headings where a page has no lede.
    "page_descriptions": {
        "index": (
            "A tally counts ballots. A settle weighs them: each voter listens "
            "to the voters it trusts and moves toward their opinion."
        ),
        "getting-started": (
            "Three voters, two options, and a trust graph that changes the answer."
        ),
        "howto": (
            "Settle the tracker's ballots under the pack's rows, feed rows from "
            "anywhere, anchor the voters, read the answer in a script, and "
            "cross-check with Seldon."
        ),
        "reference": (
            "The ljos-consensus command line, the Friedkin-Johnsen step, and the library."
        ),
        "explanation": (
            "Counting is right when every voter is worth the same. A maintainer "
            "who has read the code for years, a reviewer who was wrong twice, and "
            "a first-time contributor each cast one ballot."
        ),
        "derivation": (
            "Where the settle's guarantees come from: the closed form the iteration "
            "solves for, the round count that follows from it, the floating-point "
            "budget the stopping rule respects, the proof assistant the statements "
            "mirror, and the social science behind the models."
        ),
        "seat": "This crate is one habitat. The seat that sits on it is ljos.",
        "search": "Search the consensus documentation.",
        "genindex": "Index of the consensus documentation.",
    },
}

html_theme_options = {
    "accent_color": "gold",
    "color_mode": "dark",
    "dark_code": True,
    "github_url": "https://github.com/leidarljos/consensus",
    "nav_links": [
        {"title": "Get started", "url": "getting-started"},
        {"title": "How-to", "url": "howto"},
        {"title": "Reference", "url": "reference"},
        {"title": "Explanation", "url": "explanation"},
        {"title": "Search", "url": "search"},
    ],
}

# Offline builds must not reach for an inventory.
intersphinx_mapping: dict = {}

copybutton_prompt_text = r"\$ "
copybutton_prompt_is_regexp = True


def _org_title(docname: str) -> str | None:
    path = _ROOT / "docs" / "orgmode" / f"{docname}.org"
    if not path.is_file():
        return None
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.lower().startswith("#+title:"):
            title = line.split(":", 1)[1].strip()
            return title or None
    return None


def _top_sections(doctree: object) -> list:
    from docutils import nodes

    if doctree is None:
        return []
    children = getattr(doctree, "children", None)
    if not children:
        return []
    return [node for node in children if isinstance(node, nodes.section)]


def _inject_page_title(app: object, docname: str, source: list[str]) -> None:
    """Use the org title for <title>, not the first section heading."""
    title = _org_title(docname)
    if not title or ".. title::" in source[0]:
        return
    source[0] = f".. title:: {title}\n\n" + source[0]


def setup(app: object) -> None:
    from html import escape

    from docutils import nodes
    from sphinx.writers.html5 import HTML5Translator

    class ConsensusHTMLTranslator(HTML5Translator):
        """One h1, the page title. Further sections step down a level."""

        def __init__(self, document: nodes.document, builder: object) -> None:
            super().__init__(document, builder)
            if len(_top_sections(document)) > 1:
                # visit_section increments before the heading is written.
                self.section_level = 1

    def _on_html_page(
        app: object,
        pagename: str,
        templatename: str,
        context: dict,
        doctree: object,
    ) -> None:
        top = _top_sections(doctree)
        if top and isinstance(top[0][0], nodes.title):
            context["crumb_title"] = top[0][0].astext()
        if len(top) < 2:
            return
        plain = context.get("title") or _org_title(pagename) or ""
        context["body"] = (
            f'<h1 class="page-title visually-hidden">{escape(str(plain))}</h1>\n'
            + context.get("body", "")
        )

    app.set_translator("html", ConsensusHTMLTranslator, override=True)
    app.connect("source-read", _inject_page_title)
    app.connect("html-page-context", _on_html_page)
