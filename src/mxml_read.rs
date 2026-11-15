//! MusicXML (plain or compressed .mxl) -> score model.

use crate::diag::Diags;
use crate::ir::*;
use roxmltree::Node;
use std::io::Read;

pub fn read_musicxml(data: &[u8], diags: &mut Diags) -> Result<Score, String> {
    let xml_bytes: Vec<u8>;
    let bytes: &[u8] = if data.starts_with(b"PK") {
        xml_bytes = unzip_score(data)?;
        &xml_bytes
    } else {
        data
    };
    let text = decode(bytes).replace("\r\n", "\n");
    let opts = roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(&text, opts).map_err(|e| format!("XML error: {}", e))?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "score-partwise" => {}
        "score-timewise" => return Err("score-timewise MusicXML is not supported (convert to partwise first)".into()),
        other => return Err(format!("not a MusicXML score (root element <{}>)", other)),
    }
    let mut score = Score::default();
    read_header(root, &mut score);
    let mut ids: Vec<String> = vec![];
    if let Some(pl) = child(root, "part-list") {
        read_part_list(pl, &mut score, &mut ids);
    }
    for pnode in root.children().filter(|n| n.has_tag_name("part")) {
        let id = pnode.attribute("id").unwrap_or("").to_string();
        let idx = match ids.iter().position(|x| *x == id) {
            Some(i) => i,
            None => {
                ids.push(id.clone());
                score.parts.push(Part { id: id.clone(), name: id.clone(), staves: 1, ..Default::default() });
                score.part_list.push(PartListItem::Part(score.parts.len() - 1));
                score.parts.len() - 1
            }
        };
        read_part(pnode, &mut score.parts[idx], diags);
    }
    score.normalize();
    Ok(score)
}

fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(b).into_owned()
}

fn unzip_score(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| format!("not a valid .mxl archive: {}", e))?;
    let mut rootfile: Option<String> = None;
    if let Ok(mut f) = zip.by_name("META-INF/container.xml") {
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        if let Ok(doc) = roxmltree::Document::parse(&s) {
            rootfile = doc.descendants().find(|n| n.has_tag_name("rootfile")).and_then(|n| n.attribute("full-path")).map(String::from);
        }
    }
    let name = match rootfile {
        Some(n) => n,
        None => (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
            .find(|n| !n.starts_with("META-INF") && (n.ends_with(".xml") || n.ends_with(".musicxml")))
            .ok_or("no score found in the .mxl archive")?,
    };
    let mut f = zip.by_name(&name).map_err(|e| format!("{} in .mxl: {}", name, e))?;
    let mut out = vec![];
    f.read_to_end(&mut out).map_err(|e| format!("{} in .mxl: {}", name, e))?;
    Ok(out)
}

// ---------------------------------------------------------------- xml helpers

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}
fn children<'a, 'i: 'a>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.has_tag_name(name))
}
fn elems<'a, 'i>(n: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(|c| c.is_element())
}
/// all text below a node
fn all_text(n: Node) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        if d.is_text() {
            s.push_str(d.text().unwrap_or(""));
        }
    }
    s
}
fn text_of(n: Node, name: &str) -> Option<String> {
    child(n, name).map(|c| all_text(c).trim().to_string())
}
fn nonempty(s: Option<String>) -> Option<String> {
    s.filter(|x| !x.trim().is_empty())
}
fn yes(n: Node, attr: &str) -> bool {
    n.attribute(attr) == Some("yes")
}

// ---------------------------------------------------------------- header

fn read_header(root: Node, score: &mut Score) {
    let h = &mut score.header;
    let work = child(root, "work").and_then(|w| nonempty(text_of(w, "work-title")));
    let movement = nonempty(text_of(root, "movement-title"));
    match (work, movement) {
        (Some(w), Some(m)) if w != m => {
            h.title = Some(w);
            h.subtitle = Some(m);
        }
        (w, m) => h.title = w.or(m),
    }
    if let Some(id) = child(root, "identification") {
        for c in children(id, "creator") {
            let t = all_text(c).trim().to_string();
            if t.is_empty() {
                continue;
            }
            match c.attribute("type").unwrap_or("composer") {
                "composer" => h.composer = Some(t),
                "lyricist" | "poet" => h.poet = Some(t),
                "arranger" => h.arranger = Some(t),
                _ => {}
            }
        }
        let rights: Vec<String> = children(id, "rights").map(|r| all_text(r).trim().to_string()).filter(|s| !s.is_empty()).collect();
        if !rights.is_empty() {
            h.copyright = Some(rights.join("\n"));
        }
        if let Some(misc) = child(id, "miscellaneous") {
            for f in children(misc, "miscellaneous-field") {
                let v = all_text(f).trim().to_string();
                match f.attribute("name").unwrap_or("") {
                    "bar-numbers" => {
                        score.layout.bar_numbers = Some(v != "false" && v != "no" && v != "none");
                        score.layout.bar_numbers_all = v == "all";
                    }
                    "hide-empty" => score.layout.hide_empty = v == "true" || v == "yes",
                    "first-page" => score.layout.first_page = v.parse().ok(),
                    _ => {}
                }
            }
        }
    }
    if let Some(d) = child(root, "defaults") {
        let mut mm_per_tenth = None;
        if let Some(sc) = child(d, "scaling") {
            let mm: Option<f64> = text_of(sc, "millimeters").and_then(|s| s.parse().ok());
            let tenths: Option<f64> = text_of(sc, "tenths").and_then(|s| s.parse().ok());
            if let (Some(mm), Some(t)) = (mm, tenths) {
                if t > 0.0 && mm > 0.0 {
                    mm_per_tenth = Some(mm / t);
                    score.layout.staff_size = Some(((mm / t * 40.0) * 72.0 / 25.4 * 100.0).round() / 100.0);
                }
            }
        }
        if let (Some(pl), Some(k)) = (child(d, "page-layout"), mm_per_tenth) {
            score.layout.page_height_mm = text_of(pl, "page-height").and_then(|s| s.parse::<f64>().ok()).map(|v| v * k);
            score.layout.page_width_mm = text_of(pl, "page-width").and_then(|s| s.parse::<f64>().ok()).map(|v| v * k);
        }
    }
    // credits: typed ones fill what <identification> left open
    for cr in children(root, "credit") {
        let words: Vec<String> = children(cr, "credit-words").map(|w| all_text(w).trim().to_string()).filter(|s| !s.is_empty()).collect();
        if words.is_empty() {
            continue;
        }
        let text = words.join("\n");
        let h = &mut score.header;
        // a credit fills an empty field, and replaces the plain <creator> text when it is
        // the same words with *italic* marks
        let set = |slot: &mut Option<String>| {
            let same = slot.as_deref().map(|s| s.trim() == text.replace('*', "").trim()).unwrap_or(false);
            if slot.is_none() || same {
                *slot = Some(text.clone());
            }
        };
        match text_of(cr, "credit-type").unwrap_or_default().as_str() {
            "title" => set(&mut h.title),
            "subtitle" => set(&mut h.subtitle),
            "dedication" => set(&mut h.dedication),
            "composer" => set(&mut h.composer),
            "lyricist" | "poet" => set(&mut h.poet),
            "arranger" => set(&mut h.arranger),
            "rights" | "copyright" => set(&mut h.copyright),
            "footnote" => set(&mut h.footnote),
            "composer_lines" => h.composer_lines.push(text.clone()),
            "poet_lines" => h.poet_lines.push(text.clone()),
            _ => {}
        }
    }
}

fn display_text(n: Node) -> String {
    let mut s = String::new();
    for c in elems(n) {
        match c.tag_name().name() {
            "display-text" => s.push_str(c.text().unwrap_or("")),
            "accidental-text" => s.push_str(match c.text().unwrap_or("").trim() {
                "flat" => "\u{266d}",
                "sharp" => "\u{266f}",
                "natural" => "\u{266e}",
                _ => "",
            }),
            _ => {}
        }
    }
    s
}

fn read_part_list(pl: Node, score: &mut Score, ids: &mut Vec<String>) {
    for c in elems(pl) {
        match c.tag_name().name() {
            "part-group" => {
                let number = c.attribute("number").unwrap_or("1").to_string();
                if c.attribute("type") == Some("start") {
                    let symbol = match text_of(c, "group-symbol").as_deref() {
                        Some("bracket") => GroupSymbol::Bracket,
                        Some("brace") => GroupSymbol::Brace,
                        Some("line") => GroupSymbol::Line,
                        Some("square") => GroupSymbol::Square,
                        _ => GroupSymbol::None,
                    };
                    let name = child(c, "group-name-display").map(display_text).or_else(|| text_of(c, "group-name"));
                    let barline = text_of(c, "group-barline").map(|b| b == "yes");
                    score.part_list.push(PartListItem::GroupStart { number, symbol, name: nonempty(name), barline });
                } else {
                    score.part_list.push(PartListItem::GroupStop { number });
                }
            }
            "score-part" => {
                let id = c.attribute("id").unwrap_or("").to_string();
                let mut p = Part { id: id.clone(), staves: 1, ..Default::default() };
                let pn = child(c, "part-name");
                p.name = pn.map(|n| all_text(n).trim().to_string()).unwrap_or_default();
                if let Some(d) = child(c, "part-name-display") {
                    p.display_name = Some(display_text(d));
                } else if pn.map(|n| n.attribute("print-object") == Some("no")).unwrap_or(false) {
                    p.display_name = Some(String::new());
                }
                if let Some(d) = child(c, "part-abbreviation-display") {
                    p.abbr = nonempty(Some(display_text(d)));
                } else if let Some(a) = child(c, "part-abbreviation") {
                    if a.attribute("print-object") != Some("no") {
                        p.abbr = nonempty(Some(all_text(a).trim().to_string()));
                    }
                }
                p.instrument = child(c, "score-instrument").and_then(|i| nonempty(text_of(i, "instrument-name")));
                p.name = p.name.replace('\r', "");
                p.display_name = p.display_name.map(|s| s.replace('\r', ""));
                p.abbr = p.abbr.map(|s| s.replace('\r', ""));
                ids.push(id);
                score.parts.push(p);
                score.part_list.push(PartListItem::Part(score.parts.len() - 1));
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- parts

fn note_log(t: &str) -> Option<i8> {
    Some(match t {
        "maxima" | "long" => -2,
        "breve" => -1,
        "whole" => 0,
        "half" => 1,
        "quarter" => 2,
        "eighth" => 3,
        "16th" => 4,
        "32nd" => 5,
        "64th" => 6,
        "128th" => 7,
        "256th" => 8,
        _ => return None,
    })
}

struct PartReader<'d> {
    diags: &'d mut Diags,
    pid: String,
    divisions: i64,
    /// current time signature length
    bar_len: Frac,
    lyric_lines: Vec<String>,
}

fn read_part(pnode: Node, part: &mut Part, diags: &mut Diags) {
    let mut r = PartReader { diags, pid: part.id.clone(), divisions: 1, bar_len: Frac::int(1), lyric_lines: vec![] };
    let mnodes: Vec<Node> = children(pnode, "measure").collect();
    let n = mnodes.len();
    for (i, mn) in mnodes.into_iter().enumerate() {
        let m = r.read_measure(mn, part, i == 0 || i + 1 == n);
        // a staff that is used counts even when <staves> was not declared
        for v in &m.voices {
            part.staves = part.staves.max(v.staff);
        }
        part.measures.push(m);
    }
}

impl<'d> PartReader<'d> {
    fn dur(&self, n: Node) -> Frac {
        let v: f64 = text_of(n, "duration").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        // durations are integers in practice; tolerate decimals
        if v.fract() == 0.0 {
            Frac::new(v as i64, self.divisions * 4)
        } else {
            Frac::new((v * 1000.0).round() as i64, self.divisions * 4000)
        }
    }

    fn read_measure(&mut self, mn: Node, part: &mut Part, edge: bool) -> Measure {
        let number = mn.attribute("number").unwrap_or("").to_string();
        let mut m = Measure { number: number.clone(), implicit: yes(mn, "implicit"), ..Default::default() };
        let mut cursor = Frac::ZERO;
        let mut reach = Frac::ZERO;
        // (staff, voice) of the last non-chord note, to attach chord notes
        let mut last_key: Option<(u32, u32)> = None;
        let mut any_note = false;

        for c in elems(mn) {
            match c.tag_name().name() {
                "attributes" => self.read_attributes(c, cursor, &mut m, part),
                "note" => {
                    any_note = true;
                    self.read_note(c, &mut cursor, &mut m, &mut last_key);
                    if cursor > reach {
                        reach = cursor;
                    }
                }
                "backup" => {
                    cursor = cursor - self.dur(c);
                    if cursor < Frac::ZERO {
                        cursor = Frac::ZERO;
                    }
                }
                "forward" => {
                    cursor += self.dur(c);
                    if cursor > reach {
                        reach = cursor;
                    }
                }
                "direction" => self.read_direction(c, cursor, &mut m),
                "barline" => self.read_barline(c, &mut m),
                "print" => {
                    m.new_system |= yes(c, "new-system");
                    m.new_page |= yes(c, "new-page");
                }
                "harmony" => {
                    let step = |s: Option<String>| -> Option<u8> { s.and_then(|s| "CDEFGAB".find(s.trim())).map(|i| i as u8) };
                    let alter = |n: Option<Node>, name: &str| -> i8 { n.and_then(|n| text_of(n, name)).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0).round() as i8 };
                    let root = child(c, "root");
                    match root.and_then(|r| step(text_of(r, "root-step"))) {
                        Some(rs) => {
                            let bass = child(c, "bass");
                            let degrees = children(c, "degree")
                                .filter_map(|d| {
                                    let v: u8 = text_of(d, "degree-value")?.parse().ok()?;
                                    let a = alter(Some(d), "degree-alter");
                                    let t = match text_of(d, "degree-type").as_deref() {
                                        Some("alter") => 'l',
                                        Some("subtract") => 's',
                                        _ => 'a',
                                    };
                                    Some((v, a, t))
                                })
                                .collect();
                            m.harmonies.push(Harmony {
                                pos: cursor,
                                root: (rs, alter(root, "root-alter")),
                                kind: text_of(c, "kind").unwrap_or_else(|| "major".into()),
                                bass: bass.and_then(|b| step(text_of(b, "bass-step"))).map(|s| (s, alter(bass, "bass-alter"))),
                                degrees,
                            });
                        }
                        None => self.diags.dropped(Some(&self.pid), Some(&number), "chord symbol without a root (function or numeral)"),
                    }
                }
                "figured-bass" => self.diags.dropped(Some(&self.pid), Some(&number), "figured bass is not supported"),
                _ => {}
            }
        }
        m.len = if reach.is_zero() { self.bar_len } else { reach };
        let _ = (any_note, edge);
        // two slurs starting on one chord, placed apart: a double slur
        for v in &mut m.voices {
            for e in &mut v.events {
                e.double_slur = e.double_slur && e.slur_starts.len() >= 2 && e.notes().len() >= 2;
            }
        }
        // whole-measure rests
        for v in &mut m.voices {
            if v.events.len() == 1 {
                let e = &mut v.events[0];
                if let EventKind::Rest { whole_measure } = &mut e.kind {
                    let whole_type = e.ntype.map(|t| t.log == 0 && t.dots == 0).unwrap_or(true);
                    if e.dur == m.len && e.pos.is_zero() && (*whole_measure || whole_type) {
                        *whole_measure = true;
                    } else {
                        *whole_measure = false;
                    }
                }
            } else {
                for e in &mut v.events {
                    if let EventKind::Rest { whole_measure } = &mut e.kind {
                        *whole_measure = false;
                    }
                }
            }
        }
        m
    }

    fn read_attributes(&mut self, a: Node, pos: Frac, m: &mut Measure, part: &mut Part) {
        for c in elems(a) {
            match c.tag_name().name() {
                "divisions" => {
                    if let Some(d) = c.text().and_then(|t| t.trim().parse::<f64>().ok()) {
                        if d >= 1.0 {
                            self.divisions = d as i64;
                        }
                    }
                }
                "key" => {
                    if c.attribute("number").map(|n| n != "1").unwrap_or(false) {
                        continue;
                    }
                    match text_of(c, "fifths").and_then(|s| s.parse::<i8>().ok()) {
                        Some(fifths) => {
                            let minor = text_of(c, "mode").as_deref() == Some("minor");
                            m.attrs.push(AttrChange { pos, kind: Attr::Key { fifths, minor } });
                        }
                        None => self.diags.dropped(Some(&self.pid), Some(&m.number), "non-traditional key signature"),
                    }
                }
                "time" => {
                    if c.attribute("number").map(|n| n != "1").unwrap_or(false) {
                        continue;
                    }
                    if child(c, "senza-misura").is_some() {
                        self.diags.warn(Some(&self.pid), Some(&m.number), "senza misura: bar lengths taken from the notes");
                        continue;
                    }
                    let beats: Vec<String> = children(c, "beats").map(|b| all_text(b).trim().to_string()).collect();
                    let types: Vec<u32> = children(c, "beat-type").filter_map(|b| all_text(b).trim().parse().ok()).collect();
                    if beats.is_empty() || types.is_empty() {
                        continue;
                    }
                    if types.len() > 1 {
                        self.diags.warn(Some(&self.pid), Some(&m.number), "composite time signature: only its first part is kept");
                    }
                    let symbol = match c.attribute("symbol") {
                        Some("common") => Some(TimeSymbol::Common),
                        Some("cut") => Some(TimeSymbol::Cut),
                        _ => None,
                    };
                    let t = Attr::Time { beats: beats[0].clone(), beat_type: types[0], symbol, print: c.attribute("print-object") != Some("no") };
                    if let Some(l) = t.measure_len() {
                        self.bar_len = l;
                    }
                    m.attrs.push(AttrChange { pos, kind: t });
                }
                "clef" => {
                    let staff = c.attribute("number").and_then(|s| s.parse().ok()).unwrap_or(1);
                    let sign = match text_of(c, "sign").as_deref() {
                        Some("G") => 'G',
                        Some("F") => 'F',
                        Some("C") => 'C',
                        Some("percussion") => 'P',
                        Some("TAB") => 'T',
                        _ => 'G',
                    };
                    let default_line = match sign {
                        'G' => 2,
                        'F' => 4,
                        'C' => 3,
                        _ => 0,
                    };
                    let line = text_of(c, "line").and_then(|s| s.parse().ok()).unwrap_or(default_line);
                    let octave = text_of(c, "clef-octave-change").and_then(|s| s.parse().ok()).unwrap_or(0);
                    m.attrs.push(AttrChange { pos, kind: Attr::Clef { staff, sign, line, octave } });
                }
                "staff-details" => {
                    if let Some(n) = text_of(c, "staff-lines").and_then(|s| s.parse::<u32>().ok()) {
                        if n != 5 {
                            part.staff_lines = Some(n);
                        }
                    }
                }
                "staves" => {
                    if let Some(n) = c.text().and_then(|t| t.trim().parse::<u32>().ok()) {
                        part.staves = part.staves.max(n);
                    }
                }
                "transpose" => self.diags.warn(Some(&self.pid), Some(&m.number), "transposing part: written pitches are kept, the transposition is not carried over"),
                _ => {}
            }
        }
    }

    fn lyric_line(&mut self, number: &str) -> u32 {
        if let Ok(n) = number.parse::<u32>() {
            if n >= 1 {
                return n;
            }
        }
        match self.lyric_lines.iter().position(|x| x == number) {
            Some(i) => 100 + i as u32,
            None => {
                self.lyric_lines.push(number.to_string());
                100 + self.lyric_lines.len() as u32 - 1
            }
        }
    }

    fn read_note(&mut self, n: Node, cursor: &mut Frac, m: &mut Measure, last_key: &mut Option<(u32, u32)>) {
        let pid = self.pid.clone();
        let mnum = m.number.clone();
        let grace = child(n, "grace");
        let is_chord = child(n, "chord").is_some();
        let dur = if grace.is_some() { Frac::ZERO } else { self.dur(n) };
        let voice: u32 = text_of(n, "voice").and_then(|s| s.parse().ok()).unwrap_or(1);
        let staff: u32 = text_of(n, "staff").and_then(|s| s.parse().ok()).unwrap_or(1);

        // pitch
        let mut note: Option<Note> = None;
        let rest = child(n, "rest");
        if let Some(p) = child(n, "pitch").or_else(|| child(n, "unpitched")) {
            let unpitched = p.has_tag_name("unpitched");
            let step_name = if unpitched { "display-step" } else { "step" };
            let oct_name = if unpitched { "display-octave" } else { "octave" };
            let step = match text_of(p, step_name).as_deref() {
                Some("C") => 0,
                Some("D") => 1,
                Some("E") => 2,
                Some("F") => 3,
                Some("G") => 4,
                Some("A") => 5,
                Some("B") => 6,
                _ => 6,
            };
            let alter_f: f64 = text_of(p, "alter").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            if alter_f.fract() != 0.0 {
                self.diags.warn(Some(&pid), Some(&mnum), "microtonal alteration rounded to a semitone");
            }
            let octave: i8 = text_of(p, oct_name).and_then(|s| s.parse().ok()).unwrap_or(4);
            let mut nt = Note::new(step, alter_f.round() as i8, octave);
            if let Some(acc) = child(n, "accidental") {
                nt.acc_shown = true;
                nt.acc_caut = yes(acc, "cautionary") || yes(acc, "parentheses") || yes(acc, "editorial");
            }
            note = Some(nt);
        }

        let mut ev = Event::new(*cursor, dur, EventKind::Rest { whole_measure: false });
        if let Some(r) = rest {
            ev.kind = EventKind::Rest { whole_measure: yes(r, "measure") };
        }
        ev.grace = grace.map(|g| Grace { slash: yes(g, "slash") });
        ev.cue = child(n, "cue").is_some();
        ev.unpitched = child(n, "unpitched").is_some();
        ev.notehead_x = matches!(text_of(n, "notehead").as_deref(), Some("x") | Some("cross") | Some("circle-x"));
        if let Some(t) = text_of(n, "type") {
            if let Some(log) = note_log(&t) {
                ev.ntype = Some(NoteType { log, dots: children(n, "dot").count() as u8 });
            }
        }
        if let Some(tm) = child(n, "time-modification") {
            let a = text_of(tm, "actual-notes").and_then(|s| s.parse().ok()).unwrap_or(1);
            let b = text_of(tm, "normal-notes").and_then(|s| s.parse().ok()).unwrap_or(1);
            if a != b && a > 0 && b > 0 {
                ev.tuplet = Some(Tuplet { actual: a, normal: b });
            }
        }
        ev.stem = match text_of(n, "stem").as_deref() {
            Some("up") => Some(StemDir::Up),
            Some("down") => Some(StemDir::Down),
            _ => None,
        };
        for b in children(n, "beam") {
            if b.attribute("number").unwrap_or("1") == "1" {
                ev.beam = match all_text(b).trim() {
                    "begin" => Some(Beam::Begin),
                    "continue" => Some(Beam::Continue),
                    "end" => Some(Beam::End),
                    _ => None,
                };
            }
        }
        // ties
        let (mut tie_start, mut tie_stop) = (false, false);
        for t in children(n, "tie") {
            match t.attribute("type") {
                Some("start") => tie_start = true,
                Some("stop") => tie_stop = true,
                _ => {}
            }
        }
        let mut dirs: Vec<Direction> = vec![];
        for nots in children(n, "notations") {
            for c in elems(nots) {
                match c.tag_name().name() {
                    "tied" => match c.attribute("type") {
                        Some("start") => {
                            tie_start = true;
                            ev.tie_style = line_style(c).max(ev.tie_style);
                        }
                        Some("stop") => tie_stop = true,
                        Some("continue") => {
                            tie_start = true;
                            tie_stop = true;
                        }
                        _ => {}
                    },
                    "slur" => {
                        let num: u8 = c.attribute("number").and_then(|s| s.parse().ok()).unwrap_or(1);
                        match c.attribute("type") {
                            Some("start") => {
                                ev.slur_starts.push(num);
                                ev.slur_style = line_style(c).max(ev.slur_style);
                                // a candidate for a double slur: decided once the chord is whole
                                ev.double_slur |= c.attribute("placement").is_some();
                            }
                            Some("stop") => ev.slur_stops.push(num),
                            _ => {}
                        }
                    }
                    "tuplet" => match c.attribute("type") {
                        Some("start") => {
                            ev.tuplet_start = true;
                            ev.tuplet_number = c.attribute("show-number") != Some("none");
                            ev.tuplet_bracket = c.attribute("bracket").map(|b| b == "yes");
                        }
                        Some("stop") => ev.tuplet_stop = true,
                        _ => {}
                    },
                    "articulations" => {
                        for a in elems(c) {
                            let art = match a.tag_name().name() {
                                "staccato" => Art::Staccato,
                                "staccatissimo" | "spiccato" => Art::Staccatissimo,
                                "accent" => Art::Accent,
                                "strong-accent" => Art::StrongAccent,
                                "tenuto" => Art::Tenuto,
                                "detached-legato" => Art::DetachedLegato,
                                "breath-mark" => Art::Breath,
                                "falloff" => Art::Falloff,
                                "doit" => Art::Doit,
                                "caesura" => Art::Caesura,
                                other => {
                                    self.diags.dropped(Some(&pid), Some(&mnum), format!("articulation <{}>", other));
                                    continue;
                                }
                            };
                            ev.arts.push(art);
                        }
                    }
                    "ornaments" => {
                        for a in elems(c) {
                            let art = match a.tag_name().name() {
                                "trill-mark" => Art::Trill,
                                "mordent" => Art::Mordent,
                                "inverted-mordent" => Art::Prall,
                                "turn" | "inverted-turn" | "delayed-turn" => Art::Turn,
                                "tremolo" => {
                                    if matches!(a.attribute("type"), Some("start") | Some("stop")) {
                                        self.diags.dropped(Some(&pid), Some(&mnum), "two-note tremolo marking (the notes are kept)");
                                        continue;
                                    }
                                    Art::Tremolo(all_text(a).trim().parse().unwrap_or(3))
                                }
                                "accidental-mark" | "wavy-line" => continue,
                                other => {
                                    self.diags.dropped(Some(&pid), Some(&mnum), format!("ornament <{}>", other));
                                    continue;
                                }
                            };
                            ev.arts.push(art);
                        }
                    }
                    "technical" => {
                        for a in elems(c) {
                            let art = match a.tag_name().name() {
                                "up-bow" => Art::UpBow,
                                "down-bow" => Art::DownBow,
                                "harmonic" => Art::Harmonic,
                                "open-string" => Art::OpenString,
                                "stopped" => Art::Stopped,
                                "fingering" => Art::Fingering(all_text(a).trim().to_string()),
                                other => {
                                    self.diags.dropped(Some(&pid), Some(&mnum), format!("technical mark <{}>", other));
                                    continue;
                                }
                            };
                            ev.arts.push(art);
                        }
                    }
                    "fermata" => ev.arts.push(Art::Fermata { inverted: c.attribute("type") == Some("inverted") }),
                    "arpeggiate" => ev.arts.push(Art::Arpeggio),
                    "glissando" | "slide" => {
                        if c.attribute("type") == Some("start") {
                            ev.arts.push(if c.attribute("line-type") == Some("wavy") { Art::GlissandoWavy } else { Art::Glissando });
                        }
                    }
                    "dynamics" => {
                        for d in elems(c) {
                            let name = if d.has_tag_name("other-dynamics") { all_text(d).trim().to_string() } else { d.tag_name().name().to_string() };
                            dirs.push(Direction {
                                pos: *cursor,
                                staff,
                                voice: Some(voice),
                                placement: placement(c),
                                kind: DirKind::Dynamic(name),
                            });
                        }
                    }
                    other => self.diags.dropped(Some(&pid), Some(&mnum), format!("notation <{}>", other)),
                }
            }
        }
        // lyrics
        for l in children(n, "lyric") {
            if l.attribute("print-object") == Some("no") {
                continue;
            }
            let mut text = String::new();
            let mut syllabic = Syllabic::Single;
            let mut first = true;
            let mut has_text = false;
            for c in elems(l) {
                match c.tag_name().name() {
                    "syllabic" if first => {
                        syllabic = match all_text(c).trim() {
                            "begin" => Syllabic::Begin,
                            "middle" => Syllabic::Middle,
                            "end" => Syllabic::End,
                            _ => Syllabic::Single,
                        };
                    }
                    "syllabic" => {
                        // a later syllable in an elision decides how the group ends
                        match all_text(c).trim() {
                            "begin" | "middle" => {
                                syllabic = if matches!(syllabic, Syllabic::Single | Syllabic::Begin) { Syllabic::Begin } else { Syllabic::Middle }
                            }
                            _ => syllabic = if matches!(syllabic, Syllabic::Single | Syllabic::Begin) { Syllabic::Single } else { Syllabic::End },
                        }
                    }
                    "text" => {
                        text.push_str(c.text().unwrap_or(""));
                        has_text = true;
                        first = false;
                    }
                    "elision" => text.push('\u{203f}'),
                    _ => {}
                }
            }
            if !has_text {
                continue;
            }
            let ext = child(l, "extend").map(|e| e.attribute("type").map(|t| t == "start").unwrap_or(true)).unwrap_or(false);
            let line = self.lyric_line(l.attribute("number").unwrap_or("1"));
            ev.lyrics.push(Lyric { line, stanza: l.attribute("name").filter(|s| !s.is_empty()).map(String::from), text, syllabic, extend: ext });
        }

        if is_chord {
            // merge into the previous event of that voice
            let key = last_key.unwrap_or((staff, voice));
            if let Some(seg) = m.voices.iter_mut().find(|v| (v.staff, v.voice) == key) {
                if let Some(prev) = seg.events.last_mut() {
                    if let (EventKind::Notes(ns), Some(mut nt)) = (&mut prev.kind, note.clone()) {
                        nt.tie_start = tie_start;
                        nt.tie_stop = tie_stop;
                        ns.push(nt);
                        prev.slur_starts.extend(ev.slur_starts);
                        prev.double_slur |= ev.double_slur;
                        prev.slur_style = prev.slur_style.max(ev.slur_style);
                        prev.tie_style = prev.tie_style.max(ev.tie_style);
                        prev.notehead_x |= ev.notehead_x;
                        prev.slur_stops.extend(ev.slur_stops);
                        for a in ev.arts {
                            if !prev.arts.contains(&a) {
                                prev.arts.push(a);
                            }
                        }
                        for l in ev.lyrics {
                            if !prev.lyrics.iter().any(|x| x.line == l.line) {
                                prev.lyrics.push(l);
                            }
                        }
                        for mut d in dirs {
                            d.pos = prev.pos;
                            m.directions.push(d);
                        }
                        return;
                    }
                }
            }
            self.diags.warn(Some(&pid), Some(&mnum), "chord note without a preceding note: read as a separate note");
        }
        if let Some(mut nt) = note {
            nt.tie_start = tie_start;
            nt.tie_stop = tie_stop;
            ev.kind = EventKind::Notes(vec![nt]);
        } else if rest.is_none() {
            // neither pitch nor rest: treat as a rest
            ev.kind = EventKind::Rest { whole_measure: false };
        }
        m.directions.extend(dirs);
        let key = (staff, voice);
        match m.voices.iter_mut().find(|v| (v.staff, v.voice) == key) {
            Some(seg) => seg.events.push(ev),
            None => m.voices.push(VoiceSeg { staff, voice, events: vec![ev] }),
        }
        *last_key = Some(key);
        *cursor += dur;
    }

    fn read_direction(&mut self, d: Node, pos: Frac, m: &mut Measure) {
        let staff: u32 = text_of(d, "staff").and_then(|s| s.parse().ok()).unwrap_or(1);
        let voice: Option<u32> = text_of(d, "voice").and_then(|s| s.parse().ok());
        let place = placement(d);
        let mut words: Vec<(String, bool, bool)> = vec![];
        let mut metronome: Option<(NoteType, String)> = None;
        let mut out: Vec<DirKind> = vec![];
        for dt in children(d, "direction-type") {
            for c in elems(dt) {
                match c.tag_name().name() {
                    "words" => {
                        let t = all_text(c);
                        if !t.trim().is_empty() {
                            words.push((t.trim().to_string(), c.attribute("font-style") == Some("italic"), c.attribute("font-weight") == Some("bold")));
                        }
                    }
                    "metronome" => {
                        let unit = text_of(c, "beat-unit").and_then(|u| note_log(&u));
                        let dots = children(c, "beat-unit-dot").count() as u8;
                        let pm = text_of(c, "per-minute");
                        match (unit, pm) {
                            (Some(log), Some(pm)) => metronome = Some((NoteType { log, dots }, pm)),
                            _ => self.diags.dropped(Some(&self.pid), Some(&m.number), "metronome mark of an unsupported form"),
                        }
                    }
                    "dynamics" => {
                        for dy in elems(c) {
                            let name = if dy.has_tag_name("other-dynamics") { all_text(dy).trim().to_string() } else { dy.tag_name().name().to_string() };
                            out.push(DirKind::Dynamic(name));
                        }
                    }
                    "wedge" => match c.attribute("type") {
                        Some("crescendo") => out.push(DirKind::Wedge(Wedge::Cresc)),
                        Some("diminuendo") => out.push(DirKind::Wedge(Wedge::Dim)),
                        Some("stop") => out.push(DirKind::Wedge(Wedge::Stop)),
                        _ => {}
                    },
                    "rehearsal" => out.push(DirKind::Rehearsal(all_text(c).trim().to_string())),
                    "segno" => out.push(DirKind::Segno),
                    "coda" => out.push(DirKind::Coda),
                    "octave-shift" => {
                        let size: i8 = c.attribute("size").and_then(|s| s.parse().ok()).unwrap_or(8);
                        let n = if size >= 15 { 2 } else { 1 };
                        match c.attribute("type") {
                            Some("down") => out.push(DirKind::Ottava(n)),
                            Some("up") => out.push(DirKind::Ottava(-n)),
                            Some("stop") => out.push(DirKind::Ottava(0)),
                            _ => {}
                        }
                    }
                    "pedal" => match c.attribute("type") {
                        Some("start") => out.push(DirKind::Pedal(Pedal::Start)),
                        Some("stop") => out.push(DirKind::Pedal(Pedal::Stop)),
                        Some("change") => out.push(DirKind::Pedal(Pedal::Change)),
                        _ => {}
                    },
                    "dashes" | "bracket" => {}
                    other => self.diags.dropped(Some(&self.pid), Some(&m.number), format!("direction <{}>", other)),
                }
            }
        }
        let has_sound_tempo = child(d, "sound").map(|s| s.attribute("tempo").is_some()).unwrap_or(false);
        if metronome.is_some() {
            let text = if words.is_empty() { None } else { Some(words.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ")) };
            out.insert(0, DirKind::Tempo { text, metronome });
        } else if !words.is_empty() {
            let all_bold = words.iter().all(|w| w.2);
            if (all_bold || has_sound_tempo) && place != Some(Placement::Below) {
                out.insert(0, DirKind::Tempo { text: Some(words.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ")), metronome: None });
            } else {
                for (i, (text, italic, bold)) in words.into_iter().enumerate() {
                    out.insert(i, DirKind::Words { text, italic, bold });
                }
            }
        }
        for kind in out {
            m.directions.push(Direction { pos, staff, voice, placement: place, kind });
        }
    }

    fn read_barline(&mut self, b: Node, m: &mut Measure) {
        let loc = b.attribute("location").unwrap_or("right");
        if loc == "middle" {
            self.diags.dropped(Some(&self.pid), Some(&m.number), "barline in the middle of a bar");
            return;
        }
        let left = loc == "left";
        let bl = if left { &mut m.left } else { &mut m.right };
        if let Some(s) = text_of(b, "bar-style") {
            if s != "regular" {
                bl.style = Some(s);
            }
        }
        if let Some(r) = child(b, "repeat") {
            let fwd = r.attribute("direction") == Some("forward");
            if fwd == left {
                bl.repeat = true;
                bl.times = r.attribute("times").and_then(|t| t.parse().ok());
            } else if fwd {
                // forward repeat written on a right barline: it opens the next bar; keep as style
                self.diags.warn(Some(&self.pid), Some(&m.number), "forward repeat on a right barline ignored");
            } else {
                self.diags.warn(Some(&self.pid), Some(&m.number), "backward repeat on a left barline ignored");
            }
        }
        if let Some(e) = child(b, "ending") {
            let number = e.attribute("number").unwrap_or("1").to_string();
            match e.attribute("type") {
                Some("start") => {
                    let text = nonempty(Some(all_text(e).trim().to_string()));
                    m.left.ending_start = Some(Ending { number, text });
                }
                Some("stop") => m.right.ending_stop = Some(EndingStop { number, open: false }),
                Some("discontinue") => m.right.ending_stop = Some(EndingStop { number, open: true }),
                _ => {}
            }
        }
    }
}

/// 0 solid, 1 dashed, 2 dotted
fn line_style(n: Node) -> u8 {
    match n.attribute("line-type") {
        Some("dashed") => 1,
        Some("dotted") => 2,
        _ => 0,
    }
}

fn placement(n: Node) -> Option<Placement> {
    match n.attribute("placement") {
        Some("above") => Some(Placement::Above),
        Some("below") => Some(Placement::Below),
        _ => None,
    }
}
