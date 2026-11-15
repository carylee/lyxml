//! Optional changes to a score before it is engraved.

use crate::ir::*;

/// `mode`: "keep" (all breaks), "ignore" (none), "pages" (only page breaks).
/// `page_breaks`: bar numbers that start a new page, added to what is kept.
pub fn apply_breaks(score: &mut Score, mode: &str, page_breaks: &[String]) -> Vec<String> {
    let mut missing: Vec<String> = page_breaks.to_vec();
    for p in &mut score.parts {
        for m in &mut p.measures {
            match mode {
                "ignore" => {
                    m.new_system = false;
                    m.new_page = false;
                }
                "pages" => m.new_system = false,
                _ => {}
            }
            if !m.implicit && page_breaks.iter().any(|b| *b == m.number) {
                m.new_page = true;
                missing.retain(|b| *b != m.number);
            }
        }
    }
    missing
}

/// the whole words sung on one lyric line over a bar
fn words(m: &Measure, line: u32) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut evs: Vec<&Event> = m.voices.iter().flat_map(|v| v.events.iter()).collect();
    evs.sort_by(|a, b| a.pos.cmp(&b.pos));
    for e in evs {
        for l in e.lyrics.iter().filter(|l| l.line == line) {
            cur.push_str(&l.text);
            if matches!(l.syllabic, Syllabic::Single | Syllabic::End) {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // only the sung words count: "in‿a" on one note is the same words as "in a" on two
    out.iter().flat_map(|w| w.split(|c: char| c == '\u{203f}' || c.is_whitespace())).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Typographic quotes in the words: a `"` that opens a syllable becomes “, any other ”.
pub fn curly_quotes(score: &mut Score) {
    for e in score.parts.iter_mut().flat_map(|p| p.measures.iter_mut()).flat_map(|m| m.voices.iter_mut()).flat_map(|v| v.events.iter_mut()) {
        for l in &mut e.lyrics {
            if l.text.contains('"') {
                let mut out = String::with_capacity(l.text.len() + 4);
                for (i, c) in l.text.chars().enumerate() {
                    match c {
                        '"' if i == 0 => out.push('\u{201c}'),
                        '"' => out.push('\u{201d}'),
                        c => out.push(c),
                    }
                }
                l.text = out;
            }
        }
    }
}

/// Words shared by a staff and the staff above it print once, under the upper staff:
/// bar by bar and verse by verse, the lower staff's words are removed where they are the
/// same words as the upper staff's. Different words stay. Returns how many bars lost a line.
pub fn hide_shared_lyrics(score: &mut Score) -> usize {
    let mut dropped = 0;
    let original = score.parts.clone();
    for i in 1..score.parts.len() {
        let upper = &original[i - 1];
        if upper.staves > 1 || score.parts[i].staves > 1 {
            continue;
        }
        for (mi, m) in score.parts[i].measures.iter_mut().enumerate() {
            let Some(mu) = upper.measures.get(mi) else { continue };
            let mut lines: Vec<u32> = m.voices.iter().flat_map(|v| v.events.iter()).flat_map(|e| e.lyrics.iter().map(|l| l.line)).collect();
            lines.sort();
            lines.dedup();
            for line in lines {
                let w = words(&original[i].measures[mi], line);
                if !w.is_empty() && w == words(mu, line) {
                    for v in &mut m.voices {
                        for e in &mut v.events {
                            // an empty syllable stays in place of each hidden one for now
                            for l in e.lyrics.iter_mut().filter(|l| l.line == line) {
                                l.text = String::new();
                                l.syllabic = Syllabic::Single;
                                l.extend = false;
                                l.stanza = None;
                            }
                        }
                    }
                    dropped += 1;
                }
            }
        }
        // Of the hidden syllables keep only those that follow a printed syllable with an
        // extender line: the empty syllable ends that line where the hidden words begin,
        // instead of letting it run on to the staff's next printed word.
        let mut ext_open: std::collections::HashMap<(u32, u32, u32), bool> = Default::default();
        for m in score.parts[i].measures.iter_mut() {
            for v in &mut m.voices {
                for e in &mut v.events {
                    let key = |l: &Lyric| (v.staff, v.voice, l.line);
                    let mut keep: Vec<Lyric> = vec![];
                    for l in e.lyrics.drain(..) {
                        let open = ext_open.entry(key(&l)).or_insert(false);
                        if l.text.is_empty() {
                            if *open {
                                *open = false;
                                keep.push(l);
                            }
                        } else {
                            *open = l.extend;
                            keep.push(l);
                        }
                    }
                    e.lyrics = keep;
                }
            }
        }
    }
    dropped
}
