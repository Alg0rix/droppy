//! Markdown for chat replies: a small CommonMark-ish parser, a pure wrapping
//! layout (so callers know the height before drawing), and a painter that
//! emits scene commands. Supported: headings, paragraphs, bullet/numbered
//! lists (nested), block quotes, fenced code, pipe tables (as monospace),
//! rules, and inline **bold**, *italic*, `code`, [links](url) and bare URLs.
//!
//! Text is estimated with `est_w` for wrapping and measured for real by the
//! renderer (`Cmd::Runs`), so a line can never overlap itself — at worst a
//! wrap lands a touch early.

use super::layout::est_w;
use super::scene::{Color, Rect, Run, Scene, Weight, CHIP_PAD};
use super::theme::{pal, theme_index};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    /// `marker` is empty for a bullet dot, else the number ("1.").
    Item {
        depth: u8,
        marker: String,
        spans: Vec<Span>,
    },
    Quote(Vec<Span>),
    Code {
        lang: String,
        lines: Vec<String>,
    },
    Rule,
}

// ---------------------------------------------------------------- parsing

pub fn parse(src: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(inline(&para.join(" "))));
            para.clear();
        }
    };
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i].trim_end();
        let t = raw.trim_start();
        i += 1;
        if let Some(rest) = t.strip_prefix("```").or_else(|| t.strip_prefix("~~~")) {
            flush(&mut para, &mut out);
            let lang = rest.trim().to_string();
            let mut body = Vec::new();
            // An unclosed fence (a reply cut off mid-block) still renders as code.
            while i < lines.len() {
                let l = lines[i];
                i += 1;
                if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
                    break;
                }
                body.push(l.trim_end().replace('\t', "    "));
            }
            out.push(Block::Code { lang, lines: body });
            continue;
        }
        if t.is_empty() {
            flush(&mut para, &mut out);
            continue;
        }
        if is_rule(t) {
            flush(&mut para, &mut out);
            out.push(Block::Rule);
            continue;
        }
        let hashes = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            flush(&mut para, &mut out);
            out.push(Block::Heading(hashes as u8, inline(t[hashes..].trim().trim_end_matches('#').trim_end())));
            continue;
        }
        if let Some(rest) = t.strip_prefix('>') {
            flush(&mut para, &mut out);
            out.push(Block::Quote(inline(rest.trim_start())));
            continue;
        }
        if t.starts_with('|') {
            // Pipe table: shown as aligned monospace rows; the |---| rule is dropped.
            flush(&mut para, &mut out);
            let mut rows = vec![t.to_string()];
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(lines[i].trim().to_string());
                i += 1;
            }
            rows.retain(|r| !r.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')));
            out.push(Block::Code { lang: String::new(), lines: rows.iter().map(|r| table_row(r)).collect() });
            continue;
        }
        let indent = raw.len() - t.len();
        if let Some((marker, rest)) = list_marker(t) {
            flush(&mut para, &mut out);
            out.push(Block::Item { depth: (indent / 2).min(3) as u8, marker, spans: inline(rest) });
            continue;
        }
        // A wrapped continuation line of the previous list item.
        if indent > 0 && para.is_empty() {
            if let Some(Block::Item { spans, .. }) = out.last_mut() {
                spans.push(Span { text: " ".into(), style: Style::default() });
                spans.extend(inline(t));
                continue;
            }
        }
        para.push(t);
    }
    flush(&mut para, &mut out);
    out
}

fn is_rule(t: &str) -> bool {
    let s: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    s.len() >= 3 && (s.chars().all(|c| c == '-') || s.chars().all(|c| c == '*') || s.chars().all(|c| c == '_'))
}

fn table_row(r: &str) -> String {
    r.trim().trim_matches('|').split('|').map(|c| c.trim()).collect::<Vec<_>>().join("  \u{2502}  ")
}

fn list_marker(t: &str) -> Option<(String, &str)> {
    for b in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(b) {
            return Some((String::new(), rest.trim_start()));
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=3).contains(&digits) {
        let after = &t[digits..];
        if let Some(rest) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return Some((format!("{}.", &t[..digits]), rest.trim_start()));
        }
    }
    None
}

/// Inline spans: **bold**, *italic*/_italic_, `code`, [label](url), bare URLs.
/// Markers without a closing partner stay literal.
pub fn inline(src: &str) -> Vec<Span> {
    let ch: Vec<char> = src.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut cur = String::new();
    let (mut bold, mut italic) = (false, false);
    let push = |spans: &mut Vec<Span>, cur: &mut String, st: Style| {
        if cur.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some(l) if l.style == st => l.text.push_str(cur),
            _ => spans.push(Span { text: std::mem::take(cur), style: st }),
        }
        cur.clear();
    };
    let has_ahead = |from: usize, pat: &[char]| -> bool { (from..ch.len().saturating_sub(pat.len() - 1)).any(|k| ch[k..k + pat.len()] == *pat) };
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        let st = Style { bold, italic, ..Default::default() };
        let next = ch.get(i + 1).copied();
        let prev_alnum = i > 0 && ch[i - 1].is_alphanumeric();
        match c {
            '\\' if next.is_some_and(|n| n.is_ascii_punctuation()) => {
                cur.push(next.unwrap());
                i += 2;
            }
            '`' => {
                if let Some(end) = (i + 1..ch.len()).find(|&k| ch[k] == '`') {
                    push(&mut spans, &mut cur, st);
                    let text: String = ch[i + 1..end].iter().collect();
                    spans.push(Span { text, style: Style { code: true, bold, italic, link: false } });
                    i = end + 1;
                } else {
                    cur.push(c);
                    i += 1;
                }
            }
            '*' | '_' => {
                let double = next == Some(c);
                let n = if double { 2 } else { 1 };
                let pat = vec![c; n];
                let after = ch.get(i + n).copied();
                let closing = if double { bold } else { italic };
                let can_open = after.is_some_and(|a| !a.is_whitespace()) && (c == '*' || !prev_alnum) && has_ahead(i + n, &pat);
                let can_close = closing && !(c == '_' && after.is_some_and(|a| a.is_alphanumeric()));
                if can_close || (!closing && can_open) {
                    push(&mut spans, &mut cur, st);
                    if double {
                        bold = !bold;
                    } else {
                        italic = !italic;
                    }
                    i += n;
                } else {
                    cur.push(c);
                    i += 1;
                }
            }
            '[' => {
                let close = (i + 1..ch.len()).find(|&k| ch[k] == ']');
                let link = close.filter(|&k| ch.get(k + 1) == Some(&'(')).and_then(|k| (k + 2..ch.len()).find(|&e| ch[e] == ')').map(|e| (k, e)));
                if let Some((k, e)) = link {
                    push(&mut spans, &mut cur, st);
                    let label: String = ch[i + 1..k].iter().collect();
                    spans.push(Span { text: label, style: Style { link: true, bold, italic, code: false } });
                    i = e + 1;
                } else {
                    cur.push(c);
                    i += 1;
                }
            }
            'h' if (ch[i..].starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/']) || ch[i..].starts_with(&['h', 't', 't', 'p', ':', '/', '/'])) && !prev_alnum => {
                push(&mut spans, &mut cur, st);
                let end = (i..ch.len()).find(|&k| ch[k].is_whitespace()).unwrap_or(ch.len());
                let mut end2 = end;
                while end2 > i && matches!(ch[end2 - 1], '.' | ',' | ')' | ';' | ':' | '!' | '?') {
                    end2 -= 1;
                }
                spans.push(Span { text: ch[i..end2].iter().collect(), style: Style { link: true, bold, italic, code: false } });
                i = end2;
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    let st = Style { bold, italic, ..Default::default() };
    push(&mut spans, &mut cur, st);
    spans
}

// ----------------------------------------------------------------- layout

/// Where a laid-out thing goes, relative to the markdown box's top-left.
#[derive(Debug, Clone)]
pub enum Item {
    Text { x: f32, y: f32, h: f32, size: f32, weight: Weight, color: Color, runs: Vec<(String, Style)> },
    Dot { x: f32, y: f32 },
    Number { x: f32, y: f32, h: f32, label: String, size: f32 },
    QuoteBar { x: f32, y: f32, h: f32 },
    CodeBox { y: f32, h: f32, lang: String },
    CodeLine { x: f32, y: f32, h: f32, text: String },
    Rule { y: f32 },
}

#[derive(Debug, Clone, Default)]
pub struct Laid {
    pub items: Vec<Item>,
    pub height: f32,
}

const BODY: f32 = 12.5;
const LH: f32 = 18.0;
const CODE_SIZE: f32 = 10.5;
const CODE_LH: f32 = 15.0;
const CODE_PAD: f32 = 9.0;

fn token_w(text: &str, st: Style, size: f32) -> f32 {
    if st.code {
        text.chars().count() as f32 * size * 0.56 + CHIP_PAD * 2.0
    } else {
        est_w(text, size) * if st.bold { 1.13 } else { 1.04 }
    }
}

/// Greedy-wrap `spans` into lines of runs no wider than `width` (estimated).
fn wrap_spans(spans: &[Span], width: f32, size: f32) -> Vec<Vec<(String, Style)>> {
    // Tokens: a word plus its trailing space; code spans never split.
    let mut toks: Vec<(String, Style)> = Vec::new();
    for sp in spans {
        if sp.style.code {
            toks.push((sp.text.clone(), sp.style));
            continue;
        }
        let mut w = String::new();
        for c in sp.text.chars() {
            w.push(c);
            if c == ' ' {
                toks.push((std::mem::take(&mut w), sp.style));
            }
        }
        if !w.is_empty() {
            toks.push((w, sp.style));
        }
    }
    let mut lines: Vec<Vec<(String, Style)>> = vec![Vec::new()];
    let mut used = 0.0;
    for (text, st) in toks {
        let mut tw = token_w(text.trim_end(), st, size);
        // Break a single over-long token (URL, path) into pieces that fit.
        let mut pieces = vec![text];
        if tw > width && !st.code {
            let budget = ((width / (tw / pieces[0].chars().count().max(1) as f32)).floor() as usize).max(4);
            let chars: Vec<char> = pieces.remove(0).chars().collect();
            pieces = chars.chunks(budget).map(|c| c.iter().collect()).collect();
            tw = token_w(pieces[0].trim_end(), st, size);
        }
        for p in pieces {
            let pw = token_w(p.trim_end(), st, size).min(width);
            let line = lines.last_mut().unwrap();
            if used + pw > width && !line.is_empty() {
                // Drop the trailing space the break swallows.
                if let Some(l) = line.last_mut() {
                    l.0 = l.0.trim_end().to_string();
                }
                lines.push(Vec::new());
                used = 0.0;
            }
            lines.last_mut().unwrap().push((p, st));
            used += token_w(&lines.last().unwrap().last().unwrap().0, st, size).min(width);
        }
        let _ = tw;
    }
    if let Some(l) = lines.last_mut().and_then(|l| l.last_mut()) {
        l.0 = l.0.trim_end().to_string();
    }
    lines.retain(|l| !l.is_empty());
    lines
}

pub fn layout(blocks: &[Block], width: f32) -> Laid {
    let mut items = Vec::new();
    let mut y = 0.0_f32;
    let mut prev: Option<&Block> = None;
    for b in blocks {
        // Vertical rhythm: tight inside lists, airy around headings and code.
        let gap = match (prev, b) {
            (None, _) => 0.0,
            (Some(Block::Item { .. }), Block::Item { .. }) => 3.0,
            (_, Block::Heading(..)) => 10.0,
            (Some(Block::Heading(..)), _) => 4.0,
            _ => 7.0,
        };
        y += gap;
        match b {
            Block::Heading(level, spans) => {
                let (size, lh) = match level {
                    1 => (15.5, 22.0),
                    2 => (14.0, 20.0),
                    _ => (13.0, 19.0),
                };
                for line in wrap_spans(spans, width, size) {
                    items.push(Item::Text { x: 0.0, y, h: lh, size, weight: Weight::Bold, color: pal().text, runs: line });
                    y += lh;
                }
            }
            Block::Para(spans) => {
                for line in wrap_spans(spans, width, BODY) {
                    items.push(Item::Text { x: 0.0, y, h: LH, size: BODY, weight: Weight::Regular, color: pal().text, runs: line });
                    y += LH;
                }
            }
            Block::Item { depth, marker, spans } => {
                let ind = *depth as f32 * 16.0;
                let text_x = ind + if marker.is_empty() { 16.0 } else { 22.0 };
                if marker.is_empty() {
                    items.push(Item::Dot { x: ind + 5.0, y: y + LH / 2.0 });
                } else {
                    items.push(Item::Number { x: ind, y, h: LH, label: marker.clone(), size: BODY });
                }
                for line in wrap_spans(spans, width - text_x, BODY) {
                    items.push(Item::Text { x: text_x, y, h: LH, size: BODY, weight: Weight::Regular, color: pal().text, runs: line });
                    y += LH;
                }
            }
            Block::Quote(spans) => {
                let lines = wrap_spans(spans, width - 14.0, BODY);
                items.push(Item::QuoteBar { x: 0.0, y: y + 1.0, h: lines.len() as f32 * LH - 2.0 });
                for line in lines {
                    items.push(Item::Text { x: 14.0, y, h: LH, size: BODY, weight: Weight::Regular, color: pal().text_2, runs: line });
                    y += LH;
                }
            }
            Block::Code { lang, lines } => {
                let per = (((width - CODE_PAD * 2.0) / (CODE_SIZE * 0.56)).floor() as usize).max(8);
                let mut rows: Vec<String> = Vec::new();
                for l in lines {
                    let chars: Vec<char> = l.chars().collect();
                    if chars.is_empty() {
                        rows.push(String::new());
                    }
                    for c in chars.chunks(per) {
                        rows.push(c.iter().collect());
                    }
                }
                if rows.is_empty() {
                    rows.push(String::new());
                }
                let head = if lang.is_empty() { 0.0 } else { 6.0 };
                let h = rows.len() as f32 * CODE_LH + CODE_PAD * 2.0 - 2.0 + head;
                items.push(Item::CodeBox { y, h, lang: lang.clone() });
                for (k, r) in rows.into_iter().enumerate() {
                    items.push(Item::CodeLine { x: CODE_PAD, y: y + CODE_PAD - 1.0 + head + k as f32 * CODE_LH, h: CODE_LH, text: r });
                }
                y += h;
            }
            Block::Rule => {
                items.push(Item::Rule { y: y + 4.0 });
                y += 9.0;
            }
        }
        prev = Some(b);
    }
    Laid { items, height: y }
}

thread_local! {
    static CACHE: RefCell<HashMap<(u64, u32, usize), Rc<Laid>>> = RefCell::new(HashMap::new());
}

/// Parse + lay out `src` at `width`, memoized: chat redraws every frame.
/// The active theme is part of the key — item colours are baked at layout.
pub fn laid(src: &str, width: f32) -> Rc<Laid> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    let key = (h.finish(), width.round() as u32, theme_index());
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(l) = c.get(&key) {
            return l.clone();
        }
        if c.len() > 96 {
            c.clear();
        }
        let l = Rc::new(layout(&parse(src), width));
        c.insert(key, l.clone());
        l
    })
}

// ----------------------------------------------------------------- drawing

fn runs_of(runs: &[(String, Style)], base: Color, weight: Weight, acc: Color) -> Vec<Run> {
    runs.iter()
        .map(|(text, st)| Run {
            text: text.clone(),
            weight: if st.bold { Weight::Bold } else { weight },
            italic: st.italic,
            mono: st.code,
            color: if st.link {
                acc
            } else if st.code {
                pal().text
            } else {
                base
            },
            underline: st.link,
            chip: st.code.then(|| pal().ink(0.11)),
        })
        .collect()
}

impl Laid {
    /// Paint at `(x, y)` (top-left) with total width `w`.
    pub fn draw(&self, s: &mut Scene, x: f32, y: f32, w: f32, acc: Color) {
        for it in &self.items {
            match it {
                Item::Text { x: ix, y: iy, h, size, weight, color, runs } => {
                    s.runs(Rect::new(x + ix, y + iy, w - ix, *h), *size, runs_of(runs, *color, *weight, acc));
                }
                Item::Dot { x: ix, y: iy } => s.circle(x + ix, y + iy, 2.2, acc),
                Item::Number { x: ix, y: iy, h, label, size } => {
                    s.text(label.clone(), Rect::new(x + ix, y + iy, 22.0, *h), *size, Weight::Semibold, acc, super::scene::Align::Left);
                }
                Item::QuoteBar { x: ix, y: iy, h } => s.rect(Rect::new(x + ix, y + iy, 3.0, *h), 1.5, acc.fade(0.6)),
                Item::CodeBox { y: iy, h, lang } => {
                    let r = Rect::new(x, y + iy, w, *h);
                    s.rect(r, 9.0, Color::rgba(0, 0, 0, 0.34));
                    s.stroke(r, 9.0, 1.0, pal().ink(0.07));
                    if !lang.is_empty() {
                        s.text(lang.clone(), Rect::new(r.x + 10.0, r.y + 3.0, r.w - 20.0, 13.0), 9.0, Weight::Semibold, pal().text_3, super::scene::Align::Right);
                    }
                }
                Item::CodeLine { x: ix, y: iy, h, text } => {
                    s.runs(
                        Rect::new(x + ix, y + iy, w - ix * 2.0, *h),
                        CODE_SIZE,
                        vec![Run { text: text.clone(), weight: Weight::Regular, italic: false, mono: true, color: Color::rgba(255, 255, 255, 0.86), underline: false, chip: None }],
                    );
                }
                Item::Rule { y: iy } => s.line(x, y + iy, x + w, y + iy, 1.0, pal().ink(0.12)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(b: &Block) -> String {
        match b {
            Block::Para(s) | Block::Heading(_, s) | Block::Quote(s) | Block::Item { spans: s, .. } => s.iter().map(|x| x.text.as_str()).collect(),
            _ => String::new(),
        }
    }

    #[test]
    fn inline_styles() {
        let s = inline("a **bold** and *it* with `code` and [site](http://x.io) end");
        let find = |t: &str| s.iter().find(|x| x.text == t).unwrap_or_else(|| panic!("no span {t}: {s:?}")).style;
        assert!(find("bold").bold && !find("bold").italic);
        assert!(find("it").italic);
        assert!(find("code").code);
        assert!(find("site").link);
        assert_eq!(s.iter().map(|x| x.text.as_str()).collect::<String>(), "a bold and it with code and site end");
    }

    #[test]
    fn unmatched_markers_stay_literal() {
        let s = inline("2 * 3 = 6, snake_case_name and **open");
        assert_eq!(s.iter().map(|x| x.text.as_str()).collect::<String>(), "2 * 3 = 6, snake_case_name and **open");
        assert!(s.iter().all(|x| !x.style.bold && !x.style.italic));
    }

    #[test]
    fn bare_url_is_a_link_without_trailing_punctuation() {
        let s = inline("see https://example.com/a?b=1.");
        let l = s.iter().find(|x| x.style.link).unwrap();
        assert_eq!(l.text, "https://example.com/a?b=1");
    }

    #[test]
    fn blocks() {
        let b = parse("# Title\n\nHello\nworld\n\n- one\n- two\n  - nested\n1. first\n\n> quote\n\n```rust\nlet x = 1;\n```\n---\n");
        assert_eq!(b[0], Block::Heading(1, vec![Span { text: "Title".into(), style: Style::default() }]));
        assert_eq!(plain(&b[1]), "Hello world", "soft breaks join");
        assert!(matches!(&b[2], Block::Item { depth: 0, marker, .. } if marker.is_empty()));
        assert!(matches!(&b[4], Block::Item { depth: 1, .. }));
        assert!(matches!(&b[5], Block::Item { marker, .. } if marker == "1."));
        assert!(matches!(&b[6], Block::Quote(_)));
        assert!(matches!(&b[7], Block::Code { lang, lines } if lang == "rust" && lines == &["let x = 1;"]));
        assert_eq!(b[8], Block::Rule);
    }

    #[test]
    fn unclosed_fence_still_renders_as_code() {
        let b = parse("```py\nprint(1)");
        assert!(matches!(&b[0], Block::Code { lines, .. } if lines.len() == 1));
    }

    #[test]
    fn table_rows_drop_the_separator() {
        let b = parse("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(matches!(&b[0], Block::Code { lines, .. } if lines.len() == 2));
    }

    #[test]
    fn wrapping_grows_with_narrower_width_and_never_empties() {
        let src = "The quick brown fox jumps over the lazy dog. ".repeat(6);
        let wide = layout(&parse(&src), 520.0).height;
        let narrow = layout(&parse(&src), 200.0).height;
        assert!(narrow > wide, "{narrow} vs {wide}");
        assert!(layout(&parse(""), 300.0).height == 0.0);
        // One giant unbreakable token is split instead of overflowing.
        let long = "x".repeat(400);
        assert!(layout(&parse(&long), 200.0).height > LH);
    }

    #[test]
    fn cache_returns_the_same_layout() {
        let a = laid("**hi** there", 300.0);
        let b = laid("**hi** there", 300.0);
        assert!(Rc::ptr_eq(&a, &b));
    }
}
