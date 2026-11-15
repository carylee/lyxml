//! LilyPond reader: source text -> score model.
//!
//! Three stages: `parse` builds a syntax tree (resolving variables and durations),
//! `interp` walks it like LilyPond's iterators to get per-voice timelines plus one global
//! timeline, and `build` cuts those into measures.

mod ast;
mod build;
mod interp;
mod lex;
mod parse;
#[cfg(test)]
mod tests;

use crate::diag::{Diags, Level};
use crate::ir::Score;
use ast::{Music, Val};
use std::path::Path;

/// Parse LilyPond source. `base_dir` is where `\include`d files are looked up.
pub fn read_ly(src: &str, base_dir: Option<&Path>, diags: &mut Diags) -> Result<Score, String> {
    let mut parser = parse::Parser::new(src, base_dir.map(|p| p.to_path_buf()), diags);
    parser.parse_toplevel(false)?;
    let file = std::mem::take(&mut parser.file);
    drop(parser);

    let mut score = Score::default();
    // header: the last assignment of a field wins
    let get = |key: &str| -> Option<String> {
        file.header.iter().rev().find(|(k, _)| k == key).and_then(|(_, v)| match v {
            Val::Scheme(lex::Sexp::Bool(false)) => None,
            // one line of markup keeps its *italic* marks
            Val::Markup(m) if m.lines.len() == 1 => Some(m.lines[0].clone()),
            Val::Markup(m) if m.lines.is_empty() && m.italic && !m.text.contains('\n') && !m.text.is_empty() => Some(format!("*{}*", m.text)),
            v => v.text(),
        })
        .filter(|s| !s.trim().is_empty())
    };
    score.header.title = get("title");
    score.header.subtitle = get("subtitle");
    score.header.dedication = get("dedication");
    score.header.composer = get("composer");
    if let Some((_, Val::Markup(m))) = file.header.iter().rev().find(|(k, _)| k == "composer") {
        if !m.lines.is_empty() {
            // a column of credit lines; with several, the first is the composer proper
            score.header.composer_lines = m.lines.clone();
            score.header.composer = if m.lines.len() > 1 { m.lines.first().filter(|l| !l.starts_with('*')).cloned() } else { None };
        }
    }
    score.header.poet = get("poet");
    if let Some((_, Val::Markup(m))) = file.header.iter().rev().find(|(k, _)| k == "poet") {
        if !m.lines.is_empty() {
            score.header.poet_lines = m.lines.clone();
            score.header.poet = if m.lines.len() > 1 { m.lines.first().filter(|l| !l.starts_with('*')).cloned() } else { None };
        }
    }
    score.header.arranger = get("arranger");
    score.header.copyright = get("copyright");

    // layout
    let num = |key: &str| -> Option<f64> {
        file.paper.iter().rev().find(|(k, _)| k == key).and_then(|(_, v)| match v {
            Val::Num(n) => Some(*n),
            Val::Scheme(s) => s.as_f64(),
            _ => None,
        })
    };
    if let Some(name) = &file.paper_size {
        if let Some((w, h)) = paper_size(name) {
            score.layout.page_width_mm = Some(w);
            score.layout.page_height_mm = Some(h);
        }
    }
    if let Some(w) = num("paper-width") {
        score.layout.page_width_mm = Some(w);
    }
    if let Some(h) = num("paper-height") {
        score.layout.page_height_mm = Some(h);
    }
    score.layout.first_page = num("first-page-number").map(|n| n as i32);
    score.layout.staff_size = file.staff_size;
    let lay = &file.layout_raw;
    if lay.contains("RemoveAllEmptyStaves") || lay.contains("RemoveEmptyStaves") {
        score.layout.hide_empty = true;
    }
    if lay.contains("\\omit BarNumber") || lay.contains("Bar_number_engraver") || lay.contains("\\omit Score.BarNumber") {
        score.layout.bar_numbers = Some(false);
    }
    let numeric = lay.contains("numericTimeSignature") || lay.contains("TimeSignature.style");

    // the music: the first \score that has any
    let mut scores: Vec<&Music> = file.scores.iter().filter(|m| has_events(m)).collect();
    if scores.is_empty() {
        return Err("no music found in the LilyPond source".into());
    }
    if scores.len() > 1 {
        diags.at_line(Level::Dropped, 0, format!("the file has {} scores; only the first is converted", scores.len()));
    }
    let music = scores.remove(0);
    let mut it = interp::Interp::new(diags, numeric);
    it.own_dialect = src.contains("Written by lyxml");
    it.run(music);
    let mut score = build::build(it, score);
    // an empty syllable only stops an extender; it is not a word
    for e in score.parts.iter_mut().flat_map(|p| p.measures.iter_mut()).flat_map(|m| m.voices.iter_mut()).flat_map(|v| v.events.iter_mut()) {
        e.lyrics.retain(|l| !l.text.is_empty());
    }
    Ok(score)
}

fn has_events(m: &Music) -> bool {
    match m {
        Music::Ev(_) => true,
        Music::Seq(v) | Music::Sim(v) | Music::Voices(v) | Music::Alternative(v) => v.iter().any(has_events),
        Music::Var(m) => has_events(m),
        Music::Relative(_, b)
        | Music::Fixed(_, b)
        | Music::Absolute(b)
        | Music::Transpose(_, _, b)
        | Music::Context { body: b, .. }
        | Music::Tuplet { body: b, .. }
        | Music::Scale(_, b)
        | Music::Grace { body: b, .. }
        | Music::Volta(_, b)
        | Music::After(_, _, b)
        | Music::LyricsTo(_, b) => has_events(b),
        Music::AfterGrace(a, b) => has_events(a) || has_events(b),
        Music::Repeat { body, alts, .. } => has_events(body) || alts.iter().any(has_events),
        Music::AddLyrics(m, _) => has_events(m),
        _ => false,
    }
}

fn paper_size(name: &str) -> Option<(f64, f64)> {
    let lower = name.to_lowercase();
    let (base, landscape) = match lower.strip_suffix("landscape") {
        Some(b) => (b.trim().to_string(), true),
        None => (lower.clone(), false),
    };
    let (w, h) = match base.as_str() {
        "a2" => (420.0, 594.0),
        "a3" => (297.0, 420.0),
        "a4" => (210.0, 297.0),
        "a5" => (148.0, 210.0),
        "a6" => (105.0, 148.0),
        "b4" => (250.0, 353.0),
        "b5" => (176.0, 250.0),
        "letter" => (215.9, 279.4),
        "legal" => (215.9, 355.6),
        "tabloid" | "11x17" => (279.4, 431.8),
        "ledger" => (431.8, 279.4),
        "octavo" => (171.45, 266.7),
        "quarto" => (215.9, 275.082),
        "executive" => (184.15, 266.7),
        "statement" | "half letter" => (139.7, 215.9),
        _ => return None,
    };
    Some(if landscape { (h, w) } else { (w, h) })
}
