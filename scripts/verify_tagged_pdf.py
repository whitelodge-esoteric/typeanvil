#!/usr/bin/env python3
"""Read back a tagged PDF's structure tree and check it against its source HTML.

Spec: docs/specifications/tagged-pdf.spec.md (validation log). Issue: CORE-373.

Usage:
    verify_tagged_pdf.py <tagged.pdf> [--html <source.html>] [--tree] [--json]

Uses pdfium (pypdfium2) the same way CORE-111's validation did. pdfium only
exposes the structure tree PER PAGE (each page's tree holds the elements whose
content sits on that page, with their ancestors), so whole-document figures
are built from per-page walks:

* coverage   - every text/image/path object on a page is either inside a
               marked-content sequence the tree references (MCID), or marked
               as an Artifact. Anything else is "untagged" content.
* dangling   - MCIDs the tree references that no page object carries, and
               MCIDs page objects carry that no tree element references.
* roles      - structure-type counts (an element split across pages counts
               once per page it appears on).
* order      - the text read in tree order (pages ascending, pre-order walk)
               compared with the source HTML's DOM text order (word-level
               similarity, 1.0 = identical sequence).
* expected   - counts of semantic HTML elements in the source, so a reader
               can compare e.g. 12 <h2> against the H2 count.

Exit status: 0 always for a readable PDF (this is a measurement tool, not a
gate); 2 for usage/IO errors.
"""
from __future__ import annotations

import argparse
import ctypes
import difflib
import html.parser
import json
import re
import sys
from collections import Counter

import pypdfium2 as pdfium
import pypdfium2.raw as R

# ---------------------------------------------------------------- pdfium utils


def _wstr(fn, *args) -> str:
    """Call a pdfium (…, buffer, buflen) -> bytes-needed UTF-16LE getter."""
    n = fn(*args, None, 0)
    if n <= 2:
        return ""
    buf = ctypes.create_string_buffer(n)
    # Some getters declare void*, others FPDF_WCHAR* — cast to whatever the
    # binding expects for the buffer slot.
    fn(*args, ctypes.cast(buf, fn.argtypes[len(args)]), n)
    return buf.raw[: n - 2].decode("utf-16-le", errors="replace")


def _mark_names(obj) -> list[str]:
    names = []
    for i in range(R.FPDFPageObj_CountMarks(obj)):
        mark = R.FPDFPageObj_GetMark(obj, i)
        # (mark, buffer, buflen, out_buflen*) -> bool; out_buflen = bytes needed.
        need = ctypes.c_ulong(0)
        R.FPDFPageObjMark_GetName(mark, None, 0, ctypes.byref(need))
        if need.value <= 2:
            names.append("")
            continue
        buf = ctypes.create_string_buffer(need.value)
        R.FPDFPageObjMark_GetName(mark, ctypes.cast(buf, R.FPDFPageObjMark_GetName.argtypes[1]),
                                  need.value, ctypes.byref(need))
        names.append(buf.raw[: need.value - 2].decode("utf-16-le", errors="replace"))
    return names


OBJ_KIND = {
    R.FPDF_PAGEOBJ_TEXT: "text",
    R.FPDF_PAGEOBJ_PATH: "path",
    R.FPDF_PAGEOBJ_IMAGE: "image",
    R.FPDF_PAGEOBJ_SHADING: "shading",
    R.FPDF_PAGEOBJ_FORM: "form",
}


def _page_objects(page, textpage):
    """Yield (kind, mcid, is_artifact, text) for every leaf page object."""

    def walk(get_count, get_obj, parent):
        for i in range(get_count(parent)):
            obj = get_obj(parent, i)
            kind = OBJ_KIND.get(R.FPDFPageObj_GetType(obj), "other")
            if kind == "form":
                yield from walk(R.FPDFFormObj_CountObjects, R.FPDFFormObj_GetObject, obj)
                continue
            mcid = R.FPDFPageObj_GetMarkedContentID(obj)
            art = "Artifact" in _mark_names(obj)
            text = _wstr(R.FPDFTextObj_GetText, obj, textpage) if kind == "text" else ""
            yield kind, mcid, art, text

    yield from walk(R.FPDFPage_CountObjects, R.FPDFPage_GetObject, page.raw)


def _elem_mcids(el) -> list[int]:
    out = []
    n = R.FPDF_StructElement_GetMarkedContentIdCount(el)
    for i in range(max(n, 0)):
        m = R.FPDF_StructElement_GetMarkedContentIdAtIndex(el, i)
        if m >= 0:
            out.append(m)
    return out


def _walk_elem(el, depth, page_no, mcid_text, sink):
    """Pre-order walk. Appends dicts to sink; returns nothing."""
    node = {
        "page": page_no,
        "depth": depth,
        "type": _wstr(R.FPDF_StructElement_GetType, el),
        "title": _wstr(R.FPDF_StructElement_GetTitle, el),
        "alt": _wstr(R.FPDF_StructElement_GetAltText, el),
        "actual": _wstr(R.FPDF_StructElement_GetActualText, el),
        "lang": _wstr(R.FPDF_StructElement_GetLang, el),
        "mcids": [],
        "text": "",
    }
    sink.append(node)
    # Children: struct elements, or MCID kids (child handle is NULL for those).
    own = []
    has_struct_kids = False
    for i in range(R.FPDF_StructElement_CountChildren(el)):
        child = R.FPDF_StructElement_GetChildAtIndex(el, i)
        if child:
            has_struct_kids = True
            _walk_elem(child, depth + 1, page_no, mcid_text, sink)
        else:
            m = R.FPDF_StructElement_GetChildMarkedContentID(el, i)
            if m >= 0:
                own.append(m)
    # A bare-integer /K is not exposed as a child; pdfium reports it through
    # the element's own MCID list. Only consult that list for leaf elements —
    # for a parent it can echo a descendant's MCID (double-counting it).
    if not own and not has_struct_kids:
        own = _elem_mcids(el)
    node["mcids"] = own
    node["text"] = " ".join(mcid_text[m] for m in own if m in mcid_text)


# ---------------------------------------------------------------- HTML source

SEMANTIC = {
    "h1", "h2", "h3", "h4", "h5", "h6", "p", "ul", "ol", "dl", "li", "dt", "dd",
    "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption", "figure",
    "figcaption", "img", "svg", "a", "blockquote", "pre", "code", "section",
    "article", "aside", "nav", "header", "footer", "main", "strong", "em",
}
SKIP = {"head", "style", "script", "title", "template", "noscript"}
VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link",
        "meta", "source", "track", "wbr"}


class _Source(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.counts: Counter[str] = Counter()
        self.words: list[str] = []
        self.lang = None
        self.title = ""
        self._skip = 0
        self._in_title = False
        self.footnote_like = 0

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "html":
            self.lang = a.get("lang")
        if tag == "title":
            self._in_title = True
        if tag in SKIP:
            self._skip += 1
            return
        if tag in SEMANTIC:
            self.counts[tag] += 1
        if "footnote" in (a.get("class") or "") or "fn" == (a.get("class") or ""):
            self.footnote_like += 1

    def handle_startendtag(self, tag, attrs):
        if tag in SEMANTIC:
            self.counts[tag] += 1

    def handle_endtag(self, tag):
        if tag == "title":
            self._in_title = False
        if tag in SKIP and self._skip:
            self._skip -= 1

    def handle_data(self, data):
        if self._in_title:
            self.title += data
        if self._skip:
            return
        self.words.extend(_words(data))


def _words(s: str) -> list[str]:
    s = s.replace("\u00ad", "").replace("\ufffe", "")
    return re.findall(r"[0-9A-Za-z\u00c0-\uffff]+", s.lower())


# ---------------------------------------------------------------- main


def analyse(pdf_path: str, html_path: str | None) -> dict:
    doc = pdfium.PdfDocument(pdf_path)
    pages_out = []
    roles: Counter[str] = Counter()
    tree_words: list[str] = []
    totals = Counter()
    lang_root = None
    alts = []
    titled_headings = 0
    for pno in range(len(doc)):
        page = doc[pno]
        tp = page.get_textpage()
        objs = list(_page_objects(page, tp))
        mcid_text: dict[int, str] = {}
        carried = set()
        untagged = Counter()
        untagged_text = []
        artifacts = 0
        for kind, mcid, art, text in objs:
            if art:
                artifacts += 1
                continue
            if mcid >= 0:
                carried.add(mcid)
                if text:
                    # Separate text objects: a sequence often spans several
                    # lines, and line ends carry no space character.
                    prev = mcid_text.get(mcid, "")
                    mcid_text[mcid] = f"{prev} {text}" if prev else text
            else:
                untagged[kind] += 1
                if text.strip():
                    untagged_text.append(text)
        tree = R.FPDF_StructTree_GetForPage(page.raw)
        nodes: list[dict] = []
        if tree:
            for i in range(R.FPDF_StructTree_CountChildren(tree)):
                el = R.FPDF_StructTree_GetChildAtIndex(tree, i)
                if el:
                    _walk_elem(el, 0, pno + 1, mcid_text, nodes)
            R.FPDF_StructTree_Close(tree)
        referenced = Counter(m for n in nodes for m in n["mcids"])
        dup = sorted(m for m, c in referenced.items() if c > 1)
        missing_in_tree = sorted(carried - set(referenced))
        dangling = sorted(set(referenced) - carried)
        for n in nodes:
            roles[n["type"]] += 1
            if n["lang"] and lang_root is None:
                lang_root = n["lang"]
            if n["alt"]:
                alts.append(n["alt"])
            if re.fullmatch(r"H[1-6]", n["type"]) and n["title"]:
                titled_headings += 1
            tree_words.extend(_words(n["text"]))
        totals["objects"] += len(objs)
        totals["artifacts"] += artifacts
        totals["untagged"] += sum(untagged.values())
        totals["mcids_not_in_tree"] += len(missing_in_tree)
        totals["mcids_dangling"] += len(dangling)
        totals["mcids_duplicated"] += len(dup)
        pages_out.append({
            "page": pno + 1,
            "objects": len(objs),
            "artifacts": artifacts,
            "untagged": dict(untagged),
            "untagged_text": untagged_text[:5],
            "mcids_not_in_tree": missing_in_tree[:10],
            "mcids_dangling": dangling[:10],
            "mcids_duplicated": dup[:10],
            "nodes": nodes,
        })
    out = {
        "pdf": pdf_path,
        "pages": len(doc),
        "tagged": any(p["nodes"] for p in pages_out),
        "totals": dict(totals),
        "roles": dict(sorted(roles.items())),
        "lang": lang_root,
        "alt_texts": alts,
        "titled_headings": titled_headings,
        "page_detail": pages_out,
    }
    if html_path:
        src = _Source()
        with open(html_path, encoding="utf-8", errors="replace") as fh:
            src.feed(fh.read())
        sm = difflib.SequenceMatcher(None, src.words, tree_words, autojunk=False)
        out["source"] = {
            "html": html_path,
            "lang": src.lang,
            "title": src.title.strip(),
            "expected": dict(sorted(src.counts.items())),
            "words": len(src.words),
        }
        out["order"] = {
            "tree_words": len(tree_words),
            "similarity": round(sm.ratio(), 4),
            # Fraction of source words found, in order, in the tree reading.
            "source_coverage": round(
                sum(b.size for b in sm.get_matching_blocks()) / max(len(src.words), 1), 4),
        }
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("pdf")
    ap.add_argument("--html", help="source HTML for expected counts + reading order")
    ap.add_argument("--tree", action="store_true", help="print the per-page tree")
    ap.add_argument("--json", action="store_true", help="emit the full JSON report")
    a = ap.parse_args(argv)
    try:
        rep = analyse(a.pdf, a.html)
    except Exception as e:  # noqa: BLE001 - report and exit 2
        print(f"error: {e}", file=sys.stderr)
        return 2
    if a.json:
        if not a.tree:
            for p in rep["page_detail"]:
                p.pop("nodes")
        json.dump(rep, sys.stdout, indent=2, ensure_ascii=False)
        print()
        return 0
    print(f"{rep['pdf']}: pages={rep['pages']} tagged={rep['tagged']} lang={rep['lang']}")
    print("totals:", rep["totals"])
    print("roles:", rep["roles"])
    if "source" in rep:
        print("expected (HTML):", rep["source"]["expected"])
        print("order:", rep["order"])
    if a.tree:
        for p in rep["page_detail"]:
            print(f"--- page {p['page']}")
            for n in p["nodes"]:
                extra = "".join(
                    f" {k}={n[k]!r}" for k in ("title", "alt", "actual", "lang") if n[k])
                txt = n["text"][:60].replace("\n", " ")
                print(f"{'  ' * n['depth']}{n['type']}{extra}  {txt!r}" if txt else
                      f"{'  ' * n['depth']}{n['type']}{extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
