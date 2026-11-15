//! Score model -> MusicXML 4.0 (partwise).

use crate::diag::Diags;
use crate::ir::*;
use std::fmt::Write as _;
use std::io::Write as _;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

struct X {
    s: String,
    depth: usize,
}

impl X {
    fn pad(&mut self) {
        for _ in 0..self.depth {
            self.s.push_str("  ");
        }
    }
    /// `<tag attrs>`
    fn open(&mut self, tag: &str) {
        self.pad();
        let _ = writeln!(self.s, "<{}>", tag);
        self.depth += 1;
    }
    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.pad();
        let _ = writeln!(self.s, "</{}>", tag);
    }
    /// `<tag attrs>text</tag>`
    fn leaf(&mut self, tag: &str, text: &str) {
        self.pad();
        let name = tag.split(' ').next().unwrap_or(tag);
        let _ = writeln!(self.s, "<{}>{}</{}>", tag, esc(text), name);
    }
    fn empty(&mut self, tag: &str) {
        self.pad();
        let _ = writeln!(self.s, "<{}/>", tag);
    }
}

fn lcm(a: i64, b: i64) -> i64 {
    a / gcd(a, b).max(1) * b
}

fn type_name(log: i8) -> &'static str {
    match log {
        i8::MIN..=-2 => "long",
        -1 => "breve",
        0 => "whole",
        1 => "half",
        2 => "quarter",
        3 => "eighth",
        4 => "16th",
        5 => "32nd",
        6 => "64th",
        7 => "128th",
        _ => "256th",
    }
}

fn line_attr(style: u8) -> &'static str {
    match style {
        1 => " line-type=\"dashed\"",
        2 => " line-type=\"dotted\"",
        _ => "",
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn write_musicxml(score: &Score, _diags: &mut Diags) -> String {
    let mut x = X { s: String::new(), depth: 0 };
    x.s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.s.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    x.open("score-partwise version=\"4.0\"");
    let h = &score.header;
    if let Some(t) = &h.title {
        x.open("work");
        x.leaf("work-title", t);
        x.close("work");
    }
    if let Some(t) = &h.subtitle {
        x.leaf("movement-title", t);
    }
    x.open("identification");
    if let Some(t) = &h.composer {
        x.leaf("creator type=\"composer\"", t);
    }
    if let Some(t) = &h.poet {
        x.leaf("creator type=\"lyricist\"", t);
    }
    if let Some(t) = &h.arranger {
        x.leaf("creator type=\"arranger\"", t);
    }
    if let Some(t) = &h.copyright {
        x.leaf("rights", t);
    }
    x.open("encoding");
    x.leaf("software", &format!("lyxml {}", env!("CARGO_PKG_VERSION")));
    x.close("encoding");
    let l = &score.layout;
    if l.bar_numbers.is_some() || l.hide_empty || l.first_page.is_some() {
        x.open("miscellaneous");
        if let Some(b) = l.bar_numbers {
            x.leaf("miscellaneous-field name=\"bar-numbers\"", if !b { "false" } else if l.bar_numbers_all { "all" } else { "true" });
        }
        if l.hide_empty {
            x.leaf("miscellaneous-field name=\"hide-empty\"", "true");
        }
        if let Some(p) = l.first_page {
            x.leaf("miscellaneous-field name=\"first-page\"", &p.to_string());
        }
        x.close("miscellaneous");
    }
    x.close("identification");
    if l.staff_size.is_some() || l.page_width_mm.is_some() {
        let size = l.staff_size.unwrap_or(20.0);
        let mm = size * 25.4 / 72.0; // height of 40 tenths
        x.open("defaults");
        x.open("scaling");
        x.leaf("millimeters", &fmt_num(mm));
        x.leaf("tenths", "40");
        x.close("scaling");
        if let (Some(w), Some(hh)) = (l.page_width_mm, l.page_height_mm) {
            x.open("page-layout");
            x.leaf("page-height", &fmt_num((hh / (mm / 40.0) * 100.0).round() / 100.0));
            x.leaf("page-width", &fmt_num((w / (mm / 40.0) * 100.0).round() / 100.0));
            x.close("page-layout");
        }
        x.close("defaults");
    }
    let mut credit = |x: &mut X, kind: &str, text: &str| {
        x.open("credit page=\"1\"");
        x.leaf("credit-type", kind);
        x.leaf("credit-words", text);
        x.close("credit");
    };
    if let Some(t) = &h.dedication {
        credit(&mut x, "dedication", t);
    }
    for t in &h.poet_lines {
        credit(&mut x, "poet_lines", t);
    }
    for t in &h.composer_lines {
        credit(&mut x, "composer_lines", t);
    }
    if let Some(t) = &h.footnote {
        credit(&mut x, "footnote", t);
    }

    // part list
    x.open("part-list");
    for item in &score.part_list {
        match item {
            PartListItem::GroupStart { number, symbol, name, barline } => {
                x.open(&format!("part-group number=\"{}\" type=\"start\"", esc(number)));
                if let Some(n) = name {
                    x.leaf("group-name", n);
                }
                let sym = match symbol {
                    GroupSymbol::Bracket => "bracket",
                    GroupSymbol::Brace => "brace",
                    GroupSymbol::Line => "line",
                    GroupSymbol::Square => "square",
                    GroupSymbol::None => "none",
                };
                x.leaf("group-symbol", sym);
                if let Some(b) = barline {
                    x.leaf("group-barline", if *b { "yes" } else { "no" });
                }
                x.close("part-group");
            }
            PartListItem::GroupStop { number } => x.empty(&format!("part-group number=\"{}\" type=\"stop\"", esc(number))),
            PartListItem::Part(i) => {
                let p = &score.parts[*i];
                x.open(&format!("score-part id=\"{}\"", esc(&p.id)));
                x.leaf("part-name", &p.name);
                if let Some(d) = &p.display_name {
                    x.open("part-name-display");
                    x.leaf("display-text", d);
                    x.close("part-name-display");
                }
                if let Some(a) = &p.abbr {
                    x.leaf("part-abbreviation", a);
                }
                if let Some(ins) = &p.instrument {
                    x.open(&format!("score-instrument id=\"{}-I1\"", esc(&p.id)));
                    x.leaf("instrument-name", ins);
                    x.close("score-instrument");
                }
                x.close("score-part");
            }
        }
    }
    x.close("part-list");

    for p in &score.parts {
        write_part(&mut x, p);
    }
    x.close("score-partwise");
    x.s
}

fn write_part(x: &mut X, p: &Part) {
    // divisions: every position and length must be a whole number of them
    let mut div: i64 = 1;
    for m in &p.measures {
        let mut see = |f: Frac| {
            let q = f * Frac::int(4);
            if q.d < 100_000 {
                div = lcm(div, q.d);
            }
        };
        see(m.len);
        for v in &m.voices {
            for e in &v.events {
                see(e.pos);
                see(e.dur);
            }
        }
        for d in &m.directions {
            see(d.pos);
        }
        for a in &m.attrs {
            see(a.pos);
        }
    }
    let ticks = |f: Frac| -> i64 { (f * Frac::int(4 * div)).n / (f * Frac::int(4 * div)).d.max(1) };
    let defaults = default_accidentals(p);
    let multi = p.staves > 1;

    x.open(&format!("part id=\"{}\"", esc(&p.id)));
    let mut ottava_size = 8;
    for (mi, m) in p.measures.iter().enumerate() {
        let mut tag = format!("measure number=\"{}\"", esc(&m.number));
        if m.implicit {
            tag.push_str(" implicit=\"yes\"");
        }
        x.open(&tag);
        if m.new_page {
            x.empty("print new-page=\"yes\"");
        } else if m.new_system {
            x.empty("print new-system=\"yes\"");
        }
        // attributes at the start of the bar
        let write_attrs = |x: &mut X, attrs: &[&AttrChange], first: bool| {
            if attrs.is_empty() && !first {
                return;
            }
            x.open("attributes");
            if first {
                x.leaf("divisions", &div.to_string());
            }
            for a in attrs {
                if let Attr::Key { fifths, minor } = &a.kind {
                    x.open("key");
                    x.leaf("fifths", &fifths.to_string());
                    if *minor {
                        x.leaf("mode", "minor");
                    }
                    x.close("key");
                }
            }
            for a in attrs {
                if let Attr::Time { beats, beat_type, symbol, print } = &a.kind {
                    let mut t = String::from("time");
                    match symbol {
                        Some(TimeSymbol::Common) => t.push_str(" symbol=\"common\""),
                        Some(TimeSymbol::Cut) => t.push_str(" symbol=\"cut\""),
                        None => {}
                    }
                    if !*print {
                        t.push_str(" print-object=\"no\"");
                    }
                    x.open(&t);
                    x.leaf("beats", beats);
                    x.leaf("beat-type", &beat_type.to_string());
                    x.close("time");
                }
            }
            if first && multi {
                x.leaf("staves", &p.staves.to_string());
            }
            for a in attrs {
                if let Attr::Clef { staff, sign, line, octave } = &a.kind {
                    if multi {
                        x.open(&format!("clef number=\"{}\"", staff));
                    } else {
                        x.open("clef");
                    }
                    let s = match sign {
                        'P' => "percussion".to_string(),
                        'T' => "TAB".to_string(),
                        c => c.to_string(),
                    };
                    x.leaf("sign", &s);
                    if *line > 0 {
                        x.leaf("line", &line.to_string());
                    }
                    if *octave != 0 {
                        x.leaf("clef-octave-change", &octave.to_string());
                    }
                    x.close("clef");
                }
            }
            if first {
                if let Some(n) = p.staff_lines {
                    x.open("staff-details");
                    x.leaf("staff-lines", &n.to_string());
                    x.close("staff-details");
                }
            }
            x.close("attributes");
        };
        let start_attrs: Vec<&AttrChange> = m.attrs.iter().filter(|a| a.pos.is_zero()).collect();
        write_attrs(x, &start_attrs, mi == 0);
        write_barline(x, &m.left, true);
        // chord symbols, each at its position
        if !m.harmonies.is_empty() {
            let mut at = Frac::ZERO;
            const STEPS: [&str; 7] = ["C", "D", "E", "F", "G", "A", "B"];
            for h in &m.harmonies {
                if h.pos > at {
                    x.open("forward");
                    x.leaf("duration", &ticks(h.pos - at).to_string());
                    x.close("forward");
                    at = h.pos;
                }
                x.open("harmony");
                x.open("root");
                x.leaf("root-step", STEPS[h.root.0 as usize % 7]);
                if h.root.1 != 0 {
                    x.leaf("root-alter", &h.root.1.to_string());
                }
                x.close("root");
                x.leaf("kind", &h.kind);
                if let Some(b) = h.bass {
                    x.open("bass");
                    x.leaf("bass-step", STEPS[b.0 as usize % 7]);
                    if b.1 != 0 {
                        x.leaf("bass-alter", &b.1.to_string());
                    }
                    x.close("bass");
                }
                for (v, a, t) in &h.degrees {
                    x.open("degree");
                    x.leaf("degree-value", &v.to_string());
                    x.leaf("degree-alter", &a.to_string());
                    x.leaf(
                        "degree-type",
                        match t {
                            'l' => "alter",
                            's' => "subtract",
                            _ => "add",
                        },
                    );
                    x.close("degree");
                }
                x.close("harmony");
            }
            if !at.is_zero() {
                x.open("backup");
                x.leaf("duration", &ticks(at).to_string());
                x.close("backup");
            }
        }

        // each direction is written in front of the note that starts at its position in
        // its voice; the rest are placed with <forward> afterwards
        let mut dir_done = vec![false; m.directions.len()];
        let mut attr_done: Vec<bool> = m.attrs.iter().map(|a| a.pos.is_zero()).collect();
        let dir_target = |d: &Direction| -> Option<(u32, u32)> {
            if let Some(v) = d.voice {
                if m.voices.iter().any(|s| s.staff == d.staff && s.voice == v) {
                    return Some((d.staff, v));
                }
            }
            m.voices.iter().filter(|s| s.staff == d.staff).map(|s| (s.staff, s.voice)).min()
        };
        let mut cursor = Frac::ZERO;
        let move_to = |x: &mut X, cursor: &mut Frac, pos: Frac| {
            if pos > *cursor {
                x.open("forward");
                x.leaf("duration", &ticks(pos - *cursor).to_string());
                x.close("forward");
            } else if pos < *cursor {
                x.open("backup");
                x.leaf("duration", &ticks(*cursor - pos).to_string());
                x.close("backup");
            }
            *cursor = pos;
        };
        for (vi, v) in m.voices.iter().enumerate() {
            let first_on_staff = m.voices.iter().position(|s| s.staff == v.staff) == Some(vi);
            for (ei, e) in v.events.iter().enumerate() {
                if e.pos != cursor {
                    move_to(x, &mut cursor, e.pos);
                }
                // mid-bar clef / key changes ride with the first voice of the staff
                if first_on_staff {
                    let here: Vec<usize> = (0..m.attrs.len())
                        .filter(|&i| {
                            !attr_done[i]
                                && m.attrs[i].pos <= e.pos
                                && match &m.attrs[i].kind {
                                    Attr::Clef { staff, .. } => *staff == v.staff,
                                    _ => v.staff == 1,
                                }
                        })
                        .collect();
                    if !here.is_empty() {
                        let list: Vec<&AttrChange> = here.iter().map(|&i| &m.attrs[i]).collect();
                        write_attrs(x, &list, false);
                        for i in here {
                            attr_done[i] = true;
                        }
                    }
                }
                let first_at_pos = ei == 0 || v.events[ei - 1].pos != e.pos;
                if first_at_pos {
                    for (di, d) in m.directions.iter().enumerate() {
                        if !dir_done[di] && d.pos == e.pos && dir_target(d) == Some((v.staff, v.voice)) {
                            write_direction(x, d, multi, &mut ottava_size);
                            dir_done[di] = true;
                        }
                    }
                }
                write_event(x, e, v, multi, &defaults[mi][vi][ei], &ticks, m.len);
                cursor = e.pos + e.dur;
            }
        }
        // leftovers
        let left_attrs: Vec<usize> = (0..m.attrs.len()).filter(|&i| !attr_done[i]).collect();
        for i in left_attrs {
            move_to(x, &mut cursor, m.attrs[i].pos.min(m.len));
            write_attrs(x, &[&m.attrs[i]], false);
        }
        for (di, d) in m.directions.iter().enumerate() {
            if !dir_done[di] {
                move_to(x, &mut cursor, d.pos.min(m.len));
                write_direction(x, d, multi, &mut ottava_size);
            }
        }
        if cursor != m.len {
            move_to(x, &mut cursor, m.len);
        }
        write_barline(x, &m.right, false);
        x.close("measure");
    }
    x.close("part");
}

fn write_barline(x: &mut X, b: &Barline, left: bool) {
    if b.is_plain() {
        return;
    }
    x.open(&format!("barline location=\"{}\"", if left { "left" } else { "right" }));
    let style = b.style.clone().or_else(|| if b.repeat { Some(if left { "heavy-light".to_string() } else { "light-heavy".to_string() }) } else { None });
    if let Some(s) = style {
        x.leaf("bar-style", &s);
    }
    if let Some(e) = &b.ending_start {
        let text = e.text.clone().unwrap_or_else(|| format!("{}.", e.number));
        x.leaf(&format!("ending number=\"{}\" type=\"start\"", esc(&e.number)), &text);
    }
    if let Some(e) = &b.ending_stop {
        x.empty(&format!("ending number=\"{}\" type=\"{}\"", esc(&e.number), if e.open { "discontinue" } else { "stop" }));
    }
    if b.repeat {
        let mut t = format!("repeat direction=\"{}\"", if left { "forward" } else { "backward" });
        if let Some(n) = b.times {
            let _ = write!(t, " times=\"{}\"", n);
        }
        x.empty(&t);
    }
    x.close("barline");
}

fn write_direction(x: &mut X, d: &Direction, multi: bool, ottava_size: &mut i32) {
    let mut tag = String::from("direction");
    match d.placement {
        Some(Placement::Above) => tag.push_str(" placement=\"above\""),
        Some(Placement::Below) => tag.push_str(" placement=\"below\""),
        None => {}
    }
    x.open(&tag);
    let mut sound: Option<String> = None;
    match &d.kind {
        DirKind::Dynamic(name) => {
            x.open("direction-type");
            x.open("dynamics");
            const KNOWN: [&str; 24] = [
                "p", "pp", "ppp", "pppp", "ppppp", "pppppp", "f", "ff", "fff", "ffff", "fffff", "ffffff", "mp", "mf", "sf", "sfp", "sfpp", "fp", "rf", "rfz",
                "sfz", "sffz", "fz", "n",
            ];
            if KNOWN.contains(&name.as_str()) {
                x.empty(name);
            } else {
                x.leaf("other-dynamics", name);
            }
            x.close("dynamics");
            x.close("direction-type");
        }
        DirKind::Wedge(w) => {
            x.open("direction-type");
            x.empty(&format!(
                "wedge type=\"{}\"",
                match w {
                    Wedge::Cresc => "crescendo",
                    Wedge::Dim => "diminuendo",
                    Wedge::Stop => "stop",
                }
            ));
            x.close("direction-type");
        }
        DirKind::Words { text, italic, bold } => {
            x.open("direction-type");
            let mut t = String::from("words");
            if *italic {
                t.push_str(" font-style=\"italic\"");
            }
            if *bold {
                t.push_str(" font-weight=\"bold\"");
            }
            x.leaf(&t, text);
            x.close("direction-type");
        }
        DirKind::Tempo { text, metronome } => {
            if let Some(t) = text {
                x.open("direction-type");
                x.leaf("words font-weight=\"bold\"", t);
                x.close("direction-type");
            }
            if let Some((unit, pm)) = metronome {
                x.open("direction-type");
                x.open("metronome");
                x.leaf("beat-unit", type_name(unit.log));
                for _ in 0..unit.dots {
                    x.empty("beat-unit-dot");
                }
                x.leaf("per-minute", pm);
                x.close("metronome");
                x.close("direction-type");
                if let Ok(n) = pm.trim().parse::<f64>() {
                    let quarters = n * unit.len().to_f64() * 4.0;
                    sound = Some(format!("sound tempo=\"{}\"", fmt_num(quarters)));
                }
            }
        }
        DirKind::Rehearsal(t) => {
            x.open("direction-type");
            x.leaf("rehearsal", t);
            x.close("direction-type");
        }
        DirKind::Segno => {
            x.open("direction-type");
            x.empty("segno");
            x.close("direction-type");
        }
        DirKind::Coda => {
            x.open("direction-type");
            x.empty("coda");
            x.close("direction-type");
        }
        DirKind::Ottava(n) => {
            x.open("direction-type");
            if *n == 0 {
                x.empty(&format!("octave-shift type=\"stop\" size=\"{}\"", ottava_size));
            } else {
                *ottava_size = if n.abs() >= 2 { 15 } else { 8 };
                x.empty(&format!("octave-shift type=\"{}\" size=\"{}\"", if *n > 0 { "down" } else { "up" }, ottava_size));
            }
            x.close("direction-type");
        }
        DirKind::Pedal(p) => {
            x.open("direction-type");
            x.empty(&format!(
                "pedal type=\"{}\"",
                match p {
                    Pedal::Start => "start",
                    Pedal::Stop => "stop",
                    Pedal::Change => "change",
                }
            ));
            x.close("direction-type");
        }
    }
    if let Some(v) = d.voice {
        x.leaf("voice", &v.to_string());
    }
    if multi {
        x.leaf("staff", &d.staff.to_string());
    }
    if let Some(s) = sound {
        x.empty(&s);
    }
    x.close("direction");
}

fn write_event(x: &mut X, e: &Event, v: &VoiceSeg, multi: bool, default_acc: &[bool], ticks: &dyn Fn(Frac) -> i64, bar_len: Frac) {
    let scale = e.tuplet.map(|t| Frac::new(t.normal as i64, t.actual as i64)).unwrap_or(Frac::int(1));
    let ntype: Option<NoteType> = if e.grace.is_some() {
        Some(e.ntype.unwrap_or(NoteType { log: 3, dots: 0 }))
    } else {
        match e.ntype {
            Some(t) if t.len() * scale == e.dur => Some(t),
            _ => NoteType::from_len(e.dur / scale),
        }
    };
    let whole_rest = matches!(e.kind, EventKind::Rest { whole_measure: true });
    let count = e.notes().len().max(1);
    for ni in 0..count {
        x.open("note");
        if let Some(g) = e.grace {
            x.empty(if g.slash { "grace slash=\"yes\"" } else { "grace" });
        }
        if e.cue {
            x.empty("cue");
        }
        if ni > 0 {
            x.empty("chord");
        }
        let note = e.notes().get(ni);
        match note {
            Some(n) if e.unpitched => {
                x.open("unpitched");
                x.leaf("display-step", &["C", "D", "E", "F", "G", "A", "B"][n.step as usize % 7].to_string());
                x.leaf("display-octave", &n.octave.to_string());
                x.close("unpitched");
            }
            Some(n) => {
                x.open("pitch");
                x.leaf("step", &["C", "D", "E", "F", "G", "A", "B"][n.step as usize % 7].to_string());
                if n.alter != 0 {
                    x.leaf("alter", &n.alter.to_string());
                }
                x.leaf("octave", &n.octave.to_string());
                x.close("pitch");
            }
            None => {
                if whole_rest {
                    x.empty("rest measure=\"yes\"");
                } else {
                    x.empty("rest");
                }
            }
        }
        if e.grace.is_none() {
            x.leaf("duration", &ticks(e.dur).to_string());
        }
        if let Some(n) = note {
            if n.tie_stop {
                x.empty("tie type=\"stop\"");
            }
            if n.tie_start {
                x.empty("tie type=\"start\"");
            }
        }
        x.leaf("voice", &v.voice.to_string());
        if !(whole_rest && e.dur == bar_len) {
            if let Some(t) = ntype {
                x.leaf("type", type_name(t.log));
                for _ in 0..t.dots {
                    x.empty("dot");
                }
            }
        }
        if let Some(n) = note {
            let shown = n.acc_shown || n.acc_caut || default_acc.get(ni).copied().unwrap_or(false);
            if shown {
                let name = match n.alter {
                    i8::MIN..=-2 => "flat-flat",
                    -1 => "flat",
                    0 => "natural",
                    1 => "sharp",
                    _ => "double-sharp",
                };
                x.leaf(if n.acc_caut { "accidental parentheses=\"yes\"" } else { "accidental" }, name);
            }
        }
        if let Some(t) = e.tuplet {
            x.open("time-modification");
            x.leaf("actual-notes", &t.actual.to_string());
            x.leaf("normal-notes", &t.normal.to_string());
            x.close("time-modification");
        }
        if ni == 0 {
            match e.stem {
                Some(StemDir::Up) => x.leaf("stem", "up"),
                Some(StemDir::Down) => x.leaf("stem", "down"),
                None => {}
            }
        }
        if e.notehead_x && note.is_some() {
            x.leaf("notehead", "x");
        }
        if multi {
            x.leaf("staff", &v.staff.to_string());
        }
        if ni == 0 {
            match e.beam {
                Some(Beam::Begin) => x.leaf("beam number=\"1\"", "begin"),
                Some(Beam::Continue) => x.leaf("beam number=\"1\"", "continue"),
                Some(Beam::End) => x.leaf("beam number=\"1\"", "end"),
                None => {}
            }
        }
        // notations
        let mut nx = X { s: String::new(), depth: x.depth + 1 };
        if let Some(n) = note {
            if n.tie_stop {
                nx.empty("tied type=\"stop\"");
            }
            if n.tie_start {
                nx.empty(&format!("tied{} type=\"start\"", line_attr(e.tie_style)));
            }
        }
        if ni == 0 {
            for s in &e.slur_stops {
                nx.empty(&format!("slur number=\"{}\" type=\"stop\"", s));
            }
            for s in &e.slur_starts {
                let place = if e.double_slur { if Some(s) == e.slur_starts.first() { " placement=\"above\"" } else { " placement=\"below\"" } } else { "" };
                nx.empty(&format!("slur{} number=\"{}\"{} type=\"start\"", line_attr(e.slur_style), s, place));
            }
            if e.tuplet_stop {
                nx.empty("tuplet type=\"stop\"");
            }
            if e.tuplet_start {
                let mut t = String::from("tuplet type=\"start\"");
                if let Some(b) = e.tuplet_bracket {
                    let _ = write!(t, " bracket=\"{}\"", if b { "yes" } else { "no" });
                }
                if !e.tuplet_number {
                    t.push_str(" show-number=\"none\"");
                }
                nx.empty(&t);
            }
            let mut arts: Vec<&str> = vec![];
            let mut orns: Vec<String> = vec![];
            let mut tech: Vec<String> = vec![];
            let mut other: Vec<String> = vec![];
            for a in &e.arts {
                match a {
                    Art::Staccato => arts.push("staccato"),
                    Art::Staccatissimo => arts.push("staccatissimo"),
                    Art::Accent => arts.push("accent"),
                    Art::StrongAccent => arts.push("strong-accent"),
                    Art::Tenuto => arts.push("tenuto"),
                    Art::DetachedLegato => arts.push("detached-legato"),
                    Art::Breath => arts.push("breath-mark"),
                    Art::Falloff => arts.push("falloff"),
                    Art::Doit => arts.push("doit"),
                    Art::Caesura => arts.push("caesura"),
                    Art::Trill => orns.push("<trill-mark/>".into()),
                    Art::Mordent => orns.push("<mordent/>".into()),
                    Art::Prall => orns.push("<inverted-mordent/>".into()),
                    Art::Turn => orns.push("<turn/>".into()),
                    Art::Tremolo(n) => orns.push(format!("<tremolo type=\"single\">{}</tremolo>", n)),
                    Art::UpBow => tech.push("<up-bow/>".into()),
                    Art::DownBow => tech.push("<down-bow/>".into()),
                    Art::Harmonic => tech.push("<harmonic/>".into()),
                    Art::OpenString => tech.push("<open-string/>".into()),
                    Art::Stopped => tech.push("<stopped/>".into()),
                    Art::Fingering(f) => tech.push(format!("<fingering>{}</fingering>", esc(f))),
                    Art::Fermata { inverted } => other.push(if *inverted { "<fermata type=\"inverted\"/>".into() } else { "<fermata/>".into() }),
                    Art::Arpeggio => other.push("<arpeggiate/>".into()),
                    Art::Glissando => other.push("<glissando type=\"start\"/>".into()),
                    Art::GlissandoWavy => other.push("<glissando line-type=\"wavy\" type=\"start\"/>".into()),
                }
            }
            let mut group = |nx: &mut X, name: &str, items: &[String]| {
                if !items.is_empty() {
                    nx.open(name);
                    for i in items {
                        nx.pad();
                        nx.s.push_str(i);
                        nx.s.push('\n');
                    }
                    nx.close(name);
                }
            };
            group(&mut nx, "ornaments", &orns);
            group(&mut nx, "technical", &tech);
            let arts_s: Vec<String> = arts.iter().map(|a| format!("<{}/>", a)).collect();
            group(&mut nx, "articulations", &arts_s);
            for o in &other {
                nx.pad();
                nx.s.push_str(o);
                nx.s.push('\n');
            }
        }
        if !nx.s.is_empty() {
            x.open("notations");
            x.s.push_str(&nx.s);
            x.close("notations");
        }
        if ni == 0 {
            for l in &e.lyrics {
                let mut t = format!("lyric number=\"{}\"", l.line);
                if let Some(st) = &l.stanza {
                    let _ = write!(t, " name=\"{}\"", esc(st));
                }
                x.open(&t);
                x.leaf(
                    "syllabic",
                    match l.syllabic {
                        Syllabic::Single => "single",
                        Syllabic::Begin => "begin",
                        Syllabic::Middle => "middle",
                        Syllabic::End => "end",
                    },
                );
                x.leaf("text", &l.text);
                if l.extend {
                    x.empty("extend");
                }
                x.close("lyric");
            }
        }
        x.close("note");
    }
}

pub fn write_mxl(xml: &str) -> Result<Vec<u8>, String> {
    use zip::write::SimpleFileOptions;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let e = |e: zip::result::ZipError| e.to_string();
        z.start_file("mimetype", stored).map_err(e)?;
        z.write_all(b"application/vnd.recordare.musicxml").map_err(|e| e.to_string())?;
        z.start_file("META-INF/container.xml", deflated).map_err(e)?;
        z.write_all(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container>\n  <rootfiles>\n    <rootfile full-path=\"score.musicxml\" media-type=\"application/vnd.recordare.musicxml+xml\"/>\n  </rootfiles>\n</container>\n",
        )
        .map_err(|e| e.to_string())?;
        z.start_file("score.musicxml", deflated).map_err(e)?;
        z.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
        z.finish().map_err(e)?;
    }
    Ok(buf.into_inner())
}
