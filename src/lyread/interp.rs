//! Interpreter: walks the syntax tree the way LilyPond's iterators do, producing a
//! timeline per voice plus one global timeline (meter, bar lines, repeats, breaks).
//! `build.rs` then cuts the timelines into measures.

use super::ast::*;
use super::lex::Sexp;
use super::parse::collect_lyrics;
use crate::diag::{Diags, Level};
use crate::ir::{
    Art, Attr, Beam, DirKind, Ending, Event, EventKind, Frac, Grace, GroupSymbol, Lyric, Note, NoteType, Pedal, Placement, Syllabic, TimeSymbol, Tuplet, Wedge,
};
use std::collections::{HashMap, HashSet};

pub struct TEv {
    pub time: Frac,
    pub ev: Event,
    /// continues a melisma: takes no syllable of its own
    pub mel_cont: bool,
    /// grace notes after their main note (\afterGrace): they stay in the bar that ends here
    pub trailing: bool,
}

#[derive(Clone)]
pub struct TDir {
    pub time: Frac,
    /// belongs to the end of the bar that finishes here
    pub at_end: bool,
    pub voice: Option<usize>,
    pub placement: Option<Placement>,
    pub kind: DirKind,
}

#[derive(Default, Clone)]
struct VState {
    open_slurs: i32,
    open_beam: bool,
    prev_tied: bool,
    mel_off: bool,
    manual_melisma: bool,
    last_chord: Vec<Note>,
    last_notes: Vec<Note>,
}

pub struct VoiceD {
    pub staff: usize,
    pub name: Option<String>,
    pub slot: u32,
    pub events: Vec<TEv>,
    st: VState,
    pub lyric_lines: u32,
}

#[derive(Default)]
pub struct StaffD {
    pub name: Option<String>,
    pub inst: Option<String>,
    pub short: Option<String>,
    pub attrs: Vec<(Frac, Attr)>,
    pub dirs: Vec<TDir>,
    default_voice: Option<usize>,
    pub piano: Option<usize>,
    n_slots: u32,
}

#[derive(Default)]
pub struct PianoD {
    pub inst: Option<String>,
    pub short: Option<String>,
}

pub enum ListItem {
    GroupStart { symbol: GroupSymbol, name: Option<String>, barline: Option<bool> },
    GroupStop,
    Staff(usize),
}

#[derive(Default)]
pub struct Global {
    /// (time, beats, beat type, symbol, printed)
    pub times: Vec<(Frac, String, u32, Option<TimeSymbol>, bool)>,
    pub partials: Vec<(Frac, Frac)>,
    pub bars: Vec<(Frac, String)>,
    /// (time, page break)
    pub breaks: Vec<(Frac, bool)>,
    pub barnums: Vec<(Frac, i64)>,
    pub mlens: Vec<(Frac, Option<Frac>)>,
    pub cadenzas: Vec<(Frac, Option<Frac>)>,
    pub left_rep: Vec<Frac>,
    pub right_rep: Vec<(Frac, Option<u32>)>,
    pub end_start: Vec<(Frac, Ending)>,
    pub end_stop: Vec<Frac>,
    pub dirs: Vec<TDir>,
    pub keys: Vec<(Frac, Attr)>,
    pub num_hints: Vec<(Frac, String)>,
    /// (time, index of the next staff when they were read, chord symbol)
    pub harmonies: Vec<(Frac, usize, crate::ir::Harmony)>,
    pub end: Frac,
}

#[derive(Clone)]
struct Frame {
    staff: Option<usize>,
    voice: Option<usize>,
    /// a context whose notes we do not keep (Devnull, ChordNames ...)
    mute: bool,
}

#[derive(Clone)]
enum PMode {
    Abs,
    Fixed(i32),
    Rel { step: u8, oct: i32 },
}

struct TupCtx {
    span: Option<Frac>,
    first_pending: bool,
    last: Option<(usize, usize)>,
    elapsed: Frac,
    number: bool,
    bracket: Option<bool>,
}

enum LyTarget {
    Name(String),
    Voice(usize),
    /// lyrics with their own durations: aligned by time with this staff's first voice
    Timed { staff: Option<usize>, start: Frac },
}

struct LyJob {
    target: LyTarget,
    items: Vec<LyItem>,
}

pub struct Interp<'d> {
    pub diags: &'d mut Diags,
    pub staves: Vec<StaffD>,
    pub voices: Vec<VoiceD>,
    pub pianos: Vec<PianoD>,
    pub list: Vec<ListItem>,
    pub glob: Global,
    pub no_bar_numbers: bool,
    /// the source was written by lyxml itself
    pub own_dialect: bool,
    time: Frac,
    frames: Vec<Frame>,
    pmode: PMode,
    /// (diatonic steps, semitones, fifths)
    transpose: (i32, i32, i32),
    scale: Frac,
    tuplets: Vec<TupCtx>,
    tup_ratio: Vec<(u32, u32)>,
    grace: Option<Grace>,
    grace_slur_start: bool,
    grace_slur_stop: Option<usize>,
    grace_trailing: bool,
    group_stack: Vec<usize>,
    /// inside a ChordNames context: the index the next staff will get
    chord_names: Option<usize>,
    piano_stack: Vec<usize>,
    numeric_time: bool,
    /// `\once \override Glissando.style` seen: the next glissando is wavy
    wavy_gliss: bool,
    /// one-shot marks for the next note: x heads, a caesura instead of a breath, a double
    /// slur, dashed (1) or dotted (2) slur and tie
    xnote_next: bool,
    caesura_next: bool,
    double_next: bool,
    double_open: bool,
    slur_style_next: u8,
    tie_style_next: u8,
hide_time_all: bool,
    hide_time_once: bool,
    omit_tnum: bool,
    omit_tbr: bool,
    mark_counter: u32,
    hairpins: HashMap<(usize, Option<usize>), (bool, bool)>,
    touch: Vec<usize>,
    jobs: Vec<LyJob>,
    warned: HashSet<String>,
    default_staff: Option<usize>,
}

const SEMI: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const LOF: [i32; 7] = [0, 2, 4, -1, 1, 3, 5];

impl<'d> Interp<'d> {
    pub fn new(diags: &'d mut Diags, numeric_time: bool) -> Interp<'d> {
        Interp {
            diags,
            staves: vec![],
            voices: vec![],
            pianos: vec![],
            list: vec![],
            glob: Global::default(),
            no_bar_numbers: false,
            own_dialect: false,
            time: Frac::ZERO,
            frames: vec![Frame { staff: None, voice: None, mute: false }],
            pmode: PMode::Abs,
            transpose: (0, 0, 0),
            scale: Frac::int(1),
            tuplets: vec![],
            tup_ratio: vec![],
            grace: None,
            grace_slur_start: false,
            grace_slur_stop: None,
            grace_trailing: false,
            group_stack: vec![],
            chord_names: None,
            piano_stack: vec![],
            numeric_time,
            wavy_gliss: false,
            xnote_next: false,
            caesura_next: false,
            double_next: false,
            double_open: false,
            slur_style_next: 0,
            tie_style_next: 0,
            hide_time_all: false,
            hide_time_once: false,
            omit_tnum: false,
            omit_tbr: false,
            mark_counter: 0,
            hairpins: HashMap::new(),
            touch: vec![],
            jobs: vec![],
            warned: HashSet::new(),
            default_staff: None,
        }
    }

    fn note(&mut self, level: Level, line: u32, msg: String) {
        if self.warned.insert(msg.clone()) {
            if line == 0 {
                self.diags.push(level, None, None, msg);
            } else {
                self.diags.at_line(level, line as usize, msg);
            }
        }
    }

    fn advance(&mut self, d: Frac) {
        self.time += d;
        if self.time > self.glob.end {
            self.glob.end = self.time;
        }
    }

    // ------------------------------------------------------------ contexts

    fn new_staff(&mut self, name: Option<String>) -> usize {
        let piano = self.piano_stack.last().copied();
        self.staves.push(StaffD { name, piano, ..Default::default() });
        let si = self.staves.len() - 1;
        self.list.push(ListItem::Staff(si));
        si
    }

    fn cur_staff(&mut self, create: bool) -> Option<usize> {
        let f = self.frames.last().unwrap();
        if f.mute {
            return None;
        }
        if let Some(s) = f.staff {
            return Some(s);
        }
        if let Some(s) = self.default_staff {
            return Some(s);
        }
        if create {
            let si = self.new_staff(None);
            self.default_staff = Some(si);
            return Some(si);
        }
        None
    }

    fn new_voice(&mut self, staff: usize, name: Option<String>, slot: Option<u32>) -> usize {
        let slot = match slot {
            Some(s) => s,
            None => {
                self.staves[staff].n_slots += 1;
                self.staves[staff].n_slots
            }
        };
        self.voices.push(VoiceD { staff, name, slot, events: vec![], st: VState::default(), lyric_lines: 0 });
        self.voices.len() - 1
    }

    /// the voice notes go to now; created when needed
    fn cur_voice(&mut self) -> Option<usize> {
        let f = self.frames.last().unwrap().clone();
        if f.mute {
            return None;
        }
        if let Some(v) = f.voice {
            return Some(v);
        }
        let si = self.cur_staff(true)?;
        if let Some(v) = self.staves[si].default_voice {
            return Some(v);
        }
        let v = self.new_voice(si, None, None);
        self.staves[si].default_voice = Some(v);
        Some(v)
    }

    /// the current voice if there is one already
    fn peek_voice(&self) -> Option<usize> {
        let f = self.frames.last().unwrap();
        if f.mute {
            return None;
        }
        if f.voice.is_some() {
            return f.voice;
        }
        let si = f.staff.or(self.default_staff)?;
        self.staves[si].default_voice
    }

    fn with_text(with: &[(String, Val)], key: &str) -> Option<String> {
        with.iter().rev().find(|(k, _)| k == key).and_then(|(_, v)| v.text())
    }

    fn context(&mut self, new: bool, ctype: &str, name: &Option<String>, with: &[(String, Val)], body: &Music) {
        let inst = Self::with_text(with, "instrumentName");
        let short = Self::with_text(with, "shortInstrumentName");
        match ctype {
            "PianoStaff" => {
                self.pianos.push(PianoD { inst, short });
                self.piano_stack.push(self.pianos.len() - 1);
                self.run(body);
                self.piano_stack.pop();
            }
            "StaffGroup" | "ChoirStaff" | "GrandStaff" => {
                let mut symbol = if ctype == "GrandStaff" { GroupSymbol::Brace } else { GroupSymbol::Bracket };
                for (k, v) in with {
                    if k.contains("systemStartDelimiter") {
                        match v.text().as_deref() {
                            Some("SystemStartBar") => symbol = GroupSymbol::Line,
                            Some("SystemStartSquare") => symbol = GroupSymbol::Square,
                            Some("SystemStartBrace") => symbol = GroupSymbol::Brace,
                            Some("SystemStartBracket") => symbol = GroupSymbol::Bracket,
                            _ => {}
                        }
                    }
                }
                let barline = Some(ctype != "ChoirStaff");
                // inside a PianoStaff the staves already form one part
                let nested_in_piano = !self.piano_stack.is_empty();
                if !nested_in_piano {
                    self.list.push(ListItem::GroupStart { symbol, name: inst, barline });
                    self.group_stack.push(self.list.len() - 1);
                }
                self.run(body);
                if !nested_in_piano {
                    self.group_stack.pop();
                    self.list.push(ListItem::GroupStop);
                }
            }
            "Lyrics" => self.lyrics_context(body, with),
            "Dynamics" => {
                let staff = self.staves.len().checked_sub(1).or(self.frames.last().unwrap().staff);
                self.frames.push(Frame { staff, voice: None, mute: staff.is_none() });
                self.run(body);
                self.frames.pop();
            }
            "ChordNames" => {
                let save = self.chord_names.replace(self.staves.len());
                self.frames.push(Frame { staff: None, voice: None, mute: true });
                self.run(body);
                self.frames.pop();
                self.chord_names = save;
            }
            "Devnull" | "FretBoards" | "FiguredBass" | "NoteNames" | "Global" => {
                if ctype != "Devnull" && ctype != "Global" {
                    self.note(Level::Dropped, 0, format!("\\new {}: this kind of context is not supported; its content is left out", ctype));
                }
                self.frames.push(Frame { staff: None, voice: None, mute: true });
                self.run(body);
                self.frames.pop();
            }
            "Score" | "Timing" => self.run(body),
            t if t.ends_with("Staff") => {
                let existing = if new {
                    None
                } else {
                    match name {
                        Some(n) => self.staves.iter().position(|s| s.name.as_ref() == Some(n)),
                        // an unnamed \context Staff is the staff we are in
                        None => self.frames.last().unwrap().staff,
                    }
                };
                let si = match existing {
                    Some(s) => s,
                    None => self.new_staff(name.clone()),
                };
                if inst.is_some() {
                    self.staves[si].inst = inst;
                }
                if short.is_some() {
                    self.staves[si].short = short;
                }
                self.frames.push(Frame { staff: Some(si), voice: None, mute: false });
                self.run(body);
                self.frames.pop();
            }
            t if t.ends_with("Voice") => {
                if self.frames.last().unwrap().mute {
                    self.run(body);
                    return;
                }
                let existing = if !new { name.as_ref().and_then(|n| self.voices.iter().position(|v| v.name.as_ref() == Some(n))) } else { None };
                let vi = match existing {
                    Some(v) => v,
                    None => {
                        // an unnamed \context Voice is the current one
                        if !new && name.is_none() {
                            match self.cur_voice() {
                                Some(v) => v,
                                None => return,
                            }
                        } else {
                            let si = match self.cur_staff(true) {
                                Some(s) => s,
                                None => return,
                            };
                            self.new_voice(si, name.clone(), None)
                        }
                    }
                };
                let staff = self.voices[vi].staff;
                self.frames.push(Frame { staff: Some(staff), voice: Some(vi), mute: false });
                self.run(body);
                self.frames.pop();
            }
            _ => self.run(body),
        }
    }

    fn find_lyricsto<'a>(m: &'a Music) -> Option<(&'a str, &'a Music)> {
        match m {
            Music::LyricsTo(n, b) => Some((n.as_str(), b)),
            Music::Seq(items) | Music::Sim(items) => items.iter().find_map(Self::find_lyricsto),
            Music::Var(v) => Self::find_lyricsto(v),
            Music::Context { body, .. } => Self::find_lyricsto(body),
            _ => None,
        }
    }

    fn lyrics_context(&mut self, body: &Music, with: &[(String, Val)]) {
        let mut items = vec![];
        for (k, v) in with {
            if k == "includeGraceNotes" {
                items.push(LyItem::IncludeGrace(matches!(v, Val::Scheme(Sexp::Bool(true)))));
            }
        }
        if let Some((name, b)) = Self::find_lyricsto(body) {
            // settings written before the \lyricsto inside the context
            collect_settings(body, &mut items);
            collect_lyrics(b, &mut items);
            self.jobs.push(LyJob { target: LyTarget::Name(name.to_string()), items });
        } else {
            collect_lyrics(body, &mut items);
            if items.iter().any(|i| matches!(i, LyItem::Syl { .. })) {
                let staff = self.staves.len().checked_sub(1);
                self.jobs.push(LyJob { target: LyTarget::Timed { staff, start: self.time }, items });
            }
        }
    }

    // ------------------------------------------------------------ pitches

    fn resolve(&mut self, p: &Pitch, chord_prev: Option<(u8, i32)>) -> (Note, (u8, i32)) {
        let (step, oct) = match &self.pmode {
            PMode::Abs => (p.step, 3 + p.marks),
            PMode::Fixed(base) => (p.step, 3 + base + p.marks),
            PMode::Rel { step: ls, oct: lo } => {
                let (ls, lo) = chord_prev.unwrap_or((*ls, *lo));
                let mut d = p.step as i32 - ls as i32;
                if d > 3 {
                    d -= 7;
                } else if d < -3 {
                    d += 7;
                }
                let idx = lo * 7 + ls as i32 + d + 7 * p.marks;
                (p.step, idx.div_euclid(7))
            }
        };
        let written = (step, oct);
        let (mut nstep, mut noct, mut alter) = (step, oct, p.alter as i32);
        if self.transpose.0 != 0 || self.transpose.1 != 0 {
            let midi = (oct + 1) * 12 + SEMI[step as usize] + p.alter as i32 + self.transpose.1;
            let idx = oct * 7 + step as i32 + self.transpose.0;
            nstep = idx.rem_euclid(7) as u8;
            noct = idx.div_euclid(7);
            alter = midi - ((noct + 1) * 12 + SEMI[nstep as usize]);
        }
        let mut n = Note::new(nstep, alter.clamp(-3, 3) as i8, noct.clamp(-2, 12) as i8);
        n.acc_shown = p.force_acc || p.caut_acc;
        n.acc_caut = p.caut_acc;
        (n, written)
    }

    // ------------------------------------------------------------ events

    fn dir_staff(&mut self) -> Option<usize> {
        let f = self.frames.last().unwrap();
        if let Some(v) = f.voice {
            return Some(self.voices[v].staff);
        }
        f.staff.or(self.default_staff).or(if self.staves.is_empty() { None } else { Some(self.staves.len() - 1) })
    }

    fn push_dir(&mut self, staff: Option<usize>, d: TDir) {
        match staff {
            Some(s) => self.staves[s].dirs.push(d),
            None => self.glob.dirs.push(d),
        }
    }

    /// dynamics, hairpins, text and pedal marks attached to an event at the current time
    fn posts_as_dirs(&mut self, posts: &[Post]) {
        if !posts.iter().any(|p| matches!(p, Post::Dynamic(..) | Post::Wedge(..) | Post::TextSpan(_) | Post::Text { .. } | Post::Pedal(_))) {
            return;
        }
        let staff = self.dir_staff();
        let voice = self.peek_voice();
        let key = (staff.unwrap_or(usize::MAX), voice);
        let (mut hairpin, mut textspan) = self.hairpins.get(&key).copied().unwrap_or((false, false));
        let time = self.time;
        let mk = |placement: Option<Placement>, kind: DirKind| TDir { time, at_end: false, voice, placement, kind };
        // \sustainOff\sustainOn stays two marks, as written
        let pedal_change = false;
        let mut out = vec![];
        let mut auto_stopped = false;
        // an explicit \! on the same note as a dynamic comes first
        let stop_first = posts.iter().any(|p| matches!(p, Post::Wedge(Wedge::Stop, _)));
        if stop_first && hairpin {
            out.push(mk(None, DirKind::Wedge(Wedge::Stop)));
            hairpin = false;
            auto_stopped = true;
        }
        for p in posts {
            match p {
                Post::Dynamic(name, pl) => {
                    if hairpin {
                        out.push(mk(None, DirKind::Wedge(Wedge::Stop)));
                        hairpin = false;
                        auto_stopped = true;
                    }
                    textspan = false;
                    out.push(mk(*pl, DirKind::Dynamic(name.clone())));
                }
                Post::Wedge(Wedge::Stop, pl) => {
                    if textspan && !hairpin {
                        textspan = false;
                    } else if auto_stopped && !hairpin {
                        // `c\f\!`: the dynamic already ended the hairpin
                    } else {
                        out.push(mk(*pl, DirKind::Wedge(Wedge::Stop)));
                        hairpin = false;
                    }
                }
                Post::Wedge(w, pl) => {
                    if hairpin {
                        out.push(mk(None, DirKind::Wedge(Wedge::Stop)));
                    }
                    out.push(mk(*pl, DirKind::Wedge(*w)));
                    hairpin = true;
                }
                Post::TextSpan(t) => {
                    out.push(mk(None, DirKind::Words { text: t.clone(), italic: true, bold: false }));
                    textspan = true;
                }
                // a dynamic LilyPond has no command for, written as \markup { \dynamic "sfpp" }
                Post::Text { text, italic: false, bold: false, placement } if !text.is_empty() && text.len() <= 6 && text.chars().all(|c| "pfmszrn".contains(c)) => {
                    let pl = if *placement == Some(Placement::Above) { *placement } else { None };
                    out.push(mk(pl, DirKind::Dynamic(text.clone())));
                }
                Post::Text { text, italic, bold, placement } => {
                    out.push(mk(*placement, DirKind::Words { text: text.clone(), italic: *italic, bold: *bold }));
                }
                Post::Pedal(k) => {
                    if pedal_change {
                        if *k == Pedal::Start {
                            out.push(mk(None, DirKind::Pedal(Pedal::Change)));
                        }
                    } else {
                        out.push(mk(None, DirKind::Pedal(*k)));
                    }
                }
                _ => {}
            }
        }
        self.hairpins.insert(key, (hairpin, textspan));
        for d in out {
            self.push_dir(staff, d);
        }
    }

    fn event(&mut self, ne: &NoteEv) {
        let written = ne.dur.len();
        let dur = written * self.scale;
        let in_grace = self.grace.is_some();
        if ne.kind == EvKind::Skip {
            self.posts_as_dirs(&ne.posts);
            if !in_grace {
                self.tuplet_elapsed(dur);
                self.advance(dur);
            }
            return;
        }
        let vi = match self.cur_voice() {
            Some(v) => v,
            None => {
                if !in_grace {
                    self.advance(dur);
                }
                return;
            }
        };
        let kind = match &ne.kind {
            EvKind::Rest => EventKind::Rest { whole_measure: false },
            EvKind::MeasureRest => EventKind::Rest { whole_measure: true },
            EvKind::Notes(ns) => {
                let mut notes = vec![];
                let mut prev: Option<(u8, i32)> = None;
                let mut first: Option<(u8, i32)> = None;
                for cn in ns {
                    let (mut n, w) = self.resolve(&cn.pitch, prev);
                    n.tie_start = cn.tie;
                    prev = Some(w);
                    first.get_or_insert(w);
                    notes.push(n);
                }
                if let (PMode::Rel { .. }, Some((s, o))) = (&self.pmode, first) {
                    self.pmode = PMode::Rel { step: s, oct: o };
                }
                let st = &mut self.voices[vi].st;
                let mut clean = notes.clone();
                for n in &mut clean {
                    n.tie_start = false;
                }
                if ns.len() > 1 {
                    st.last_chord = clean.clone();
                }
                st.last_notes = clean;
                EventKind::Notes(notes)
            }
            EvKind::ChordRepeat | EvKind::Again => {
                let st = &self.voices[vi].st;
                let prev = if ne.kind == EvKind::ChordRepeat && !st.last_chord.is_empty() { st.last_chord.clone() } else { st.last_notes.clone() };
                if prev.is_empty() {
                    self.note(Level::Warning, ne.line, "nothing to repeat here; a rest is written".into());
                    EventKind::Rest { whole_measure: false }
                } else {
                    EventKind::Notes(prev)
                }
            }
            EvKind::Skip => unreachable!(),
        };
        let is_note = matches!(kind, EventKind::Notes(_));
        let mut ev = Event::new(Frac::ZERO, if in_grace { Frac::ZERO } else { dur }, kind);
        ev.ntype = if ne.kind == EvKind::MeasureRest {
            None
        } else if ne.dur.mult == Frac::int(1) {
            Some(ne.dur.ntype())
        } else {
            NoteType::from_len(written)
        };
        if !self.tup_ratio.is_empty() {
            let (a, n) = self.tup_ratio.iter().fold((1u32, 1u32), |acc, r| (acc.0 * r.0, acc.1 * r.1));
            ev.tuplet = Some(Tuplet { actual: a, normal: n });
        }
        ev.grace = self.grace;
        if is_note {
            ev.notehead_x = std::mem::take(&mut self.xnote_next);
            if ne.posts.iter().any(|p| matches!(p, Post::SlurStart(_))) {
                ev.slur_style = std::mem::take(&mut self.slur_style_next);
            }
            if ne.posts.iter().any(|p| matches!(p, Post::Tie | Post::TieOut)) || matches!(&ne.kind, EvKind::Notes(ns) if ns.iter().any(|n| n.tie)) {
                ev.tie_style = std::mem::take(&mut self.tie_style_next);
            }
        }
        let double_start = is_note && self.double_next && ne.posts.iter().any(|p| matches!(p, Post::SlurStart(1)));
        let double_stop = self.double_open && !double_start && ne.posts.iter().any(|p| matches!(p, Post::SlurStop(1)));
        if double_start {
            self.double_next = false;
            self.double_open = true;
        }
        if double_stop {
            self.double_open = false;
        }

        let st = &mut self.voices[vi].st;
        let mel_cont = is_note && !in_grace && !st.mel_off && (st.prev_tied || st.open_slurs > 0 || st.open_beam || st.manual_melisma);
        let beam_was_open = st.open_beam;
        let mut tie_all = false;
        let mut tie_in = false;
        for p in &ne.posts {
            match p {
                Post::Tie | Post::TieOut => tie_all = true,
                Post::TieIn => tie_in = true,
                Post::SlurStart(n) => {
                    ev.slur_starts.push(*n);
                    if *n == 1 && double_start {
                        // the second curve of a double slur
                        ev.slur_starts.push(2);
                        ev.double_slur = true;
                    }
                    if *n != 2 && !in_grace {
                        st.open_slurs += 1;
                    }
                }
                Post::SlurStop(n) => {
                    ev.slur_stops.push(*n);
                    if *n == 1 && double_stop {
                        ev.slur_stops.push(2);
                    }
                    if *n != 2 {
                        st.open_slurs = (st.open_slurs - 1).max(0);
                    }
                }
                Post::BeamStart => {
                    ev.beam = Some(Beam::Begin);
                    st.open_beam = true;
                }
                Post::BeamStop => {
                    ev.beam = Some(Beam::End);
                    st.open_beam = false;
                }
                Post::Art(Art::Tremolo(sub), _) => {
                    // strokes drawn = subdivision beyond the note's own flags
                    let own = ev.ntype.map(|t| t.log.max(2)).unwrap_or(2) as i32;
                    ev.arts.push(Art::Tremolo((*sub as i32 - own).clamp(1, 5) as u8));
                }
                Post::Art(Art::Glissando, _) if self.wavy_gliss => {
                    self.wavy_gliss = false;
                    ev.arts.push(Art::GlissandoWavy);
                }
                Post::Art(a, _) => ev.arts.push(a.clone()),
                _ => {}
            }
        }
        if ev.beam.is_none() && beam_was_open {
            ev.beam = Some(Beam::Continue);
        }
        if let EventKind::Notes(notes) = &mut ev.kind {
            for n in notes.iter_mut() {
                n.tie_start |= tie_all;
                n.tie_stop |= tie_in;
            }
            if !in_grace {
                st.prev_tied = notes.iter().any(|n| n.tie_start);
            }
        } else {
            st.prev_tied = false;
        }
        // grace slur of \acciaccatura / \appoggiatura
        if in_grace {
            if self.grace_slur_start {
                self.grace_slur_start = false;
                ev.slur_starts.push(1);
                self.grace_slur_stop = Some(vi);
            }
        } else if self.grace_slur_stop == Some(vi) {
            self.grace_slur_stop = None;
            if is_note {
                ev.slur_stops.push(1);
            }
        }
        // tuplet brackets
        if !in_grace {
            let idx = self.voices[vi].events.len();
            for t in &mut self.tuplets {
                if t.first_pending {
                    t.first_pending = false;
                    ev.tuplet_start = true;
                    ev.tuplet_number = t.number;
                    ev.tuplet_bracket = t.bracket;
                }
                t.last = Some((vi, idx));
            }
        }
        self.posts_as_dirs(&ne.posts);
        let time = self.time;
        let trailing = in_grace && self.grace_trailing;
        self.voices[vi].events.push(TEv { time, ev, mel_cont, trailing });
        if self.touch.last() != Some(&vi) {
            self.touch.push(vi);
        }
        if !in_grace {
            self.tuplet_elapsed(dur);
            self.advance(dur);
        }
    }

    /// tuplets with a span duration close their bracket every `span`
    fn tuplet_elapsed(&mut self, dur: Frac) {
        let mut stops = vec![];
        for t in &mut self.tuplets {
            if let Some(span) = t.span {
                t.elapsed += dur;
                if t.elapsed >= span {
                    t.elapsed = Frac::ZERO;
                    t.first_pending = true;
                    if let Some(l) = t.last.take() {
                        stops.push(l);
                    }
                }
            }
        }
        for (vi, idx) in stops {
            self.voices[vi].events[idx].ev.tuplet_stop = true;
        }
    }

    fn last_event_mut(&mut self) -> Option<&mut Event> {
        let vi = self.peek_voice()?;
        self.voices[vi].events.last_mut().map(|e| &mut e.ev)
    }

    // ------------------------------------------------------------ walking

    pub fn run(&mut self, m: &Music) {
        match m {
            Music::Seq(items) => {
                for i in items {
                    self.run(i);
                }
            }
            Music::Sim(items) => {
                let t0 = self.time;
                let mut end = t0;
                for i in items {
                    self.time = t0;
                    self.run(i);
                    if self.time > end {
                        end = self.time;
                    }
                }
                self.time = end;
            }
            Music::Voices(items) => {
                let t0 = self.time;
                let mut end = t0;
                let muted = self.frames.last().unwrap().mute;
                for (i, b) in items.iter().enumerate() {
                    self.time = t0;
                    if muted {
                        self.run(b);
                    } else if let Some(si) = self.cur_staff(true) {
                        let inherit = self.peek_voice().map(|v| self.voices[v].st.mel_off).unwrap_or(false);
                        let vi = self.new_voice(si, None, Some(i as u32 + 1));
                        self.voices[vi].st.mel_off = inherit;
                        self.frames.push(Frame { staff: Some(si), voice: Some(vi), mute: false });
                        self.run(b);
                        self.frames.pop();
                    }
                    if self.time > end {
                        end = self.time;
                    }
                }
                self.time = end;
            }
            Music::Ev(ne) => self.event(ne),
            Music::Var(v) => self.run(v),
            Music::Relative(p, body) => {
                let start = match p {
                    Some(p) => PMode::Rel { step: p.step, oct: 3 + p.marks },
                    None => PMode::Rel { step: 3, oct: 3 },
                };
                let save = std::mem::replace(&mut self.pmode, start);
                self.run(body);
                self.pmode = save;
            }
            Music::Fixed(p, body) => {
                let save = std::mem::replace(&mut self.pmode, PMode::Fixed(p.marks));
                self.run(body);
                self.pmode = save;
            }
            Music::Absolute(body) => {
                let save = std::mem::replace(&mut self.pmode, PMode::Abs);
                self.run(body);
                self.pmode = save;
            }
            Music::Transpose(a, b, body) => {
                let idx = |p: &Pitch| (3 + p.marks) * 7 + p.step as i32;
                let midi = |p: &Pitch| (3 + p.marks + 1) * 12 + SEMI[p.step as usize] + p.alter as i32;
                let lof = |p: &Pitch| LOF[p.step as usize] + 7 * p.alter as i32;
                let save = self.transpose;
                self.transpose = (save.0 + idx(b) - idx(a), save.1 + midi(b) - midi(a), save.2 + lof(b) - lof(a));
                // an enclosing \relative does not reach into \transpose
                let mode = std::mem::replace(&mut self.pmode, PMode::Abs);
                self.run(body);
                self.pmode = mode;
                self.transpose = save;
            }
            Music::Context { new, ctype, name, with, body } => self.context(*new, ctype, name, with, body),
            Music::Tuplet { num, den, span, body } => {
                let save = self.scale;
                self.scale = self.scale * Frac::new(*den as i64, *num as i64);
                let number = !self.omit_tnum;
                let bracket = if self.omit_tbr { Some(false) } else { None };
                self.omit_tnum = false;
                self.omit_tbr = false;
                self.tuplets.push(TupCtx { span: *span, first_pending: true, last: None, elapsed: Frac::ZERO, number, bracket });
                self.tup_ratio.push((*num, *den));
                self.run(body);
                self.tup_ratio.pop();
                if let Some(t) = self.tuplets.pop() {
                    if let Some((vi, idx)) = t.last {
                        self.voices[vi].events[idx].ev.tuplet_stop = true;
                    }
                }
                self.scale = save;
            }
            Music::Scale(f, body) => {
                let save = self.scale;
                self.scale = self.scale * *f;
                self.run(body);
                self.scale = save;
            }
            Music::Grace { slash, slur, body } => {
                let save = self.grace;
                self.grace = Some(Grace { slash: *slash });
                self.grace_slur_start = *slur;
                self.run(body);
                self.grace_slur_start = false;
                self.grace = save;
            }
            Music::AfterGrace(main, grace) => {
                self.run(main);
                let save = self.grace;
                self.grace = Some(Grace { slash: false });
                self.grace_trailing = true;
                self.run(grace);
                self.grace_trailing = false;
                self.grace = save;
            }
            Music::Repeat { kind, count, body, alts } => self.repeat(kind, *count, body, alts),
            Music::Alternative(alts) => {
                for a in alts {
                    self.run(a);
                }
            }
            Music::Volta(_, body) => self.run(body),
            Music::Time { beats, beat_type } => {
                let symbol = if self.numeric_time {
                    None
                } else if beats == "4" && *beat_type == 4 {
                    Some(TimeSymbol::Common)
                } else if beats == "2" && *beat_type == 2 {
                    Some(TimeSymbol::Cut)
                } else {
                    None
                };
                let print = !(self.hide_time_all || self.hide_time_once);
                self.hide_time_once = false;
                self.glob.times.push((self.time, beats.clone(), *beat_type, symbol, print));
            }
            Music::Key { fifths, minor } => {
                let a = Attr::Key { fifths: (*fifths as i32 + self.transpose.2).clamp(-12, 12) as i8, minor: *minor };
                match self.cur_staff(false) {
                    Some(s) => self.staves[s].attrs.push((self.time, a)),
                    None => self.glob.keys.push((self.time, a)),
                }
            }
            Music::Clef { sign, line, octave } => {
                if let Some(s) = self.cur_staff(true) {
                    self.staves[s].attrs.push((self.time, Attr::Clef { staff: 0, sign: *sign, line: *line, octave: *octave }));
                }
            }
            Music::Partial(d) => self.glob.partials.push((self.time, *d)),
            Music::Tempo { text, unit } => {
                let d = TDir { time: self.time, at_end: false, voice: None, placement: Some(Placement::Above), kind: DirKind::Tempo { text: text.clone(), metronome: unit.clone() } };
                self.glob.dirs.push(d);
            }
            Music::Bar(s) => self.glob.bars.push((self.time, s.clone())),
            Music::Mark(kind) => {
                let kind = match kind {
                    MarkKind::Text(m) => match m.glyph.as_deref() {
                        Some("segno") => DirKind::Segno,
                        Some("coda") => DirKind::Coda,
                        _ => DirKind::Rehearsal(m.text.clone()),
                    },
                    MarkKind::Default => {
                        self.mark_counter += 1;
                        DirKind::Rehearsal(mark_letter(self.mark_counter))
                    }
                    MarkKind::Number(n) => {
                        self.mark_counter = *n;
                        DirKind::Rehearsal(mark_letter(*n))
                    }
                };
                self.glob.dirs.push(TDir { time: self.time, at_end: false, voice: None, placement: Some(Placement::Above), kind });
            }
            Music::Break => self.glob.breaks.push((self.time, false)),
            Music::PageBreak => self.glob.breaks.push((self.time, true)),
            Music::Set { ctx, prop, val, line } => self.set(ctx.as_deref(), prop, val, *line),
            Music::Unset { prop, .. } => match prop.as_str() {
                "measureLength" => self.glob.mlens.push((self.time, None)),
                "melismaBusyProperties" => {
                    if let Some(v) = self.cur_voice() {
                        self.voices[v].st.mel_off = false;
                    }
                }
                _ => {}
            },
            Music::Omit { once, path } => {
                if path.contains("TimeSignature") {
                    if *once {
                        self.hide_time_once = true;
                    } else {
                        self.hide_time_all = true;
                    }
                } else if path.contains("BarNumber") {
                    self.no_bar_numbers = true;
                } else if path.contains("TupletNumber") {
                    self.omit_tnum = true;
                } else if path.contains("TupletBracket") {
                    self.omit_tbr = true;
                }
            }
            Music::Cmd(name) => self.cmd(name),
            Music::Ottava(n) => {
                let staff = self.dir_staff();
                let voice = self.peek_voice();
                self.push_dir(staff, TDir { time: self.time, at_end: false, voice, placement: None, kind: DirKind::Ottava(*n) });
            }
            Music::EndText(m) => {
                let kind = DirKind::Words { text: m.text.clone(), italic: m.italic, bold: m.bold };
                self.glob.dirs.push(TDir { time: self.time, at_end: true, voice: None, placement: Some(Placement::Above), kind });
            }
            Music::StartText(m) => {
                let kind = DirKind::Words { text: m.text.clone(), italic: m.italic, bold: m.bold };
                self.glob.dirs.push(TDir { time: self.time, at_end: false, voice: None, placement: Some(Placement::Above), kind });
            }
            Music::Segno => self.glob.dirs.push(TDir { time: self.time, at_end: false, voice: None, placement: Some(Placement::Above), kind: DirKind::Segno }),
            Music::Coda => self.glob.dirs.push(TDir { time: self.time, at_end: false, voice: None, placement: Some(Placement::Above), kind: DirKind::Coda }),
            Music::Lyrics(_) => self.lyrics_context(m, &[]),
            Music::LyricsTo(name, body) => {
                let mut items = vec![];
                collect_lyrics(body, &mut items);
                self.jobs.push(LyJob { target: LyTarget::Name(name.clone()), items });
            }
            Music::AddLyrics(music, lyrs) => {
                let mark = self.touch.len();
                let before = self.touch.last().copied();
                self.run(music);
                let target = self.touch.get(mark).copied().or(before);
                for l in lyrs {
                    let mut items = vec![];
                    collect_lyrics(l, &mut items);
                    match target {
                        Some(v) => self.jobs.push(LyJob { target: LyTarget::Voice(v), items }),
                        None => self.note(Level::Dropped, 0, "\\addlyrics: no notes to put the words on".into()),
                    }
                }
            }
            Music::After(delta, posts, body) => {
                let t0 = self.time;
                self.run(body);
                let t1 = self.time;
                self.time = t0 + *delta * self.scale;
                self.posts_as_dirs(posts);
                self.time = t1;
            }
            Music::Chords(items) => {
                if self.chord_names.is_none() {
                    self.note(Level::Dropped, 0, "\\chordmode outside a ChordNames context (chords written as notes) is not supported; left out".into());
                }
                for (h, dur) in items {
                    if let (Some(h), Some(hint)) = (h, self.chord_names) {
                        let mut h = h.clone();
                        if self.transpose.0 != 0 || self.transpose.1 != 0 {
                            let tr = |(step, alter): (u8, i8)| -> (u8, i8) {
                                let midi = SEMI[step as usize] + alter as i32 + self.transpose.1;
                                let idx = step as i32 + self.transpose.0;
                                let ns = idx.rem_euclid(7);
                                let mut a = (midi - SEMI[ns as usize] - 12 * idx.div_euclid(7)).rem_euclid(12);
                                if a > 6 {
                                    a -= 12;
                                }
                                (ns as u8, a as i8)
                            };
                            h.root = tr(h.root);
                            h.bass = h.bass.map(tr);
                        }
                        self.glob.harmonies.push((self.time, hint, h));
                    }
                    self.advance(*dur * self.scale);
                }
            }
            Music::Unsupported(what, line) => {
                self.note(Level::Dropped, *line, format!("{} is not supported; its content is left out", what));
            }
            Music::BarNumber(n) => {
                if !self.time.is_zero() && self.grace.is_none() {
                    self.glob.num_hints.push((self.time, n.clone()));
                }
            }
            Music::Nothing => {}
        }
    }

    fn cmd(&mut self, name: &str) {
        match name {
            "cadenzaOn" => self.glob.cadenzas.push((self.time, None)),
            "cadenzaOff" => {
                let t = self.time;
                if let Some(c) = self.glob.cadenzas.last_mut() {
                    if c.1.is_none() {
                        c.1 = Some(t);
                    }
                }
            }
            "numericTimeSignature" => self.numeric_time = true,
            "wavyGlissando" => self.wavy_gliss = true,
            "defaultTimeSignature" => self.numeric_time = false,
            "breathe" => {
                let caesura = std::mem::take(&mut self.caesura_next);
                if let Some(e) = self.last_event_mut() {
                    e.arts.push(if caesura { Art::Caesura } else { Art::Breath });
                }
            }
            "xNote" => self.xnote_next = true,
            "caesuraNext" => self.caesura_next = true,
            "slurDashed" => self.slur_style_next = 1,
            "slurDotted" => self.slur_style_next = 2,
            "tieDashed" => self.tie_style_next = 1,
            "tieDotted" => self.tie_style_next = 2,
            "caesura" => {
                if let Some(e) = self.last_event_mut() {
                    e.arts.push(Art::Caesura);
                }
            }
            "melisma" => {
                if let Some(v) = self.cur_voice() {
                    self.voices[v].st.manual_melisma = true;
                }
            }
            "melismaEnd" => {
                if let Some(v) = self.cur_voice() {
                    self.voices[v].st.manual_melisma = false;
                }
            }
            "fine" => {
                self.glob.bars.push((self.time, "|.".into()));
                let kind = DirKind::Words { text: "Fine".into(), italic: false, bold: false };
                self.glob.dirs.push(TDir { time: self.time, at_end: true, voice: None, placement: Some(Placement::Above), kind });
            }
            "section" => self.glob.bars.push((self.time, "||".into())),
            "sustainOn" => self.posts_as_dirs(&[Post::Pedal(Pedal::Start)]),
            "sustainOff" => self.posts_as_dirs(&[Post::Pedal(Pedal::Stop)]),
            _ => {}
        }
    }

    fn set(&mut self, ctx: Option<&str>, prop: &str, val: &Val, line: u32) {
        match prop {
            "instrumentName" | "shortInstrumentName" => {
                let text = val.text();
                let group_ctx = matches!(ctx, Some("PianoStaff") | Some("GrandStaff") | Some("StaffGroup") | Some("ChoirStaff"));
                let staff = if group_ctx { None } else { self.cur_staff(false) };
                match (staff, self.piano_stack.last().copied()) {
                    (Some(s), _) => {
                        if prop == "instrumentName" {
                            self.staves[s].inst = text;
                        } else {
                            self.staves[s].short = text;
                        }
                    }
                    (None, Some(p)) => {
                        if prop == "instrumentName" {
                            self.pianos[p].inst = text;
                        } else {
                            self.pianos[p].short = text;
                        }
                    }
                    (None, None) => {
                        if let (Some(&gi), true) = (self.group_stack.last(), prop == "instrumentName") {
                            if let ListItem::GroupStart { name, .. } = &mut self.list[gi] {
                                *name = text;
                            }
                        }
                    }
                }
            }
            "repeatCommands" => {
                if let Val::Scheme(s) = val {
                    if let Sexp::List(items) = s.unquoted() {
                        for it in items {
                            match it.unquoted() {
                                Sexp::Sym(x) if x == "start-repeat" => self.glob.left_rep.push(self.time),
                                Sexp::Sym(x) if x == "end-repeat" => self.glob.right_rep.push((self.time, None)),
                                Sexp::List(l) if l.first().and_then(|x| x.as_str()) == Some("volta") => match l.get(1).map(|x| x.unquoted()) {
                                    Some(Sexp::Bool(false)) => self.glob.end_stop.push(self.time),
                                    Some(Sexp::Str(t)) => {
                                        self.glob.end_start.push((self.time, Ending { number: ending_number(t), text: Some(t.clone()) }));
                                    }
                                    _ => self.note(Level::Warning, line, "repeatCommands: a volta text that is not a plain string is not read".into()),
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
            "currentBarNumber" => {
                let n = match val {
                    Val::Num(n) => Some(*n),
                    Val::Scheme(s) => s.as_f64(),
                    _ => None,
                };
                if let Some(n) = n {
                    self.glob.barnums.push((self.time, n as i64));
                }
            }
            "measureLength" => {
                let f = match val {
                    Val::Scheme(s) => match s.unquoted() {
                        Sexp::Rat(n, d) => Some(Frac::new(*n, *d)),
                        Sexp::Num(n) => Some(Frac::new((*n * 1920.0).round() as i64, 1920)),
                        Sexp::List(l) if l.first().and_then(|x| x.as_str()) == Some("ly:make-moment") => match (l.get(1), l.get(2)) {
                            (Some(Sexp::Rat(n, d)), _) => Some(Frac::new(*n, *d)),
                            (Some(Sexp::Num(n)), Some(Sexp::Num(d))) => Some(Frac::new(*n as i64, *d as i64)),
                            (Some(Sexp::Num(n)), None) => Some(Frac::new(*n as i64, 1)),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                };
                match f {
                    Some(f) if f > Frac::ZERO => self.glob.mlens.push((self.time, Some(f))),
                    _ => self.note(Level::Warning, line, "measureLength: value not understood".into()),
                }
            }
            "doubleSlurs" => self.double_next = true,
            "melismaBusyProperties" => {
                let off = matches!(val, Val::Scheme(s) if matches!(s.unquoted(), Sexp::List(l) if l.is_empty()));
                if let Some(v) = self.cur_voice() {
                    self.voices[v].st.mel_off = off;
                }
            }
            _ => {}
        }
    }

    fn repeat(&mut self, kind: &RepKind, count: u32, body: &Music, alts: &[Music]) {
        match kind {
            RepKind::Volta | RepKind::Segno => {
                if *kind == RepKind::Segno {
                    self.note(Level::Warning, 0, "\\repeat segno is read as \\repeat volta".into());
                }
                if self.time > Frac::ZERO {
                    self.glob.left_rep.push(self.time);
                }
                self.run(body);
                if alts.is_empty() {
                    self.glob.right_rep.push((self.time, if count > 2 { Some(count) } else { None }));
                    return;
                }
                let n = alts.len() as u32;
                let first_span = count.saturating_sub(n) + 1;
                for (i, alt) in alts.iter().enumerate() {
                    let nums: Vec<u32> = match alt {
                        Music::Volta(nums, _) if !nums.is_empty() => nums.clone(),
                        _ if i == 0 => (1..=first_span.max(1)).collect(),
                        _ => vec![first_span + i as u32],
                    };
                    let number = nums.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                    self.glob.end_start.push((self.time, Ending { number, text: None }));
                    self.run(alt);
                    self.glob.end_stop.push(self.time);
                    if i + 1 < alts.len() {
                        self.glob.right_rep.push((self.time, None));
                    }
                }
            }
            RepKind::Unfold | RepKind::Percent => {
                let n = alts.len() as u32;
                for i in 0..count {
                    self.run(body);
                    if n > 0 {
                        let idx = if i < count.saturating_sub(n) { 0 } else { (i - count.saturating_sub(n)) as usize };
                        self.run(&alts[idx.min(alts.len() - 1)]);
                    }
                }
            }
            RepKind::Tremolo => {
                // one note: a tremolo on a single stem; several: written out
                let single = match body {
                    Music::Ev(ne) => Some(ne),
                    Music::Seq(items) if items.len() == 1 => match &items[0] {
                        Music::Ev(ne) => Some(ne),
                        _ => None,
                    },
                    _ => None,
                };
                match single {
                    Some(ne) if matches!(ne.kind, EvKind::Notes(_)) => {
                        let mut big = ne.clone();
                        big.dur.mult = big.dur.mult * Frac::int(count as i64);
                        big.posts.push(Post::Art(Art::Tremolo(ne.dur.log.max(3) as u8), None));
                        self.event(&big);
                    }
                    _ => {
                        self.note(Level::Warning, 0, "\\repeat tremolo between notes is written out as repeated notes".into());
                        for _ in 0..count {
                            self.run(body);
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ lyrics

    /// put the collected lyrics on the notes; call once after the whole score has run
    pub fn attach_lyrics(&mut self) {
        let jobs = std::mem::take(&mut self.jobs);
        for job in jobs {
            match job.target {
                LyTarget::Name(n) => match self.voices.iter().position(|v| v.name.as_deref() == Some(n.as_str())) {
                    Some(v) => self.attach(v, &job.items),
                    None => {
                        let words = job.items.iter().filter(|i| matches!(i, LyItem::Syl { .. })).count();
                        self.note(Level::Dropped, 0, format!("\\lyricsto \"{}\": no voice with that name; {} syllables left out", n, words));
                    }
                },
                LyTarget::Voice(v) => self.attach(v, &job.items),
                LyTarget::Timed { staff, start } => self.attach_timed(staff, start, &job.items),
            }
        }
    }

    fn attach(&mut self, vi: usize, items: &[LyItem]) {
        self.voices[vi].lyric_lines += 1;
        let line = self.voices[vi].lyric_lines;
        let mut include_grace = false;
        let mut ignore_melismata = false;
        let mut i = 0;
        let mut hyphen_before = false;
        let mut stanza: Option<String> = None;
        let mut left_over = 0;
        let n_events = self.voices[vi].events.len();
        let mut e = 0;
        while i < items.len() {
            // settings that take no note
            match &items[i] {
                LyItem::Stanza(s) => {
                    stanza = Some(s.clone());
                    i += 1;
                    continue;
                }
                LyItem::IncludeGrace(b) => {
                    include_grace = *b;
                    i += 1;
                    continue;
                }
                LyItem::IgnoreMelismata(b) => {
                    ignore_melismata = *b;
                    i += 1;
                    continue;
                }
                LyItem::Hyphen => {
                    // a hyphen with no syllable before it: the next one ends a word begun elsewhere
                    hyphen_before = true;
                    i += 1;
                    continue;
                }
                LyItem::Extender => {
                    i += 1;
                    continue;
                }
                _ => {}
            }
            // next note that takes a syllable
            while e < n_events {
                let te = &self.voices[vi].events[e];
                let takes = !te.ev.is_rest() && (!te.mel_cont || ignore_melismata) && (te.ev.grace.is_none() || include_grace);
                if takes {
                    break;
                }
                e += 1;
            }
            match &items[i] {
                LyItem::Skip(_) => {
                    i += 1;
                    e += 1;
                }
                LyItem::Syl { text, .. } => {
                    let mut hyphen_after = false;
                    let mut extend = false;
                    let mut j = i + 1;
                    while j < items.len() {
                        match &items[j] {
                            LyItem::Hyphen => hyphen_after = true,
                            LyItem::Extender => extend = true,
                            _ => break,
                        }
                        j += 1;
                    }
                    if e < n_events {
                        let syllabic = match (hyphen_before, hyphen_after) {
                            (false, false) => Syllabic::Single,
                            (false, true) => Syllabic::Begin,
                            (true, true) => Syllabic::Middle,
                            (true, false) => Syllabic::End,
                        };
                        self.voices[vi].events[e].ev.lyrics.push(Lyric { line, stanza: stanza.take(), text: text.clone(), syllabic, extend });
                        e += 1;
                    } else {
                        left_over += 1;
                    }
                    hyphen_before = hyphen_after;
                    i = j;
                }
                _ => i += 1,
            }
        }
        if left_over > 0 {
            let name = self.voices[vi].name.clone().unwrap_or_else(|| format!("#{}", vi + 1));
            self.note(Level::Dropped, 0, format!("lyrics line {} of voice {}: {} syllables have no note to go on", line, name, left_over));
        }
    }

    fn attach_timed(&mut self, staff: Option<usize>, start: Frac, items: &[LyItem]) {
        let vi = staff.and_then(|s| self.voices.iter().position(|v| v.staff == s));
        let vi = match vi {
            Some(v) => v,
            None => {
                self.note(Level::Dropped, 0, "lyrics without \\lyricsto: no staff above to put them on".into());
                return;
            }
        };
        self.voices[vi].lyric_lines += 1;
        let line = self.voices[vi].lyric_lines;
        let mut t = start;
        let mut last = Frac::new(1, 4);
        let mut hyphen_before = false;
        let mut stanza = None;
        let mut lost = 0;
        let mut i = 0;
        while i < items.len() {
            match &items[i] {
                LyItem::Stanza(s) => stanza = Some(s.clone()),
                LyItem::Skip(d) => {
                    last = d.unwrap_or(last);
                    t += last;
                }
                LyItem::Syl { text, dur } => {
                    last = dur.unwrap_or(last);
                    let mut hyphen_after = false;
                    let mut extend = false;
                    while i + 1 < items.len() {
                        match &items[i + 1] {
                            LyItem::Hyphen => hyphen_after = true,
                            LyItem::Extender => extend = true,
                            _ => break,
                        }
                        i += 1;
                    }
                    let syllabic = match (hyphen_before, hyphen_after) {
                        (false, false) => Syllabic::Single,
                        (false, true) => Syllabic::Begin,
                        (true, true) => Syllabic::Middle,
                        (true, false) => Syllabic::End,
                    };
                    hyphen_before = hyphen_after;
                    match self.voices[vi].events.iter_mut().find(|e| e.time == t && !e.ev.is_rest() && e.ev.grace.is_none()) {
                        Some(e) => e.ev.lyrics.push(Lyric { line, stanza: stanza.take(), text: text.clone(), syllabic, extend }),
                        None => lost += 1,
                    }
                    t += last;
                }
                _ => {}
            }
            i += 1;
        }
        if lost > 0 {
            self.note(Level::Dropped, 0, format!("lyrics without \\lyricsto: {} syllables fall where the staff above has no note", lost));
        }
    }
}

fn collect_settings(m: &Music, out: &mut Vec<LyItem>) {
    match m {
        Music::Seq(items) | Music::Sim(items) => {
            for i in items {
                collect_settings(i, out);
            }
        }
        Music::Set { prop, val, .. } if prop == "stanza" => out.push(LyItem::Stanza(val.text().unwrap_or_default())),
        Music::Set { prop, val, .. } if prop == "includeGraceNotes" => out.push(LyItem::IncludeGrace(matches!(val, Val::Scheme(Sexp::Bool(true))))),
        Music::Var(v) => collect_settings(v, out),
        _ => {}
    }
}

/// rehearsal letter number n (1 = A; I is skipped, as LilyPond does)
fn mark_letter(n: u32) -> String {
    const L: &[u8] = b"ABCDEFGHJKLMNOPQRSTUVWXYZ";
    let n = n.max(1) - 1;
    let mut s = String::new();
    if n as usize >= L.len() {
        s.push(L[(n as usize / L.len() - 1) % L.len()] as char);
    }
    s.push(L[n as usize % L.len()] as char);
    s
}

/// "1." -> "1", "1.2." -> "1, 2", "1.-3." -> "1, 2, 3"
pub fn ending_number(text: &str) -> String {
    let mut nums: Vec<u32> = vec![];
    let mut cur = String::new();
    let mut range = false;
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                let n: u32 = cur.parse().unwrap_or(1);
                if range {
                    if let Some(&last) = nums.last() {
                        for k in last + 1..n.min(last + 50) {
                            nums.push(k);
                        }
                    }
                    range = false;
                }
                nums.push(n);
                cur.clear();
            }
            if c == '-' || c == '\u{2013}' || c == '\u{2014}' {
                range = true;
            }
        }
    }
    if nums.is_empty() {
        return "1".into();
    }
    nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
}
