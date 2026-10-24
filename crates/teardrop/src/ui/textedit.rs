//! One-line/multi-line editable text buffer with a movable caret and a
//! selection — the model behind every text field in the app. Pure data +
//! char-index logic (multibyte safe), so it is fully unit-testable.
//!
//! `caret` is where text inserts; `anchor` is the other end of the selection
//! (`anchor == caret` means nothing is selected). Both are char indices.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextField {
    text: String,
    caret: usize,
    anchor: usize,
}

/// Word-boundary classes, Windows-like: whitespace / word chars / punctuation.
fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

impl TextField {
    pub fn new(s: impl Into<String>) -> Self {
        let text = s.into();
        let end = text.chars().count();
        Self { text, caret: end, anchor: end }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Caret position in chars (0..=len).
    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Replace the whole buffer; caret goes to the end, selection cleared.
    pub fn set_text(&mut self, s: impl Into<String>) {
        *self = Self::new(s);
    }

    /// Ordered (start, end) char range of the selection, None when empty.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.anchor != self.caret).then(|| (self.anchor.min(self.caret), self.anchor.max(self.caret)))
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.slice(a, b))
    }

    /// All of a non-empty buffer is selected.
    pub fn all_selected(&self) -> bool {
        !self.text.is_empty() && self.selection() == Some((0, self.len()))
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.len();
    }

    pub fn collapse_to_end(&mut self) {
        self.caret = self.len();
        self.anchor = self.caret;
    }

    /// Drop the selection, keeping the caret where it is.
    pub fn deselect(&mut self) {
        self.anchor = self.caret;
    }

    /// Insert `s` at the caret, replacing any selection. Truncates the
    /// insertion so the total stays within `cap` chars. Returns true on change.
    pub fn insert(&mut self, s: &str, cap: usize) -> bool {
        let had_sel = self.delete_selection();
        let room = cap.saturating_sub(self.len());
        let ins: String = s.chars().take(room).collect();
        if ins.is_empty() {
            return had_sel;
        }
        let b = self.byte_idx(self.caret);
        let n = ins.chars().count();
        self.text.insert_str(b, &ins);
        self.caret += n;
        self.anchor = self.caret;
        true
    }

    /// Backspace: deletes the selection, else the char (word=true: word) before the caret.
    pub fn backspace(&mut self, word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let start = if word { self.word_left(self.caret) } else { self.caret - 1 };
        self.delete_range(start, self.caret);
        true
    }

    /// Delete key: deletes the selection, else the char (word=true: word) after the caret.
    pub fn delete(&mut self, word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret >= self.len() {
            return false;
        }
        let end = if word { self.word_right(self.caret) } else { self.caret + 1 };
        self.delete_range(self.caret, end);
        true
    }

    /// Left (-1) / Right (+1). Without `extend`, an active selection collapses
    /// to the side moved toward (Windows behaviour). `word` = Ctrl jumps.
    pub fn move_h(&mut self, dir: i32, word: bool, extend: bool) {
        let target = if !extend {
            if let Some((a, b)) = self.selection() {
                if dir < 0 { a } else { b }
            } else if dir < 0 {
                if word { self.word_left(self.caret) } else { self.caret.saturating_sub(1) }
            } else if word {
                self.word_right(self.caret)
            } else {
                (self.caret + 1).min(self.len())
            }
        } else if dir < 0 {
            if word { self.word_left(self.caret) } else { self.caret.saturating_sub(1) }
        } else if word {
            self.word_right(self.caret)
        } else {
            (self.caret + 1).min(self.len())
        };
        self.caret = target;
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Home: `doc` -> start of text, else start of the current logical ('\n') line.
    pub fn home(&mut self, extend: bool, doc: bool) {
        self.caret = if doc { 0 } else { self.line_start(self.caret) };
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// End: `doc` -> end of text, else end of the current logical ('\n') line.
    pub fn end(&mut self, extend: bool, doc: bool) {
        self.caret = if doc { self.len() } else { self.line_end(self.caret) };
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Up (-1) / Down (+1) across logical '\n' lines, keeping the column
    /// (clamped to a shorter line). Off the ends goes home/doc-end.
    pub fn move_v(&mut self, dir: i32, extend: bool) {
        let lines = self.lines();
        let cur = lines.iter().position(|&(a, b)| self.caret >= a && self.caret <= b).unwrap_or(lines.len() - 1);
        let col = self.caret - lines[cur].0;
        self.caret = if dir < 0 {
            if cur == 0 {
                0
            } else {
                let (ps, pe) = lines[cur - 1];
                (ps + col).min(pe)
            }
        } else if cur + 1 >= lines.len() {
            self.len()
        } else {
            let (ns, ne) = lines[cur + 1];
            (ns + col).min(ne)
        };
        if !extend {
            self.anchor = self.caret;
        }
    }

    // ------------------------------------------------------------ internals

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    fn byte_idx(&self, ci: usize) -> usize {
        self.text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    fn slice(&self, a: usize, b: usize) -> String {
        self.text[self.byte_idx(a)..self.byte_idx(b)].to_string()
    }

    /// Logical-line char ranges [start, end) split on '\n' ('\n' excluded).
    fn lines(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0usize;
        for (i, ch) in self.text.chars().enumerate() {
            if ch == '\n' {
                out.push((start, i));
                start = i + 1;
            }
        }
        out.push((start, self.len()));
        out
    }

    fn line_start(&self, ci: usize) -> usize {
        self.lines().into_iter().find(|&(_s, e)| ci <= e).map(|(s, _)| s).unwrap_or(0)
    }

    fn line_end(&self, ci: usize) -> usize {
        self.lines().into_iter().find(|&(_s, e)| ci <= e).map(|(_, e)| e).unwrap_or(self.len())
    }

    fn delete_range(&mut self, a: usize, b: usize) {
        let (ba, bb) = (self.byte_idx(a), self.byte_idx(b));
        self.text.replace_range(ba..bb, "");
        self.caret = a;
        self.anchor = a;
    }

    fn delete_selection(&mut self) -> bool {
        match self.selection() {
            Some((a, b)) => {
                self.delete_range(a, b);
                true
            }
            None => false,
        }
    }

    /// Previous word boundary before char index `p` (Ctrl+Left).
    fn word_left(&self, p: usize) -> usize {
        let ch = self.chars();
        let mut i = p.min(ch.len());
        while i > 0 && class(ch[i - 1]) == 0 {
            i -= 1;
        }
        if i > 0 {
            let cl = class(ch[i - 1]);
            while i > 0 && class(ch[i - 1]) == cl {
                i -= 1;
            }
        }
        i
    }

    /// Next word boundary after char index `p` (Ctrl+Right).
    fn word_right(&self, p: usize) -> usize {
        let ch = self.chars();
        let mut i = p.min(ch.len());
        if i < ch.len() && class(ch[i]) != 0 {
            let cl = class(ch[i]);
            while i < ch.len() && class(ch[i]) == cl {
                i += 1;
            }
        }
        while i < ch.len() && class(ch[i]) == 0 {
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> TextField {
        TextField::new(s)
    }

    #[test]
    fn new_puts_caret_at_end() {
        let t = f("héllo");
        assert_eq!(t.caret(), 5);
        assert_eq!(t.selection(), None);
        assert!(!t.is_empty());
    }

    #[test]
    fn insert_mid_text() {
        let mut t = f("helo");
        t.move_h(-1, false, false);
        t.move_h(-1, false, false);
        t.insert("l", 120);
        assert_eq!(t.as_str(), "hello");
        assert_eq!(t.caret(), 3);
    }

    #[test]
    fn insert_replaces_selection() {
        let mut t = f("hello world");
        t.select_all();
        assert!(t.all_selected());
        t.insert("bye", 120);
        assert_eq!(t.as_str(), "bye");
        assert_eq!(t.caret(), 3);
    }

    #[test]
    fn insert_honours_cap() {
        let mut t = f("ab");
        t.insert("cdef", 4);
        assert_eq!(t.as_str(), "abcd");
        assert!(!t.insert("x", 4));
    }

    #[test]
    fn backspace_char_and_word() {
        let mut t = f("hello,  world  ");
        t.backspace(false); // trailing space
        assert_eq!(t.as_str(), "hello,  world ");
        t.backspace(true); // spaces + "world"
        assert_eq!(t.as_str(), "hello,  ");
        t.backspace(true); // ",  " punctuation run then spaces
        assert_eq!(t.as_str(), "hello");
        t.backspace(true);
        assert_eq!(t.as_str(), "");
        assert!(!t.backspace(true));
    }

    #[test]
    fn backspace_punct_only_deletes_punct_run() {
        let mut t = f("foo--bar");
        t.backspace(true);
        assert_eq!(t.as_str(), "foo--");
        t.backspace(true);
        assert_eq!(t.as_str(), "foo");
    }

    #[test]
    fn delete_char_and_word() {
        let mut t = f("hello,  world");
        t.home(false, true);
        t.delete(true); // "hello" — stops before ',' (punct is its own class)
        assert_eq!(t.as_str(), ",  world");
        t.delete(true); // ",  " punct run + trailing spaces
        assert_eq!(t.as_str(), "world");
        let mut t = f("x");
        t.collapse_to_end();
        assert!(!t.delete(true));
    }

    #[test]
    fn delete_selection_via_backspace_or_delete() {
        for word in [false, true] {
            let mut t = f("hello");
            t.select_all();
            t.backspace(word);
            assert_eq!(t.as_str(), "");
            let mut t = f("hello");
            t.select_all();
            t.delete(word);
            assert_eq!(t.as_str(), "");
        }
    }

    #[test]
    fn move_h_word_jumps() {
        let mut t = f("foo bar, baz");
        t.move_h(-1, true, false);
        assert_eq!(t.caret(), 9); // start of "baz"
        t.move_h(-1, true, false);
        assert_eq!(t.caret(), 7); // ',' punct run (skips the gap first)
        t.move_h(-1, true, false);
        assert_eq!(t.caret(), 4); // start of "bar"
        t.move_h(1, true, false);
        assert_eq!(t.caret(), 7); // end of "bar", before ','
        t.move_h(1, true, false);
        assert_eq!(t.caret(), 9); // past ", " to start of "baz"
    }

    #[test]
    fn arrow_collapses_selection_without_shift() {
        let mut t = f("hello");
        t.select_all();
        t.move_h(-1, false, false);
        assert_eq!(t.caret(), 0);
        assert_eq!(t.selection(), None);
        t.select_all();
        t.move_h(1, false, false);
        assert_eq!(t.caret(), 5);
        t.move_h(-1, true, false); // with extend=false no selection now: word jump
        assert_eq!(t.caret(), 0);
    }

    #[test]
    fn extend_then_type_replaces() {
        let mut t = f("hello");
        t.move_h(-1, false, true);
        t.move_h(-1, false, true);
        assert_eq!(t.selected_text().as_deref(), Some("lo"));
        t.insert("p", 120);
        assert_eq!(t.as_str(), "help");
        assert_eq!(t.caret(), 4);
    }

    #[test]
    fn home_end_line_vs_doc() {
        let mut t = f("one\ntwo three\nfour");
        // caret at end of text
        t.home(false, false);
        assert_eq!(t.caret(), 14); // start of "four"
        t.end(false, false);
        assert_eq!(t.caret(), 18);
        t.home(false, true);
        assert_eq!(t.caret(), 0);
        t.end(false, true);
        assert_eq!(t.caret(), 18);
        // inside middle line
        let mut t = f("one\ntwo three\nfour");
        for _ in 0..8 {
            t.move_h(-1, false, false); // caret 18 -> 10 = "e" end of "three"? 18-8=10
        }
        t.home(false, false);
        assert_eq!(t.caret(), 4);
        t.end(false, false);
        assert_eq!(t.caret(), 13);
    }

    #[test]
    fn move_v_keeps_column_clamped() {
        let mut t = f("long line here\nhi\nmid line x");
        // caret at end (27). col on line 2 = 27-17=10 -> move up to "hi" clamps to 2
        t.move_v(-1, false);
        assert_eq!(t.caret(), 17); // end of "hi" (15..17)
        t.move_v(-1, false);
        assert_eq!(t.caret(), 2); // col 2 of first line
        t.move_v(-1, false);
        assert_eq!(t.caret(), 0); // home on first line
        t.move_v(1, false);
        assert_eq!(t.caret(), 15); // col 0 of "hi"
        t.move_v(1, false);
        assert_eq!(t.caret(), 18); // col 0 of "mid line x"
        t.move_v(1, false);
        assert_eq!(t.len(), t.caret()); // off the end -> doc end
    }

    #[test]
    fn emoji_and_cjk_are_single_chars() {
        let mut t = f("a🎉bc");
        assert_eq!(t.len(), 4); // the emoji is one char
        t.move_h(-1, false, false); // 4 -> 3, before 'c'
        t.move_h(-1, false, false); // 3 -> 2, before 'b'
        t.insert("X", 120);
        assert_eq!(t.as_str(), "a🎉Xbc");
        t.move_h(1, false, false); // past 'b'
        t.backspace(false);
        assert_eq!(t.as_str(), "a🎉Xc");
        // word jump treats CJK as word chars
        let mut t = f("\u{4e2d}\u{6587} test");
        t.move_h(-1, true, false);
        assert_eq!(t.caret(), 3);
    }

    #[test]
    fn select_all_empty_is_not_all_selected() {
        let mut t = TextField::default();
        t.select_all();
        assert!(!t.all_selected());
        assert_eq!(t.selected_text(), None);
    }
}
