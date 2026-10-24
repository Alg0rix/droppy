//! Time-synced lyrics (LRC parsing + lookup).
#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    pub at_ms: u64,
    pub text: String,
}

pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in input.lines() {
        // One line may carry several [mm:ss.xx] tags.
        let mut rest = raw;
        let mut tags = Vec::new();
        loop {
            let Some(open) = rest.find('[') else { break };
            let Some(close_rel) = rest[open..].find(']') else { break };
            let close = open + close_rel;
            let tag = &rest[open + 1..close];
            if let Some(ms) = parse_tag(tag) {
                tags.push(ms);
                rest = &rest[close + 1..];
            } else {
                // Metadata like [ar:Artist] — stop tag scanning, keep remainder if after colon content?
                rest = &rest[close + 1..];
                // metadata lines have no lyric text; skip if line started with it
                if raw.trim_start().starts_with('[') && tag.contains(':') && !tag.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    // could still be lyric if malformed; treat rest as text only when tags found
                    if tags.is_empty() { break; }
                }
            }
        }
        let text = rest.trim().to_string();
        if text.is_empty() { continue; }
        for ms in tags {
            out.push(LyricLine { at_ms: ms, text: text.clone() });
        }
    }
    out.sort_by_key(|l| l.at_ms);
    out
}

fn parse_tag(tag: &str) -> Option<u64> {
    let mut parts = tag.split([':', '.']);
    let m: u64 = parts.next()?.trim().parse().ok()?;
    let s: u64 = parts.next()?.trim().parse().ok()?;
    let frac = parts.next().unwrap_or("0");
    if m > 999 || s > 99 { return None; }
    let frac_ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac[..3].parse::<u64>().ok()?,
    };
    Some(m * 60_000 + s * 1000 + frac_ms)
}

/// Index of the line playing at `pos_ms`.
pub fn line_at(lines: &[LyricLine], pos_ms: u64) -> Option<usize> {
    if lines.is_empty() { return None; }
    let mut lo = 0usize;
    let mut hi = lines.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if lines[mid].at_ms <= pos_ms { lo = mid + 1; } else { hi = mid; }
    }
    lo.checked_sub(1)
}

/// Karaoke progress 0..1 within the active line.
pub fn line_progress(lines: &[LyricLine], idx: usize, pos_ms: u64) -> f32 {
    let Some(cur) = lines.get(idx) else { return 0.0 };
    let next_at = lines.get(idx + 1).map(|l| l.at_ms).unwrap_or(cur.at_ms + 4000);
    if pos_ms < cur.at_ms || next_at <= cur.at_ms { return 0.0; }
    // A line is sung in roughly 85 ms a character, not across the whole gap to
    // the next one (instrumental breaks would make the fill crawl).
    let sung = (cur.text.chars().count() as u64 * 85).max(1200);
    let span = (next_at - cur.at_ms).min(sung);
    ((pos_ms - cur.at_ms) as f32 / span as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_basic() {
        let lines = parse_lrc("[00:12.00]hello\n[00:15.50]world");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at_ms, 12_000);
        assert_eq!(lines[1].at_ms, 15_500);
        assert_eq!(line_at(&lines, 13_000), Some(0));
        assert_eq!(line_at(&lines, 11_000), None);
    }
    #[test]
    fn multi_tag_and_meta() {
        let lines = parse_lrc("[ar:Someone]\n[00:01.00][00:02.00]repeat");
        assert_eq!(lines.len(), 2);
    }
}
