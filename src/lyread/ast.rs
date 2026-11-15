//! Syntax tree of the LilyPond subset we read.

use super::lex::Sexp;
use crate::ir::{Art, Frac, NoteType, Pedal, Placement, Wedge};
use std::rc::Rc;

/// A pitch as written: octave marks are not yet resolved (that depends on \relative).
#[derive(Clone, Debug, PartialEq)]
pub struct Pitch {
    /// 0 = C ... 6 = B
    pub step: u8,
    pub alter: i8,
    /// number of `'` minus number of `,`
    pub marks: i32,
    /// `!`
    pub force_acc: bool,
    /// `?`
    pub caut_acc: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dur {
    pub log: i8,
    pub dots: u8,
    /// product of the `*n/d` factors
    pub mult: Frac,
}

impl Dur {
    pub fn ntype(&self) -> NoteType {
        NoteType { log: self.log, dots: self.dots }
    }
    pub fn len(&self) -> Frac {
        self.ntype().len() * self.mult
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Post {
    Tie,
    /// \laissezVibrer: a tie to nothing
    TieOut,
    /// \repeatTie: a tie from nothing
    TieIn,
    SlurStart(u8),
    SlurStop(u8),
    BeamStart,
    BeamStop,
    Art(Art, Option<Placement>),
    Dynamic(String, Option<Placement>),
    Wedge(Wedge, Option<Placement>),
    /// \cresc, \dim: text that behaves like a hairpin (ended by \!)
    TextSpan(String),
    Text { text: String, italic: bool, bold: bool, placement: Option<Placement> },
    Pedal(Pedal),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChordNote {
    pub pitch: Pitch,
    pub tie: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvKind {
    Notes(Vec<ChordNote>),
    /// `q`: the previous chord again
    ChordRepeat,
    /// a bare duration: the previous pitches again
    Again,
    Rest,
    /// `R`
    MeasureRest,
    /// `s`, `\skip`, `<>`
    Skip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoteEv {
    pub kind: EvKind,
    pub dur: Dur,
    pub posts: Vec<Post>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RepKind {
    Volta,
    Unfold,
    Percent,
    Tremolo,
    Segno,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MarkupText {
    /// lines separated by "\n"
    pub text: String,
    pub italic: bool,
    pub bold: bool,
    pub boxed: bool,
    /// a note glyph was part of the markup (metronome-like text)
    pub note: Option<NoteType>,
    /// "segno" / "coda" when the markup is that glyph
    pub glyph: Option<String>,
    /// the lines of a column, italic ones as `*text*`
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Str(String),
    Num(f64),
    Scheme(Sexp),
    Markup(MarkupText),
    Music(Rc<Music>),
    Other,
}

impl Val {
    /// text of a string or markup value
    pub fn text(&self) -> Option<String> {
        match self {
            Val::Str(s) => Some(s.clone()),
            Val::Markup(m) => Some(m.text.clone()),
            Val::Scheme(s) => s.as_str().map(String::from),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarkKind {
    /// \mark \default
    Default,
    Number(u32),
    Text(MarkupText),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LyItem {
    Syl { text: String, dur: Option<Frac> },
    Hyphen,
    Extender,
    /// `_` or \skip
    Skip(Option<Frac>),
    Stanza(String),
    IncludeGrace(bool),
    /// \set ignoreMelismata: every note takes a syllable
    IgnoreMelismata(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Music {
    Seq(Vec<Music>),
    /// << a b >>
    Sim(Vec<Music>),
    /// << a \\ b >>
    Voices(Vec<Music>),
    Ev(NoteEv),
    Var(Rc<Music>),
    Relative(Option<Pitch>, Box<Music>),
    Fixed(Pitch, Box<Music>),
    Absolute(Box<Music>),
    Transpose(Pitch, Pitch, Box<Music>),
    Context { new: bool, ctype: String, name: Option<String>, with: Vec<(String, Val)>, body: Box<Music> },
    /// num notes in the time of den
    Tuplet { num: u32, den: u32, span: Option<Frac>, body: Box<Music> },
    Scale(Frac, Box<Music>),
    Grace { slash: bool, slur: bool, body: Box<Music> },
    AfterGrace(Box<Music>, Box<Music>),
    Repeat { kind: RepKind, count: u32, body: Box<Music>, alts: Vec<Music> },
    /// only as the tail of a repeat body (2.24 syntax); lifted by the parser
    Alternative(Vec<Music>),
    Volta(Vec<u32>, Box<Music>),
    Time { beats: String, beat_type: u32 },
    Key { fifths: i8, minor: bool },
    Clef { sign: char, line: i8, octave: i8 },
    Partial(Frac),
    Tempo { text: Option<String>, unit: Option<(NoteType, String)> },
    Bar(String),
    Mark(MarkKind),
    Break,
    PageBreak,
    Set { ctx: Option<String>, prop: String, val: Val, line: u32 },
    Unset { ctx: Option<String>, prop: String },
    Omit { once: bool, path: String },
    /// commands without arguments that the interpreter cares about
    Cmd(String),
    Ottava(i8),
    /// \jump, \textEndMark: words at the end of the bar
    EndText(MarkupText),
    /// \textMark, \sectionLabel: words at the start of the bar
    StartText(MarkupText),
    Segno,
    Coda,
    Lyrics(Vec<LyItem>),
    LyricsTo(String, Box<Music>),
    AddLyrics(Box<Music>, Vec<Music>),
    /// \after delta post-events music
    After(Frac, Vec<Post>, Box<Music>),
    /// the source's number for the bar that ends here (from lyxml's own `% N` comments)
    BarNumber(String),
    /// content we could not read (chord mode, drum mode): the interpreter reports it
    Unsupported(String, u32),
    /// \chordmode: chord symbols (None = rest or skip) with their lengths
    Chords(Vec<(Option<crate::ir::Harmony>, Frac)>),
    Nothing,
}

/// Everything read from one file.
#[derive(Default, Debug)]
pub struct File {
    pub header: Vec<(String, Val)>,
    pub paper: Vec<(String, Val)>,
    /// raw text of every \layout block
    pub layout_raw: String,
    pub staff_size: Option<f64>,
    pub paper_size: Option<String>,
    pub scores: Vec<Music>,
}
