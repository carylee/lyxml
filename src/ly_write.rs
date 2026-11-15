//! Score model -> LilyPond source.
//!
//! Principles:
//! - structure is written literally: every repeat sign, volta bracket, double bar and
//!   break is exactly where the source has it; nothing is inferred;
//! - words are aligned explicitly: every note takes a syllable or `_`, so LilyPond's
//!   melisma guessing never moves a word;
//! - one bar per line, with a bar check and the source's bar number in a comment;
//! - the same input always gives the same output.

use crate::diag::Diags;
use crate::ir::*;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct LyOptions {
    pub paper: Option<String>,
    pub staff_size: Option<f64>,
    pub hide_empty: Option<bool>,
    pub bar_numbers: Option<bool>,
    pub breaks: bool,
    /// Some(true) = the file's beams, Some(false) = LilyPond's, None = file's when it has any
    pub beams: Option<bool>,
    pub midi: bool,
    pub version: String,
    /// title in large small capitals (the choral octavo look)
    pub title_smallcaps: bool,
    /// serif text font family ("Times")
    pub text_font: Option<String>,
    /// staves (by part id, name or short name) hidden on systems where they only rest
    pub hide_staves: Vec<String>,
    /// Some(true) = a bar number on every bar
    pub bar_numbers_all: Option<bool>,
}

impl Default for LyOptions {
    fn default() -> Self {
        LyOptions { paper: None, staff_size: None, hide_empty: None, bar_numbers: None, breaks: true, beams: None, midi: false, version: "2.24.0".into(), title_smallcaps: false, text_font: None, hide_staves: vec![], bar_numbers_all: None }
    }
}

// ---------------------------------------------------------------- small helpers

pub fn ly_string(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' | '\r' | '\t' => o.push(' '),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// `*text*` -> italic; plain otherwise
fn markup_word(s: &str) -> String {
    let t = s.trim();
    let has_span = t.matches('*').count() >= 2;
    if !has_span {
        return ly_string(t);
    }
    // *span* toggles italic
    let segs: Vec<&str> = t.split('*').collect();
    let mut parts: Vec<String> = vec![];
    for (i, seg) in segs.iter().enumerate() {
        if seg.is_empty() {
            continue;
        }
        let italic = i % 2 == 1 && i + 1 < segs.len();
        parts.push(if italic { format!("\\italic {}", ly_string(seg)) } else { ly_string(seg) });
    }
    match parts.len() {
        0 => "\"\"".into(),
        1 => parts.remove(0),
        _ => format!("\\concat {{ {} }}", parts.join(" ")),
    }
}

/// a header / name value: a string, or a column for several lines
fn markup_lines(s: &str, column: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    if lines.len() == 1 && s.matches('*').count() < 2 {
        ly_string(s)
    } else if lines.len() == 1 {
        format!("\\markup {{ {} }}", markup_word(s))
    } else {
        let parts: Vec<String> = lines.iter().map(|l| markup_word(l)).collect();
        format!("\\markup \\{} {{ {} }}", column, parts.join(" "))
    }
}

fn type_token(t: NoteType) -> String {
    let base = match t.log {
        -1 => "\\breve".to_string(),
        l if l <= -2 => "\\longa".to_string(),
        l => (1u32 << l).to_string(),
    };
    format!("{}{}", base, ".".repeat(t.dots as usize))
}

/// a duration of exactly this length: "4.", or "1*5/8" when no note value fits
fn dur_token(len: Frac) -> String {
    match NoteType::from_len(len) {
        Some(t) => type_token(t),
        None => {
            if len.d == 1 {
                format!("1*{}", len.n)
            } else {
                format!("1*{}/{}", len.n, len.d)
            }
        }
    }
}

fn pitch_token(n: &Note) -> String {
    const STEPS: [char; 7] = ['c', 'd', 'e', 'f', 'g', 'a', 'b'];
    let mut s = String::new();
    s.push(STEPS[n.step as usize % 7]);
    for _ in 0..n.alter.max(0) {
        s.push_str("is");
    }
    for _ in 0..(-n.alter).max(0) {
        s.push_str("es");
    }
    let marks = n.octave as i32 - 3;
    for _ in 0..marks.max(0) {
        s.push('\'');
    }
    for _ in 0..(-marks).max(0) {
        s.push(',');
    }
    s
}

const NUM_WORDS: [&str; 21] = [
    "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen",
    "Sixteen", "Seventeen", "Eighteen", "Nineteen", "Twenty",
];

fn num_word(n: usize) -> String {
    if n < NUM_WORDS.len() {
        NUM_WORDS[n].to_string()
    } else {
        // letters: 21 -> "U", ...
        let mut s = String::new();
        let mut k = n;
        while k > 0 {
            s.insert(0, (b'A' + ((k - 1) % 26) as u8) as char);
            k = (k - 1) / 26;
        }
        format!("N{}", s)
    }
}

fn clef_name(sign: char, line: i8, octave: i8) -> String {
    let base = match (sign, line) {
        ('G', 2) => "treble",
        ('G', 1) => "french",
        ('F', 4) => "bass",
        ('F', 3) => "varbaritone",
        ('F', 5) => "subbass",
        ('C', 1) => "soprano",
        ('C', 2) => "mezzosoprano",
        ('C', 3) => "alto",
        ('C', 4) => "tenor",
        ('C', 5) => "baritone",
        ('P', _) => "percussion",
        ('T', _) => "tab",
        ('F', _) => "bass",
        ('C', _) => "alto",
        _ => "treble",
    };
    match octave {
        0 => base.to_string(),
        -1 => format!("{}_8", base),
        1 => format!("{}^8", base),
        -2 => format!("{}_15", base),
        2 => format!("{}^15", base),
        _ => base.to_string(),
    }
}

fn key_token(fifths: i8, minor: bool) -> String {
    const MAJOR: [&str; 15] = ["ces", "ges", "des", "aes", "ees", "bes", "f", "c", "g", "d", "a", "e", "b", "fis", "cis"];
    const MINOR: [&str; 15] = ["aes", "ees", "bes", "f", "c", "g", "d", "a", "e", "b", "fis", "cis", "gis", "dis", "ais"];
    let i = (fifths.clamp(-7, 7) + 7) as usize;
    if minor {
        format!("\\key {} \\minor", MINOR[i])
    } else {
        format!("\\key {} \\major", MAJOR[i])
    }
}

const DYNAMICS: [&str; 21] =
    ["ppppp", "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fffff", "fp", "sf", "sff", "sp", "spp", "sfz", "rfz", "fz", "n"];

fn is_jump_text(t: &str) -> bool {
    let l = t.trim().to_lowercase();
    ["d.s", "d.c", "d. s", "d. c", "ds ", "dc ", "dal segno", "da capo", "fine", "to coda", "al coda", "al fine"].iter().any(|p| l.starts_with(p))
        || l == "ds"
        || l == "dc"
}

fn bar_glyph(style: Option<&str>) -> Option<&'static str> {
    Some(match style? {
        "light-light" => "||",
        "light-heavy" => "|.",
        "heavy-light" => ".|",
        "heavy-heavy" => "..",
        "dashed" => "!",
        "dotted" => ";",
        "none" => "",
        "tick" => "'",
        "short" => ",",
        "heavy" => ".",
        _ => return None,
    })
}

fn lyric_token(text: &str) -> String {
    let t = text.replace('\u{203f}', "~");
    let safe = !t.is_empty()
        && t.chars().any(|c| c.is_alphabetic())
        && t.chars().all(|c| c.is_alphabetic() || matches!(c, '\'' | '\u{2019}' | ',' | '.' | '!' | '?' | ';' | ':' | '~'))
        && !t.starts_with('~');
    if safe {
        t
    } else {
        ly_string(&t)
    }
}

// ---------------------------------------------------------------- per-voice data

type VKey = (u32, u32); // (staff, voice)

/// (position in bar, comes-before-the-note, token)
type Item = (Frac, bool, String);

struct VoiceData {
    key: VKey,
    var: String,
    /// rank among the voices of its staff
    rank: usize,
    staff_voices: usize,
    primary: bool,
    measures: Vec<Vec<Event>>,
    items: Vec<Vec<Item>>,
    lyric_lines: Vec<u32>,
    has_grace: bool,
}

struct Slot {
    pos: Frac,
    dur: Frac,
    ev: Option<usize>,
    pre: Vec<String>,
    post: Vec<String>,
    /// (offset inside the slot, before-note, token)
    interior: Vec<(Frac, bool, String)>,
}

struct Timing {
    lens: Vec<Frac>,
    /// a bar check may follow this measure
    check: Vec<bool>,
    /// the measure fills one LilyPond bar (so `R` is allowed)
    full: Vec<bool>,
    numbers: Vec<String>,
}

struct Writer<'a> {
    score: &'a Score,
    opts: &'a LyOptions,
    diags: &'a mut Diags,
    manual_beams: bool,
    out: String,
}

pub fn write_ly(score: &Score, opts: &LyOptions, diags: &mut Diags) -> String {
    let has_beams = score.parts.iter().any(|p| p.measures.iter().any(|m| m.voices.iter().any(|v| v.events.iter().any(|e| e.beam.is_some()))));
    let manual_beams = opts.beams.unwrap_or(has_beams) && has_beams;
    let mut w = Writer { score, opts, diags, manual_beams, out: String::new() };
    w.run();
    w.out
}

impl<'a> Writer<'a> {
    fn run(&mut self) {
        let score = self.score;
        let n_meas = score.parts.iter().map(|p| p.measures.len()).max().unwrap_or(0);

        // per-part structure items (tempo, marks, jumps) are collected while routing directions
        let mut struct_items: Vec<Vec<(Frac, String)>> = vec![vec![]; n_meas];
        let lens: Vec<Frac> = (0..n_meas)
            .map(|i| score.parts.iter().filter_map(|p| p.measures.get(i)).map(|m| m.len).max().unwrap_or(Frac::int(1)))
            .collect();

        let names = self.part_names();
        let mut all_voices: Vec<Vec<VoiceData>> = vec![];
        for (pi, part) in score.parts.iter().enumerate() {
            let vs = self.build_voices(part, &names[pi], n_meas, &lens, &mut struct_items);
            all_voices.push(vs);
        }
        let (structure, timing) = self.build_structure(n_meas, lens, &struct_items);

        // ---- preamble
        let _ = writeln!(self.out, "\\version {}", ly_string(&self.opts.version));
        self.out.push_str("% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.\n\n");
        self.out.push_str("\\pointAndClickOff\n");
        if let Some(sz) = self.opts.staff_size.or(score.layout.staff_size) {
            if (sz - 20.0).abs() > 0.01 {
                let _ = writeln!(self.out, "#(set-global-staff-size {})", fmt_num(sz));
            }
        }
        self.out.push('\n');
        self.write_header();
        self.write_paper();

        self.out.push_str("% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.\n");
        self.out.push_str("Structure = {\n");
        self.out.push_str(&structure);
        self.out.push_str("}\n\n");

        for (pi, part) in score.parts.iter().enumerate() {
            let mut voices = std::mem::take(&mut all_voices[pi]);
            for v in voices.iter_mut() {
                let text = self.emit_voice(part, v, &timing);
                let _ = writeln!(self.out, "{} = {{", v.var);
                self.out.push_str(&text);
                self.out.push_str("}\n\n");
                for (li, line) in v.lyric_lines.clone().iter().enumerate() {
                    let text = self.emit_lyrics(v, *line, &timing);
                    let _ = writeln!(self.out, "{}Verse{} = \\lyricmode {{", v.var, num_word(li + 1));
                    self.out.push_str(&text);
                    self.out.push_str("}\n\n");
                }
            }
            all_voices[pi] = voices;
            if part.measures.iter().any(|m| !m.harmonies.is_empty()) {
                let text = self.emit_chords(part, &timing);
                let _ = writeln!(self.out, "{}Chords = \\chordmode {{", names[pi]);
                self.out.push_str(&text);
                self.out.push_str("}\n\n");
            }
        }
        self.write_score(&all_voices);
    }

    // ------------------------------------------------------------ names

    fn part_names(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for (i, p) in self.score.parts.iter().enumerate() {
            let src = if !p.name.trim().is_empty() { p.name.clone() } else { p.display_name.clone().unwrap_or_default() };
            let mut name = String::new();
            let mut up = true;
            for c in src.chars() {
                if c.is_ascii_alphabetic() {
                    name.push(if up { c.to_ascii_uppercase() } else { c });
                    up = false;
                } else {
                    up = true;
                }
            }
            if name.len() < 2 || name == "Structure" || out.contains(&name) || name.len() > 24 {
                name = format!("Part{}", num_word(i + 1));
            }
            out.push(name);
        }
        out
    }

    // ------------------------------------------------------------ header, paper

    fn write_header(&mut self) {
        let h = &self.score.header;
        let mut f: Vec<String> = vec![];
        let octavo = self.opts.title_smallcaps;
        if let Some(t) = &h.dedication {
            if octavo {
                // bottom-aligned, so its lines stack upwards, clear of the large title
                let lines: Vec<String> = t.split('\n').map(ly_string).collect();
                f.push(format!("  dedication = \\markup \\italic \\fontsize #-1 \\general-align #Y #DOWN \\center-column {{ {} }}", lines.join(" ")));
            } else {
                f.push(format!("  dedication = {}", markup_lines(t, "center-column")));
            }
        }
        if let Some(t) = &h.title {
            if self.opts.title_smallcaps {
                let lines: Vec<String> = t.split('\n').map(ly_string).collect();
                let body = if lines.len() == 1 { lines[0].clone() } else { format!("\\center-column {{ {} }}", lines.join(" ")) };
                f.push(format!("  title = \\markup \\column {{ \\vspace #1 \\normal-text \\fontsize #5 \\smallCaps {} }}", body));
            } else {
                f.push(format!("  title = {}", markup_lines(t, "center-column")));
            }
        }
        match (&h.subtitle, octavo) {
            // below the large title's descenders
            (Some(t), true) => {
                let lines: Vec<String> = t.split('\n').map(ly_string).collect();
                f.push(format!("  subtitle = \\markup \\column {{ \\vspace #1.4 {} }}", if lines.len() == 1 { lines[0].clone() } else { format!("\\center-column {{ {} }}", lines.join(" ")) }));
            }
            // a spacer: without it the credits ride up beside the large title
            (None, true) if h.title.is_some() => f.push("  subtitle = \\markup \\vspace #0.6".into()),
            (Some(t), false) => f.push(format!("  subtitle = {}", markup_lines(t, "center-column"))),
            _ => {}
        }
        if !h.poet_lines.is_empty() {
            // always a column, so the lines read back as credit lines
            let lines: Vec<String> = h.poet_lines.iter().map(|l| markup_word(l)).collect();
            f.push(format!("  poet = \\markup \\left-column {{ {} }}", lines.join(" ")));
        } else if let Some(t) = &h.poet {
            f.push(format!("  poet = {}", markup_lines(t, "left-column")));
        }
        if !h.composer_lines.is_empty() {
            let lines: Vec<String> = h.composer_lines.iter().map(|l| markup_word(l)).collect();
            f.push(format!("  composer = \\markup \\right-column {{ {} }}", lines.join(" ")));
        } else if let Some(t) = &h.composer {
            f.push(format!("  composer = {}", markup_lines(t, "right-column")));
        }
        if let Some(t) = &h.arranger {
            f.push(format!("  arranger = {}", markup_lines(t, "right-column")));
        }
        if let Some(n) = &h.footnote {
            // The footnote is a paragraph across the page (it wraps), a size smaller, above
            // the centred copyright at the foot of page 1. *spans* are italic.
            let mut words: Vec<String> = vec![];
            let has_span = n.matches('*').count() >= 2;
            for (i, seg) in n.split('*').enumerate() {
                let ws: Vec<String> = seg.split_whitespace().map(ly_string).collect();
                if ws.is_empty() {
                    continue;
                }
                if has_span && i % 2 == 1 {
                    words.push(format!("\\italic {{ {} }}", ws.join(" ")));
                } else {
                    words.extend(ws);
                }
            }
            let mut body = format!("\\fontsize #-1 \\justify {{ {} }}", words.join(" "));
            if let Some(c) = &h.copyright {
                let lines: Vec<String> = c.split('\n').map(ly_string).collect();
                body = format!("{} \\vspace #{} \\fill-line {{ \\center-column {{ {} }} }}", body, if octavo { 4 } else { 1 }, lines.join(" "));
            }
            f.push(format!("  copyright = \\markup \\column {{ {} }}", body));
        } else if let Some(c) = &h.copyright {
            if octavo {
                let lines: Vec<String> = c.split('\n').map(ly_string).collect();
                f.push(format!("  copyright = \\markup \\fill-line {{ \\center-column {{ {} }} }}", lines.join(" ")));
            } else {
                f.push(format!("  copyright = {}", markup_lines(c, "center-column")));
            }
        }
        f.push("  tagline = ##f".into());
        self.out.push_str("\\header {\n");
        self.out.push_str(&f.join("\n"));
        self.out.push_str("\n}\n\n");
    }

    fn write_paper(&mut self) {
        let l = &self.score.layout;
        let mut lines: Vec<String> = vec![];
        let named = |w: f64, h: f64| -> Option<&'static str> {
            for (name, pw, ph) in [("letter", 215.9, 279.4), ("a4", 210.0, 297.0), ("legal", 215.9, 355.6), ("a5", 148.0, 210.0), ("a3", 297.0, 420.0)] {
                if (w - pw).abs() < 1.0 && (h - ph).abs() < 1.0 {
                    return Some(name);
                }
            }
            None
        };
        let dims: Option<(f64, f64)> = match self.opts.paper.as_deref() {
            Some("letter") => Some((215.9, 279.4)),
            Some("a4") => Some((210.0, 297.0)),
            Some("octavo") => Some((177.8, 254.0)),
            Some("remarkable") => Some((157.8, 210.4)),
            Some(other) => {
                let p: Vec<f64> = other.split(|c| c == 'x' || c == 'X').filter_map(|s| s.trim().parse().ok()).collect();
                if p.len() == 2 {
                    Some((p[0], p[1]))
                } else {
                    self.diags.warn(None, None, format!("unknown paper `{}`: using the file's page size", other));
                    l.page_width_mm.zip(l.page_height_mm)
                }
            }
            None => l.page_width_mm.zip(l.page_height_mm),
        };
        if let Some((w, h)) = dims {
            match named(w, h) {
                Some(n) => lines.push(format!("  #(set-paper-size \"{}\")", n)),
                None => {
                    lines.push(format!("  paper-width = {}\\mm", fmt_num(w)));
                    lines.push(format!("  paper-height = {}\\mm", fmt_num(h)));
                }
            }
        }
        if let Some(p) = l.first_page {
            lines.push(format!("  first-page-number = {}", p));
        }
        if let Some(font) = &self.opts.text_font {
            // the font interface changed in LilyPond 2.25
            let v: Vec<u32> = self.opts.version.split('.').filter_map(|x| x.parse().ok()).collect();
            if v.len() >= 2 && (v[0], v[1]) >= (2, 25) {
                lines.push(format!("  property-defaults.fonts.serif = {}", ly_string(font)));
            } else {
                lines.push(format!("  #(define fonts (set-global-fonts #:roman {} #:factor (/ staff-height pt 20)))", ly_string(font)));
            }
        }
        if !lines.is_empty() {
            self.out.push_str("\\paper {\n");
            self.out.push_str(&lines.join("\n"));
            self.out.push_str("\n}\n\n");
        }
    }

    // ------------------------------------------------------------ structure

    fn build_structure(&mut self, n_meas: usize, lens: Vec<Frac>, struct_items: &[Vec<(Frac, String)>]) -> (String, Timing) {
        let score = self.score;
        let mut out = String::new();
        let mut check = vec![false; n_meas];
        let mut full = vec![false; n_meas];
        let mut numbers = vec![String::new(); n_meas];
        // the measure as the first part that has it gives it
        let meas = |i: usize| -> Option<&Measure> { score.parts.iter().filter_map(|p| p.measures.get(i)).next() };
        let any = |i: usize, f: &dyn Fn(&Measure) -> bool| -> bool { score.parts.iter().filter_map(|p| p.measures.get(i)).any(|m| f(m)) };
        let time_at = |i: usize| -> Option<Attr> {
            for p in &score.parts {
                if let Some(m) = p.measures.get(i) {
                    for a in &m.attrs {
                        if matches!(a.kind, Attr::Time { .. }) {
                            return Some(a.kind.clone());
                        }
                    }
                }
            }
            None
        };
        // barline sides merged over parts (the first part that says something wins)
        let side = |i: usize, left: bool| -> Barline {
            let mut b = Barline::default();
            for p in &score.parts {
                if let Some(m) = p.measures.get(i) {
                    let s = if left { &m.left } else { &m.right };
                    if b.style.is_none() {
                        b.style = s.style.clone();
                    }
                    b.repeat |= s.repeat;
                    if b.ending_start.is_none() {
                        b.ending_start = s.ending_start.clone();
                    }
                    if b.ending_stop.is_none() {
                        b.ending_stop = s.ending_stop.clone();
                    }
                }
            }
            b
        };

        let symbols_used = (0..n_meas).filter_map(|i| time_at(i)).any(|t| matches!(t, Attr::Time { symbol: Some(_), .. }));
        let breaks = self.opts.breaks;
        let any_sys = breaks && (1..n_meas).any(|i| any(i, &|m| m.new_system));
        let any_page = breaks && (1..n_meas).any(|i| any(i, &|m| m.new_page));
        if any_sys {
            out.push_str("  \\autoLineBreaksOff\n");
        }
        // A file that fixes its systems and pages is a facsimile: pages break only where it
        // says. With page breaks alone, LilyPond lays out everything in between.
        if any_page && any_sys {
            out.push_str("  \\autoPageBreaksOff\n");
        }

        let mut mlen = Frac::int(1); // LilyPond's measure length
        let mut mpos = Frac::ZERO; // position inside LilyPond's current bar
        let mut barnum: i64 = 1;
        let mut irregular = false;
        let mut volta_open = false;

        for i in 0..n_meas {
            let len = lens[i];
            let m = meas(i);
            numbers[i] = m.map(|m| m.number.clone()).unwrap_or_default();
            let implicit = m.map(|m| m.implicit).unwrap_or(false);
            let mut pre: Vec<String> = vec![];
            let at_boundary = mpos.is_zero();

            // breaks
            if i > 0 && breaks {
                if any(i, &|m| m.new_page) {
                    pre.push("\\pageBreak".into());
                } else if any(i, &|m| m.new_system) {
                    pre.push("\\break".into());
                }
            }
            // barline between the previous bar and this one
            let left = side(i, true);
            let prev_right = if i > 0 { side(i - 1, false) } else { Barline::default() };
            let glyph: Option<String> = match (prev_right.repeat, left.repeat) {
                (true, true) => Some(":..:".into()),
                (true, false) => Some(":|.".into()),
                // a double or final bar that a repeat starts after keeps both
                (false, true) => Some(match prev_right.style.as_deref() {
                    Some("light-light") => ".|:-||".into(),
                    Some("light-heavy") => ".|:-|.".into(),
                    _ => ".|:".into(),
                }),
                _ => bar_glyph(prev_right.style.as_deref()).or(bar_glyph(left.style.as_deref())).map(String::from),
            };
            match glyph {
                Some(g) => pre.push(format!("\\bar {}", ly_string(&g))),
                None if !at_boundary => pre.push("\\bar \"|\"".into()),
                None => {}
            }
            // volta brackets
            let mut cmds: Vec<String> = vec![];
            if prev_right.ending_stop.is_some() && volta_open {
                cmds.push("(volta #f)".into());
                volta_open = false;
            }
            if let Some(e) = &left.ending_start {
                if volta_open {
                    cmds.push("(volta #f)".into());
                }
                // an ending the source leaves open on the right ("discontinue") has no closing hook
                let open_end = (i..n_meas).map(|j| side(j, false).ending_stop).find(|s| s.is_some()).flatten().map(|s| s.open).unwrap_or(false);
                if open_end {
                    pre.push("\\once \\override Score.VoltaBracket.edge-height = #'(2.0 . 0)".into());
                }
                let text = e.text.clone().unwrap_or_else(|| format!("{}.", e.number));
                cmds.push(format!("(volta {})", ly_string(&text)));
                volta_open = true;
            }
            if !cmds.is_empty() {
                pre.push(format!("\\set Score.repeatCommands = #'({})", cmds.join(" ")));
            }
            // time signature
            let time = time_at(i);
            let mut time_here = false;
            if let Some(Attr::Time { beats, beat_type, symbol, print }) = &time {
                time_here = true;
                if !*print {
                    pre.push("\\once \\omit Score.TimeSignature".into());
                }
                if symbols_used && symbol.is_none() && ((beats == "4" && *beat_type == 4) || (beats == "2" && *beat_type == 2)) {
                    pre.push("\\once \\override Score.TimeSignature.style = #'numbered".into());
                }
                if beats.contains('+') {
                    let parts: Vec<&str> = beats.split('+').map(|s| s.trim()).collect();
                    pre.push(format!("\\compoundMeter #'(({} {}))", parts.join(" "), beat_type));
                } else {
                    pre.push(format!("\\time {}/{}", beats, beat_type));
                }
                if let Some(l) = time.as_ref().and_then(|t| t.measure_len()) {
                    mlen = l;
                }
                if !at_boundary {
                    self.diags.warn(None, Some(&numbers[i]), "time signature changes inside a bar");
                    mpos = Frac::ZERO;
                }
            }
            // how this measure sits in LilyPond's bars
            let mut counted = false;
            if mpos.is_zero() {
                if irregular {
                    if !time_here {
                        pre.push(format!("\\set Timing.measureLength = #(ly:make-moment {})", moment(mlen)));
                    }
                    irregular = false;
                }
                // bar number
                if !implicit && !(i == 0 && len < mlen) {
                    if let Ok(n) = numbers[i].trim().parse::<i64>() {
                        if n != barnum {
                            pre.push(format!("\\set Score.currentBarNumber = #{}", n));
                            barnum = n;
                        }
                    }
                }
                if len == mlen {
                    full[i] = true;
                    check[i] = true;
                    counted = true;
                } else if i == 0 && len < mlen {
                    pre.push(format!("\\partial {}", dur_token(len)));
                    check[i] = true;
                } else if i + 1 == n_meas && len < mlen {
                    // a short last bar needs nothing
                } else if len < mlen && self.completes(i, &lens, mlen, &time_at) {
                    mpos = len;
                } else {
                    if !implicit && time.as_ref().map(|t| matches!(t, Attr::Time { print: true, .. })).unwrap_or(true) {
                        self.diags.warn(None, Some(&numbers[i]), format!("bar holds {} of a whole note but the time signature gives {}", len, mlen));
                    }
                    pre.push(format!("\\set Timing.measureLength = #(ly:make-moment {})", moment(len)));
                    irregular = true;
                    full[i] = true;
                    check[i] = true;
                    counted = true;
                }
            } else {
                mpos += len;
                if mpos >= mlen {
                    mpos = Frac::ZERO;
                    check[i] = true;
                    counted = true;
                }
            }
            if counted {
                barnum += 1;
            }

            // positioned items
            let mut items: Vec<(Frac, String)> = struct_items[i].clone();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            let mut line = String::from(" ");
            for p in &pre {
                line.push(' ');
                line.push_str(p);
            }
            let mut cursor = Frac::ZERO;
            for (pos, tok) in &items {
                let pos = if *pos > len { len } else { *pos };
                if pos > cursor {
                    let _ = write!(line, " s{}", dur_token(pos - cursor));
                    cursor = pos;
                }
                line.push(' ');
                line.push_str(tok);
            }
            if len > cursor {
                let _ = write!(line, " s{}", dur_token(len - cursor));
            }
            // closing barline of the piece
            if i + 1 == n_meas {
                let right = side(i, false);
                if right.ending_stop.is_some() && volta_open {
                    line.push_str(" \\set Score.repeatCommands = #'((volta #f))");
                }
                let g = if right.repeat { Some(":|.") } else { bar_glyph(right.style.as_deref()) };
                if let Some(g) = g {
                    let _ = write!(line, " \\bar {}", ly_string(g));
                }
            }
            if check[i] && i + 1 < n_meas {
                line.push_str(" |");
            }
            let _ = writeln!(out, "{} % {}", line, numbers[i]);
        }
        (out, Timing { lens, check, full, numbers })
    }

    /// do the bars from `i` on add up to exactly one bar of the meter (a bar split in two)?
    fn completes(&self, i: usize, lens: &[Frac], mlen: Frac, time_at: &dyn Fn(usize) -> Option<Attr>) -> bool {
        let mut sum = lens[i];
        let mut j = i + 1;
        while sum < mlen && j < lens.len() {
            if time_at(j).is_some() {
                return false;
            }
            sum += lens[j];
            j += 1;
        }
        sum == mlen
    }

    // ------------------------------------------------------------ voices

    fn build_voices(&mut self, part: &Part, base: &str, n_meas: usize, lens: &[Frac], struct_items: &mut [Vec<(Frac, String)>]) -> Vec<VoiceData> {
        // which voices exist, per staff
        let mut keys: Vec<VKey> = vec![];
        for m in &part.measures {
            for v in &m.voices {
                if !keys.contains(&(v.staff, v.voice)) {
                    keys.push((v.staff, v.voice));
                }
            }
        }
        // every staff has at least one voice, so clefs and keys have a home
        for s in 1..=part.staves.max(1) {
            if !keys.iter().any(|k| k.0 == s) {
                keys.push((s, 1000 + s));
            }
        }
        keys.sort();
        let mut voices: Vec<VoiceData> = vec![];
        for &key in &keys {
            let on_staff: Vec<VKey> = keys.iter().filter(|k| k.0 == key.0).cloned().collect();
            let rank = on_staff.iter().position(|k| *k == key).unwrap();
            let mut var = base.to_string();
            if part.staves > 1 {
                var.push_str(match (part.staves, key.0) {
                    (2, 1) => "Upper",
                    (2, 2) => "Lower",
                    _ => "Staff",
                });
                if part.staves > 2 {
                    var.push_str(&num_word(key.0 as usize));
                }
            }
            if on_staff.len() > 1 {
                var.push_str("Voice");
                var.push_str(&num_word(rank + 1));
            }
            var.push_str("Music");
            let mut measures: Vec<Vec<Event>> = Vec::with_capacity(n_meas);
            for i in 0..n_meas {
                let evs = part.measures.get(i).and_then(|m| m.voices.iter().find(|v| (v.staff, v.voice) == key)).map(|v| v.events.clone()).unwrap_or_default();
                measures.push(evs);
            }
            voices.push(VoiceData {
                key,
                var,
                rank,
                staff_voices: on_staff.len(),
                primary: rank == 0,
                measures,
                items: vec![vec![]; n_meas],
                lyric_lines: vec![],
                has_grace: false,
            });
        }

        // forced accidentals
        let defaults = default_accidentals(part);
        for (mi, m) in part.measures.iter().enumerate() {
            for (vi, v) in m.voices.iter().enumerate() {
                let vd = voices.iter_mut().find(|x| x.key == (v.staff, v.voice)).unwrap();
                for (ei, e) in vd.measures[mi].iter_mut().enumerate() {
                    if let EventKind::Notes(ns) = &mut e.kind {
                        for (ni, n) in ns.iter_mut().enumerate() {
                            // acc_shown now means "force it": the source prints one LilyPond would not
                            n.acc_shown = n.acc_shown && !defaults[mi][vi][ei][ni];
                        }
                    }
                }
            }
        }

        for vd in voices.iter_mut() {
            self.clean_voice(part, vd, lens);
        }
        self.route(part, &mut voices, lens, struct_items);
        voices
    }

    /// Remove what LilyPond cannot take: overlapping notes, unpaired slurs, beams and ties.
    fn clean_voice(&mut self, part: &Part, vd: &mut VoiceData, lens: &[Frac]) {
        let pid = part.id.as_str();
        let num = |i: usize| part.measures.get(i).map(|m| m.number.clone()).unwrap_or_default();
        // overlaps
        for (mi, evs) in vd.measures.iter_mut().enumerate() {
            let mut cursor = Frac::ZERO;
            let mut keep: Vec<Event> = Vec::with_capacity(evs.len());
            for e in evs.drain(..) {
                if e.pos < cursor {
                    self.diags.dropped(Some(pid), Some(&num(mi)), format!("note or rest overlapping another in voice {} (staff {})", vd.key.1, vd.key.0));
                    continue;
                }
                cursor = e.pos + e.dur;
                keep.push(e);
            }
            *evs = keep;
        }
        // slurs: pair starts and stops
        let mut open: HashMap<u8, (usize, usize)> = HashMap::new();
        let mut kill_start: Vec<(usize, usize, u8)> = vec![];
        let mut swallowed: std::collections::HashSet<u8> = Default::default();
        for mi in 0..vd.measures.len() {
            for ei in 0..vd.measures[mi].len() {
                let e = &mut vd.measures[mi][ei];
                let stops = std::mem::take(&mut e.slur_stops);
                for s in stops {
                    if swallowed.remove(&s) {
                        // the second curve of a double slur: one `)` ends both
                    } else if open.remove(&s).is_some() {
                        e.slur_stops.push(s);
                    } else {
                        self.diags.dropped(Some(pid), Some(&num(mi)), "slur end without a start in the same voice");
                    }
                }
                let mut starts = std::mem::take(&mut e.slur_starts);
                if e.double_slur && starts.len() >= 2 {
                    // LilyPond draws both curves from one slur with doubleSlurs set
                    for extra in starts.drain(1..) {
                        swallowed.insert(extra);
                    }
                }
                for s in starts {
                    if open.contains_key(&s) {
                        self.diags.dropped(Some(pid), Some(&num(mi)), "slur start while the same slur is still open");
                    } else {
                        open.insert(s, (mi, ei));
                        e.slur_starts.push(s);
                    }
                }
            }
        }
        for (s, (mi, ei)) in open {
            kill_start.push((mi, ei, s));
        }
        kill_start.sort();
        for (mi, ei, s) in kill_start {
            vd.measures[mi][ei].slur_starts.retain(|x| *x != s);
            self.diags.dropped(Some(pid), Some(&num(mi)), "slur start without an end in the same voice");
        }
        // ties: the next note in the voice must have the pitch
        let flat: Vec<(usize, usize)> =
            vd.measures.iter().enumerate().flat_map(|(mi, evs)| (0..evs.len()).map(move |ei| (mi, ei))).collect();
        // A tie that leads nowhere (into a repeat, another voice) is written \laissezVibrer,
        // a tie arriving from nowhere (second ending) \repeatTie.
        let adjacent = |a: (usize, usize), b: (usize, usize), ms: &Vec<Vec<Event>>| -> bool {
            let (ea, eb) = (&ms[a.0][a.1], &ms[b.0][b.1]);
            if a.0 == b.0 {
                ea.pos + ea.dur == eb.pos
            } else {
                b.0 == a.0 + 1 && eb.pos.is_zero() && ea.pos + ea.dur >= lens[a.0]
            }
        };
        let snapshot: Vec<Vec<(i32, bool)>> = flat.iter().map(|&(a, b)| vd.measures[a][b].notes().iter().map(|n| (n.midi(), n.tie_start)).collect()).collect();
        for w in 0..flat.len() {
            let (mi, ei) = flat[w];
            let next: Vec<i32> = match flat.get(w + 1) {
                Some(&nx) if adjacent(flat[w], nx, &vd.measures) => snapshot[w + 1].iter().map(|x| x.0).collect(),
                _ => vec![],
            };
            let prev: Vec<i32> = if w > 0 && adjacent(flat[w - 1], flat[w], &vd.measures) {
                snapshot[w - 1].iter().filter(|x| x.1).map(|x| x.0).collect()
            } else {
                vec![]
            };
            let pos = vd.measures[mi][ei].pos;
            let (mut lv, mut rt) = (false, false);
            if let EventKind::Notes(ns) = &mut vd.measures[mi][ei].kind {
                for n in ns.iter_mut() {
                    if n.tie_start && !next.contains(&n.midi()) {
                        n.tie_start = false;
                        lv = true;
                    }
                    if n.tie_stop && !prev.contains(&n.midi()) {
                        rt = true;
                    }
                }
            }
            let is_grace = vd.measures[mi][ei].grace.is_some();
            if lv && is_grace {
                self.diags.dropped(Some(pid), Some(&num(mi)), "tie from a grace note to a different pitch");
            } else if lv {
                vd.items[mi].push((pos, false, "\\laissezVibrer".into()));
            }
            if rt && !is_grace {
                vd.items[mi].push((pos, false, "\\repeatTie".into()));
            }
        }
        // beams: begin ... end pairs on notes that can carry a beam
        if self.manual_beams {
            let mut open_at: Option<(usize, usize)> = None;
            for mi in 0..vd.measures.len() {
                for ei in 0..vd.measures[mi].len() {
                    let e = &mut vd.measures[mi][ei];
                    if e.grace.is_some() {
                        e.beam = None;
                        continue;
                    }
                    let beamable = e.ntype.map(|t| t.log >= 3).unwrap_or(e.dur < Frac::new(1, 4)) && !e.is_rest();
                    match e.beam {
                        Some(Beam::Begin) if beamable && open_at.is_none() => open_at = Some((mi, ei)),
                        Some(Beam::End) if beamable && open_at.is_some() => open_at = None,
                        Some(Beam::Continue) if open_at.is_some() => {}
                        _ => {
                            let kill = !beamable && open_at.is_some() && !e.is_rest();
                            e.beam = None;
                            if kill {
                                // a long note inside a beam: drop the beam that was open
                                let (a, b) = open_at.take().unwrap();
                                vd.measures[a][b].beam = None;
                            }
                        }
                    }
                }
            }
            if let Some((a, b)) = open_at {
                vd.measures[a][b].beam = None;
            }
        } else {
            for evs in vd.measures.iter_mut() {
                for e in evs.iter_mut() {
                    e.beam = None;
                }
            }
        }
        // lyric lines, grace notes
        let mut lines: Vec<u32> = vec![];
        for evs in &vd.measures {
            for e in evs {
                if e.grace.is_some() {
                    vd.has_grace = true;
                }
                for l in &e.lyrics {
                    if e.is_rest() {
                        continue;
                    }
                    if !lines.contains(&l.line) {
                        lines.push(l.line);
                    }
                }
            }
        }
        lines.sort();
        vd.lyric_lines = lines;
        for (mi, evs) in vd.measures.iter().enumerate() {
            for e in evs {
                if e.is_rest() && !e.lyrics.is_empty() {
                    let words: Vec<&str> = e.lyrics.iter().map(|l| l.text.as_str()).collect();
                    self.diags.dropped(Some(pid), Some(&num(mi)), format!("words on a rest: {}", words.join(" ")));
                }
            }
        }
    }

    /// Send every direction and attribute change to the voice (or the Structure) that carries it.
    fn route(&mut self, part: &Part, voices: &mut [VoiceData], lens: &[Frac], struct_items: &mut [Vec<(Frac, String)>]) {
        let pid = part.id.as_str();
        let n_meas = lens.len();
        let mut wedge_voice: HashMap<u32, VKey> = HashMap::new();
        for (mi, m) in part.measures.iter().enumerate() {
            let len = lens[mi];
            // clefs and keys go to the first voice of the staff
            for a in &m.attrs {
                match &a.kind {
                    Attr::Key { fifths, minor } => {
                        for v in voices.iter_mut().filter(|v| v.primary) {
                            v.items[mi].push((a.pos, true, key_token(*fifths, *minor)));
                        }
                    }
                    Attr::Clef { staff, sign, line, octave } => {
                        if let Some(v) = voices.iter_mut().find(|v| v.primary && v.key.0 == *staff) {
                            let mut tok = format!("\\clef {}", ly_string(&clef_name(*sign, *line, *octave)));
                            // unpitched notes give their place as on a treble staff
                            let unpitched = part.measures.iter().any(|m| m.voices.iter().any(|v| v.events.iter().any(|e| e.unpitched)));
                            if *sign == 'P' && unpitched {
                                tok.push_str(" \\set Staff.middleCPosition = #-6");
                            }
                            v.items[mi].push((a.pos, true, tok));
                        }
                    }
                    Attr::Time { .. } => {}
                }
            }
            for d in &m.directions {
                // score-level marks
                let st: Option<String> = match &d.kind {
                    DirKind::Tempo { text, metronome } => Some(tempo_token(text.as_deref(), metronome.as_ref())),
                    DirKind::Rehearsal(t) => Some(format!("\\mark \\markup \\box {}", ly_string(t))),
                    // plain marks: they print at bar 1 and, at a line break, on the new line
                    DirKind::Segno => Some("\\mark \\markup { \\musicglyph \"scripts.segno\" }".into()),
                    DirKind::Coda => Some("\\mark \\markup { \\musicglyph \"scripts.coda\" }".into()),
                    DirKind::Words { text, .. } if d.pos >= len && is_jump_text(text) => Some(format!("\\jump {}", ly_string(text))),
                    _ => None,
                };
                if let Some(tok) = st {
                    let pos = if d.pos > len { len } else { d.pos };
                    let is_tempo = tok.starts_with("\\tempo");
                    let clash = struct_items[mi].iter().any(|(p, t)| *p == pos && (t == &tok || (is_tempo && t.starts_with("\\tempo"))));
                    if !clash {
                        struct_items[mi].push((pos, tok));
                    } else if !struct_items[mi].iter().any(|(p, t)| *p == pos && t == &tok) {
                        self.diags.dropped(Some(pid), Some(&m.number), "a second, different tempo mark at the same moment");
                    }
                    continue;
                }
                // voice-level marks
                let (pre, tok): (bool, String) = match &d.kind {
                    DirKind::Dynamic(name) => {
                        let dir = if d.placement == Some(Placement::Above) { "^" } else { "" };
                        if DYNAMICS.contains(&name.as_str()) {
                            (false, format!("{}\\{}", dir, name))
                        } else if !name.is_empty() && name.chars().all(|c| "pfmszrn".contains(c)) {
                            (false, format!("{}\\markup {{ \\dynamic {} }}", if dir.is_empty() { "_" } else { "^" }, ly_string(name)))
                        } else {
                            (false, format!("{}\\markup {{ \\italic {} }}", if dir.is_empty() { "_" } else { "^" }, ly_string(name)))
                        }
                    }
                    DirKind::Wedge(w) => {
                        let dir = if d.placement == Some(Placement::Above) { "^" } else { "" };
                        match w {
                            Wedge::Cresc => (false, format!("{}\\<", dir)),
                            Wedge::Dim => (false, format!("{}\\>", dir)),
                            Wedge::Stop => (false, "\\!".into()),
                        }
                    }
                    DirKind::Words { text, italic, bold } => {
                        let dir = if d.placement == Some(Placement::Below) { "_" } else { "^" };
                        let lines: Vec<String> = text.split('\n').map(ly_string).collect();
                        let body = if lines.len() == 1 { lines[0].clone() } else { format!("\\column {{ {} }}", lines.join(" ")) };
                        let style = format!("{}{}", if *bold { "\\bold " } else { "" }, if *italic { "\\italic " } else { "" });
                        (false, format!("{}\\markup {{ {}{} }}", dir, style, body))
                    }
                    DirKind::Ottava(n) => (true, format!("\\ottava #{}", n)),
                    DirKind::Pedal(p) => (
                        false,
                        match p {
                            Pedal::Start => "\\sustainOn".into(),
                            Pedal::Stop => "\\sustainOff".into(),
                            Pedal::Change => "\\sustainOff\\sustainOn".into(),
                        },
                    ),
                    _ => continue,
                };
                // which voice
                let staff = if voices.iter().any(|v| v.key.0 == d.staff) { d.staff } else { 1 };
                let mut key: Option<VKey> = d.voice.map(|v| (staff, v)).filter(|k| voices.iter().any(|v| v.key == *k));
                // Two dynamics at one moment, one above and one below a staff with two voices:
                // the lower one belongs to the lower voice (one voice cannot carry both).
                if key.is_none() && d.placement == Some(Placement::Below) && matches!(d.kind, DirKind::Dynamic(_)) {
                    let twin = m.directions.iter().any(|o| {
                        o.pos == d.pos && o.staff == d.staff && o.voice.is_none() && o.placement == Some(Placement::Above) && matches!(o.kind, DirKind::Dynamic(_))
                    });
                    let mut on_staff: Vec<VKey> = m.voices.iter().filter(|v| v.staff == staff).map(|v| (v.staff, v.voice)).collect();
                    on_staff.sort();
                    if twin && on_staff.len() >= 2 {
                        key = Some(on_staff[1]);
                    }
                }
                if key.is_none() {
                    key = m.voices.iter().filter(|v| v.staff == staff).map(|v| (v.staff, v.voice)).min();
                }
                if key.is_none() {
                    key = voices.iter().find(|v| v.primary && v.key.0 == staff).map(|v| v.key);
                }
                let mut key = match key {
                    Some(k) => k,
                    None => {
                        self.diags.dropped(Some(pid), Some(&m.number), format!("direction on a staff without music: {}", tok));
                        continue;
                    }
                };
                match &d.kind {
                    DirKind::Wedge(Wedge::Stop) => match wedge_voice.remove(&staff) {
                        Some(k) => key = k,
                        None => {
                            self.diags.dropped(Some(pid), Some(&m.number), "hairpin end without a start");
                            continue;
                        }
                    },
                    DirKind::Wedge(_) => {
                        wedge_voice.insert(staff, key);
                    }
                    _ => {}
                }
                // marks at the very end of the bar belong to the next bar's first moment
                let rehome = d.pos >= len && matches!(d.kind, DirKind::Wedge(Wedge::Stop) | DirKind::Pedal(_) | DirKind::Dynamic(_) | DirKind::Ottava(_));
                let vd = voices.iter_mut().find(|v| v.key == key).unwrap();
                if rehome && mi + 1 < n_meas {
                    vd.items[mi + 1].insert(0, (Frac::ZERO, pre, tok));
                } else {
                    vd.items[mi].push((d.pos, pre, tok));
                }
            }
        }
    }

    fn emit_voice(&mut self, part: &Part, vd: &VoiceData, timing: &Timing) -> String {
        let mut out = String::new();
        let mut mode: Option<&str> = None;
        let n_meas = timing.lens.len();
        for mi in 0..n_meas {
            let len = timing.lens[mi];
            let evs = &vd.measures[mi];
            let mut line = String::from(" ");

            // stem direction mode
            if vd.staff_voices > 1 {
                let others = part.measures.get(mi).map(|m| m.voices.iter().filter(|v| v.staff == vd.key.0 && v.voice != vd.key.1).count()).unwrap_or(0);
                let want = if others == 0 || evs.is_empty() {
                    "\\oneVoice"
                } else {
                    ["\\voiceOne", "\\voiceTwo", "\\voiceThree", "\\voiceFour"][vd.rank % 4]
                };
                if mode != Some(want) && !evs.is_empty() {
                    line.push(' ');
                    line.push_str(want);
                    mode = Some(want);
                }
            }

            // slots: events and the gaps between them
            let mut slots: Vec<Slot> = vec![];
            let mut cursor = Frac::ZERO;
            for (ei, e) in evs.iter().enumerate() {
                if e.pos > cursor {
                    slots.push(Slot { pos: cursor, dur: e.pos - cursor, ev: None, pre: vec![], post: vec![], interior: vec![] });
                }
                slots.push(Slot { pos: e.pos, dur: e.dur, ev: Some(ei), pre: vec![], post: vec![], interior: vec![] });
                cursor = e.pos + e.dur;
            }
            if cursor < len {
                slots.push(Slot { pos: cursor, dur: len - cursor, ev: None, pre: vec![], post: vec![], interior: vec![] });
            } else if cursor > len {
                self.diags.warn(Some(&part.id), Some(&timing.numbers[mi]), format!("voice {} is longer than the bar", vd.key.1));
            }
            let mut tail: Vec<String> = vec![];
            for (pos, pre, tok) in &vd.items[mi] {
                attach(&mut slots, *pos, *pre, tok.clone(), &mut tail);
            }

            // emit
            let whole_bar_rest = evs.len() == 1 && matches!(evs[0].kind, EventKind::Rest { whole_measure: true }) && timing.full[mi];
            // grace notes after the last note of the bar hang on that note: \afterGrace
            let is_grace_slot = |s: &Slot| s.ev.map(|i| evs[i].grace.is_some()).unwrap_or(false);
            let mut after_grace: Option<usize> = None; // slot of the note that carries them
            if let Some(last) = slots.last() {
                if is_grace_slot(last) {
                    let mut g0 = slots.len() - 1;
                    while g0 > 0 && is_grace_slot(&slots[g0 - 1]) {
                        g0 -= 1;
                    }
                    if g0 > 0 {
                        let p = &slots[g0 - 1];
                        let main_ok = p.ev.map(|i| evs[i].tuplet.is_none()).unwrap_or(false) && slots[g0..].iter().all(|s| s.pre.is_empty());
                        if main_ok {
                            after_grace = Some(g0 - 1);
                        }
                    }
                }
            }
            let mut tup: Option<(Tuplet, bool, Frac)> = None; // ratio, explicit, sounding length so far
            let mut in_grace = false;
            let n_slots = slots.len();
            for si in 0..n_slots {
                let s = &slots[si];
                let ev = s.ev.map(|i| &evs[i]);
                let is_grace = ev.map(|e| e.grace.is_some()).unwrap_or(false);
                if in_grace && !is_grace {
                    line.push_str(" }");
                    in_grace = false;
                }
                // tuplet boundaries
                let this_tup = ev.and_then(|e| if e.grace.is_some() { tup.map(|t| t.0) } else { e.tuplet });
                if let Some((cur, _, _)) = tup {
                    if this_tup != Some(cur) {
                        line.push_str(" }");
                        tup = None;
                    }
                }
                for p in &s.pre {
                    line.push(' ');
                    line.push_str(p);
                }
                if ev.map(|e| e.arts.contains(&Art::GlissandoWavy)).unwrap_or(false) {
                    line.push_str(" \\once \\override Glissando.style = #'zigzag");
                }
                if let Some(e) = ev {
                    if e.double_slur && !e.slur_starts.is_empty() {
                        line.push_str(" \\once \\set doubleSlurs = ##t");
                    }
                    if !e.slur_starts.is_empty() {
                        match e.slur_style {
                            1 => line.push_str(" \\once \\slurDashed"),
                            2 => line.push_str(" \\once \\slurDotted"),
                            _ => {}
                        }
                    }
                    if e.notes().iter().any(|n| n.tie_start) {
                        match e.tie_style {
                            1 => line.push_str(" \\once \\tieDashed"),
                            2 => line.push_str(" \\once \\tieDotted"),
                            _ => {}
                        }
                    }
                }
                if tup.is_none() {
                    if let (Some(t), Some(e)) = (this_tup, ev) {
                        if !e.tuplet_number {
                            line.push_str(" \\once \\omit TupletNumber");
                        }
                        if e.tuplet_bracket == Some(false) || !e.tuplet_number {
                            line.push_str(" \\once \\omit TupletBracket");
                        }
                        let _ = write!(line, " \\tuplet {}/{} {{", t.actual, t.normal);
                        tup = Some((t, e.tuplet_start, Frac::ZERO));
                    }
                }
                if is_grace && !in_grace {
                    let slash = ev.and_then(|e| e.grace).map(|g| g.slash).unwrap_or(false);
                    if after_grace.map(|p| si > p).unwrap_or(false) {
                        line.push_str(" {");
                    } else {
                        line.push_str(if slash { " \\slashedGrace {" } else { " \\grace {" });
                    }
                    in_grace = true;
                }
                let scale = tup.map(|t| Frac::new(t.0.actual as i64, t.0.normal as i64)).unwrap_or(Frac::int(1));
                if after_grace == Some(si) {
                    line.push_str(" \\afterGrace");
                }
                let body = match ev {
                    None => format!("s{}", dur_token(s.dur * scale)),
                    Some(e) => self.event_token(e, whole_bar_rest, len),
                };
                let mut posts = s.post.concat();
                if let Some(e) = ev {
                    posts.push_str(&event_posts(e));
                }
                if s.interior.is_empty() {
                    let _ = write!(line, " {}{}", body, posts);
                } else {
                    // marks that fall inside the note: parallel skips in the same voice
                    let mut inner = String::new();
                    let mut items = s.interior.clone();
                    items.sort_by(|a, b| a.0.cmp(&b.0));
                    let mut at = Frac::ZERO;
                    let mut pending_post = String::new();
                    let mut k = 0;
                    while k < items.len() {
                        let off = items[k].0;
                        let _ = write!(inner, " s{}{}", dur_token((off - at) * scale), pending_post);
                        pending_post.clear();
                        while k < items.len() && items[k].0 == off {
                            if items[k].1 {
                                inner.push(' ');
                                inner.push_str(&items[k].2);
                            } else {
                                pending_post.push_str(&items[k].2);
                            }
                            k += 1;
                        }
                        at = off;
                    }
                    let _ = write!(inner, " s{}{}", dur_token((s.dur - at) * scale), pending_post);
                    let _ = write!(line, " << {}{} {{{} }} >>", body, posts, inner);
                }
                if let Some(e) = ev {
                    for a in &e.arts {
                        match a {
                            Art::Breath => line.push_str(" \\breathe"),
                            Art::Caesura => line.push_str(" \\once \\override BreathingSign.text = \\markup \\musicglyph \"scripts.caesura.straight\" \\breathe"),
                            _ => {}
                        }
                    }
                    // close a tuplet
                    if e.grace.is_none() {
                        if let Some((t, explicit, acc)) = tup.as_mut() {
                            *acc += e.dur;
                            let complete = if *explicit {
                                e.tuplet_stop
                            } else {
                                // a whole group: the sounding length is a plain note value again
                                let d = acc.d;
                                d & (d - 1) == 0 && {
                                    let unit = e.ntype.map(|x| NoteType { log: x.log, dots: 0 }.len()).unwrap_or(Frac::new(1, 8));
                                    let group = unit * Frac::int(t.normal as i64);
                                    (*acc / group).d == 1
                                }
                            };
                            if complete {
                                line.push_str(" }");
                                tup = None;
                            }
                        }
                    }
                } else if tup.is_some() {
                    // a gap inside a tuplet ends it
                }
            }
            if in_grace {
                line.push_str(" }");
            }
            if tup.is_some() {
                line.push_str(" }");
            }
            for t in &tail {
                line.push(' ');
                line.push_str(t);
            }
            if timing.check[mi] && mi + 1 < n_meas {
                line.push_str(" |");
            }
            let _ = writeln!(out, "{} % {}", line, timing.numbers[mi]);
        }
        out
    }

    fn emit_chords(&mut self, part: &Part, timing: &Timing) -> String {
        let name = |(step, alter): (u8, i8)| pitch_token(&Note::new(step, alter, 3));
        let mut out = String::new();
        let n_meas = timing.lens.len();
        for mi in 0..n_meas {
            let len = timing.lens[mi];
            let mut hs: Vec<&Harmony> = part.measures.get(mi).map(|m| m.harmonies.iter().collect()).unwrap_or_default();
            hs.retain(|h| h.pos < len);
            hs.sort_by(|a, b| a.pos.cmp(&b.pos));
            hs.dedup_by(|b, a| a.pos == b.pos);
            let mut line = String::from(" ");
            let mut cursor = Frac::ZERO;
            for (i, h) in hs.iter().enumerate() {
                if h.pos > cursor {
                    let _ = write!(line, " s{}", dur_token(h.pos - cursor));
                }
                let end = hs.get(i + 1).map(|n| n.pos).unwrap_or(len);
                let dur = dur_token(end - h.pos);
                cursor = end;
                if h.kind == "none" {
                    let _ = write!(line, " r{}", dur);
                    continue;
                }
                let base = match h.kind.as_str() {
                    "major" => "5",
                    "minor" => "m",
                    "augmented" => "aug",
                    "diminished" => "dim",
                    "dominant" => "7",
                    "major-seventh" => "maj7",
                    "minor-seventh" => "m7",
                    "diminished-seventh" => "dim7",
                    "augmented-seventh" => "aug7",
                    "half-diminished" => "m7.5-",
                    "major-minor" => "m7+",
                    "major-sixth" => "6",
                    "minor-sixth" => "m6",
                    "dominant-ninth" => "9",
                    "major-ninth" => "maj9",
                    "minor-ninth" => "m9",
                    "dominant-11th" => "11",
                    "major-11th" => "maj11",
                    "minor-11th" => "m11",
                    "dominant-13th" => "13.11",
                    "major-13th" => "maj13.11",
                    "minor-13th" => "m13.11",
                    "suspended-second" => "sus2",
                    "suspended-fourth" => "sus4",
                    "power" => "1.5",
                    other => {
                        let num = part.measures[mi].number.clone();
                        self.diags.warn(Some(&part.id), Some(&num), format!("chord kind `{}` written as a major chord", other));
                        "5"
                    }
                };
                let mut m = base.to_string();
                let mut removed: Vec<String> = vec![];
                for (deg, alt, kind) in &h.degrees {
                    let a = match alt {
                        1 => "+",
                        -1 => "-",
                        _ => "",
                    };
                    if *kind == 's' {
                        removed.push(deg.to_string());
                    } else {
                        let _ = write!(m, ".{}{}", deg, a);
                    }
                }
                if !removed.is_empty() {
                    let _ = write!(m, "^{}", removed.join("."));
                }
                let m = if m == "5" { String::new() } else { format!(":{}", m) };
                let bass = h.bass.map(|b| format!("/{}", name(b))).unwrap_or_default();
                let _ = write!(line, " {}{}{}{}", name(h.root), dur, m, bass);
            }
            if cursor < len {
                let _ = write!(line, " s{}", dur_token(len - cursor));
            }
            if timing.check[mi] && mi + 1 < n_meas {
                line.push_str(" |");
            }
            let _ = writeln!(out, "{} % {}", line, timing.numbers[mi]);
        }
        out
    }

    fn event_token(&mut self, e: &Event, whole_bar_rest: bool, bar_len: Frac) -> String {
        let scale = e.tuplet.map(|t| Frac::new(t.normal as i64, t.actual as i64)).unwrap_or(Frac::int(1));
        let dur = if e.grace.is_some() {
            type_token(e.ntype.unwrap_or(NoteType { log: 3, dots: 0 }))
        } else {
            match e.ntype {
                Some(t) if t.len() * scale == e.dur => type_token(t),
                _ => dur_token(e.dur / scale),
            }
        };
        let mut s = String::new();
        match &e.kind {
            EventKind::Rest { .. } => {
                if whole_bar_rest {
                    let _ = write!(s, "R{}", dur_token(bar_len));
                } else {
                    let _ = write!(s, "r{}", dur);
                }
            }
            EventKind::Notes(ns) => {
                let acc = |n: &Note| if n.acc_caut { "?" } else if n.acc_shown { "!" } else { "" };
                if ns.len() == 1 {
                    let _ = write!(s, "{}{}{}", pitch_token(&ns[0]), acc(&ns[0]), dur);
                    if ns[0].tie_start {
                        s.push('~');
                    }
                } else {
                    let all_tied = ns.iter().all(|n| n.tie_start);
                    s.push('<');
                    for (i, n) in ns.iter().enumerate() {
                        if i > 0 {
                            s.push(' ');
                        }
                        let _ = write!(s, "{}{}", pitch_token(n), acc(n));
                        if n.tie_start && !all_tied {
                            s.push('~');
                        }
                    }
                    s.push('>');
                    s.push_str(&dur);
                    if all_tied {
                        s.push('~');
                    }
                }
                for a in &e.arts {
                    if let Art::Tremolo(n) = a {
                        let log = e.ntype.map(|t| t.log.max(2)).unwrap_or(2) as u32;
                        let _ = write!(s, ":{}", 1u32 << (log + *n as u32).min(8));
                    }
                }
                if e.notehead_x {
                    s = format!("\\xNote {}", s);
                }
            }
        }
        s
    }

    fn emit_lyrics(&mut self, vd: &VoiceData, line_no: u32, timing: &Timing) -> String {
        // one entry per note LilyPond will count
        struct Entry {
            mi: usize,
            tok: Option<String>,
            extend: bool,
            after_rest: bool,
        }
        let mut entries: Vec<Entry> = vec![];
        let mut after_rest = false;
        for (mi, evs) in vd.measures.iter().enumerate() {
            for e in evs {
                if e.is_rest() {
                    after_rest = true;
                    continue;
                }
                let ly = e.lyrics.iter().find(|l| l.line == line_no);
                let tok = ly.map(|l| {
                    let mut t = String::new();
                    if let Some(st) = &l.stanza {
                        let _ = write!(t, "\\set stanza = {} ", ly_string(st));
                    }
                    t.push_str(&lyric_token(&l.text));
                    if matches!(l.syllabic, Syllabic::Begin | Syllabic::Middle) {
                        t.push_str(" --");
                    }
                    if l.extend {
                        t.push_str(" __");
                    }
                    t
                });
                entries.push(Entry { mi, tok, extend: ly.map(|l| l.extend).unwrap_or(false), after_rest });
                after_rest = false;
            }
        }
        // blanks after the last word are only needed to carry its extender to the next rest
        if let Some(last) = entries.iter().rposition(|e| e.tok.is_some()) {
            let mut keep = last + 1;
            if entries[last].extend {
                while keep < entries.len() && !entries[keep].after_rest {
                    keep += 1;
                }
            }
            entries.truncate(keep);
            // a hyphen after the very last syllable leads nowhere
            let bar = timing.numbers[entries[last].mi].clone();
            if let Some(t) = entries[last].tok.as_mut() {
                if t.contains(" --") {
                    *t = t.replace(" --", "");
                    self.diags.warn(None, Some(&bar), format!("the last syllable of a verse ends with a hyphen that leads nowhere; the hyphen is left out ({})", vd.var));
                }
            }
        } else {
            entries.clear();
        }
        // An extender never crosses a rest: LilyPond would carry it over every `_` to the
        // next syllable, so the first unsung note after a rest takes an empty syllable.
        let mut ext_open = false;
        for e in entries.iter_mut() {
            match &e.tok {
                Some(_) => ext_open = e.extend,
                None if e.after_rest && ext_open => {
                    e.tok = Some("\"\"".into());
                    ext_open = false;
                }
                None => {}
            }
        }
        let mut out = String::new();
        let mut i = 0;
        while i < entries.len() {
            let mi = entries[i].mi;
            let mut line = String::from(" ");
            while i < entries.len() && entries[i].mi == mi {
                line.push(' ');
                line.push_str(entries[i].tok.as_deref().unwrap_or("_"));
                i += 1;
            }
            let _ = writeln!(out, "{} % {}", line, timing.numbers[mi]);
        }
        out
    }

    // ------------------------------------------------------------ score block

    fn write_score(&mut self, all_voices: &[Vec<VoiceData>]) {
        let score = self.score;
        let mut o = String::from("\\score {\n  <<\n");
        let mut depth = 2usize;
        let mut open_groups: Vec<String> = vec![];
        let mut first_staff = true;
        let pad = |d: usize| "  ".repeat(d);
        for item in &score.part_list {
            match item {
                PartListItem::GroupStart { number, symbol, name, barline } => {
                    let (ctx, with): (&str, Option<&str>) = match symbol {
                        GroupSymbol::Bracket => (if *barline == Some(true) { "StaffGroup" } else { "ChoirStaff" }, None),
                        GroupSymbol::Brace => ("GrandStaff", None),
                        GroupSymbol::Line => ("StaffGroup", Some("systemStartDelimiter = #'SystemStartBar")),
                        GroupSymbol::Square => ("StaffGroup", Some("systemStartDelimiter = #'SystemStartSquare")),
                        GroupSymbol::None => ("StaffGroup", Some("systemStartDelimiter = #'SystemStartBar")),
                    };
                    let mut props: Vec<String> = vec![];
                    if let Some(w) = with {
                        props.push(w.to_string());
                    }
                    if let Some(n) = name {
                        props.push(format!("instrumentName = {}", markup_lines(n, "center-column")));
                    }
                    if props.is_empty() {
                        let _ = writeln!(o, "{}\\new {} <<", pad(depth), ctx);
                    } else {
                        let _ = writeln!(o, "{}\\new {} \\with {{ {} }} <<", pad(depth), ctx, props.join(" "));
                    }
                    open_groups.push(number.clone());
                    depth += 1;
                }
                PartListItem::GroupStop { number } => {
                    if let Some(i) = open_groups.iter().rposition(|n| n == number) {
                        while open_groups.len() > i {
                            open_groups.pop();
                            depth -= 1;
                            let _ = writeln!(o, "{}>>", pad(depth));
                        }
                    }
                }
                PartListItem::Part(pi) => {
                    let part = &score.parts[*pi];
                    let voices = &all_voices[*pi];
                    let shown = part.display_name.clone().unwrap_or_else(|| part.name.clone());
                    let mut props: Vec<String> = vec![];
                    if !shown.trim().is_empty() {
                        props.push(format!("instrumentName = {}", markup_lines(&shown, "center-column")));
                    }
                    if let Some(a) = &part.abbr {
                        props.push(format!("shortInstrumentName = {}", markup_lines(a, "center-column")));
                    }
                    let with = if props.is_empty() { String::new() } else { format!(" \\with {{ {} }}", props.join(" ")) };
                    let multi = part.staves > 1;
                    if part.measures.iter().any(|m| !m.harmonies.is_empty()) {
                        let _ = writeln!(o, "{}\\new ChordNames {{ \\{}Chords }}", pad(depth), self.part_names()[*pi]);
                    }
                    if multi {
                        let _ = writeln!(o, "{}\\new PianoStaff{} <<", pad(depth), with);
                        depth += 1;
                    }
                    for staff in 1..=part.staves.max(1) {
                        // staves named in --hide-empty-staves disappear from systems where they rest
                        let hide_this = self.opts.hide_staves.iter().any(|n| {
                            let n = n.trim();
                            n == part.id || n == part.name || Some(n) == part.abbr.as_deref() || Some(n) == part.display_name.as_deref().map(|d| d.lines().next().unwrap_or(""))
                        });
                        let mut staff_with: Vec<String> = vec![];
                        if hide_this {
                            staff_with.push("\\RemoveAllEmptyStaves".into());
                        }
                        if let Some(n) = part.staff_lines {
                            staff_with.push(format!("\\override StaffSymbol.line-count = #{}", n));
                        }
                        if !multi {
                            staff_with.extend(props.iter().cloned());
                        }
                        let sw_owned = if staff_with.is_empty() { String::new() } else { format!(" \\with {{ {} }}", staff_with.join(" ")) };
                        let sw = sw_owned.as_str();
                        let _ = writeln!(o, "{}\\new Staff = {}{} <<", pad(depth), ly_string(&format!("{}-{}", part.id, staff)), sw);
                        if first_staff {
                            let _ = writeln!(o, "{}\\Structure", pad(depth + 1));
                            first_staff = false;
                        }
                        for v in voices.iter().filter(|v| v.key.0 == staff) {
                            let mut setup = String::new();
                            if !v.lyric_lines.is_empty() {
                                setup.push_str("\\set melismaBusyProperties = #'() ");
                            }
                            if self.manual_beams {
                                setup.push_str("\\autoBeamOff ");
                            }
                            let _ = writeln!(o, "{}\\new Voice = {} {{ {}\\{} }}", pad(depth + 1), ly_string(&v.var), setup, v.var);
                        }
                        let _ = writeln!(o, "{}>>", pad(depth));
                        // words under their staff
                        let mut lyr: Vec<(u32, usize, String)> = vec![];
                        for v in voices.iter().filter(|v| v.key.0 == staff) {
                            for (li, line) in v.lyric_lines.iter().enumerate() {
                                let grace = if v.has_grace { "\\set includeGraceNotes = ##t " } else { "" };
                                lyr.push((
                                    *line,
                                    v.rank,
                                    format!("\\new Lyrics \\lyricsto {} {{ {}\\{}Verse{} }}", ly_string(&v.var), grace, v.var, num_word(li + 1)),
                                ));
                            }
                        }
                        lyr.sort();
                        for (_, _, l) in lyr {
                            let _ = writeln!(o, "{}{}", pad(depth), l);
                        }
                    }
                    if multi {
                        depth -= 1;
                        let _ = writeln!(o, "{}>>", pad(depth));
                    }
                }
            }
        }
        while open_groups.pop().is_some() {
            depth -= 1;
            let _ = writeln!(o, "{}>>", pad(depth));
        }
        o.push_str("  >>\n  \\layout {\n");
        // room for the staff names (LilyPond clips names wider than the indent)
        let size = self.opts.staff_size.or(score.layout.staff_size).unwrap_or(20.0);
        let width = |s: &str| s.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f64 * 1.9 * size / 20.0 + 3.0;
        let mut long: f64 = 0.0;
        let mut short: f64 = 0.0;
        for p in &score.parts {
            long = long.max(width(&p.display_name.clone().unwrap_or_else(|| p.name.clone())));
            if let Some(a) = &p.abbr {
                short = short.max(width(a));
            }
        }
        for item in &score.part_list {
            if let PartListItem::GroupStart { name: Some(n), .. } = item {
                long = long.max(width(n) + 4.0);
            }
        }
        if long > 15.0 {
            let _ = writeln!(o, "    indent = {}\\mm", fmt_num(long.min(60.0)));
        }
        if short > 3.0 {
            let _ = writeln!(o, "    short-indent = {}\\mm", fmt_num(short.min(40.0)));
        }
        if all_voices.iter().any(|vs| vs.iter().any(|v| !v.lyric_lines.is_empty())) {
            // LilyPond leaves a hyphen out where syllables sit close; keep every one
            o.push_str("    \\context { \\Lyrics \\override LyricHyphen.minimum-distance = #1.2 }\n");
        }
        let hide = self.opts.hide_empty.unwrap_or(score.layout.hide_empty);
        if hide {
            o.push_str("    \\context { \\Staff \\RemoveAllEmptyStaves }\n");
        }
        let mut score_ctx: Vec<&str> = vec![];
        if self.opts.bar_numbers.or(score.layout.bar_numbers) == Some(false) {
            score_ctx.push("\\omit BarNumber");
        } else if self.opts.bar_numbers_all.unwrap_or(score.layout.bar_numbers_all) {
            // a number over every bar line
            score_ctx.push("\\override BarNumber.break-visibility = ##(#f #t #t) barNumberVisibility = #all-bar-numbers-visible");
        }
        let any_symbol = score.parts.iter().any(|p| p.measures.iter().any(|m| m.attrs.iter().any(|a| matches!(a.kind, Attr::Time { symbol: Some(_), .. }))));
        if !any_symbol {
            score_ctx.push("\\override TimeSignature.style = #'numbered");
        }
        if !score_ctx.is_empty() {
            let _ = writeln!(o, "    \\context {{ \\Score {} }}", score_ctx.join(" "));
        }
        o.push_str("  }\n");
        if self.opts.midi {
            o.push_str("  \\midi { }\n");
        }
        o.push_str("}\n");
        self.out.push_str(&o);
    }
}

// ---------------------------------------------------------------- free functions

fn fmt_num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

fn moment(f: Frac) -> String {
    format!("{}/{}", f.n, f.d)
}

fn tempo_token(text: Option<&str>, metronome: Option<&(NoteType, String)>) -> String {
    match (text, metronome) {
        (Some(t), None) => format!("\\tempo {}", ly_string(t)),
        (t, Some((unit, pm))) => {
            let plain = !pm.is_empty() && pm.chars().all(|c| c.is_ascii_digit());
            if plain {
                match t {
                    Some(t) => format!("\\tempo {} {} = {}", ly_string(t), type_token(*unit), pm),
                    None => format!("\\tempo {} = {}", type_token(*unit), pm),
                }
            } else {
                let lead = t.map(|t| format!("{} ", ly_string(&format!("{} ", t)))).unwrap_or_default();
                format!(
                    "\\tempo \\markup {{ \\concat {{ {}\\smaller \\general-align #Y #DOWN \\note {{{}}} #1 {} }} }}",
                    lead,
                    type_token(*unit),
                    ly_string(&format!(" = {}", pm))
                )
            }
        }
        (None, None) => String::new(),
    }
}

/// Put a mark on the slot that starts at `pos`, or inside the note that spans it.
fn attach(slots: &mut Vec<Slot>, pos: Frac, pre: bool, tok: String, tail: &mut Vec<String>) {
    let exact = if pre { slots.iter().position(|s| s.pos == pos) } else { slots.iter().position(|s| s.pos == pos && !s.dur.is_zero()).or_else(|| slots.iter().position(|s| s.pos == pos)) };
    if let Some(i) = exact {
        if pre {
            slots[i].pre.push(tok);
        } else {
            slots[i].post.push(tok);
        }
        return;
    }
    if let Some(i) = slots.iter().position(|s| s.pos < pos && pos < s.pos + s.dur) {
        if slots[i].ev.is_none() {
            let end = slots[i].pos + slots[i].dur;
            slots[i].dur = pos - slots[i].pos;
            let mut n = Slot { pos, dur: end - pos, ev: None, pre: vec![], post: vec![], interior: vec![] };
            if pre {
                n.pre.push(tok);
            } else {
                n.post.push(tok);
            }
            slots.insert(i + 1, n);
        } else {
            let off = pos - slots[i].pos;
            slots[i].interior.push((off, pre, tok));
        }
        return;
    }
    // at or after the end of the bar
    if pre {
        tail.push(tok);
    } else if let Some(last) = slots.last_mut() {
        let closes = tok == "\\!" || tok.starts_with("\\sustainOff");
        if closes && last.ev.is_some() && !last.dur.is_zero() {
            // end a hairpin or pedal shortly before the end of the last note
            let back = if last.dur > Frac::new(1, 4) { Frac::new(1, 8) } else { last.dur * Frac::new(1, 2) };
            let off = last.dur - back;
            last.interior.push((off, false, tok));
        } else {
            last.post.push(tok);
        }
    }
}

/// post-events of a note: articulations, slurs, beams
fn event_posts(e: &Event) -> String {
    let mut s = String::new();
    for a in &e.arts {
        match a {
            Art::Staccato => s.push_str("-."),
            Art::Staccatissimo => s.push_str("-!"),
            Art::Accent => s.push_str("->"),
            Art::StrongAccent => s.push_str("-^"),
            Art::Tenuto => s.push_str("--"),
            Art::DetachedLegato => s.push_str("-_"),
            Art::Fermata { inverted } => s.push_str(if *inverted { "_\\fermata" } else { "\\fermata" }),
            Art::Trill => s.push_str("\\trill"),
            Art::Mordent => s.push_str("\\mordent"),
            Art::Prall => s.push_str("\\prall"),
            Art::Turn => s.push_str("\\turn"),
            Art::UpBow => s.push_str("\\upbow"),
            Art::DownBow => s.push_str("\\downbow"),
            Art::Harmonic => s.push_str("\\flageolet"),
            Art::OpenString => s.push_str("\\open"),
            Art::Stopped => s.push_str("\\stopped"),
            Art::Fingering(f) => {
                if f.len() == 1 && f.chars().all(|c| c.is_ascii_digit()) {
                    let _ = write!(s, "-{}", f);
                } else if !f.is_empty() {
                    let _ = write!(s, "^\\markup {{ \\finger {} }}", ly_string(f));
                }
            }
            Art::Arpeggio => s.push_str("\\arpeggio"),
            Art::Glissando | Art::GlissandoWavy => s.push_str("\\glissando"),
            Art::Falloff => s.push_str("\\bendAfter #-4"),
            Art::Doit => s.push_str("\\bendAfter #4"),
            Art::Tremolo(_) | Art::Breath | Art::Caesura => {}
        }
    }
    for n in &e.slur_stops {
        if *n == 1 {
            s.push(')');
        } else {
            let _ = write!(s, "\\={})", n);
        }
    }
    for n in &e.slur_starts {
        if *n == 1 {
            s.push('(');
        } else {
            let _ = write!(s, "\\={}(", n);
        }
    }
    match e.beam {
        Some(Beam::Begin) => s.push('['),
        Some(Beam::End) => s.push(']'),
        _ => {}
    }
    s
}

#[allow(dead_code)]
fn unused(_: BTreeMap<u8, u8>) {}
