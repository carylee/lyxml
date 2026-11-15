//! The score model shared by every reader and writer.
//!
//! Time is measured in whole notes as exact fractions (`Frac`): a quarter note is 1/4.
//! A score is a list of parts; a part is a list of measures; a measure holds, per
//! (staff, voice), a list of events with explicit positions inside the measure.

use std::cmp::Ordering;
use std::fmt;

// ---------------------------------------------------------------- fractions

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Frac {
    pub n: i64,
    pub d: i64,
}

pub fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

impl Frac {
    pub const ZERO: Frac = Frac { n: 0, d: 1 };
    pub fn new(n: i64, d: i64) -> Frac {
        if d == 0 {
            return Frac::ZERO;
        }
        Frac::wide(n as i128, d as i128)
    }
    pub fn int(n: i64) -> Frac {
        Frac { n, d: 1 }
    }
    /// n/d from wide intermediates: reduced, then fitted into i64. Values that still do
    /// not fit (only absurd input gets there) are scaled down, never wrapped.
    fn wide(n: i128, d: i128) -> Frac {
        if d == 0 {
            return Frac::ZERO;
        }
        let (mut a, mut b) = (n.abs(), d.abs());
        while b != 0 {
            let t = a % b;
            a = b;
            b = t;
        }
        let g = a.max(1);
        let s: i128 = if d < 0 { -1 } else { 1 };
        let (mut n, mut d) = (s * n / g, s * d / g);
        let max = i64::MAX as i128 / 4;
        while n.abs() > max || d > max {
            n /= 2;
            d /= 2;
            if d == 0 {
                return Frac { n: if n < 0 { -(max as i64) } else { max as i64 }, d: 1 };
            }
        }
        Frac { n: n as i64, d: d as i64 }
    }
    pub fn is_zero(self) -> bool {
        self.n == 0
    }
    pub fn to_f64(self) -> f64 {
        self.n as f64 / self.d as f64
    }
    /// floor(self / other)
    pub fn div_floor(self, other: Frac) -> i64 {
        let q = self / other;
        q.n.div_euclid(q.d)
    }
}

impl Default for Frac {
    fn default() -> Frac {
        Frac::ZERO
    }
}
impl std::ops::Add for Frac {
    type Output = Frac;
    fn add(self, o: Frac) -> Frac {
        Frac::wide(self.n as i128 * o.d as i128 + o.n as i128 * self.d as i128, self.d as i128 * o.d as i128)
    }
}
impl std::ops::Sub for Frac {
    type Output = Frac;
    fn sub(self, o: Frac) -> Frac {
        Frac::wide(self.n as i128 * o.d as i128 - o.n as i128 * self.d as i128, self.d as i128 * o.d as i128)
    }
}
impl std::ops::Mul for Frac {
    type Output = Frac;
    fn mul(self, o: Frac) -> Frac {
        Frac::wide(self.n as i128 * o.n as i128, self.d as i128 * o.d as i128)
    }
}
impl std::ops::Div for Frac {
    type Output = Frac;
    fn div(self, o: Frac) -> Frac {
        Frac::wide(self.n as i128 * o.d as i128, self.d as i128 * o.n as i128)
    }
}
impl std::ops::AddAssign for Frac {
    fn add_assign(&mut self, o: Frac) {
        *self = *self + o;
    }
}
impl PartialOrd for Frac {
    fn partial_cmp(&self, o: &Frac) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Frac {
    fn cmp(&self, o: &Frac) -> Ordering {
        (self.n as i128 * o.d as i128).cmp(&(o.n as i128 * self.d as i128))
    }
}
impl fmt::Display for Frac {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.d == 1 {
            write!(f, "{}", self.n)
        } else {
            write!(f, "{}/{}", self.n, self.d)
        }
    }
}

// ---------------------------------------------------------------- score

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub header: Header,
    pub layout: Layout,
    /// Parts and group brackets in score order.
    pub part_list: Vec<PartListItem>,
    pub parts: Vec<Part>,
}

#[derive(Clone, Debug, Default)]
pub struct Header {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub dedication: Option<String>,
    pub composer: Option<String>,
    /// lyricist / poet (printed top left)
    pub poet: Option<String>,
    pub arranger: Option<String>,
    pub copyright: Option<String>,
    /// Extra right-aligned credit lines (the skill's `composer_lines`); when present they
    /// replace composer/arranger in print. `*text*` means italic.
    pub composer_lines: Vec<String>,
    /// Left-aligned credit lines (`poet_lines`); when present they replace the poet in print.
    pub poet_lines: Vec<String>,
    pub footnote: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub page_width_mm: Option<f64>,
    pub page_height_mm: Option<f64>,
    /// staff height in points (LilyPond's global staff size)
    pub staff_size: Option<f64>,
    pub first_page: Option<i32>,
    pub hide_empty: bool,
    /// None = engraver default
    pub bar_numbers: Option<bool>,
    /// a number on every bar, not only at the start of each system
    pub bar_numbers_all: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupSymbol {
    Bracket,
    Brace,
    Line,
    Square,
    None,
}

#[derive(Clone, Debug)]
pub enum PartListItem {
    GroupStart {
        number: String,
        symbol: GroupSymbol,
        name: Option<String>,
        /// barlines drawn through the group (None = unspecified)
        barline: Option<bool>,
    },
    GroupStop {
        number: String,
    },
    /// index into `Score::parts`
    Part(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Part {
    pub id: String,
    /// logical name (MusicXML part-name)
    pub name: String,
    /// printed name on the first system; None = print `name`; Some("") = print nothing.
    /// "\n" separates lines.
    pub display_name: Option<String>,
    /// printed name on later systems
    pub abbr: Option<String>,
    pub instrument: Option<String>,
    pub staves: u32,
    /// number of staff lines when not five (1 = a one-line percussion staff)
    pub staff_lines: Option<u32>,
    pub measures: Vec<Measure>,
}

#[derive(Clone, Debug, Default)]
pub struct Measure {
    /// printed number as in the source ("1", "20a", "0" for a pickup)
    pub number: String,
    /// pickup or the second half of a split bar: not counted
    pub implicit: bool,
    pub new_system: bool,
    pub new_page: bool,
    /// actual length (may differ from the time signature: pickups, free bars)
    pub len: Frac,
    pub attrs: Vec<AttrChange>,
    pub left: Barline,
    pub right: Barline,
    /// one entry per (staff, voice) that has anything in this measure;
    /// sorted by (staff, voice)
    pub voices: Vec<VoiceSeg>,
    pub directions: Vec<Direction>,
    /// chord symbols
    pub harmonies: Vec<Harmony>,
}

/// A chord symbol above the staff.
#[derive(Clone, Debug, PartialEq)]
pub struct Harmony {
    pub pos: Frac,
    /// root: 0 = C ... 6 = B, and its alteration in semitones
    pub root: (u8, i8),
    /// MusicXML kind: "major", "minor-seventh", "dominant", "none" ...
    pub kind: String,
    pub bass: Option<(u8, i8)>,
    /// altered / added / removed degrees: (degree, alteration, 'a'dd | 'l' alter | 's'ubtract)
    pub degrees: Vec<(u8, i8, char)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Barline {
    /// MusicXML bar-style name: regular, light-light, light-heavy, heavy-light,
    /// heavy-heavy, dashed, dotted, none, tick, short
    pub style: Option<String>,
    /// repeat sign on this barline (left barline: forward; right barline: backward)
    pub repeat: bool,
    pub times: Option<u32>,
    /// a volta bracket starts here (left barline)
    pub ending_start: Option<Ending>,
    /// a volta bracket ends here (right barline)
    pub ending_stop: Option<EndingStop>,
}

impl Barline {
    pub fn is_plain(&self) -> bool {
        *self == Barline::default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ending {
    /// "1", "1, 2"
    pub number: String,
    /// printed text ("1.", "1.2."); None = derive from number
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EndingStop {
    pub number: String,
    /// bracket left open on the right (MusicXML "discontinue")
    pub open: bool,
}

#[derive(Clone, Debug)]
pub struct AttrChange {
    pub pos: Frac,
    pub kind: Attr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimeSymbol {
    Common,
    Cut,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    Key {
        fifths: i8,
        minor: bool,
    },
    Time {
        /// "4", or "3+2" for additive meters
        beats: String,
        beat_type: u32,
        symbol: Option<TimeSymbol>,
        /// false = hidden time signature (free bar)
        print: bool,
    },
    Clef {
        staff: u32,
        /// 'G', 'F', 'C', 'P' (percussion), 'T' (tab)
        sign: char,
        line: i8,
        /// -1 = sounds an octave lower (treble_8)
        octave: i8,
    },
}

impl Attr {
    /// total length of one measure, for Time
    pub fn measure_len(&self) -> Option<Frac> {
        if let Attr::Time { beats, beat_type, .. } = self {
            let n: i64 = beats.split('+').filter_map(|x| x.trim().parse::<i64>().ok()).sum();
            if n > 0 && *beat_type > 0 {
                return Some(Frac::new(n, *beat_type as i64));
            }
        }
        None
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiceSeg {
    /// 1-based staff within the part
    pub staff: u32,
    /// voice number as in the source (MusicXML <voice>); unique within the part
    pub voice: u32,
    /// sorted by pos; grace notes sit at the pos of the note they precede, before it
    pub events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteType {
    /// 0 = whole, 1 = half, 2 = quarter, 3 = eighth ...; -1 = breve, -2 = long
    pub log: i8,
    pub dots: u8,
}

impl NoteType {
    /// written length, before tuplet scaling
    pub fn len(self) -> Frac {
        let base = if self.log >= 0 { Frac::new(1, 1 << self.log) } else { Frac::int(1 << (-self.log)) };
        let mut total = base;
        let mut add = base;
        for _ in 0..self.dots {
            add = add * Frac::new(1, 2);
            total += add;
        }
        total
    }
    /// the note type (with up to 3 dots) of exactly this length, if any
    pub fn from_len(len: Frac) -> Option<NoteType> {
        for dots in 0u8..=3 {
            for log in -2i8..=8 {
                let t = NoteType { log, dots };
                if t.len() == len {
                    return Some(t);
                }
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuplet {
    /// `actual` notes in the time of `normal`
    pub actual: u32,
    pub normal: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beam {
    Begin,
    Continue,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StemDir {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Art {
    Staccato,
    Staccatissimo,
    Accent,
    /// marcato (^)
    StrongAccent,
    Tenuto,
    /// portato: tenuto + staccato
    DetachedLegato,
    Fermata { inverted: bool },
    Trill,
    Mordent,
    /// inverted mordent (LilyPond \prall)
    Prall,
    Turn,
    /// single-note tremolo with this many strokes
    Tremolo(u8),
    UpBow,
    DownBow,
    Harmonic,
    OpenString,
    Stopped,
    Fingering(String),
    Arpeggio,
    /// breath mark after the note
    Breath,
    /// caesura after the note
    Caesura,
    /// glissando line starting on this note
    Glissando,
    /// ... drawn wavy
    GlissandoWavy,
    /// a fall off the note (jazz)
    Falloff,
    /// a rise off the note
    Doit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Syllabic {
    Single,
    Begin,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lyric {
    /// 1-based verse line
    pub line: u32,
    /// printed verse number ("1.") to show before this syllable
    pub stanza: Option<String>,
    pub text: String,
    pub syllabic: Syllabic,
    /// an extender line follows
    pub extend: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// 0 = C ... 6 = B
    pub step: u8,
    /// semitones: -1 flat, 1 sharp
    pub alter: i8,
    /// scientific octave: middle C = C4
    pub octave: i8,
    pub tie_start: bool,
    pub tie_stop: bool,
    /// the source prints an accidental on this note
    pub acc_shown: bool,
    /// ... in parentheses / as a courtesy
    pub acc_caut: bool,
}

impl Note {
    pub fn new(step: u8, alter: i8, octave: i8) -> Note {
        Note { step, alter, octave, tie_start: false, tie_stop: false, acc_shown: false, acc_caut: false }
    }
    /// MIDI-style number (C4 = 60)
    pub fn midi(&self) -> i32 {
        const SEMI: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
        (self.octave as i32 + 1) * 12 + SEMI[self.step as usize % 7] + self.alter as i32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// one note or a chord
    Notes(Vec<Note>),
    Rest {
        /// a centred whole-measure rest
        whole_measure: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grace {
    pub slash: bool,
}

#[derive(Clone, Debug)]
pub struct Event {
    /// offset from the start of the measure
    pub pos: Frac,
    /// sounding length (zero for grace notes)
    pub dur: Frac,
    pub kind: EventKind,
    /// written note value; None = derive from `dur`
    pub ntype: Option<NoteType>,
    /// tuplet ratio this event is written in
    pub tuplet: Option<Tuplet>,
    /// a tuplet bracket starts / stops on this event (when the source says so)
    pub tuplet_start: bool,
    pub tuplet_stop: bool,
    /// print the tuplet number / bracket (only read on the event with tuplet_start)
    pub tuplet_number: bool,
    pub tuplet_bracket: Option<bool>,
    pub grace: Option<Grace>,
    /// slur numbers starting / stopping on this event
    pub slur_starts: Vec<u8>,
    pub slur_stops: Vec<u8>,
    /// primary (eighth-level) beam
    pub beam: Option<Beam>,
    pub arts: Vec<Art>,
    pub lyrics: Vec<Lyric>,
    pub stem: Option<StemDir>,
    pub cue: bool,
    /// cross-shaped note heads
    pub notehead_x: bool,
    /// unpitched (percussion): the notes give a staff position, not a pitch
    pub unpitched: bool,
    /// line style of the slurs / ties starting here: 0 solid, 1 dashed, 2 dotted
    pub slur_style: u8,
    pub tie_style: u8,
    /// two slurs start on this chord, one above and one below
    pub double_slur: bool,
}

impl Event {
    pub fn new(pos: Frac, dur: Frac, kind: EventKind) -> Event {
        Event {
            pos,
            dur,
            kind,
            ntype: None,
            tuplet: None,
            tuplet_start: false,
            tuplet_stop: false,
            tuplet_number: true,
            tuplet_bracket: None,
            grace: None,
            slur_starts: vec![],
            slur_stops: vec![],
            beam: None,
            arts: vec![],
            lyrics: vec![],
            stem: None,
            cue: false,
            notehead_x: false,
            unpitched: false,
            slur_style: 0,
            tie_style: 0,
            double_slur: false,
        }
    }
    pub fn is_rest(&self) -> bool {
        matches!(self.kind, EventKind::Rest { .. })
    }
    pub fn notes(&self) -> &[Note] {
        match &self.kind {
            EventKind::Notes(n) => n,
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Above,
    Below,
}

#[derive(Clone, Debug)]
pub struct Direction {
    /// offset from the start of the measure (may equal the measure length:
    /// "at the end of the bar")
    pub pos: Frac,
    pub staff: u32,
    /// voice the source ties it to, if any
    pub voice: Option<u32>,
    pub placement: Option<Placement>,
    pub kind: DirKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wedge {
    Cresc,
    Dim,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pedal {
    Start,
    Stop,
    Change,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DirKind {
    /// "p", "mf", "sfz" ...
    Dynamic(String),
    Wedge(Wedge),
    Words {
        text: String,
        italic: bool,
        bold: bool,
    },
    /// tempo mark: optional words, optional "note = number"
    Tempo {
        text: Option<String>,
        /// beat unit and what it equals ("120", "c. 88-96")
        metronome: Option<(NoteType, String)>,
    },
    Rehearsal(String),
    Segno,
    Coda,
    /// +1 = 8va (written an octave below sounding), -1 = 8vb, 2 = 15ma; 0 = end
    Ottava(i8),
    Pedal(Pedal),
}

// ---------------------------------------------------------------- helpers

impl Score {
    /// Sort events and voices into canonical order.
    pub fn normalize(&mut self) {
        for p in &mut self.parts {
            if p.staves == 0 {
                p.staves = 1;
            }
            for m in &mut p.measures {
                m.voices.retain(|v| !v.events.is_empty());
                m.voices.sort_by_key(|v| (v.staff, v.voice));
                for v in &mut m.voices {
                    // stable: grace notes stay before their main note
                    v.events.sort_by(|a, b| a.pos.cmp(&b.pos));
                }
                m.directions.sort_by(|a, b| a.pos.cmp(&b.pos));
                m.harmonies.sort_by(|a, b| a.pos.cmp(&b.pos));
                m.attrs.sort_by(|a, b| a.pos.cmp(&b.pos));
            }
        }
    }
}

/// Alteration the key signature gives to a step (0 = C).
pub fn key_alter(fifths: i8, step: u8) -> i8 {
    const ORDER_SHARP: [u8; 7] = [3, 0, 4, 1, 5, 2, 6];
    if fifths > 0 {
        ORDER_SHARP[..(fifths as usize).min(7)].contains(&step) as i8
    } else if fifths < 0 {
        let n = ((-fifths) as usize).min(7);
        -(ORDER_SHARP[7 - n..].contains(&step) as i8)
    } else {
        0
    }
}

/// Decide, for every note, whether an engraver following the common rule (an accidental
/// holds for its octave until the end of the bar; tied-over notes print none) would print
/// an accidental. Returns `[measure][voice][event][note] -> prints`.
pub fn default_accidentals(part: &Part) -> Vec<Vec<Vec<Vec<bool>>>> {
    let mut out: Vec<Vec<Vec<Vec<bool>>>> = part
        .measures
        .iter()
        .map(|m| m.voices.iter().map(|v| v.events.iter().map(|e| vec![false; e.notes().len()]).collect()).collect())
        .collect();
    for staff in 1..=part.staves.max(1) {
        let mut fifths: i8 = 0;
        for (mi, m) in part.measures.iter().enumerate() {
            let mut keys: Vec<(Frac, i8)> = m
                .attrs
                .iter()
                .filter_map(|a| if let Attr::Key { fifths, .. } = a.kind { Some((a.pos, fifths)) } else { None })
                .collect();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            // all notes of this staff in time order
            let mut items: Vec<(Frac, usize, usize, usize)> = vec![];
            for (vi, v) in m.voices.iter().enumerate() {
                if v.staff != staff {
                    continue;
                }
                for (ei, e) in v.events.iter().enumerate() {
                    for ni in 0..e.notes().len() {
                        items.push((e.pos, vi, ei, ni));
                    }
                }
            }
            items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
            let mut state: std::collections::HashMap<(u8, i8), i8> = std::collections::HashMap::new();
            let mut ki = 0;
            for (pos, vi, ei, ni) in items {
                while ki < keys.len() && keys[ki].0 <= pos {
                    fifths = keys[ki].1;
                    state.clear();
                    ki += 1;
                }
                let n = &m.voices[vi].events[ei].notes()[ni];
                let cur = *state.get(&(n.step, n.octave)).unwrap_or(&key_alter(fifths, n.step));
                out[mi][vi][ei][ni] = n.alter != cur && !n.tie_stop;
                if !n.tie_stop {
                    state.insert((n.step, n.octave), n.alter);
                }
            }
            while ki < keys.len() {
                fifths = keys[ki].1;
                ki += 1;
            }
        }
    }
    out
}
