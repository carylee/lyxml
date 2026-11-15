//! Musical comparison of two scores, whatever format each came from.
//!
//! Compared: parts, bars and their lengths, every note and rest (position, length,
//! spelling, ties, grace), words (text, hyphenation, extenders, verse line), slurs,
//! articulations, tuplets, keys, time signatures, clefs, bar lines and endings, breaks,
//! and directions. Voice numbers are compared by order within their staff, verse lines
//! by order, so renumbering is not a difference.

use crate::ir::*;
use std::collections::BTreeMap;

const MAX_LINES: usize = 200;

fn pitch_name(n: &Note) -> String {
    let mut s = String::new();
    s.push(['C', 'D', 'E', 'F', 'G', 'A', 'B'][n.step as usize % 7]);
    for _ in 0..n.alter.max(0) {
        s.push('#');
    }
    for _ in 0..(-n.alter).max(0) {
        s.push('b');
    }
    s.push_str(&n.octave.to_string());
    if n.tie_start {
        s.push('~');
    }
    s
}

fn event_sig(e: &Event, line_rank: &BTreeMap<u32, usize>) -> String {
    let mut s = format!("@{} ", e.pos);
    if e.grace.is_some() {
        s.push_str("grace ");
    }
    match &e.kind {
        EventKind::Rest { .. } => s.push_str("rest"),
        EventKind::Notes(ns) => {
            let mut names: Vec<(i32, String)> = ns.iter().map(|n| (n.midi(), pitch_name(n))).collect();
            names.sort();
            s.push_str(&names.into_iter().map(|x| x.1).collect::<Vec<_>>().join("+"));
        }
    }
    s.push_str(&format!(" len {}", e.dur));
    if let Some(t) = e.tuplet {
        s.push_str(&format!(" {}:{}", t.actual, t.normal));
    }
    let mut lyr: Vec<String> = e
        .lyrics
        .iter()
        .map(|l| {
            let syl = match l.syllabic {
                Syllabic::Single => "",
                Syllabic::Begin => "-",
                Syllabic::Middle => "-",
                Syllabic::End => "",
            };
            // only the hyphen after a syllable prints; a "middle" or "end" with no "begin"
            // before it looks the same as a "single"
            let pre = "";
            format!("v{}:{}{}{}{}", line_rank.get(&l.line).copied().unwrap_or(0) + 1, pre, l.text, syl, if l.extend { "__" } else { "" })
        })
        .collect();
    lyr.sort();
    for l in lyr {
        s.push_str(" \"");
        s.push_str(&l);
        s.push('"');
    }
    for l in &e.lyrics {
        if let Some(st) = &l.stanza {
            s.push_str(&format!(" stanza={}", st));
        }
    }
    if !e.slur_starts.is_empty() {
        s.push_str(&format!(" slur+{}", e.slur_starts.len()));
    }
    if !e.slur_stops.is_empty() {
        s.push_str(&format!(" slur-{}", e.slur_stops.len()));
    }
    let mut arts: Vec<String> = e.arts.iter().map(|a| format!("{:?}", a)).collect();
    arts.sort();
    for a in arts {
        s.push(' ');
        s.push_str(&a);
    }
    s
}

fn dir_sig(d: &Direction) -> String {
    let what = match &d.kind {
        DirKind::Dynamic(n) => format!("dynamic {}", n),
        DirKind::Wedge(w) => format!("hairpin {:?}", w),
        DirKind::Words { text, .. } => format!("words \"{}\"", text),
        DirKind::Tempo { text, metronome } => format!("tempo {:?} {:?}", text, metronome.as_ref().map(|m| (m.0.log, m.0.dots, m.1.clone()))),
        DirKind::Rehearsal(t) => format!("rehearsal {}", t),
        DirKind::Segno => "segno".into(),
        DirKind::Coda => "coda".into(),
        DirKind::Ottava(n) => format!("ottava {}", n),
        DirKind::Pedal(p) => format!("pedal {:?}", p),
    };
    format!("@{} staff {} {}", d.pos, d.staff, what)
}

fn bar_sig(b: &Barline) -> String {
    let mut s = String::new();
    if let Some(st) = &b.style {
        s.push_str(st);
    }
    if b.repeat {
        s.push_str(" repeat");
    }
    if let Some(e) = &b.ending_start {
        s.push_str(&format!(" ending-start {}", e.text.clone().unwrap_or_else(|| format!("{}.", e.number))));
    }
    if b.ending_stop.is_some() {
        s.push_str(" ending-stop");
    }
    s.trim().to_string()
}

/// voices of a part, per staff, in order -> rank
fn voice_ranks(p: &Part) -> BTreeMap<(u32, u32), usize> {
    let mut keys: Vec<(u32, u32)> = vec![];
    for m in &p.measures {
        for v in &m.voices {
            if !keys.contains(&(v.staff, v.voice)) {
                keys.push((v.staff, v.voice));
            }
        }
    }
    keys.sort();
    let mut out = BTreeMap::new();
    let mut per_staff: BTreeMap<u32, usize> = BTreeMap::new();
    for k in keys {
        let r = per_staff.entry(k.0).or_insert(0);
        out.insert(k, *r);
        *r += 1;
    }
    out
}

fn line_ranks(p: &Part) -> BTreeMap<(u32, u32), BTreeMap<u32, usize>> {
    let mut lines: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for m in &p.measures {
        for v in &m.voices {
            for e in &v.events {
                for l in &e.lyrics {
                    let list = lines.entry((v.staff, v.voice)).or_default();
                    if !list.contains(&l.line) {
                        list.push(l.line);
                    }
                }
            }
        }
    }
    lines
        .into_iter()
        .map(|(k, mut v)| {
            v.sort();
            (k, v.into_iter().enumerate().map(|(i, l)| (l, i)).collect())
        })
        .collect()
}

/// What prints the same compares the same: a volta bracket ends where the next one
/// starts, whether or not the source says so; a tempo mark belongs to the score.
fn normalized(s: &Score) -> (Score, Vec<Vec<String>>) {
    let mut s = s.clone();
    let n = s.parts.iter().map(|p| p.measures.len()).max().unwrap_or(0);
    let mut tempi: Vec<Vec<String>> = vec![vec![]; n];
    for p in &mut s.parts {
        // what an engraver assumes when the first bar does not say: treble clef, no
        // sharps or flats, common time
        let staves = p.staves.max(1);
        if let Some(m) = p.measures.first_mut() {
            if !m.attrs.iter().any(|a| a.pos.is_zero() && matches!(a.kind, Attr::Key { .. })) {
                m.attrs.push(AttrChange { pos: Frac::ZERO, kind: Attr::Key { fifths: 0, minor: false } });
            }
            if !m.attrs.iter().any(|a| a.pos.is_zero() && matches!(a.kind, Attr::Time { .. })) {
                m.attrs.push(AttrChange { pos: Frac::ZERO, kind: Attr::Time { beats: "4".into(), beat_type: 4, symbol: Some(TimeSymbol::Common), print: true } });
            }
            for st in 1..=staves {
                if !m.attrs.iter().any(|a| a.pos.is_zero() && matches!(a.kind, Attr::Clef { staff, .. } if staff == st)) {
                    m.attrs.push(AttrChange { pos: Frac::ZERO, kind: Attr::Clef { staff: st, sign: 'G', line: 2, octave: 0 } });
                }
            }
        }
        for m in &mut p.measures {
            for a in &mut m.attrs {
                // a percussion clef has no line to sit on
                if let Attr::Clef { sign: 'P', line, .. } = &mut a.kind {
                    *line = 0;
                }
                // the C and cut-C symbols only exist for 4/4 and 2/2
                if let Attr::Time { beats, beat_type, symbol, .. } = &mut a.kind {
                    let fits = matches!((beats.as_str(), *beat_type, &symbol), ("4", 4, Some(TimeSymbol::Common)) | ("2", 2, Some(TimeSymbol::Cut)));
                    if !fits {
                        *symbol = None;
                    }
                }
            }
            // a pedal change is a release and a new press
            let mut dirs: Vec<Direction> = vec![];
            for d in m.directions.drain(..) {
                if d.kind == DirKind::Pedal(Pedal::Change) {
                    dirs.push(Direction { kind: DirKind::Pedal(Pedal::Stop), ..d.clone() });
                    dirs.push(Direction { kind: DirKind::Pedal(Pedal::Start), ..d });
                } else {
                    dirs.push(d);
                }
            }
            m.directions = dirs;
        }
        let mut open = false;
        for i in 0..p.measures.len() {
            if p.measures[i].left.ending_start.is_some() {
                if open && i > 0 && p.measures[i - 1].right.ending_stop.is_none() {
                    p.measures[i - 1].right.ending_stop = Some(EndingStop { number: String::new(), open: false });
                }
                open = true;
            }
            if p.measures[i].right.ending_stop.is_some() {
                open = false;
            }
            let m = &mut p.measures[i];
            for d in m.directions.iter().filter(|d| matches!(d.kind, DirKind::Tempo { .. })) {
                let sig = format!("@{} {:?}", d.pos, d.kind);
                if !tempi[i].contains(&sig) {
                    tempi[i].push(sig);
                }
            }
            m.directions.retain(|d| !matches!(d.kind, DirKind::Tempo { .. }));
        }
    }
    for t in &mut tempi {
        t.sort();
    }
    (s, tempi)
}

pub fn diff(a: &Score, b: &Score) -> Vec<String> {
    let ((a, ta), (b, tb)) = (normalized(a), normalized(b));
    let (a, b) = (&a, &b);
    let mut out: Vec<String> = vec![];
    let mut push = |s: String| out.push(s);
    let hdr = |name: &str, x: &Option<String>, y: &Option<String>, push: &mut dyn FnMut(String)| {
        if x != y {
            push(format!("header {}: {:?} | {:?}", name, x.as_deref().unwrap_or(""), y.as_deref().unwrap_or("")));
        }
    };
    hdr("title", &a.header.title, &b.header.title, &mut push);
    hdr("subtitle", &a.header.subtitle, &b.header.subtitle, &mut push);
    // with credit lines, they are what prints; the composer field is not
    if a.header.composer_lines.is_empty() || b.header.composer_lines.is_empty() {
        hdr("composer", &a.header.composer, &b.header.composer, &mut push);
    }
    if a.header.poet_lines.is_empty() || b.header.poet_lines.is_empty() {
        hdr("poet", &a.header.poet, &b.header.poet, &mut push);
    }
    if a.header.poet_lines != b.header.poet_lines {
        push(format!("header poet lines: {:?} | {:?}", a.header.poet_lines, b.header.poet_lines));
    }
    hdr("arranger", &a.header.arranger, &b.header.arranger, &mut push);
    if a.header.composer_lines != b.header.composer_lines {
        push(format!("header credit lines: {:?} | {:?}", a.header.composer_lines, b.header.composer_lines));
    }
    if a.parts.len() != b.parts.len() {
        push(format!("parts: {} | {}", a.parts.len(), b.parts.len()));
    }
    for (pi, (pa, pb)) in a.parts.iter().zip(b.parts.iter()).enumerate() {
        let pname = if pa.id.is_empty() { format!("#{}", pi + 1) } else { pa.id.clone() };
        if pa.staves.max(1) != pb.staves.max(1) {
            push(format!("part {}: staves {} | {}", pname, pa.staves, pb.staves));
        }
        let shown = |p: &Part| p.display_name.clone().unwrap_or_else(|| p.name.clone());
        if shown(pa) != shown(pb) {
            push(format!("part {}: printed name {:?} | {:?}", pname, shown(pa), shown(pb)));
        }
        if pa.abbr != pb.abbr {
            push(format!("part {}: short name {:?} | {:?}", pname, pa.abbr, pb.abbr));
        }
        if pa.measures.len() != pb.measures.len() {
            push(format!("part {}: bars {} | {}", pname, pa.measures.len(), pb.measures.len()));
        }
        let (ra, rb) = (voice_ranks(pa), voice_ranks(pb));
        let (la, lb) = (line_ranks(pa), line_ranks(pb));
        let empty = BTreeMap::new();
        for (mi, (ma, mb)) in pa.measures.iter().zip(pb.measures.iter()).enumerate() {
            let at = format!("part {} bar {}", pname, if ma.number.is_empty() { format!("#{}", mi + 1) } else { ma.number.clone() });
            if ma.len != mb.len {
                push(format!("{}: length {} | {}", at, ma.len, mb.len));
            }
            if ma.number != mb.number && !(ma.implicit && mb.implicit) {
                push(format!("{}: number {:?} | {:?}", at, ma.number, mb.number));
            }
            if (ma.new_system, ma.new_page) != (mb.new_system, mb.new_page) {
                push(format!("{}: break system={} page={} | system={} page={}", at, ma.new_system, ma.new_page, mb.new_system, mb.new_page));
            }
            if bar_sig(&ma.left) != bar_sig(&mb.left) {
                push(format!("{}: left barline [{}] | [{}]", at, bar_sig(&ma.left), bar_sig(&mb.left)));
            }
            if bar_sig(&ma.right) != bar_sig(&mb.right) {
                push(format!("{}: right barline [{}] | [{}]", at, bar_sig(&ma.right), bar_sig(&mb.right)));
            }
            let attrs = |m: &Measure| {
                let mut v: Vec<String> = m.attrs.iter().map(|x| format!("@{} {:?}", x.pos, x.kind)).collect();
                v.sort();
                v
            };
            let (xa, xb) = (attrs(ma), attrs(mb));
            if xa != xb {
                for x in xa.iter().filter(|x| !xb.contains(x)) {
                    push(format!("{}: only in first: {}", at, x));
                }
                for x in xb.iter().filter(|x| !xa.contains(x)) {
                    push(format!("{}: only in second: {}", at, x));
                }
            }
            // voices by rank
            let by_rank = |m: &Measure, r: &BTreeMap<(u32, u32), usize>, l: &BTreeMap<(u32, u32), BTreeMap<u32, usize>>| {
                let mut map: BTreeMap<(u32, usize), Vec<String>> = BTreeMap::new();
                for v in &m.voices {
                    let rank = r.get(&(v.staff, v.voice)).copied().unwrap_or(0);
                    let lr = l.get(&(v.staff, v.voice)).unwrap_or(&empty);
                    map.insert((v.staff, rank), v.events.iter().map(|e| event_sig(e, lr)).collect());
                }
                map
            };
            let (va, vb) = (by_rank(ma, &ra, &la), by_rank(mb, &rb, &lb));
            let keys: Vec<(u32, usize)> = va.keys().chain(vb.keys()).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            for k in keys {
                let none = vec![];
                let (ea, eb) = (va.get(&k).unwrap_or(&none), vb.get(&k).unwrap_or(&none));
                if ea == eb {
                    continue;
                }
                let who = format!("{} staff {} voice {}", at, k.0, k.1 + 1);
                // a voice that only rests is the same as no voice
                let only_rests = |v: &Vec<String>| v.iter().all(|s| s.contains(" rest len "));
                if (ea.is_empty() && only_rests(eb)) || (eb.is_empty() && only_rests(ea)) {
                    continue;
                }
                for x in ea.iter().filter(|x| !eb.contains(x)) {
                    push(format!("{}: only in first:  {}", who, x));
                }
                for x in eb.iter().filter(|x| !ea.contains(x)) {
                    push(format!("{}: only in second: {}", who, x));
                }
            }
            let dirs = |m: &Measure| {
                let mut v: Vec<String> = m.directions.iter().map(dir_sig).collect();
                v.sort();
                v
            };
            if ma.harmonies != mb.harmonies {
                push(format!("{}: chord symbols {:?} | {:?}", at, ma.harmonies, mb.harmonies));
            }
            let (da, db) = (dirs(ma), dirs(mb));
            if da != db {
                for x in da.iter().filter(|x| !db.contains(x)) {
                    push(format!("{}: only in first:  {}", at, x));
                }
                for x in db.iter().filter(|x| !da.contains(x)) {
                    push(format!("{}: only in second: {}", at, x));
                }
            }
        }
    }
    for (i, (x, y)) in ta.iter().zip(tb.iter()).enumerate() {
        if x != y {
            let num = a.parts.first().and_then(|p| p.measures.get(i)).map(|m| m.number.clone()).unwrap_or_default();
            out.push(format!("bar {}: tempo {:?} | {:?}", num, x, y));
        }
    }
    if out.len() > MAX_LINES {
        let n = out.len();
        out.truncate(MAX_LINES);
        out.push(format!("... and {} more differences", n - MAX_LINES));
    }
    out
}
