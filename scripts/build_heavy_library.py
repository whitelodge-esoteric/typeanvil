#!/usr/bin/env python3
"""Generate the four heavy library documents (CORE-491).

Inputs are downloaded source files (see library/LICENSES.md for URLs):

  --arxiv   arXiv HTML of 2302.13971 (LLaMA, CC BY 4.0)
  --pg      Project Gutenberg #1342 plain text (Pride and Prejudice)
  --fm      archive.org djvu text of FM 21-76 (US Army, public domain)

The dissertation is generated from a fixed seed, no download needed.
Output goes to library/<category>/. Output is deterministic.

    python3 -I scripts/build_heavy_library.py --arxiv A --pg B --fm C
"""
from __future__ import annotations

import argparse
import html
import random
import re
import sys
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "library"
esc = html.escape


# --------------------------------------------------------------------------
# Minimal DOM (stdlib html.parser)
# --------------------------------------------------------------------------
VOID = {"br", "img", "hr", "meta", "link", "input", "wbr", "source"}


class Node:
    def __init__(self, tag, attrs, parent=None):
        self.tag, self.attrs, self.parent, self.kids = tag, dict(attrs), parent, []

    @property
    def cls(self):
        return set((self.attrs.get("class") or "").split())


class Builder(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = Node("#root", {})
        self.cur = self.root

    def handle_starttag(self, tag, attrs):
        n = Node(tag, attrs, self.cur)
        self.cur.kids.append(n)
        if tag not in VOID:
            self.cur = n

    def handle_startendtag(self, tag, attrs):
        self.cur.kids.append(Node(tag, attrs, self.cur))

    def handle_endtag(self, tag):
        n = self.cur
        while n is not self.root and n.tag != tag:
            n = n.parent
        if n is not self.root:
            self.cur = n.parent

    def handle_data(self, data):
        self.cur.kids.append(data)


def parse(text):
    b = Builder()
    b.feed(text)
    return b.root


def find(node, pred):
    if isinstance(node, str):
        return None
    if pred(node):
        return node
    for k in node.kids:
        r = find(k, pred)
        if r:
            return r
    return None


def text_of(node):
    if isinstance(node, str):
        return node
    if node.tag in ("annotation", "annotation-xml", "script", "style"):
        return ""
    return "".join(text_of(k) for k in node.kids)


ENTRY_CSS: list[str] = []


def entry(cls, href, label):
    """TOC anchor whose text and page number come from generated `content`."""
    n = len(ENTRY_CSS)
    lab = label.replace("\\", "\\\\").replace('"', '\\"')
    ENTRY_CSS.append('  .e%d { content: "%s " leader(\'.\') target-counter(attr(href), page); }' % (n, lab))
    return '<a class="%s e%d" href="%s"></a>' % (cls, n, href)


# --------------------------------------------------------------------------
# 1. Journal article (arXiv LaTeXML HTML -> clean HTML)
# --------------------------------------------------------------------------
SKIP_CLS = {"ltx_authors", "ltx_dates", "ltx_toc", "ltx_tocentry", "ltx_page_logo",
            "ltx_role_footnotemark", "ltx_listing", "ltx_listingline", "ltx_framed",
            "ltx_figure_panel", "ltx_pagination", "ltx_rdf", "ltx_bibtag"}
SKIP_TAGS = {"script", "style", "nav", "header", "footer", "button", "svg", "img",
             "annotation", "annotation-xml", "noscript", "form", "head", "title"}
MATH_KEEP = {"math", "mi", "mo", "mn", "mrow", "msub", "msup", "msubsup", "mfrac",
             "msqrt", "mroot", "mtext", "mover", "munder", "munderover", "mspace",
             "mtable", "mtr", "mtd", "mstyle"}
KEEP = {"p": "p", "h1": "h2", "h2": "h2", "h3": "h3", "h4": "h4", "h5": "h4",
        "h6": "h4", "em": "em", "i": "em", "b": "strong", "strong": "strong",
        "sup": "sup", "sub": "sub", "ul": "ul", "ol": "ol", "li": "li",
        "table": "table", "thead": "thead", "tbody": "tbody", "tr": "tr",
        "td": "td", "th": "th", "caption": "caption", "blockquote": "blockquote",
        "code": "code", "br": "br"}


class Journal:
    def __init__(self):
        self.out = []
        self.nfoot = 0

    def emit(self, n):
        if isinstance(n, str):
            t = re.sub(r"\s+", " ", n)
            self.out.append(esc(t, quote=False))
            return
        if n.tag in SKIP_TAGS or (n.cls & SKIP_CLS):
            return
        if "ltx_note" in n.cls:
            body = find(n, lambda x: not isinstance(x, str) and "ltx_note_content" in x.cls)
            txt = text_of(body) if body else ""
            txt = re.sub(r"^\W*\d*\W*(footnote|footnotemark)?:?\s*", "", re.sub(r"\s+", " ", txt)).strip()
            if txt:
                self.out.append('<span class="fn">%s</span>' % esc(txt, quote=False))
            return
        if n.tag == "math":
            self.out.append("<math>")
            for k in n.kids:
                self.math(k)
            self.out.append("</math>")
            return
        if n.tag == "table" and "ltx_equation" in n.cls:
            for k in n.kids:
                self.emit(k)
            return
        tag = KEEP.get(n.tag)
        if "ltx_bibitem" in n.cls:
            tag = "li"
        if tag == "br":
            self.out.append("<br>")
            return
        if tag in ("td", "th"):
            self.out.append("<%s>" % tag)
        elif tag:
            self.out.append("<%s>" % tag)
        for k in n.kids:
            self.emit(k)
        if tag:
            self.out.append("</%s>" % tag)

    def math(self, n):
        if isinstance(n, str):
            self.out.append(esc(n.strip(), quote=False))
            return
        if n.tag in ("semantics",):
            for k in n.kids:
                if not isinstance(k, str):
                    self.math(k)
                    return
            return
        if n.tag not in MATH_KEEP:
            return
        self.out.append("<%s>" % n.tag)
        for k in n.kids:
            self.math(k)
        self.out.append("</%s>" % n.tag)


def build_journal(src: Path):
    root = parse(src.read_text(encoding="utf-8"))
    article = find(root, lambda x: not isinstance(x, str) and "ltx_document" in x.cls)
    abstract = find(article, lambda x: not isinstance(x, str) and "ltx_abstract" in x.cls)
    j = Journal()
    for k in article.kids:
        if isinstance(k, str):
            continue
        if "ltx_abstract" in k.cls:
            continue
        j.emit(k)
    body = "".join(j.out)
    ab = Journal()
    if abstract:
        for k in abstract.kids:
            ab.emit(k)
    abstract_html = re.sub(r"^<h\d>.*?</h\d>", "", "".join(ab.out))
    # Bibliography: make every <li> inside the reference list compact.
    page = JOURNAL_TMPL.format(abstract=abstract_html, body=body)
    (LIB / "academic").mkdir(exist_ok=True)
    (LIB / "academic" / "journal-article.html").write_text(page, encoding="utf-8")


JOURNAL_TMPL = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>LLaMA: Open and Efficient Foundation Language Models</title>
<style>
  @page {{
    size: 8.5in 11in;
    margin: 0.75in 0.75in 0.85in 0.75in;
    @top-left {{ content: "LLaMA: Open and Efficient Foundation Language Models"; font-family: Arial, Helvetica, sans-serif; font-size: 8pt; color: #555; }}
    @top-right {{ content: "Touvron et al. 2023"; font-family: Arial, Helvetica, sans-serif; font-size: 8pt; color: #555; }}
    @bottom-center {{ content: counter(page); font-family: Arial, Helvetica, sans-serif; font-size: 9pt; }}
  }}
  body {{ font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 9pt; line-height: 1.3; color: #111; }}
  h1 {{ font-size: 18pt; text-align: center; margin: 0 0 6pt 0; }}
  .byline {{ text-align: center; font-size: 10pt; margin: 0 0 4pt 0; }}
  .src {{ text-align: center; font-size: 7.5pt; color: #555; margin: 0 0 10pt 0; }}
  .abstract {{ font-size: 9pt; margin: 0 36pt 10pt 36pt; }}
  .abstract .h {{ font-weight: bold; display: block; text-align: center; margin-bottom: 3pt; }}
  .cols {{ columns: 2; column-gap: 18pt; }}
  h2 {{ font-size: 11pt; margin: 10pt 0 4pt 0; }}
  h3 {{ font-size: 9.5pt; margin: 7pt 0 3pt 0; }}
  h4 {{ font-size: 9pt; font-style: italic; margin: 5pt 0 2pt 0; }}
  p {{ text-align: justify; hyphens: auto; margin: 0 0 5pt 0; }}
  .fn {{ float: footnote; font-size: 7.5pt; color: #333; }}
  table {{ border-collapse: collapse; font-size: 7.5pt; margin: 4pt 0 8pt 0; width: 100%; }}
  td, th {{ border-bottom: 0.4pt solid #bbb; padding: 1pt 3pt; text-align: left; }}
  caption {{ font-size: 8pt; text-align: left; margin-bottom: 2pt; }}
  li {{ font-size: 7.5pt; margin: 0 0 2pt 0; }}
  ul, ol {{ padding-left: 14pt; margin: 0 0 6pt 0; }}
</style>
</head>
<body>
  <h1>LLaMA: Open and Efficient Foundation Language Models</h1>
  <div class="byline">Hugo Touvron, Thibaut Lavril, Gautier Izacard, Xavier Martinet, et al. &mdash; Meta AI</div>
  <div class="src">arXiv:2302.13971 &middot; CC BY 4.0 &middot; converted from the arXiv HTML rendering</div>
  <div class="abstract"><span class="h">Abstract</span>{abstract}</div>
  <div class="cols">
{body}
  </div>
</body>
</html>
"""


# --------------------------------------------------------------------------
# 2. Dissertation (generated)
# --------------------------------------------------------------------------
WORDS = ("river basin sediment discharge hysteresis catchment gauge rating stage flow "
         "model parameter calibration uncertainty sampling estimate variance bias "
         "observation record season storm baseflow recession infiltration channel "
         "reach cross-section roughness velocity turbidity concentration load yield "
         "transport deposition erosion threshold regime pattern response signal noise "
         "series trend method result analysis measurement instrument sensor network "
         "station survey field laboratory sample dataset framework approach estimator "
         "regression kernel prior posterior likelihood residual error window lag").split()
CONN = ["Moreover,", "However,", "In contrast,", "As a result,", "In practice,",
        "For this reason,", "Notably,", "Consequently,", "By comparison,"]


def sentence(r):
    n = r.randint(9, 22)
    s = " ".join(r.choice(WORDS) for _ in range(n))
    if r.random() < 0.3:
        s = r.choice(CONN) + " " + s
    return s[0].upper() + s[1:] + "."


def para(r, lo=3, hi=7):
    return " ".join(sentence(r) for _ in range(r.randint(lo, hi)))


def build_dissertation():
    r = random.Random(491)
    chapters = [
        "Introduction", "Literature Review", "Study Area and Data", "Methods",
        "Rating Curves under Hysteresis", "Sediment Transport Estimation",
        "Uncertainty Quantification", "Regional Transferability",
        "Discussion", "Conclusions"]
    secs_per = 5
    body, toc, lof, lot = [], [], [], []
    fig_n = tab_n = 0
    for ci, title in enumerate(chapters, 1):
        cid = f"ch{ci}"
        toc.append(entry("t1", "#" + cid, f"{ci}  {title}"))
        body.append(f'<section class="chapter"><h1 id="{cid}"><span class="num">Chapter {ci}</span>{esc(title)}</h1>')
        for si in range(1, secs_per + 1):
            sid = f"s{ci}_{si}"
            stitle = " ".join(r.choice(WORDS) for _ in range(r.randint(2, 4))).capitalize()
            toc.append(entry("t2", "#" + sid, f"{ci}.{si}  {stitle}"))
            body.append(f'<h2 id="{sid}">{ci}.{si}&emsp;{esc(stitle)}</h2>')
            for _ in range(r.randint(8, 12)):
                body.append(f"<p>{esc(para(r))}</p>")
            if si % 2 == 1:
                fig_n += 1
                fid = f"fig{fig_n}"
                cap = " ".join(r.choice(WORDS) for _ in range(r.randint(5, 9))).capitalize()
                lof.append(entry("t2", "#" + fid, f"Figure {fig_n}  {cap}"))
                body.append(f'<figure id="{fid}"><div class="plot"></div>'
                            f'<figcaption>Figure {fig_n}. {esc(cap)}.</figcaption></figure>')
            if si % 2 == 0:
                tab_n += 1
                tid = f"tab{tab_n}"
                cap = " ".join(r.choice(WORDS) for _ in range(r.randint(5, 9))).capitalize()
                lot.append(entry("t2", "#" + tid, f"Table {tab_n}  {cap}"))
                rows = "".join(
                    "<tr><td>S%03d</td><td>%s</td><td>%.2f</td><td>%.3f</td><td>%d</td></tr>"
                    % (k, r.choice(WORDS), r.uniform(0, 100), r.uniform(0, 1), r.randint(10, 999))
                    for k in range(1, 46))
                body.append(f'<table id="{tid}"><caption>Table {tab_n}. {esc(cap)}.</caption>'
                            '<thead><tr><th>Site</th><th>Class</th><th>Mean</th><th>Ratio</th><th>n</th></tr></thead>'
                            f"<tbody>{rows}</tbody></table>")
        body.append("</section>")
    refs = "".join(
        f"<li>{esc(r.choice(['Okonkwo','Rennick','Haldane','Varga','Mbeki','Lindqvist','Tanaka']))}, "
        f"{esc(r.choice('ABCDEFGHJKLMNPRSTW'))}. ({r.randint(1971, 2025)}). "
        f"{esc(sentence(r)[:-1])}. <i>J. Appl. Hydrol.</i> {r.randint(1, 60)}, {r.randint(1, 900)}.</li>"
        for _ in range(180))
    page = DISS_TMPL.format(
        toc="\n".join(toc), lof="\n".join(lof), lot="\n".join(lot),
        body="\n".join(body), refs=refs, tocss="\n".join(ENTRY_CSS),
        abstract=esc(para(r, 6, 9)))
    ENTRY_CSS.clear()
    (LIB / "academic" / "dissertation.html").write_text(page, encoding="utf-8")


DISS_TMPL = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Stage&ndash;Discharge Hysteresis and Sediment Yield in Small Catchments</title>
<style>
  @page {{
    size: 210mm 297mm;
    margin: 25mm 22mm 25mm 25mm;
    @top-left {{ content: string(chapter); font-family: Georgia, serif; font-size: 9pt; color: #555; }}
    @bottom-center {{ content: counter(page); font-family: Georgia, serif; font-size: 10pt; }}
  }}
  body {{ font-family: Georgia, "Times New Roman", serif; font-size: 11pt; line-height: 1.45; color: #111; }}
  p {{ text-align: justify; hyphens: auto; margin: 0 0 8pt 0; orphans: 2; widows: 2; }}
  .titlepage {{ text-align: center; padding-top: 80mm; }}
  .titlepage h1 {{ font-size: 24pt; line-height: 1.2; margin: 0 0 24pt 0; }}
  .titlepage p {{ text-align: center; font-size: 12pt; }}
  .front {{ break-before: page; }}
  .front h1 {{ font-size: 18pt; margin: 0 0 14pt 0; string-set: chapter content(); }}
  .toc a {{ display: block; text-decoration: none; color: #111; }}
  .toc a.t1 {{ font-weight: bold; margin-top: 6pt; }}
{tocss}
  .toc a.t1, .toc a.t2 {{ font-size: 10.5pt; }}
  .toc a.t2 {{ margin-left: 16pt; }}
  .chapter {{ break-before: page; }}
  .chapter h1 {{ font-size: 20pt; margin: 40mm 0 18pt 0; string-set: chapter content(); }}
  .chapter h1 .num {{ display: block; font-size: 11pt; color: #777; margin-bottom: 4pt; }}
  h2 {{ font-size: 13pt; margin: 16pt 0 6pt 0; break-after: avoid; }}
  figure {{ margin: 10pt 0; break-inside: avoid; }}
  .plot {{ height: 150pt; border: 0.5pt solid #999; background-color: #f1f4f8; }}
  figcaption {{ font-size: 9.5pt; margin-top: 3pt; }}
  table {{ border-collapse: collapse; width: 100%; font-size: 9.5pt; margin: 10pt 0; }}
  caption {{ text-align: left; font-size: 9.5pt; margin-bottom: 3pt; }}
  th, td {{ border-bottom: 0.4pt solid #bbb; padding: 2pt 4pt; text-align: left; }}
  thead th {{ border-bottom: 0.8pt solid #333; }}
  .refs li {{ font-size: 9.5pt; margin-bottom: 3pt; }}
</style>
</head>
<body>
  <div class="titlepage">
    <h1>Stage&ndash;Discharge Hysteresis and Sediment Yield in Small Agricultural Catchments</h1>
    <p>A dissertation submitted for the degree of Doctor of Philosophy</p>
    <p>School of Geography, Ashfield University</p>
    <p>2026</p>
  </div>
  <section class="front"><h1>Abstract</h1><p>{abstract}</p></section>
  <section class="front toc"><h1>Contents</h1>
{toc}
  </section>
  <section class="front toc"><h1>List of Figures</h1>
{lof}
  </section>
  <section class="front toc"><h1>List of Tables</h1>
{lot}
  </section>
{body}
  <section class="chapter"><h1>References</h1><ol class="refs">{refs}</ol></section>
</body>
</html>
"""


# --------------------------------------------------------------------------
# 3. Book (Gutenberg #1342)
# --------------------------------------------------------------------------
def build_book(src: Path):
    raw = src.read_text(encoding="utf-8").replace("\r\n", "\n")
    a = raw.index("*** START OF THE PROJECT GUTENBERG EBOOK")
    b = raw.index("*** END OF THE PROJECT GUTENBERG EBOOK")
    text = raw[a:b].split("\n", 1)[1]
    parts = re.split(r"\n(CHAPTER [IVXLC]+\.?)\s*\n|\n(Chapter [IVXLC]+\.\]?)\s*\n", text)
    # re.split with two groups yields (None, str) pairs; normalise.
    chapters, i = [], 1
    head = parts[0]
    while i < len(parts) - 2:
        title = parts[i] or parts[i + 1]
        chapters.append((title, parts[i + 2]))
        i += 3
    toc, out = [], []
    for n, (title, body) in enumerate(chapters, 1):
        title = title.replace("]", "").replace(".", "").title().replace("Chapter", "Chapter")
        paras = [re.sub(r"\s+", " ", p).strip() for p in re.split(r"\n\s*\n", body)]
        paras = [p for p in paras if p and not re.match(r"^\[?Illustration", p)]
        out.append(f'<section class="chapter"><h1 id="c{n}">{esc(title)}</h1>')
        for p in paras:
            p = esc(p, quote=False)
            p = re.sub(r"_([^_]+)_", r"<em>\1</em>", p)
            out.append(f"<p>{p}</p>")
        out.append("</section>")
        toc.append(entry("t", f"#c{n}", title))
    page = BOOK_TMPL.format(toc="\n".join(toc), body="\n".join(out), tocss="\n".join(ENTRY_CSS))
    ENTRY_CSS.clear()
    (LIB / "book" / "pride-and-prejudice.html").write_text(page, encoding="utf-8")
    return len(chapters)


BOOK_TMPL = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Pride and Prejudice</title>
<style>
  @page {{
    size: 6in 9in;
    margin: 0.8in 0.7in 0.85in 0.7in;
    @top-center {{ content: string(running); font-family: Georgia, serif; font-size: 8.5pt; font-style: italic; color: #555; }}
    @bottom-center {{ content: counter(page); font-family: Georgia, serif; font-size: 9pt; }}
  }}
  body {{ font-family: Georgia, "Times New Roman", serif; font-size: 10.5pt; line-height: 1.4; color: #111; }}
  p {{ text-align: justify; hyphens: auto; margin: 0; text-indent: 1.4em; orphans: 2; widows: 2; }}
  .title {{ text-align: center; padding-top: 2.2in; }}
  .title h1 {{ font-size: 28pt; margin: 0 0 8pt 0; string-set: running content(); }}
  .title p {{ text-align: center; text-indent: 0; margin: 4pt 0; }}
  .toc {{ break-before: page; }}
  .toc h2 {{ font-size: 14pt; margin: 0 0 10pt 0; }}
  .toc a {{ display: block; text-decoration: none; color: #111; font-size: 9.5pt; }}
{tocss}
  .chapter {{ break-before: page; }}
  .chapter h1 {{ font-size: 15pt; text-align: center; margin: 0.9in 0 18pt 0; string-set: running "Pride and Prejudice"; }}
  .chapter p:first-of-type {{ text-indent: 0; }}
</style>
</head>
<body>
  <div class="title"><h1>Pride and Prejudice</h1><p>Jane Austen</p><p>Project Gutenberg eBook #1342 &middot; public domain</p></div>
  <section class="toc"><h2>Contents</h2>
{toc}
  </section>
{body}
</body>
</html>
"""


# --------------------------------------------------------------------------
# 4. Field manual (FM 21-76)
# --------------------------------------------------------------------------
NOISE = re.compile(r"^(Page \d+ of \d+|FM ?21-76 US ARMY SURVIVAL MANUAL.*|Reprinted as permitted.*)$")


def build_manual(src: Path):
    lines = [l.rstrip() for l in src.read_text(encoding="utf-8", errors="replace").splitlines()]
    start = next(i for i, l in enumerate(lines) if l.startswith("CHAPTER 1 - INTRODUCTION") and i > 200)
    lines = [l for l in lines[start:] if not NOISE.match(l.strip())]
    blocks = [re.sub(r"\s+", " ", b.replace("\n", " ")).strip()
              for b in re.split(r"\n\s*\n", "\n".join(lines))]
    blocks = [b for b in blocks if b]
    out, toc, idx = [], [], {}
    chap = 0
    sec_n = 0
    i = 0
    while i < len(blocks):
        b = blocks[i]
        m = re.match(r"^CHAPTER (\d+) - (.+)$", b)
        if m:
            chap, sec_n = int(m.group(1)), 0
            if out and out[-1] != "</section>":
                out.append("</section>")
            out.append(f'<section class="chapter"><h1 id="c{chap}"><span class="num">Chapter {chap}</span>{esc(m.group(2).title())}</h1>')
            toc.append(entry("t1", f"#c{chap}", f"Chapter {chap}  {m.group(2).title()}"))
            i += 1
            continue
        if not chap:
            i += 1
            continue
        if b in ("WARNING", "CAUTION") and i + 1 < len(blocks):
            out.append(f'<div class="box {b.lower()}"><div class="lab">{b}</div><p>{esc(blocks[i + 1])}</p></div>')
            i += 2
            continue
        m = re.match(r"^(WARNING|CAUTION|NOTE):? (.{20,})$", b)
        if m:
            out.append(f'<div class="box {m.group(1).lower()}"><div class="lab">{m.group(1)}</div><p>{esc(m.group(2))}</p></div>')
            i += 1
            continue
        m = re.match(r"^Figure (\d+-\d+)\.?\s*(.*)$", b)
        if m and len(b) < 120:
            out.append(f'<figure id="f{m.group(1)}"><div class="diagram"></div><figcaption>Figure {m.group(1)}. {esc(m.group(2))}</figcaption></figure>')
            i += 1
            continue
        if len(b) < 70 and b.isupper() and len(b.split()) <= 8 and not b.endswith("."):
            sec_n += 1
            sid = f"s{chap}_{sec_n}"
            out.append(f'<h2 id="{sid}">{chap}.{sec_n}&emsp;{esc(b.title())}</h2>')
            idx.setdefault(b.title(), sid)
            i += 1
            continue
        if len(b) < 60 and not re.search(r"[.,;:]$", b) and len(b.split()) <= 6 and b[:1].isupper() \
                and i + 1 < len(blocks) and len(blocks[i + 1]) > 80:
            out.append(f"<h3>{esc(b)}</h3>")
            i += 1
            continue
        out.append(f"<p>{esc(b)}</p>")
        i += 1
    out.append("</section>")
    index_html = "".join(
        "<li>%s</li>" % entry("ix", "#" + sid, t) for t, sid in sorted(idx.items(), key=lambda kv: kv[0].lower()))
    page = MANUAL_TMPL.format(toc="\n".join(toc), body="\n".join(out), index=index_html,
                              tocss="\n".join(ENTRY_CSS))
    ENTRY_CSS.clear()
    (LIB / "manual" / "field-manual.html").write_text(page, encoding="utf-8")
    return len(idx)


MANUAL_TMPL = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>FM 21-76 Survival</title>
<style>
  @page {{
    size: 8.5in 11in;
    margin: 1in 0.9in 1in 0.9in;
    @top-left {{ content: "FM 21-76"; font-family: Arial, Helvetica, sans-serif; font-size: 9pt; font-weight: bold; }}
    @top-right {{ content: string(chap); font-family: Arial, Helvetica, sans-serif; font-size: 9pt; color: #444; }}
    @bottom-center {{ content: counter(page); font-family: Arial, Helvetica, sans-serif; font-size: 10pt; }}
  }}
  body {{ font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.35; color: #111; }}
  p {{ text-align: justify; hyphens: auto; margin: 0 0 6pt 0; }}
  .cover {{ text-align: center; padding-top: 2.2in; }}
  .cover h1 {{ font-size: 30pt; margin: 0 0 10pt 0; string-set: chap "Survival"; }}
  .cover p {{ text-align: center; }}
  .toc {{ break-before: page; }}
  .toc h2 {{ font-size: 16pt; }}
  .toc a, .index a {{ display: block; text-decoration: none; color: #111; }}
{tocss}
  .chapter {{ break-before: page; }}
  .chapter h1 {{ font-size: 20pt; margin: 0 0 14pt 0; border-bottom: 2pt solid #333; padding-bottom: 4pt; string-set: chap content(); }}
  .chapter h1 .num {{ display: block; font-size: 10pt; color: #666; }}
  h2 {{ font-size: 12.5pt; margin: 14pt 0 5pt 0; break-after: avoid; }}
  h3 {{ font-size: 10.5pt; margin: 9pt 0 3pt 0; break-after: avoid; }}
  .box {{ border: 1.5pt solid #000; margin: 8pt 0; padding: 5pt 8pt; break-inside: avoid; }}
  .box .lab {{ font-weight: bold; font-size: 10.5pt; }}
  .warning {{ border-color: #b00020; background-color: #fdeaea; }}
  .caution {{ border-color: #c77700; background-color: #fff4e0; }}
  .note {{ border-color: #555; background-color: #f1f1f1; }}
  .box p {{ margin: 2pt 0 0 0; }}
  figure {{ margin: 8pt 0; break-inside: avoid; }}
  .diagram {{ height: 120pt; border: 0.5pt solid #888; background-color: #eef2ea; }}
  figcaption {{ font-size: 9pt; font-weight: bold; margin-top: 3pt; }}
  .appendix h1 {{ font-size: 18pt; }}
  .index ul {{ columns: 2; column-gap: 18pt; list-style: none; padding: 0; margin: 0; }}
  .index li {{ font-size: 9pt; }}
</style>
</head>
<body>
  <div class="cover"><h1>Survival</h1><p>Field Manual 21-76 &middot; Department of the Army</p><p>Public-domain US government work &middot; archive.org FM2176USARMYSURVIVALMANUAL</p></div>
  <section class="toc"><h2>Contents</h2>
{toc}
  </section>
{body}
  <section class="chapter appendix"><h1><span class="num">Appendix A</span>Source and Conversion Notes</h1>
    <p>Text converted from the archive.org OCR of FM 21-76. OCR errors in the source are preserved. Figures are shown as placeholder frames with their original captions.</p>
    <div class="box note"><div class="lab">NOTE</div><p>This appendix is added by the TypeAnvil library to give the manual an appendix structure.</p></div>
  </section>
  <section class="chapter index"><h1>Index</h1><ul>{index}</ul></section>
</body>
</html>
"""


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--arxiv", type=Path, required=True)
    ap.add_argument("--pg", type=Path, required=True)
    ap.add_argument("--fm", type=Path, required=True)
    a = ap.parse_args(argv)
    build_journal(a.arxiv)
    build_dissertation()
    nch = build_book(a.pg)
    nidx = build_manual(a.fm)
    print(f"book chapters={nch} manual index terms={nidx}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
