// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-515: TeX subset to MathML Core translator for Markdown input.
//!
//! Spec: `docs/specifications/markdown-input.spec.md` (Behavior 10, Slice 3
//! decisions). Hand-built, pure Rust, no dependencies, deterministic. Only the
//! Markdown converter calls it; the HTML path never does.
//!
//! The output uses the element set of `math-rendering.spec.md` Behavior 1.
//! Any unsupported command makes the whole formula fail with `Err(message)`
//! so the caller can fall back to source text plus a diagnostic.

/// Translate TeX math to a `<math>` element string.
///
/// `display` selects `display="block"` and display-style limits on big
/// operators. `tex` is the raw (unescaped) TeX source.
pub fn tex_to_mathml(tex: &str, display: bool) -> Result<String, String> {
    let mut p = Parser {
        c: tex.chars().collect(),
        i: 0,
        display,
        depth: 0,
    };
    let nodes = p.seq()?;
    if p.i < p.c.len() {
        return Err(format!("unexpected `{}`", p.c[p.i]));
    }
    let mut out = String::from("<math");
    if display {
        out.push_str(" display=\"block\"");
    }
    out.push_str(" alttext=\"");
    out.push_str(&esc(tex.trim()));
    out.push_str("\">");
    out.push_str(&nodes.concat());
    out.push_str("</math>");
    Ok(out)
}

const MAX_DEPTH: usize = 64;

struct Parser {
    c: Vec<char>,
    i: usize,
    display: bool,
    depth: usize,
}

/// One parsed atom: its MathML and whether scripts become limits.
struct Atom {
    ml: String,
    limits: bool,
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(ch),
        }
    }
    o
}

fn mi(s: &str) -> String {
    format!("<mi>{}</mi>", esc(s))
}
fn mo(s: &str) -> String {
    format!("<mo>{}</mo>", esc(s))
}
fn mn(s: &str) -> String {
    format!("<mn>{}</mn>", esc(s))
}
fn mtext(s: &str) -> String {
    format!("<mtext>{}</mtext>", esc(s))
}
fn mrow(inner: &str) -> String {
    format!("<mrow>{inner}</mrow>")
}

/// Greek letters and symbols that map to one token.
/// `(command, text, kind)`: kind `i` identifier, `o` operator.
const SYMBOLS: &[(&str, &str, char)] = &[
    ("alpha", "α", 'i'),
    ("beta", "β", 'i'),
    ("gamma", "γ", 'i'),
    ("delta", "δ", 'i'),
    ("epsilon", "ϵ", 'i'),
    ("varepsilon", "ε", 'i'),
    ("zeta", "ζ", 'i'),
    ("eta", "η", 'i'),
    ("theta", "θ", 'i'),
    ("vartheta", "ϑ", 'i'),
    ("iota", "ι", 'i'),
    ("kappa", "κ", 'i'),
    ("lambda", "λ", 'i'),
    ("mu", "μ", 'i'),
    ("nu", "ν", 'i'),
    ("xi", "ξ", 'i'),
    ("pi", "π", 'i'),
    ("varpi", "ϖ", 'i'),
    ("rho", "ρ", 'i'),
    ("varrho", "ϱ", 'i'),
    ("sigma", "σ", 'i'),
    ("varsigma", "ς", 'i'),
    ("tau", "τ", 'i'),
    ("upsilon", "υ", 'i'),
    ("phi", "ϕ", 'i'),
    ("varphi", "φ", 'i'),
    ("chi", "χ", 'i'),
    ("psi", "ψ", 'i'),
    ("omega", "ω", 'i'),
    ("Gamma", "Γ", 'i'),
    ("Delta", "Δ", 'i'),
    ("Theta", "Θ", 'i'),
    ("Lambda", "Λ", 'i'),
    ("Xi", "Ξ", 'i'),
    ("Pi", "Π", 'i'),
    ("Sigma", "Σ", 'i'),
    ("Upsilon", "Υ", 'i'),
    ("Phi", "Φ", 'i'),
    ("Psi", "Ψ", 'i'),
    ("Omega", "Ω", 'i'),
    ("infty", "∞", 'i'),
    ("partial", "∂", 'i'),
    ("nabla", "∇", 'i'),
    ("hbar", "ℏ", 'i'),
    ("ell", "ℓ", 'i'),
    ("Re", "ℜ", 'i'),
    ("Im", "ℑ", 'i'),
    ("aleph", "ℵ", 'i'),
    ("emptyset", "∅", 'i'),
    ("varnothing", "∅", 'i'),
    ("forall", "∀", 'i'),
    ("exists", "∃", 'i'),
    ("neg", "¬", 'o'),
    ("angle", "∠", 'i'),
    ("prime", "′", 'o'),
    ("ldots", "…", 'i'),
    ("dots", "…", 'i'),
    ("cdots", "⋯", 'i'),
    ("vdots", "⋮", 'i'),
    ("ddots", "⋱", 'i'),
    ("pm", "±", 'o'),
    ("mp", "∓", 'o'),
    ("times", "×", 'o'),
    ("div", "÷", 'o'),
    ("cdot", "⋅", 'o'),
    ("ast", "∗", 'o'),
    ("star", "⋆", 'o'),
    ("circ", "∘", 'o'),
    ("bullet", "∙", 'o'),
    ("oplus", "⊕", 'o'),
    ("ominus", "⊖", 'o'),
    ("otimes", "⊗", 'o'),
    ("odot", "⊙", 'o'),
    ("cup", "∪", 'o'),
    ("cap", "∩", 'o'),
    ("setminus", "∖", 'o'),
    ("wedge", "∧", 'o'),
    ("land", "∧", 'o'),
    ("vee", "∨", 'o'),
    ("lor", "∨", 'o'),
    ("leq", "≤", 'o'),
    ("le", "≤", 'o'),
    ("geq", "≥", 'o'),
    ("ge", "≥", 'o'),
    ("neq", "≠", 'o'),
    ("ne", "≠", 'o'),
    ("approx", "≈", 'o'),
    ("equiv", "≡", 'o'),
    ("sim", "∼", 'o'),
    ("simeq", "≃", 'o'),
    ("cong", "≅", 'o'),
    ("propto", "∝", 'o'),
    ("ll", "≪", 'o'),
    ("gg", "≫", 'o'),
    ("in", "∈", 'o'),
    ("notin", "∉", 'o'),
    ("ni", "∋", 'o'),
    ("subset", "⊂", 'o'),
    ("supset", "⊃", 'o'),
    ("subseteq", "⊆", 'o'),
    ("supseteq", "⊇", 'o'),
    ("perp", "⊥", 'o'),
    ("parallel", "∥", 'o'),
    ("mid", "∣", 'o'),
    ("to", "→", 'o'),
    ("rightarrow", "→", 'o'),
    ("leftarrow", "←", 'o'),
    ("gets", "←", 'o'),
    ("leftrightarrow", "↔", 'o'),
    ("Rightarrow", "⇒", 'o'),
    ("Leftarrow", "⇐", 'o'),
    ("Leftrightarrow", "⇔", 'o'),
    ("implies", "⟹", 'o'),
    ("iff", "⟺", 'o'),
    ("mapsto", "↦", 'o'),
    ("uparrow", "↑", 'o'),
    ("downarrow", "↓", 'o'),
    ("langle", "⟨", 'o'),
    ("rangle", "⟩", 'o'),
    ("lceil", "⌈", 'o'),
    ("rceil", "⌉", 'o'),
    ("lfloor", "⌊", 'o'),
    ("rfloor", "⌋", 'o'),
    ("lvert", "|", 'o'),
    ("rvert", "|", 'o'),
    ("lVert", "‖", 'o'),
    ("rVert", "‖", 'o'),
    ("|", "‖", 'o'),
    ("{", "{", 'o'),
    ("}", "}", 'o'),
    ("%", "%", 'o'),
    ("$", "$", 'o'),
    ("&", "&", 'o'),
    ("#", "#", 'o'),
    ("_", "_", 'o'),
];

/// Big operators: their scripts become limits in display style.
const BIG_OPS: &[(&str, &str)] = &[
    ("sum", "∑"),
    ("prod", "∏"),
    ("coprod", "∐"),
    ("bigcup", "⋃"),
    ("bigcap", "⋂"),
    ("bigoplus", "⨁"),
    ("bigotimes", "⨂"),
    ("bigvee", "⋁"),
    ("bigwedge", "⋀"),
];

/// Integral signs: scripts stay at the side.
const INTEGRALS: &[(&str, &str)] = &[
    ("int", "∫"),
    ("iint", "∬"),
    ("iiint", "∭"),
    ("oint", "∮"),
];

/// Named functions. The flag marks those whose scripts become limits.
const FUNCS: &[(&str, bool)] = &[
    ("sin", false),
    ("cos", false),
    ("tan", false),
    ("cot", false),
    ("sec", false),
    ("csc", false),
    ("arcsin", false),
    ("arccos", false),
    ("arctan", false),
    ("sinh", false),
    ("cosh", false),
    ("tanh", false),
    ("log", false),
    ("ln", false),
    ("lg", false),
    ("exp", false),
    ("deg", false),
    ("dim", false),
    ("ker", false),
    ("hom", false),
    ("arg", false),
    ("det", true),
    ("gcd", true),
    ("lim", true),
    ("liminf", true),
    ("limsup", true),
    ("max", true),
    ("min", true),
    ("sup", true),
    ("inf", true),
    ("Pr", true),
];

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(|ch| ch.is_whitespace()) {
            self.i += 1;
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(k, ch)| self.c.get(self.i + k) == Some(&ch))
    }

    /// Sequence of atoms up to a closing `}`, `&`, `\\`, `\right` or `\end`.
    fn seq(&mut self) -> Result<Vec<String>, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("nesting too deep".into());
        }
        let mut out = Vec::new();
        loop {
            self.ws();
            match self.peek() {
                None | Some('}') | Some('&') => break,
                Some('\\') if self.at_terminator() => break,
                _ => {}
            }
            let base = self.atom()?;
            out.push(self.scripts(base)?);
        }
        self.depth -= 1;
        Ok(out)
    }

    fn at_terminator(&self) -> bool {
        self.starts_with("\\\\")
            || self.word_at("right")
            || self.word_at("end")
    }

    /// `\name` at the cursor, where the name ends at a non-letter.
    fn word_at(&self, name: &str) -> bool {
        if self.peek() != Some('\\') {
            return false;
        }
        let n = name.chars().count();
        let tail_ok = self
            .c
            .get(self.i + 1 + n)
            .is_none_or(|ch| !ch.is_ascii_alphabetic());
        tail_ok
            && name
                .chars()
                .enumerate()
                .all(|(k, ch)| self.c.get(self.i + 1 + k) == Some(&ch))
    }

    fn scripts(&mut self, base: Atom) -> Result<String, String> {
        let mut sub: Option<String> = None;
        let mut sup: Option<String> = None;
        loop {
            self.ws();
            match self.peek() {
                Some('_') => {
                    self.i += 1;
                    if sub.is_some() {
                        return Err("double subscript".into());
                    }
                    sub = Some(self.arg()?);
                }
                Some('^') => {
                    self.i += 1;
                    if sup.is_some() {
                        return Err("double superscript".into());
                    }
                    sup = Some(self.arg()?);
                }
                Some('\'') => {
                    self.i += 1;
                    let prime = mo("′");
                    sup = Some(match sup {
                        Some(s) => format!("{s}{prime}"),
                        None => prime,
                    });
                }
                _ => break,
            }
        }
        let b = base.ml;
        Ok(match (sub, sup) {
            (None, None) => b,
            (Some(s), None) if base.limits && self.display => {
                format!("<munder>{b}{s}</munder>")
            }
            (None, Some(u)) if base.limits && self.display => {
                format!("<mover>{b}{u}</mover>")
            }
            (Some(s), Some(u)) if base.limits && self.display => {
                format!("<munderover>{b}{s}{u}</munderover>")
            }
            (Some(s), None) => format!("<msub>{b}{s}</msub>"),
            (None, Some(u)) => format!("<msup>{b}{u}</msup>"),
            (Some(s), Some(u)) => format!("<msubsup>{b}{s}{u}</msubsup>"),
        })
    }

    /// A command or script argument: a braced group or one atom.
    fn arg(&mut self) -> Result<String, String> {
        self.ws();
        match self.peek() {
            None => Err("missing argument".into()),
            Some('}') => Err("missing argument".into()),
            Some('{') => Ok(self.group()?.ml),
            Some('^') | Some('_') => Err("misplaced script".into()),
            _ => Ok(self.atom()?.ml),
        }
    }

    fn group(&mut self) -> Result<Atom, String> {
        self.i += 1; // {
        let nodes = self.seq()?;
        if self.peek() != Some('}') {
            return Err("unbalanced braces".into());
        }
        self.i += 1;
        let ml = if nodes.len() == 1 {
            nodes.into_iter().next().unwrap_or_default()
        } else {
            mrow(&nodes.concat())
        };
        Ok(Atom { ml, limits: false })
    }

    /// Raw text of a braced argument, for `\text` and friends.
    fn raw_arg(&mut self) -> Result<String, String> {
        self.ws();
        if self.peek() != Some('{') {
            return Err("expected `{`".into());
        }
        self.i += 1;
        let mut depth = 1;
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            self.i += 1;
            match ch {
                '{' => {
                    depth += 1;
                    s.push(ch);
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(s);
                    }
                    s.push(ch);
                }
                _ => s.push(ch),
            }
        }
        Err("unbalanced braces".into())
    }

    fn atom(&mut self) -> Result<Atom, String> {
        self.ws();
        let Some(ch) = self.peek() else {
            return Err("missing argument".into());
        };
        let plain = |ml: String| Atom { ml, limits: false };
        match ch {
            '{' => self.group(),
            '\\' => self.command(),
            '^' | '_' => Ok(plain("<mrow></mrow>".into())),
            c if c.is_ascii_digit() || c == '.' => {
                let mut s = String::new();
                while let Some(d) = self.peek() {
                    if d.is_ascii_digit() || (d == '.' && s.chars().all(|x| x != '.')) {
                        s.push(d);
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                if s == "." {
                    Ok(plain(mo(".")))
                } else {
                    Ok(plain(mn(&s)))
                }
            }
            c if c.is_alphabetic() => {
                self.i += 1;
                Ok(plain(mi(&c.to_string())))
            }
            '~' => {
                self.i += 1;
                Ok(plain("<mspace width=\"0.28em\"></mspace>".into()))
            }
            '$' => Err("nested `$`".into()),
            '#' | '@' => Err(format!("unsupported `{ch}`")),
            c => {
                self.i += 1;
                Ok(plain(mo(&c.to_string())))
            }
        }
    }

    fn command(&mut self) -> Result<Atom, String> {
        self.i += 1; // backslash
        let Some(first) = self.peek() else {
            return Err("trailing backslash".into());
        };
        let mut name = String::new();
        if first.is_ascii_alphabetic() {
            while let Some(ch) = self.peek() {
                if ch.is_ascii_alphabetic() {
                    name.push(ch);
                    self.i += 1;
                } else {
                    break;
                }
            }
        } else {
            name.push(first);
            self.i += 1;
        }
        let plain = |ml: String| Atom { ml, limits: false };

        if let Some((_, t, k)) = SYMBOLS.iter().find(|(n, _, _)| *n == name) {
            return Ok(plain(if *k == 'i' { mi(t) } else { mo(t) }));
        }
        if let Some((_, t)) = BIG_OPS.iter().find(|(n, _)| *n == name) {
            return Ok(Atom {
                ml: mo(t),
                limits: true,
            });
        }
        if let Some((_, t)) = INTEGRALS.iter().find(|(n, _)| *n == name) {
            return Ok(plain(mo(t)));
        }
        if let Some((_, lim)) = FUNCS.iter().find(|(n, _)| *n == name) {
            return Ok(Atom {
                ml: format!("{}<mo>\u{2061}</mo>", mi(&name)),
                limits: *lim,
            });
        }
        match name.as_str() {
            "," => Ok(plain(space("0.17em"))),
            ":" | ">" => Ok(plain(space("0.22em"))),
            ";" => Ok(plain(space("0.28em"))),
            "!" => Ok(plain(space("-0.17em"))),
            " " => Ok(plain(space("0.28em"))),
            "quad" => Ok(plain(space("1em"))),
            "qquad" => Ok(plain(space("2em"))),
            "frac" | "dfrac" | "tfrac" => {
                let a = self.arg()?;
                let b = self.arg()?;
                Ok(plain(format!("<mfrac>{a}{b}</mfrac>")))
            }
            "binom" => {
                let a = self.arg()?;
                let b = self.arg()?;
                Ok(plain(mrow(&format!(
                    "{}<mfrac linethickness=\"0\">{a}{b}</mfrac>{}",
                    mo("("),
                    mo(")")
                ))))
            }
            "sqrt" => {
                self.ws();
                if self.peek() == Some('[') {
                    self.i += 1;
                    let mut idx = String::new();
                    loop {
                        match self.peek() {
                            Some(']') => {
                                self.i += 1;
                                break;
                            }
                            Some(ch) => {
                                idx.push(ch);
                                self.i += 1;
                            }
                            None => return Err("unterminated `[`".into()),
                        }
                    }
                    let mut sub = Parser {
                        c: idx.chars().collect(),
                        i: 0,
                        display: false,
                        depth: self.depth,
                    };
                    let n = sub.seq()?;
                    if sub.i < sub.c.len() {
                        return Err("bad root index".into());
                    }
                    let body = self.arg()?;
                    Ok(plain(format!("<mroot>{body}{}</mroot>", mrow(&n.concat()))))
                } else {
                    let body = self.arg()?;
                    Ok(plain(format!("<msqrt>{body}</msqrt>")))
                }
            }
            "text" | "textrm" | "mbox" | "textit" | "textbf" | "mathrm" => {
                let t = self.raw_arg()?;
                Ok(plain(mtext(&t)))
            }
            "operatorname" => {
                let t = self.raw_arg()?;
                Ok(plain(format!("{}<mo>\u{2061}</mo>", mi(t.trim()))))
            }
            "mathbf" | "mathit" | "mathsf" | "mathtt" | "boldsymbol" | "bm" | "mathcal"
            | "mathscr" | "mathfrak" => {
                let a = self.arg()?;
                Ok(plain(mrow(&a)))
            }
            "mathbb" => {
                let t = self.raw_arg()?;
                let mapped: String = t
                    .chars()
                    .map(|ch| match ch {
                        'R' => 'ℝ',
                        'N' => 'ℕ',
                        'Z' => 'ℤ',
                        'Q' => 'ℚ',
                        'C' => 'ℂ',
                        'P' => 'ℙ',
                        'H' => 'ℍ',
                        other => other,
                    })
                    .collect();
                Ok(plain(mi(&mapped)))
            }
            "overline" | "bar" => {
                let a = self.arg()?;
                Ok(plain(format!("<mover>{a}{}</mover>", mo("\u{203e}"))))
            }
            "underline" => {
                let a = self.arg()?;
                Ok(plain(format!("<munder>{a}{}</munder>", mo("_"))))
            }
            "hat" | "widehat" => self.accent("^"),
            "tilde" | "widetilde" => self.accent("~"),
            "vec" => self.accent("→"),
            "dot" => self.accent("˙"),
            "ddot" => self.accent("¨"),
            "left" => {
                let open = self.delim()?;
                let inner = self.seq()?;
                if !self.word_at("right") {
                    return Err("`\\left` without `\\right`".into());
                }
                self.i += "\\right".len();
                let close = self.delim()?;
                let mut s = String::new();
                if !open.is_empty() {
                    s.push_str(&mo(&open));
                }
                s.push_str(&inner.concat());
                if !close.is_empty() {
                    s.push_str(&mo(&close));
                }
                Ok(plain(mrow(&s)))
            }
            "begin" => self.environment(),
            "right" | "end" => Err(format!("unmatched `\\{name}`")),
            _ => Err(format!("unsupported command `\\{name}`")),
        }
    }

    fn accent(&mut self, mark: &str) -> Result<Atom, String> {
        let a = self.arg()?;
        Ok(Atom {
            ml: format!(
                "<mover accent=\"true\">{a}<mo>{}</mo></mover>",
                esc(mark)
            ),
            limits: false,
        })
    }

    /// Delimiter after `\left` / `\right`; `.` is the empty delimiter.
    fn delim(&mut self) -> Result<String, String> {
        self.ws();
        match self.peek() {
            Some('.') => {
                self.i += 1;
                Ok(String::new())
            }
            Some('\\') => {
                self.i += 1;
                let mut name = String::new();
                if let Some(f) = self.peek() {
                    name.push(f);
                    self.i += 1;
                    if f.is_ascii_alphabetic() {
                        while let Some(ch) = self.peek() {
                            if ch.is_ascii_alphabetic() {
                                name.push(ch);
                                self.i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
                SYMBOLS
                    .iter()
                    .find(|(n, _, k)| *n == name && *k == 'o')
                    .map(|(_, t, _)| (*t).to_string())
                    .ok_or_else(|| format!("unsupported delimiter `\\{name}`"))
            }
            Some(ch) if "()[]|/<>".contains(ch) => {
                self.i += 1;
                Ok(match ch {
                    '<' => "⟨".into(),
                    '>' => "⟩".into(),
                    _ => ch.to_string(),
                })
            }
            _ => Err("missing delimiter".into()),
        }
    }

    fn environment(&mut self) -> Result<Atom, String> {
        let env = self.raw_arg()?;
        let (open, close) = match env.as_str() {
            "matrix" => ("", ""),
            "pmatrix" => ("(", ")"),
            "bmatrix" => ("[", "]"),
            "Bmatrix" => ("{", "}"),
            "vmatrix" => ("|", "|"),
            "Vmatrix" => ("‖", "‖"),
            "cases" => ("{", ""),
            _ => return Err(format!("unsupported environment `{env}`")),
        };
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row: Vec<String> = Vec::new();
        loop {
            let cell = self.seq()?;
            row.push(format!("<mtd>{}</mtd>", cell.concat()));
            if self.peek() == Some('&') {
                self.i += 1;
            } else if self.starts_with("\\\\") {
                self.i += 2;
                rows.push(std::mem::take(&mut row));
            } else if self.word_at("end") {
                self.i += "\\end".len();
                let name = self.raw_arg()?;
                if name != env {
                    return Err(format!("`\\begin{{{env}}}` closed by `\\end{{{name}}}`"));
                }
                break;
            } else {
                return Err(format!("unterminated environment `{env}`"));
            }
        }
        // A trailing `\\` leaves one empty cell row: drop it.
        if row.len() == 1 && row[0] == "<mtd></mtd>" && !rows.is_empty() {
            row.clear();
        }
        if !row.is_empty() {
            rows.push(row);
        }
        let table: String = rows
            .iter()
            .map(|r| format!("<mtr>{}</mtr>", r.concat()))
            .collect();
        let mut s = String::new();
        if !open.is_empty() {
            s.push_str(&mo(open));
        }
        s.push_str(&format!("<mtable>{table}</mtable>"));
        if !close.is_empty() {
            s.push_str(&mo(close));
        }
        Ok(Atom {
            ml: mrow(&s),
            limits: false,
        })
    }
}

fn space(w: &str) -> String {
    format!("<mspace width=\"{w}\"></mspace>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_shapes() {
        let m = tex_to_mathml("x^2 + \\frac{a}{b}", false).unwrap();
        assert!(m.contains("<msup><mi>x</mi><mn>2</mn></msup>"), "{m}");
        assert!(m.contains("<mfrac><mi>a</mi><mi>b</mi></mfrac>"), "{m}");
    }

    #[test]
    fn unknown_command_errs() {
        assert!(tex_to_mathml("\\foo{x}", false).is_err());
    }
}
