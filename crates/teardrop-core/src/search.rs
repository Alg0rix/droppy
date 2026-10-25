//! Launcher result ranking, modelled on PowerToys Command Palette.
//!
//! Three pieces ported from CmdPal:
//! - [`fuzzy`]: `FuzzyStringMatcher.ScoreNonContiguous` — a Smith-Waterman-ish
//!   DP that allows non-contiguous matches and rewards string starts,
//!   separator boundaries, camelCase humps and consecutive runs.
//! - [`tier`] / [`rank`]: `MainListRanker` — a match is placed in a tier
//!   (exact > prefix > word/acronym > fuzzy) and the fuzzy score plus a
//!   frecency `boost` only order results *within* the tier.
//! - [`History`]: `RecentCommandsManager` — a small MRU of launches whose
//!   recency × frequency weight is the `boost` above.

use serde::{Deserialize, Serialize};

/// Fuzzy match score of `query` against `target`; 0 means no match.
///
/// Case-folded comparison (`\` folds to `/`) with a raw pass kept for
/// exact-case bonuses. The query must be a subsequence of the target.
pub fn fuzzy(query: &str, target: &str) -> u32 {
    let q: Vec<char> = query.trim().chars().collect();
    let t: Vec<char> = target.chars().collect();
    if q.is_empty() || t.len() < q.len() {
        return 0;
    }
    let qf: Vec<char> = q.iter().map(|&c| fold(c)).collect();
    let tf: Vec<char> = t.iter().map(|&c| fold(c)).collect();
    // Quick reject: the folded query must be a subsequence of the target.
    let mut qi = 0;
    for &c in &tf {
        if qi < qf.len() && c == qf[qi] {
            qi += 1;
        }
    }
    if qi < qf.len() {
        return 0;
    }
    // scores[ti] = best score aligning a prefix of the query ending at or
    // before ti; seqs[ti] = matched-run length that produced it.
    let (qlen, tlen) = (qf.len(), tf.len());
    let mut scores = vec![0u32; tlen];
    let mut seqs = vec![0u32; tlen];
    for qi in 0..qlen {
        let mut left = 0;
        let mut diag = 0;
        let mut diag_seq = 0;
        let first = qi == 0;
        let ti_max = tlen - qlen + qi;
        for ti in 0..=ti_max {
            let up = scores[ti];
            let up_seq = seqs[ti];
            let mut cs = 0;
            if (diag != 0 || first) && qf[qi] == tf[ti] {
                cs = char_score(q[qi], ti > 0, if ti > 0 { t[ti - 1] } else { '\0' }, t[ti], diag_seq);
            }
            let cand = diag + cs;
            if cs != 0 && cand >= left {
                scores[ti] = cand;
                seqs[ti] = diag_seq + 1;
            } else {
                scores[ti] = left;
                seqs[ti] = 0;
            }
            left = scores[ti];
            diag = up;
            diag_seq = up_seq;
        }
        if left == 0 {
            return 0;
        }
        if qi == qlen - 1 {
            return left;
        }
    }
    0
}

/// Fold for comparison: path separators unify, everything else uppercases.
fn fold(c: char) -> char {
    if c == '\\' {
        '/'
    } else {
        c.to_uppercase().next().unwrap_or(c)
    }
}

/// Score of matching one query char at this target position. `seq` is the
/// length of the consecutive-match run the match would extend.
fn char_score(q_raw: char, has_prev: bool, prev: char, cur: char, seq: u32) -> u32 {
    let mut s = 1; // CharacterMatch
    if seq > 0 {
        s += seq * 5; // ConsecutiveMultiplier
    }
    if q_raw == cur {
        s += 1; // ExactCase
    }
    if !has_prev {
        return s + 8; // StringStart
    }
    let sep = match prev {
        '/' | '\\' => 5,
        '_' | '-' | '.' | ' ' | '\'' | '"' | ':' => 4,
        _ => 0,
    };
    if sep != 0 {
        return s + sep;
    }
    if seq == 0 && cur.is_uppercase() {
        return s + 2; // CamelCase
    }
    s
}

/// Which kind of match a hit is; ordering is the ranking priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    None = 0,
    Fuzzy = 2,
    WordOrAcronym = 3,
    Prefix = 4,
    Exact = 5,
}

/// Classify `title` against `query`. `matched` is whether the fuzzy pass
/// scored it at all (title or subtitle); it only lifts None to Fuzzy.
pub fn tier(query: &str, title: &str, matched: bool) -> Tier {
    let q = query.trim();
    if q.is_empty() || title.is_empty() {
        return if matched { Tier::Fuzzy } else { Tier::None };
    }
    if ci_eq(title, q) {
        return Tier::Exact;
    }
    if ci_starts_with(title, q) {
        return Tier::Prefix;
    }
    if word_or_acronym(title, q) {
        return Tier::WordOrAcronym;
    }
    if matched {
        Tier::Fuzzy
    } else {
        Tier::None
    }
}

fn ci_eq(a: &str, b: &str) -> bool {
    a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase))
}

fn ci_starts_with(title: &str, q: &str) -> bool {
    let mut t = title.chars().flat_map(char::to_lowercase);
    let mut q = q.chars().flat_map(char::to_lowercase);
    loop {
        match q.next() {
            None => return true,
            Some(c) => {
                if t.next() != Some(c) {
                    return false;
                }
            }
        }
    }
}

/// Word-prefix or acronym match: "studio" hits "Visual Studio", "toys" hits
/// "PowerToys", and a ≥2-char query contained in the word initials hits too
/// ("vsc" -> "Visual Studio Code").
fn word_or_acronym(title: &str, q: &str) -> bool {
    let t: Vec<char> = title.chars().collect();
    let ql: Vec<char> = q.chars().flat_map(char::to_lowercase).collect();
    let mut initials = String::new();
    for i in 0..t.len() {
        if !is_word_start(&t, i) {
            continue;
        }
        // Case-insensitive prefix test of `q` against title[i..].
        let mut ti = t[i..].iter().copied().flat_map(char::to_lowercase);
        let mut qi = ql.iter().copied();
        let hits = loop {
            match qi.next() {
                None => break true,
                Some(c) => {
                    if ti.next() != Some(c) {
                        break false;
                    }
                }
            }
        };
        if hits {
            return true;
        }
        initials.extend(t[i].to_lowercase());
    }
    ql.len() >= 2 && initials.contains(&ql.iter().collect::<String>())
}

/// Start of a word: position 0, a non-alphanumeric boundary, or an
/// uppercase hump after lowercase/digit ("Toys" in "PowerToys").
fn is_word_start(t: &[char], i: usize) -> bool {
    let cur = t[i];
    if i == 0 {
        return cur.is_alphanumeric();
    }
    let prev = t[i - 1];
    (!prev.is_alphanumeric() && cur.is_alphanumeric()) || (cur.is_uppercase() && (prev.is_lowercase() || prev.is_ascii_digit()))
}

/// Within-tier room: scores are `tier * TIER_STRIDE + within`.
pub const TIER_STRIDE: u32 = 10_000_000;

/// Packed rank for one hit; 0 means filtered out. A higher tier always wins —
/// the fuzzy score and `boost` (frecency etc.) only reorder within a tier.
pub fn rank(query: &str, title: &str, subtitle: &str, boost: f64) -> u32 {
    let name = fuzzy(query, title) as f64;
    let sub = fuzzy(query, subtitle);
    // Subtitle matches count at half weight past a small floor, so a weak
    // folder-name hit never outranks a name hit.
    let desc = (sub as f64 - 4.0) / 2.0;
    let lexical = name.max(desc);
    let matched = name > 0.0 || sub > 0;
    let t = tier(query, title, matched);
    if t == Tier::None {
        return 0;
    }
    let within = (lexical * 10.0 + boost).round().clamp(0.0, (TIER_STRIDE - 1) as f64) as u32;
    t as u32 * TIER_STRIDE + within
}

/// [`rank`] plus a keyword-phrase fallback: when the title alone can't do
/// better than Fuzzy, a `;`-separated keyword phrase matching the query
/// (exact, prefix, or word/acronym) lifts the hit into WordOrAcronym tier —
/// never higher, so a real title prefix/exact always wins.
pub fn rank_with_keywords(query: &str, title: &str, subtitle: &str, keywords: &str, boost: f64) -> u32 {
    let base = rank(query, title, subtitle, boost);
    let q = query.trim();
    if q.is_empty() || tier_of(base) >= Tier::WordOrAcronym {
        return base;
    }
    let mut best = 0u32;
    for phrase in keywords.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        if ci_eq(phrase, q) || ci_starts_with(phrase, q) || word_or_acronym(phrase, q) {
            best = best.max(fuzzy(q, phrase));
        }
    }
    if best == 0 {
        return base;
    }
    let within = (best as f64 * 5.0 + boost).round().clamp(0.0, (TIER_STRIDE - 1) as f64) as u32;
    base.max(Tier::WordOrAcronym as u32 * TIER_STRIDE + within)
}

/// Recover the tier a [`rank`] score was built with.
pub fn tier_of(score: u32) -> Tier {
    match score / TIER_STRIDE {
        2 => Tier::Fuzzy,
        3 => Tier::WordOrAcronym,
        4 => Tier::Prefix,
        5.. => Tier::Exact,
        _ => Tier::None,
    }
}

/// Launch history ("most recent first"), persisted as JSON next to the app
/// settings; powers the frecency boost in [`rank`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct History {
    #[serde(default)]
    pub entries: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    /// Launch id (a path), stored lowercased — ids compare case-insensitively.
    pub id: String,
    pub uses: u32,
    /// Unix seconds of the last launch.
    pub last_used: i64,
}

impl History {
    /// Cap on remembered entries; the least recently used fall off.
    pub const MAX: usize = 500;

    /// Frecency weight for `id`: half-life ~3 days on recency, log-scaled
    /// frequency, clamped to [0, 70]. Unknown ids weigh 0.
    pub fn weight(&self, id: &str, now: i64) -> f64 {
        let id = id.to_lowercase();
        let Some(e) = self.entries.iter().find(|e| e.id == id) else {
            return 0.0;
        };
        let age_days = (now - e.last_used).max(0) as f64 / 86400.0;
        let recency = (-age_days / 3.0f64).exp2();
        let freq = (e.uses as f64 + 1.0).log2();
        (recency * (10.0 + 10.0 * freq)).round().clamp(0.0, 70.0)
    }

    /// Record a launch of `id` at `now`: moves it to the front, bumps its
    /// use count, drops entries past [`History::MAX`].
    pub fn record(&mut self, id: &str, now: i64) {
        let id = id.to_lowercase();
        let uses = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .map(|i| self.entries.remove(i).uses)
            .unwrap_or(0);
        self.entries.insert(0, HistoryEntry { id, uses: uses + 1, last_used: now });
        self.entries.truncate(Self::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches() {
        assert!(fuzzy("vsc", "Visual Studio Code") > 0);
        assert_eq!(fuzzy("xyz", "Visual Studio Code"), 0);
        assert_eq!(fuzzy("", "Notepad"), 0);
        assert_eq!(fuzzy("notepadplusplus", "Notepad"), 0);
    }

    #[test]
    fn fuzzy_prefers_name_and_boundary_hits() {
        // "code" lands on a word boundary in VS Code, mid-word in "Decoder".
        assert!(fuzzy("code", "Visual Studio Code") > fuzzy("code", "Decoder"));
        // String start beats a mid-word match.
        assert!(fuzzy("no", "Notepad") > fuzzy("no", "Piano"));
    }

    #[test]
    fn fuzzy_folds_path_separators() {
        assert!(fuzzy("a/b", "a\\b") > 0);
    }

    #[test]
    fn tiers_classify() {
        assert_eq!(tier("notepad", "Notepad", true), Tier::Exact);
        assert_eq!(tier("note", "Notepad", true), Tier::Prefix);
        assert_eq!(tier("vs", "Visual Studio 2022", true), Tier::WordOrAcronym);
        assert_eq!(tier("studio", "Visual Studio", true), Tier::WordOrAcronym);
        assert_eq!(tier("toys", "PowerToys", true), Tier::WordOrAcronym);
        assert_eq!(tier("vsc", "Visual Studio Code", true), Tier::WordOrAcronym);
        assert_eq!(tier("vsud", "Visual Studio", true), Tier::Fuzzy);
        assert_eq!(tier("zz", "Visual Studio", false), Tier::None);
        // Blank inputs only ever reach Fuzzy via `matched`.
        assert_eq!(tier("", "Notepad", true), Tier::Fuzzy);
        assert_eq!(tier("n", "", false), Tier::None);
    }

    #[test]
    fn rank_tier_beats_boost() {
        let exact = rank("code", "Code", "", 0.0);
        let prefix = rank("code", "Code Insiders", "", 70.0);
        let word = rank("code", "Visual Studio Code", "", 70.0);
        let fuzz = rank("vsud", "Visual Studio", "", 70.0);
        assert_eq!(tier_of(exact), Tier::Exact);
        assert_eq!(tier_of(prefix), Tier::Prefix);
        assert_eq!(tier_of(word), Tier::WordOrAcronym);
        assert_eq!(tier_of(fuzz), Tier::Fuzzy);
        assert!(exact > prefix && prefix > word && word > fuzz);
        assert_eq!(rank("zzzzz", "Code", "", 70.0), 0);
    }

    #[test]
    fn rank_boost_reorders_within_a_tier() {
        let plain = rank("vs", "Visual Studio 2022", "", 0.0);
        let boosted = rank("vs", "Visual Studio Code", "", 40.0);
        assert_eq!(tier_of(plain), Tier::WordOrAcronym);
        assert_eq!(tier_of(boosted), Tier::WordOrAcronym);
        assert!(boosted > plain);
    }

    #[test]
    fn rank_with_keywords_lifts_to_word_tier_only() {
        // "terminal" isn't in the title at all — a keyword phrase carries it.
        let kw = rank_with_keywords("terminal", "Command Prompt", "", "cmd;terminal;console;shell", 0.0);
        assert_eq!(tier_of(kw), Tier::WordOrAcronym);
        // A real title match still outranks the keyword-lifted hit.
        let title = rank_with_keywords("terminal", "Terminal", "", "", 0.0);
        assert!(title > kw);
        // No keywords: the non-match stays out.
        let none = rank_with_keywords("terminal", "Command Prompt", "", "", 0.0);
        assert!(tier_of(none) < Tier::WordOrAcronym);
        // A title prefix keeps its own (better) tier — keywords can't lift.
        assert_eq!(tier_of(rank_with_keywords("terminal", "Terminal settings", "", "x", 0.0)), Tier::Prefix);
        // Multi-word phrases match on a word boundary inside them.
        let phrase = rank_with_keywords("terminal", "Command Prompt", "", "default terminal", 0.0);
        assert_eq!(tier_of(phrase), Tier::WordOrAcronym);
    }

    #[test]
    fn tier_of_round_trips() {
        for (q, title) in [
            ("code", "Code"),
            ("code", "Code Insiders"),
            ("code", "Visual Studio Code"),
            ("vsud", "Visual Studio"),
        ] {
            let score = rank(q, title, "", 0.0);
            assert_eq!(tier_of(score), tier(q, title, true), "{q} / {title}");
        }
        assert_eq!(tier_of(0), Tier::None);
    }

    #[test]
    fn history_weight_decays_and_caps() {
        let mut h = History::default();
        let now = 1_700_000_000;
        assert_eq!(h.weight("missing", now), 0.0);
        h.record("App", now);
        assert_eq!(h.weight("app", now), 20.0); // case-insensitive id
        assert_eq!(h.weight("app", now + 3 * 86400), 10.0);
        for _ in 0..99 {
            h.record("app", now);
        }
        assert_eq!(h.weight("app", now), 70.0);
    }

    #[test]
    fn history_record_is_mru() {
        let mut h = History::default();
        h.record("a", 10);
        h.record("b", 20);
        h.record("a", 30);
        assert_eq!(h.entries[0].id, "a");
        assert_eq!(h.entries[0].uses, 2);
        assert_eq!(h.entries[0].last_used, 30);
        assert_eq!(h.entries[1].id, "b");
    }

    #[test]
    fn history_truncates_at_max() {
        let mut h = History::default();
        for i in 0..History::MAX + 5 {
            h.record(&format!("app-{i}"), i as i64);
        }
        assert_eq!(h.entries.len(), History::MAX);
        assert_eq!(h.entries[0].id, format!("app-{}", History::MAX + 4));
        assert_eq!(h.entries.last().unwrap().id, "app-5"); // app-0..4 dropped
    }

    #[test]
    fn history_serde_round_trip() {
        let mut h = History::default();
        h.record("a", 10);
        h.record("B", 20);
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(serde_json::from_str::<History>(&json).unwrap(), h);
        assert_eq!(serde_json::from_str::<History>("{}").unwrap(), History::default());
    }
}
