// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-513: Markdown input. Maps to the acceptance criteria of
//! `docs/specifications/markdown-input.spec.md`.

use std::path::PathBuf;
use std::process::{Command, Output};

use typeanvil::markdown::convert;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ta-md-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn render(dir: &PathBuf, file: &str, extra: &[&str]) -> Output {
    let mut args = vec![
        "render".to_string(),
        dir.join(file).to_string_lossy().into_owned(),
        "--page-width".into(),
        "5in".into(),
        "--page-height".into(),
        "3in".into(),
        "--margin-top".into(),
        "0.5in".into(),
        "--margin-bottom".into(),
        "0.5in".into(),
        "--margin-left".into(),
        "0.5in".into(),
        "--margin-right".into(),
        "0.5in".into(),
        "-o".into(),
        dir.join("out.pdf").to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .args(&args)
        .output()
        .expect("spawn typeanvil")
}

const BASIC: &str = "# Title Heading\n\nA paragraph with *emphasis*.\n\n## Second Heading\n\n- one\n- two\n";

/// AC1, AC13: `.md` renders; two renders are byte-identical.
#[test]
fn md_extension_renders_and_is_deterministic() {
    let d = scratch("basic");
    std::fs::write(d.join("basic.md"), BASIC).unwrap();
    let out = render(&d, "basic.md", &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let a = std::fs::read(d.join("out.pdf")).unwrap();
    assert!(a.starts_with(b"%PDF"));
    render(&d, "basic.md", &[]);
    let b = std::fs::read(d.join("out.pdf")).unwrap();
    assert_eq!(a, b, "Markdown render must be byte-identical");
}

/// AC2: `--from` overrides the extension; an unknown value exits 2.
#[test]
fn from_flag_overrides_extension() {
    let d = scratch("from");
    std::fs::write(d.join("doc.txt"), "# Hi\n").unwrap();
    assert!(render(&d, "doc.txt", &["--from", "markdown", "--no-default-css"])
        .status
        .success());
    std::fs::write(d.join("doc.md"), "<p>x</p>").unwrap();
    assert!(render(&d, "doc.md", &["--from", "html"]).status.success());
    let bad = render(&d, "doc.md", &["--from", "rst"]);
    assert_eq!(bad.status.code(), Some(2));
}

/// AC3, AC5: frontmatter maps to metadata; no invented title.
#[test]
fn frontmatter_maps_to_html_metadata() {
    let src = "---\ntitle: My Title\nauthor:\n  - Ann\n  - Bob\ndescription: A summary\nkeywords: [a, b]\nlang: fr\n---\n# Heading\n";
    let doc = convert(src, &[], true).unwrap();
    assert!(doc.html.contains("<title>My Title</title>"));
    assert!(doc.html.contains("<meta name=\"author\" content=\"Ann, Bob\">"));
    assert!(doc.html.contains("<meta name=\"description\" content=\"A summary\">"));
    assert!(doc.html.contains("<meta name=\"keywords\" content=\"a, b\">"));
    assert!(doc.html.contains("<html lang=\"fr\">"));
    assert!(!doc.html.contains("---"));
    let none = convert("# Only heading\n", &[], true).unwrap();
    assert!(!none.html.contains("<title>"));
}

/// AC3: PDF Info carries the frontmatter; `--title` overrides.
#[test]
fn frontmatter_reaches_pdf_info() {
    let d = scratch("info");
    std::fs::write(
        d.join("f.md"),
        "---\ntitle: FrontTitle\nauthor: Zed\n---\nBody\n",
    )
    .unwrap();
    assert!(render(&d, "f.md", &[]).status.success());
    let pdf = std::fs::read(d.join("out.pdf")).unwrap();
    let s = String::from_utf8_lossy(&pdf);
    assert!(s.contains("FrontTitle"), "title missing from PDF");
    assert!(render(&d, "f.md", &["--title", "Over"]).status.success());
    let s = String::from_utf8_lossy(&std::fs::read(d.join("out.pdf")).unwrap()).into_owned();
    assert!(s.contains("Over") && !s.contains("FrontTitle"));
}

/// AC4: malformed frontmatter exits 1, names the line, writes no PDF.
#[test]
fn bad_frontmatter_exits_one_naming_line() {
    let d = scratch("bad");
    std::fs::write(d.join("b.md"), "---\ntitle: ok\nnot a mapping line\n---\nBody\n").unwrap();
    let out = render(&d, "b.md", &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("line 3"), "stderr must name the line: {err}");
    assert!(!d.join("out.pdf").exists());
}

/// AC6, AC7, AC8 (generated HTML): table, del, code fence class, raw HTML.
#[test]
fn gfm_and_raw_html_in_generated_html() {
    let src = "| a | b |\n|---|--:|\n| 1 | 2 |\n\n~~gone~~\n\n```rust\nfn main() {}\n```\n\n<div class=\"x\">raw</div>\n";
    let h = convert(src, &[], true).unwrap().html;
    assert!(h.contains("<table>") && h.contains("text-align:right"), "{h}");
    assert!(h.contains("<del>gone</del>"));
    assert!(h.contains("class=\"language-rust\""));
    assert!(h.contains("<div class=\"x\">raw</div>"));
}

/// AC11, AC12: default sheet first, no `@page`, removable; `--css` after it.
#[test]
fn default_sheet_placement_and_flags() {
    let with = convert("x\n", &["h1{color:red}".to_string()], true).unwrap().html;
    let d = with.find("<style>").unwrap();
    let a = with.find("h1{color:red}").unwrap();
    assert!(d < a, "default sheet must come first");
    assert!(!typeanvil::markdown::DEFAULT_CSS.contains("@page"));
    let without = convert("x\n", &[], false).unwrap().html;
    assert!(!without.contains("<style>"));
}

/// AC11/AC12 CLI: `--css` with `@page` changes the page size.
#[test]
fn css_flag_sets_page_size() {
    let d = scratch("css");
    std::fs::write(d.join("a.md"), "# Hi\n").unwrap();
    std::fs::write(d.join("x.css"), "@page { size: 4in 2in }").unwrap();
    let css = d.join("x.css");
    let out = render(&d, "a.md", &["--css", css.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pdf = String::from_utf8_lossy(&std::fs::read(d.join("out.pdf")).unwrap()).into_owned();
    assert!(pdf.contains("/MediaBox [0 0 288 144]") || pdf.contains("288 144"), "page size");
}

/// AC14: unknown frontmatter key yields one `source: "markdown"` event.
#[test]
fn unknown_key_diagnostic() {
    let d = scratch("diag");
    std::fs::write(d.join("k.md"), "---\ntitle: T\nmood: odd\n---\nBody\n").unwrap();
    let out = render(&d, "k.md", &["--diagnostics", "json"]);
    assert!(out.status.success());
    let json = String::from_utf8_lossy(&out.stdout);
    assert_eq!(json.matches("\"source\": \"markdown\"").count(), 1, "{json}");
    assert!(json.contains("mood"));
}

/// AC15 / B14: help lists the flags; `--css` with HTML input exits 2.
#[test]
fn help_and_html_rejects_css() {
    let help = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .arg("--help")
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&help.stdout);
    for n in ["--from", "--css", "--no-default-css", "Markdown"] {
        assert!(t.contains(n), "help must mention {n}");
    }
    let d = scratch("rej");
    std::fs::write(d.join("x.html"), "<p>x</p>").unwrap();
    std::fs::write(d.join("a.css"), "p{}").unwrap();
    let a = d.join("a.css");
    assert_eq!(render(&d, "x.html", &["--css", a.to_str().unwrap()]).status.code(), Some(2));
    assert_eq!(render(&d, "x.html", &["--no-default-css"]).status.code(), Some(2));
}

const FOOTNOTE: &str = "A[^1] and B[^two] and C[^nope].\n\n[^1]: Alpha *note*.\n[^two]: Beta note.\n";

/// AC10 (B9): references become inline float spans, no trailing list, and an
/// undefined reference stays literal text with a diagnostic.
#[test]
fn footnote_refs_become_float_spans() {
    let h = convert(FOOTNOTE, &[], true).unwrap();
    assert_eq!(h.html.matches("float: footnote").count(), 2, "{}", h.html);
    assert!(h.html.contains("Alpha <em>note</em>."), "{}", h.html);
    assert!(!h.html.contains("class=\"footnotes\""), "{}", h.html);
    assert!(!h.html.contains("footnote-ref"), "{}", h.html);
    assert!(h.html.contains("C[^nope]."), "{}", h.html);
    let d: Vec<_> = h.diagnostics.iter().filter(|d| d.code == "markdown-unresolved-footnote").collect();
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("nope"));
}

/// AC10 end to end: two footnote areas on page 1, text in the PDF.
#[test]
fn footnote_markdown_renders_notes() {
    let d = scratch("footnote");
    std::fs::write(d.join("footnote.md"), FOOTNOTE).unwrap();
    let out = render(&d, "footnote.md", &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pdf = std::fs::read(d.join("out.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

/// Gap 6: footnote spans inside a table cell and a list item convert.
#[test]
fn footnote_inside_table_and_list_converts() {
    let src = "| a | b |\n|---|---|\n| x[^1] | y |\n\n- item[^2]\n\n[^1]: Cell note.\n[^2]: List note.\n";
    let h = convert(src, &[], true).unwrap();
    assert_eq!(h.html.matches("float: footnote").count(), 2, "{}", h.html);
}

/// Gap 1/2 and B10: task items, long code lines, math fallback.
#[test]
fn task_list_code_wrap_and_math_fallback() {
    let h = convert("- [x] done\n- [ ] todo\n", &[], true).unwrap();
    assert!(h.html.contains("☑") && h.html.contains("☐"), "{}", h.html);
    assert!(typeanvil::markdown::DEFAULT_CSS.contains("pre-wrap"));
    // Translatable math becomes <math>, with no diagnostic.
    let m = convert("Inline $a+b$ here.\n", &[], true).unwrap();
    assert!(m.html.contains("<math") && !m.html.contains("<code class=\"math\""), "{}", m.html);
    assert!(m.diagnostics.is_empty());
    // Untranslatable math falls back to <code> source plus one diagnostic.
    let f = convert("Bad $\\notacommand{x}$ here.\n", &[], true).unwrap();
    assert!(f.html.contains("<code class=\"math\">"), "{}", f.html);
    assert_eq!(f.diagnostics.len(), 1);
    assert_eq!(f.diagnostics[0].code, "markdown-math-untranslated");
}

/// AC16 / B10: the subset translates to the MathML element set.
#[test]
fn tex_math_translates_to_mathml() {
    use typeanvil::texmath::tex_to_mathml;
    let m = tex_to_mathml("x^2 + \\frac{a}{b} + \\sqrt{y} + \\alpha_1", false).unwrap();
    for needle in ["<msup>", "<mfrac>", "<msqrt>", "<msub>", "<mi>α</mi>"] {
        assert!(m.contains(needle), "{needle} in {m}");
    }
    let d = tex_to_mathml("\\sum_{i=1}^{n} i", true).unwrap();
    assert!(d.contains("display=\"block\"") && d.contains("<munderover>"), "{d}");
    let s = tex_to_mathml("\\sum_{i=1}^{n} i", false).unwrap();
    assert!(s.contains("<msubsup>"), "{s}");
    let r = tex_to_mathml("\\sqrt[3]{x}", false).unwrap();
    assert!(r.contains("<mroot>"), "{r}");
    let t = tex_to_mathml("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}", false).unwrap();
    assert!(t.contains("<mtable>") && t.matches("<mtr>").count() == 2, "{t}");
    let l = tex_to_mathml("\\left( x \\right)", false).unwrap();
    assert!(l.contains("<mo>(</mo>") && l.contains("<mo>)</mo>"), "{l}");
    assert!(tex_to_mathml("\\frac{a}", false).is_err());
    assert!(tex_to_mathml("{a", false).is_err());
    assert!(tex_to_mathml("a}", false).is_err());
    assert!(tex_to_mathml("\\foo", false).is_err());
    // `<` in TeX is escaped in the output.
    assert!(tex_to_mathml("a<b", false).unwrap().contains("&lt;"));
}

/// AC16: `math.md` renders through the engine's MathML layout, byte-stable.
#[test]
fn math_md_renders_deterministically() {
    let d = scratch("math");
    let src = "# Math\n\nInline $E = mc^2$ and $\\frac{a}{b}$.\n\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\n\nPrice $5 and $6 stay text. Bad $\\nope$ here.\n";
    std::fs::write(d.join("math.md"), src).unwrap();
    let out = render(&d, "math.md", &["--diagnostics", "json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json = String::from_utf8_lossy(&out.stdout);
    assert_eq!(json.matches("markdown-math-untranslated").count(), 1, "{json}");
    let a = std::fs::read(d.join("out.pdf")).unwrap();
    render(&d, "math.md", &[]);
    let b = std::fs::read(d.join("out.pdf")).unwrap();
    assert_eq!(a, b);
    let h = convert(src, &[], true).unwrap().html;
    assert_eq!(h.matches("<math").count(), 3, "{h}");
    assert!(h.contains("display=\"block\""));
    assert!(h.contains("$5 and $6"), "currency stays text: {h}");
}
