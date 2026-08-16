//! Trivial block layout.
//!
//! Block-level elements are stacked vertically inside the page content box.
//! Each block's inline text is greedily broken into lines at the available
//! width (first-fit; Knuth-Plass is a later issue). When the next line would
//! overflow the current page, a new page begins — the seed of the
//! fragmentation model, kept deliberately trivial.
//!
//! Text width is approximated (0.5 em per character) rather than shaped;
//! real shaping via HarfRust is out of scope for the skeleton.

use crate::css::{cascade, Color, ComputedStyle, Display, Stylesheet};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::geom::{PageGeometry, Point, Rect, Scalar};

/// Line-height multiple applied to font-size.
const LINE_HEIGHT_FACTOR: f64 = 1.2;
/// Approximate average glyph advance as a fraction of the em (font-size).
const AVG_ADVANCE_EM: f64 = 0.5;

/// A single laid-out line of text, positioned absolutely (points, top-left).
#[derive(Clone, Debug)]
pub struct TextLine {
    pub text: String,
    pub origin: Point,
    pub font_size: Scalar,
    pub color: Color,
    /// Resolved font family (unused by the skeleton PDF backend, which embeds a
    /// single font; carried for the shaping stage).
    #[allow(dead_code)]
    pub font_family: String,
}

/// A laid-out block box background (only emitted when it has a background).
#[derive(Clone, Debug)]
pub struct BlockRect {
    pub rect: Rect,
    pub color: Color,
}

/// One output page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub rects: Vec<BlockRect>,
    pub lines: Vec<TextLine>,
}

/// The full paginated layout.
#[derive(Clone, Debug)]
pub struct Layout {
    pub geometry: PageGeometry,
    pub pages: Vec<Page>,
}

/// Layout state threaded through block stacking.
struct Cursor<'a> {
    dom: &'a Dom,
    styles: &'a [ComputedStyle],
    content: Rect,
    pages: Vec<Page>,
    /// Current y within the content box (points, top-down).
    y: Scalar,
}

impl<'a> Cursor<'a> {
    fn new(dom: &'a Dom, styles: &'a [ComputedStyle], geometry: PageGeometry) -> Self {
        let content = geometry.content_rect();
        Cursor {
            dom,
            styles,
            content,
            pages: vec![Page::default()],
            y: content.y,
        }
    }

    fn cur_page(&mut self) -> &mut Page {
        self.pages.last_mut().expect("always one page")
    }

    /// Bottom edge of the content box.
    fn content_bottom(&self) -> Scalar {
        self.content.y + self.content.height
    }

    /// Start a fresh page and reset the vertical cursor.
    fn new_page(&mut self) {
        self.pages.push(Page::default());
        self.y = self.content.y;
    }

    /// Lay out one block element subtree.
    fn layout_block(&mut self, id: NodeId) {
        let style = self.styles[id].clone();
        if style.display == Display::None {
            return;
        }

        // Top margin + padding advance the cursor.
        self.y += style.margin_top;
        let box_top = self.y;
        let box_left = self.content.x + style.margin_left + style.padding_left;
        let inner_width = self.content.width
            - style.margin_left
            - style.margin_right
            - style.padding_left
            - style.padding_right;
        self.y += style.padding_top;

        // Gather this block's own inline text (direct text descendants that are
        // not inside a nested block). For the skeleton we treat any text in the
        // subtree that is not under a child block as this block's content.
        let text = self.collect_inline_text(id);
        if !text.trim().is_empty() {
            self.layout_text(&text, box_left, inner_width, &style);
        }

        // Recurse into child block elements.
        for &child in &self.dom.nodes[id].children {
            if let NodeKind::Element(_) = &self.dom.nodes[child].kind {
                if self.styles[child].display == Display::Block {
                    self.layout_block(child);
                }
            }
        }

        self.y += style.padding_bottom;

        // Emit background rect if present (covering top→current y).
        if let Some(bg) = style.background_color {
            let height = self.y - box_top;
            if height.get() > 0.0 {
                let rect = Rect::new(
                    self.content.x + style.margin_left,
                    box_top,
                    self.content.width - style.margin_left - style.margin_right,
                    height,
                );
                self.cur_page().rects.push(BlockRect { rect, color: bg });
            }
        }

        self.y += style.margin_bottom;
    }

    /// Collect text that belongs directly to this block (stops at nested
    /// block-level elements, whose text is laid out when we recurse).
    fn collect_inline_text(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_inline_rec(id, &mut out);
        out
    }

    fn collect_inline_rec(&self, id: NodeId, out: &mut String) {
        for &child in &self.dom.nodes[id].children {
            match &self.dom.nodes[child].kind {
                NodeKind::Text(t) => out.push_str(t),
                NodeKind::Element(_) => {
                    if self.styles[child].display == Display::Block {
                        continue; // handled by recursion in layout_block
                    }
                    self.collect_inline_rec(child, out);
                }
                NodeKind::Root => {}
            }
        }
    }

    /// Greedy first-fit line breaking. Emits `TextLine`s, paginating on overflow.
    fn layout_text(&mut self, text: &str, left: Scalar, max_width: Scalar, style: &ComputedStyle) {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return;
        }
        let line_height = style.font_size * LINE_HEIGHT_FACTOR;
        let advance = style.font_size.get() * AVG_ADVANCE_EM;

        let mut line = String::new();
        let mut line_w = 0.0f64;
        let space_w = advance; // one avg-advance per space

        for word in words {
            let word_w = word.chars().count() as f64 * advance;
            let added = if line.is_empty() {
                word_w
            } else {
                line_w + space_w + word_w
            };
            if !line.is_empty() && added > max_width.get() {
                self.emit_line(&line, left, line_height, style);
                line.clear();
                line_w = 0.0;
            }
            if line.is_empty() {
                line.push_str(word);
                line_w = word_w;
            } else {
                line.push(' ');
                line.push_str(word);
                line_w += space_w + word_w;
            }
        }
        if !line.is_empty() {
            self.emit_line(&line, left, line_height, style);
        }
    }

    fn emit_line(&mut self, text: &str, left: Scalar, line_height: Scalar, style: &ComputedStyle) {
        // Paginate if this line would overflow the content box.
        if self.y + line_height > self.content_bottom() && self.has_content_on_page() {
            self.new_page();
        }
        // Baseline sits near the bottom of the line box (approximate).
        let baseline = self.y + style.font_size;
        let line = TextLine {
            text: text.to_string(),
            origin: Point::new(left, baseline),
            font_size: style.font_size,
            color: style.color,
            font_family: style.font_family.clone(),
        };
        self.cur_page().lines.push(line);
        self.y += line_height;
    }

    fn has_content_on_page(&self) -> bool {
        self.pages
            .last()
            .map(|p| !p.lines.is_empty() || !p.rects.is_empty())
            .unwrap_or(false)
    }
}

/// Run the full pipeline stage: cascade already done, produce paginated layout.
pub fn layout(dom: &Dom, stylesheet: &Stylesheet, geometry: PageGeometry) -> Layout {
    let styles = cascade(dom, stylesheet);
    let mut cursor = Cursor::new(dom, &styles, geometry);

    // Find the <body>; fall back to the root if absent.
    let start = dom.find_tag("body").unwrap_or(dom.root);

    // Lay out block children of body (and any block body itself contains).
    match &dom.nodes[start].kind {
        NodeKind::Element(_) => cursor.layout_block(start),
        _ => {
            let children = dom.nodes[start].children.clone();
            for child in children {
                if let NodeKind::Element(_) = &dom.nodes[child].kind {
                    cursor.layout_block(child);
                }
            }
        }
    }

    Layout {
        geometry,
        pages: cursor.pages,
    }
}
