//! Cuts the interpreter's timelines into measures and assembles the score model.

use super::interp::{Interp, ListItem, TDir, TEv};
use crate::diag::Level;
use crate::ir::*;

struct MInfo {
    start: Frac,
    len: Frac,
    number: String,
    implicit: bool,
}

/// what a LilyPond bar type means for the bar that ends here (right) and the one that starts (left)
fn bar_meaning(s: &str) -> (Option<(&'static str, bool)>, Option<(&'static str, bool)>) {
    // a double or final bar that a repeat starts after
    if s == ".|:-||" {
        return (Some(("heavy-light", true)), Some(("light-light", false)));
    }
    if s == ".|:-|." {
        return (Some(("heavy-light", true)), Some(("light-heavy", false)));
    }
    let base = s.split('-').next().unwrap_or(s);
    match base {
        "" => (None, Some(("none", false))),
        "|." => (None, Some(("light-heavy", false))),
        "||" => (None, Some(("light-light", false))),
        ".|" => (None, Some(("heavy-light", false))),
        ".." => (None, Some(("heavy-heavy", false))),
        "." => (None, Some(("heavy", false))),
        "!" => (None, Some(("dashed", false))),
        ";" => (None, Some(("dotted", false))),
        "'" => (None, Some(("tick", false))),
        "," => (None, Some(("short", false))),
        ".|:" | "[|:" | "|:" => (Some(("heavy-light", true)), None),
        ":|." | ":|]" | ":|" => (None, Some(("light-heavy", true))),
        ":..:" | ":|.|:" | ":|.:" | ":.|.:" | ":|][|:" | ":|:" => (Some(("heavy-light", true)), Some(("light-heavy", true))),
        _ => (None, None),
    }
}

fn measure_infos(it: &Interp) -> Vec<MInfo> {
    let g = &it.glob;
    let total = g.end;
    let mut out = vec![];
    let mut cur_len = Frac::int(1);
    let mut override_len: Option<Frac> = None;
    let mut t = Frac::ZERO;
    let mut num: i64 = 1;
    let mut carry: Option<Frac> = None;
    // unmetered stretches; every voice states them, so merge the overlapping ones
    let mut cadenzas: Vec<(Frac, Frac)> = g.cadenzas.iter().map(|(a, b)| (*a, b.unwrap_or(total))).filter(|(a, b)| b > a).collect();
    cadenzas.sort();
    let mut merged: Vec<(Frac, Frac)> = vec![];
    for (a, b) in cadenzas {
        match merged.last_mut() {
            Some(last) if a <= last.1 => {
                if b > last.1 {
                    last.1 = b;
                }
            }
            _ => merged.push((a, b)),
        }
    }
    let cadenzas = merged;
    while t < total && out.len() < 100_000 {
        for (tt, beats, bt, _, _) in &g.times {
            if *tt <= t {
                if let Some(l) = (Attr::Time { beats: beats.clone(), beat_type: *bt, symbol: None, print: true }).measure_len() {
                    cur_len = l;
                }
            }
        }
        // the latest of \time and measureLength wins
        let last_time = g.times.iter().filter(|x| x.0 <= t).map(|x| x.0).max();
        override_len = match g.mlens.iter().filter(|x| x.0 <= t).last() {
            Some((mt, v)) if last_time.map(|lt| *mt >= lt).unwrap_or(true) => *v,
            Some(_) => None,
            None => override_len.and(None),
        };
        if let Some((_, n)) = g.barnums.iter().filter(|x| x.0 == t).last() {
            num = *n;
        }
        let mut len = override_len.unwrap_or(cur_len);
        let mut implicit = false;
        let mut counts = true;
        let mut number = num.to_string();
        if let Some(r) = carry.take() {
            len = r;
            implicit = true;
            counts = false;
            number = (num - 1).to_string();
        } else if let Some((_, p)) = g.partials.iter().find(|x| x.0 == t) {
            if *p < len {
                len = *p;
                implicit = true;
                counts = false;
                number = if t.is_zero() { "0".to_string() } else { (num - 1).to_string() };
            }
        }
        if len <= Frac::ZERO {
            len = Frac::int(1);
        }
        let mut end = t + len;
        for (a, b) in &cadenzas {
            if *a >= t && *a < end {
                end += *b - *a;
            }
        }
        // a bar line inside the bar splits it
        let split = g.bars.iter().filter(|(bt, _)| *bt > t && *bt < end).map(|x| x.0).min();
        if let Some(b) = split {
            carry = Some(end - b);
            end = b;
        }
        if let Some(tt) = g.times.iter().map(|x| x.0).chain(g.partials.iter().map(|x| x.0)).filter(|x| *x > t && *x < end).min() {
            end = tt;
            carry = None;
        }
        if end > total {
            end = total;
            carry = None;
        }
        if end <= t {
            break;
        }
        out.push(MInfo { start: t, len: end - t, number, implicit });
        if counts {
            num += 1;
        }
        t = end;
    }
    out
}

struct Cutter<'a> {
    infos: &'a [MInfo],
}

impl<'a> Cutter<'a> {
    /// index of the measure that contains time t
    fn find(&self, t: Frac) -> usize {
        let n = self.infos.len();
        if n == 0 {
            return 0;
        }
        let (mut lo, mut hi) = (0usize, n);
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.infos[mid].start <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
    /// the measure that ends at t (for things attached to the end of a bar)
    fn ending_at(&self, t: Frac) -> usize {
        let i = self.find(t);
        if i > 0 && self.infos[i].start == t {
            i - 1
        } else {
            i
        }
    }
}

pub fn build(mut it: Interp, mut score: Score) -> Score {
    it.attach_lyrics();
    resolve_ties(&mut it);
    let infos = measure_infos(&it);
    let cut = Cutter { infos: &infos };
    let n_meas = infos.len();

    // --- parts: one per staff, or one per PianoStaff
    struct PartD {
        staves: Vec<usize>,
        piano: Option<usize>,
    }
    let mut parts: Vec<PartD> = vec![];
    let mut group_no = 0;
    let mut open_groups: Vec<String> = vec![];
    for item in &it.list {
        match item {
            ListItem::GroupStart { symbol, name, barline } => {
                group_no += 1;
                open_groups.push(group_no.to_string());
                score.part_list.push(PartListItem::GroupStart { number: group_no.to_string(), symbol: symbol.clone(), name: name.clone(), barline: *barline });
            }
            ListItem::GroupStop => {
                if let Some(n) = open_groups.pop() {
                    score.part_list.push(PartListItem::GroupStop { number: n });
                }
            }
            ListItem::Staff(si) => {
                // a staff that never got anything is not a part
                let used = it.voices.iter().any(|v| v.staff == *si && !v.events.is_empty()) || !it.staves[*si].dirs.is_empty();
                if !used && it.staves[*si].piano.is_none() {
                    continue;
                }
                match it.staves[*si].piano {
                    Some(p) => match parts.iter().position(|x| x.piano == Some(p)) {
                        Some(pi) => parts[pi].staves.push(*si),
                        None => {
                            parts.push(PartD { staves: vec![*si], piano: Some(p) });
                            score.part_list.push(PartListItem::Part(parts.len() - 1));
                        }
                    },
                    None => {
                        parts.push(PartD { staves: vec![*si], piano: None });
                        score.part_list.push(PartListItem::Part(parts.len() - 1));
                    }
                }
            }
        }
    }
    // drop groups that ended up empty
    loop {
        let mut removed = false;
        for i in 0..score.part_list.len().saturating_sub(1) {
            if matches!(score.part_list[i], PartListItem::GroupStart { .. }) && matches!(score.part_list[i + 1], PartListItem::GroupStop { .. }) {
                score.part_list.drain(i..i + 2);
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }

    // --- global bar structure, shared by all parts
    let mut skeleton: Vec<Measure> = infos
        .iter()
        .map(|i| Measure { number: i.number.clone(), implicit: i.implicit, len: i.len, ..Default::default() })
        .collect();
    if n_meas > 0 {
        let g = &it.glob;
        let total = g.end;
        if !g.times.iter().any(|x| x.0.is_zero()) {
            skeleton[0].attrs.push(AttrChange { pos: Frac::ZERO, kind: Attr::Time { beats: "4".into(), beat_type: 4, symbol: Some(TimeSymbol::Common), print: true } });
        }
        for (t, beats, bt, symbol, print) in &g.times {
            if *t >= total {
                continue;
            }
            let mi = cut.find(*t);
            skeleton[mi].attrs.push(AttrChange { pos: *t - infos[mi].start, kind: Attr::Time { beats: beats.clone(), beat_type: *bt, symbol: symbol.clone(), print: *print } });
        }
        for (t, page) in &g.breaks {
            if *t >= total || t.is_zero() {
                continue;
            }
            let mi = cut.find(*t);
            if *page {
                skeleton[mi].new_page = true;
            } else {
                skeleton[mi].new_system = true;
            }
        }
        for (t, s) in &g.bars {
            let (left, right) = bar_meaning(s);
            if let Some((style, rep)) = right {
                if !t.is_zero() {
                    let m = &mut skeleton[cut.ending_at(*t)];
                    m.right.style = Some(style.to_string());
                    m.right.repeat |= rep;
                }
            }
            if let Some((style, rep)) = left {
                if *t < total {
                    let m = &mut skeleton[cut.find(*t)];
                    m.left.style = Some(style.to_string());
                    m.left.repeat |= rep;
                }
            }
        }
        for t in &g.left_rep {
            if *t < total {
                let m = &mut skeleton[cut.find(*t)];
                m.left.repeat = true;
                m.left.style.get_or_insert_with(|| "heavy-light".to_string());
            }
        }
        for (t, times) in &g.right_rep {
            if !t.is_zero() {
                let m = &mut skeleton[cut.ending_at(*t)];
                m.right.repeat = true;
                m.right.style = Some("light-heavy".to_string());
                if times.is_some() {
                    m.right.times = *times;
                }
            }
        }
        // endings: stops refer to the bracket that is open
        let mut evs: Vec<(Frac, u8, Option<&Ending>)> = g.end_stop.iter().map(|t| (*t, 0u8, None)).collect();
        evs.extend(g.end_start.iter().map(|(t, e)| (*t, 1u8, Some(e))));
        evs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut open: Option<String> = None;
        for (t, _, e) in evs {
            match e {
                Some(e) => {
                    if t < total {
                        skeleton[cut.find(t)].left.ending_start = Some(e.clone());
                        open = Some(e.number.clone());
                    }
                }
                None => {
                    if !t.is_zero() {
                        let number = open.take().unwrap_or_else(|| "1".to_string());
                        skeleton[cut.ending_at(t)].right.ending_stop = Some(EndingStop { number, open: false });
                    }
                }
            }
        }
    }

    // bar numbers the source gave explicitly
    let mut hinted = vec![false; n_meas];
    for (t, n) in &it.glob.num_hints {
        if n_meas == 0 {
            break;
        }
        let mi = cut.ending_at(*t);
        if !hinted[mi] {
            hinted[mi] = true;
            skeleton[mi].number = n.clone();
        }
    }

    // --- parts
    let place_dir = |d: &TDir, staff: u32, voice: Option<u32>, measures: &mut Vec<Measure>| {
        if measures.is_empty() {
            return;
        }
        let mi = if d.at_end || d.time >= it.glob.end { cut.ending_at(d.time) } else { cut.find(d.time) };
        let pos = d.time - infos[mi].start;
        measures[mi].directions.push(Direction { pos, staff, voice, placement: d.placement, kind: d.kind.clone() });
    };

    for (pi, pd) in parts.iter().enumerate() {
        let mut measures = skeleton.clone();
        // voice slots
        let max_slot = it.voices.iter().filter(|v| pd.staves.contains(&v.staff)).map(|v| v.slot + 1).max().unwrap_or(1);
        let per_staff = max_slot.max(4);
        let mut split_warned = false;
        for (local, si) in pd.staves.iter().enumerate() {
            let staff_no = local as u32 + 1;
            let st = &it.staves[*si];
            // clefs and keys
            if n_meas > 0 {
                let mut has_clef0 = false;
                let mut has_key0 = false;
                for (t, a) in &st.attrs {
                    if *t >= it.glob.end {
                        continue;
                    }
                    let mi = cut.find(*t);
                    let pos = *t - infos[mi].start;
                    match a {
                        Attr::Clef { sign, line, octave, .. } => {
                            if t.is_zero() {
                                has_clef0 = true;
                            }
                            measures[mi].attrs.push(AttrChange { pos, kind: Attr::Clef { staff: staff_no, sign: *sign, line: *line, octave: *octave } });
                        }
                        Attr::Key { .. } => {
                            if t.is_zero() {
                                has_key0 = true;
                            }
                            if !measures[mi].attrs.iter().any(|x| x.pos == pos && matches!(x.kind, Attr::Key { .. })) {
                                measures[mi].attrs.push(AttrChange { pos, kind: a.clone() });
                            }
                        }
                        _ => {}
                    }
                }
                for (t, a) in &it.glob.keys {
                    if *t >= it.glob.end {
                        continue;
                    }
                    let mi = cut.find(*t);
                    let pos = *t - infos[mi].start;
                    if t.is_zero() {
                        has_key0 = true;
                    }
                    if !measures[mi].attrs.iter().any(|x| x.pos == pos && matches!(x.kind, Attr::Key { .. })) {
                        measures[mi].attrs.push(AttrChange { pos, kind: a.clone() });
                    }
                }
                if !has_clef0 {
                    measures[0].attrs.insert(0, AttrChange { pos: Frac::ZERO, kind: Attr::Clef { staff: staff_no, sign: 'G', line: 2, octave: 0 } });
                }
                if !has_key0 && local == 0 && !measures[0].attrs.iter().any(|x| x.pos.is_zero() && matches!(x.kind, Attr::Key { .. })) {
                    measures[0].attrs.insert(0, AttrChange { pos: Frac::ZERO, kind: Attr::Key { fifths: 0, minor: false } });
                }
            }
            // events
            for vi in 0..it.voices.len() {
                if it.voices[vi].staff != *si {
                    continue;
                }
                let slot = it.voices[vi].slot;
                let events = std::mem::take(&mut it.voices[vi].events);
                let (main, overflow) = untangle(events);
                for (list, pref) in [(main, slot), (overflow, slot + 1)] {
                    // per measure
                    let mut buckets: Vec<(usize, Vec<Event>)> = vec![];
                    for te in list {
                        for (mi, ev, was_split) in split_event(te, &cut) {
                            if was_split && !split_warned && !ev.is_rest() {
                                split_warned = true;
                                it.diags.push(Level::Warning, Some(&format!("P{}", pi + 1)), Some(&infos[mi].number), "a note runs across the bar line; split and tied");
                            }
                            match buckets.last_mut() {
                                Some((m, v)) if *m == mi => v.push(ev),
                                _ => buckets.push((mi, vec![ev])),
                            }
                        }
                    }
                    for (mi, evs) in buckets {
                        let m = &mut measures[mi];
                        let mut s = pref;
                        loop {
                            let vno = (staff_no - 1) * per_staff + s;
                            match m.voices.iter_mut().find(|v| v.staff == staff_no && v.voice == vno) {
                                Some(seg) => {
                                    if overlaps(&seg.events, &evs) {
                                        s += 1;
                                        continue;
                                    }
                                    seg.events.extend(evs);
                                }
                                None => m.voices.push(VoiceSeg { staff: staff_no, voice: vno, events: evs }),
                            }
                            break;
                        }
                    }
                }
            }
            // directions
            let dirs = st.dirs.clone();
            for d in &dirs {
                let voice = d.voice.map(|v| (staff_no - 1) * per_staff + it.voices[v].slot);
                place_dir(d, staff_no, voice, &mut measures);
            }
        }
        // chord symbols belong to the staff that follows their ChordNames context
        for (t, staff_hint, h) in &it.glob.harmonies {
            let owner = parts.iter().position(|p| p.staves.iter().any(|s| s >= staff_hint)).unwrap_or(parts.len() - 1);
            if owner == pi && n_meas > 0 && *t < it.glob.end {
                let mi = cut.find(*t);
                let mut h = h.clone();
                h.pos = *t - infos[mi].start;
                measures[mi].harmonies.push(h);
            }
        }
        let first = &it.staves[pd.staves[0]];
        let (inst, short) = match pd.piano {
            Some(p) => (it.pianos[p].inst.clone().or(first.inst.clone()), it.pianos[p].short.clone().or(first.short.clone())),
            None => (first.inst.clone(), first.short.clone()),
        };
        let inst = inst.unwrap_or_default();
        let (name, display_name) = if inst.contains('\n') { (inst.replace('\n', " "), Some(inst)) } else { (inst, None) };
        score.parts.push(Part {
            id: format!("P{}", pi + 1),
            name,
            display_name,
            abbr: short.filter(|s| !s.is_empty()),
            instrument: None,
            staves: pd.staves.len() as u32,
            measures,
            ..Default::default()
        });
    }
    // score-wide marks (tempo, rehearsal marks, D.S.) go on the first part. In a file
    // lyxml wrote itself, a tempo mark goes back where such files keep it: on the first
    // part that plays in that bar.
    if !score.parts.is_empty() && n_meas > 0 {
        for d in &it.glob.dirs {
            let mut target = 0;
            if it.own_dialect && matches!(d.kind, DirKind::Tempo { .. }) {
                let mi = cut.find(d.time);
                let plays = |p: &Part| p.measures[mi].voices.iter().any(|v| v.events.iter().any(|e| !e.is_rest()));
                target = score.parts.iter().position(plays).unwrap_or(0);
            }
            place_dir(d, 1, None, &mut score.parts[target].measures);
        }
    }
    if it.no_bar_numbers {
        score.layout.bar_numbers = Some(false);
    }
    score.normalize();
    score
}

/// ties end on the next note of the voice with the same pitch; a tie to nothing is no tie
fn resolve_ties(it: &mut Interp) {
    for v in &mut it.voices {
        let n = v.events.len();
        for i in 0..n {
            if v.events[i].ev.grace.is_some() || !v.events[i].ev.notes().iter().any(|x| x.tie_start) {
                continue;
            }
            // next non-grace event
            let mut j = i + 1;
            while j < n && v.events[j].ev.grace.is_some() {
                j += 1;
            }
            let (head, tail) = v.events.split_at_mut(j.min(n));
            let cur = &mut head[i].ev;
            let next = tail.first_mut().map(|t| &mut t.ev);
            if let EventKind::Notes(notes) = &mut cur.kind {
                match next {
                    Some(Event { kind: EventKind::Notes(nn), .. }) => {
                        for a in notes.iter_mut().filter(|a| a.tie_start) {
                            if let Some(b) = nn.iter_mut().find(|b| b.step == a.step && b.alter == a.alter && b.octave == a.octave) {
                                b.tie_stop = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// One voice context can hold simultaneous music (`<< {..} {..} >>` without `\\`).
/// Notes that coincide exactly become chords; anything else that overlaps moves out.
fn untangle(mut events: Vec<TEv>) -> (Vec<TEv>, Vec<TEv>) {
    let sorted = events.windows(2).all(|w| w[0].time <= w[1].time);
    if !sorted {
        events.sort_by(|a, b| a.time.cmp(&b.time));
    }
    let mut main: Vec<TEv> = Vec::with_capacity(events.len());
    let mut over: Vec<TEv> = vec![];
    for e in events {
        let clash = match main.iter().rev().find(|p| p.ev.grace.is_none()) {
            Some(p) if e.ev.grace.is_none() => e.time < p.time + p.ev.dur,
            _ => false,
        };
        if !clash {
            main.push(e);
            continue;
        }
        let idx = main.iter().rposition(|p| p.ev.grace.is_none()).unwrap();
        let p = &mut main[idx];
        let same = p.time == e.time && p.ev.dur == e.ev.dur;
        match (&mut p.ev.kind, &e.ev.kind) {
            (EventKind::Notes(a), EventKind::Notes(b)) if same => {
                a.extend(b.iter().cloned());
                p.ev.arts.extend(e.ev.arts.iter().cloned());
                p.ev.slur_starts.extend(e.ev.slur_starts.iter().cloned());
                p.ev.slur_stops.extend(e.ev.slur_stops.iter().cloned());
                p.ev.lyrics.extend(e.ev.lyrics.iter().cloned());
            }
            (EventKind::Rest { .. }, EventKind::Notes(_)) if same => {
                // a rest under notes at the same time: the notes win
                let lyr = std::mem::take(&mut p.ev.lyrics);
                let mut e = e;
                e.ev.lyrics.extend(lyr);
                main[idx] = e;
            }
            (_, EventKind::Rest { .. }) if same => {}
            _ => over.push(e),
        }
    }
    (main, over)
}

fn overlaps(a: &[Event], b: &[Event]) -> bool {
    for x in a {
        if x.dur.is_zero() {
            continue;
        }
        for y in b {
            if y.dur.is_zero() {
                continue;
            }
            if x.pos < y.pos + y.dur && y.pos < x.pos + x.dur {
                return true;
            }
        }
    }
    false
}

/// (measure index, event with its position in the measure, came from a split)
fn split_event(te: TEv, cut: &Cutter) -> Vec<(usize, Event, bool)> {
    let infos = cut.infos;
    if infos.is_empty() {
        return vec![];
    }
    let mi = if te.trailing { cut.ending_at(te.time) } else { cut.find(te.time) };
    let m = &infos[mi];
    let pos = te.time - m.start;
    let mut ev = te.ev;
    ev.pos = pos;
    if ev.dur.is_zero() || pos + ev.dur <= m.len || mi + 1 >= infos.len() {
        if pos + ev.dur > m.len && !ev.dur.is_zero() {
            ev.dur = m.len - pos;
        }
        return vec![(mi, ev, false)];
    }
    let mut out = vec![];
    let mut rem = ev.dur;
    let mut mi = mi;
    let mut pos = pos;
    let mut first = true;
    let whole = matches!(ev.kind, EventKind::Rest { whole_measure: true });
    let stops = std::mem::take(&mut ev.slur_stops);
    while rem > Frac::ZERO && mi < infos.len() {
        let avail = infos[mi].len - pos;
        let d = if rem < avail { rem } else { avail };
        let mut piece = ev.clone();
        piece.pos = pos;
        piece.dur = d;
        piece.ntype = if whole { None } else { NoteType::from_len(d) };
        piece.tuplet = None;
        piece.tuplet_start = false;
        piece.tuplet_stop = false;
        let last = rem <= avail || mi + 1 >= infos.len();
        if !first {
            piece.lyrics.clear();
            piece.arts.clear();
            piece.slur_starts.clear();
            piece.beam = None;
        }
        if let EventKind::Notes(notes) = &mut piece.kind {
            for n in notes.iter_mut() {
                if !first {
                    n.tie_stop = true;
                }
                if !last {
                    n.tie_start = true;
                }
            }
        }
        if last {
            piece.slur_stops = stops.clone();
        }
        out.push((mi, piece, !whole));
        rem = rem - d;
        mi += 1;
        pos = Frac::ZERO;
        first = false;
    }
    out
}
