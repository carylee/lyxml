//! Recursive-descent parser: LilyPond source -> syntax tree.
//!
//! Durations are resolved here (the "previous duration" is lexical state); pitches keep
//! their octave marks, because what they mean depends on the enclosing \relative, which
//! may be applied to a variable long after it was parsed.

use super::ast::*;
use super::lex::{is_alpha, Lexer, Sexp};
use crate::diag::{Diags, Level};
use crate::ir::{Art, Frac, NoteType, Pedal, Placement, Wedge};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Nederlands,
    English,
    Deutsch,
    Romance,
}

fn lang_of(name: &str) -> Option<Lang> {
    let n = name.trim_end_matches(".ly").to_lowercase();
    Some(match n.as_str() {
        "nederlands" | "dutch" => Lang::Nederlands,
        "english" => Lang::English,
        "deutsch" | "german" | "norsk" | "suomi" | "svenska" => Lang::Deutsch,
        "italiano" | "français" | "francais" | "español" | "espanol" | "català" | "catalan" | "portugues" | "português" | "vlaams" => Lang::Romance,
        _ => return None,
    })
}

const DYNAMICS: &[&str] =
    &["ppppp", "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fffff", "fp", "sf", "sff", "sp", "spp", "sfz", "rfz", "fz", "sfp", "n"];

fn art_of(name: &str) -> Option<Art> {
    Some(match name {
        "staccato" => Art::Staccato,
        "staccatissimo" => Art::Staccatissimo,
        "accent" => Art::Accent,
        "marcato" => Art::StrongAccent,
        "tenuto" => Art::Tenuto,
        "portato" => Art::DetachedLegato,
        "fermata" | "shortfermata" | "longfermata" | "verylongfermata" | "veryshortfermata" | "fermataMarkup" => Art::Fermata { inverted: false },
        "trill" | "startTrillSpan" => Art::Trill,
        "mordent" => Art::Mordent,
        "prall" => Art::Prall,
        "turn" => Art::Turn,
        "upbow" => Art::UpBow,
        "downbow" => Art::DownBow,
        "flageolet" | "harmonic" => Art::Harmonic,
        "open" => Art::OpenString,
        "stopped" => Art::Stopped,
        "arpeggio" => Art::Arpeggio,
        "glissando" => Art::Glissando,
        _ => return None,
    })
}

/// post-event commands we accept and ignore
const POST_IGNORED: &[&str] = &[
    "startTextSpan", "stopTextSpan", "stopTrillSpan", "noBeam", "espressivo", "startGroup", "stopGroup",
    "sostenutoOn", "sostenutoOff", "unaCorda", "treCorde", "segno", "coda", "varcoda", "snappizzicato", "thumb", "halfopen", "lheel", "rheel",
    "ltoe", "rtoe", "upprall", "downprall", "prallprall", "prallmordent", "upmordent", "downmordent", "lineprall", "pralldown", "prallup",
    "reverseturn", "signumcongruentiae", "henzeshortfermata", "henzelongfermata", "slashturn", "haydnturn", "startMeasureCount", "stopMeasureCount",
    "startMeasureSpanner", "stopMeasureSpanner", "episemInitium", "episemFinis", "cr", "decr", "rtoe", "accentus", "circulus", "ictus", "semicirculus",
];

/// commands taking no arguments that the interpreter wants to see
const SIMPLE: &[&str] = &[
    "voiceOne", "voiceTwo", "voiceThree", "voiceFour", "oneVoice", "autoBeamOff", "autoBeamOn", "cadenzaOn", "cadenzaOff", "numericTimeSignature",
    "defaultTimeSignature", "breathe", "caesura", "melisma", "melismaEnd", "fine", "section", "sustainOn", "sustainOff", "arpeggio",
    "xNote", "slurDashed", "slurDotted", "tieDashed", "tieDotted",
];

/// commands taking no arguments that we drop without comment
const SILENT: &[&str] = &[
    "noBreak", "noPageBreak", "allowPageBreak", "allowBreak", "pageTurn", "noPageTurn", "allowPageTurn", "showStaffSwitch", "hideStaffSwitch", "stopStaff",
    "startStaff", "normalsize", "small", "tiny", "teeny", "huge", "large", "compressEmptyMeasures", "expandEmptyMeasures", "compressFullBarRests",
    "expandFullBarRests", "hideNotes", "unHideNotes", "crescTextCresc", "dimTextDim", "dimTextDecresc", "dimTextDecr", "crescHairpin", "dimHairpin",
    "temporary", "undo", "single", "parenthesize", "default", "rest", "pitchedTrill", "frenchChords", "germanChords", "italianChords", "semiGermanChords",
    "pointAndClickOff", "pointAndClickOn", "sacredHarpHeads", "aikenHeads", "funkHeads", "walkerHeads", "southernHarmonyHeads", "cueClefUnset",
    "voiceNeutralStyle", "voiceOneStyle", "voiceTwoStyle", "voiceThreeStyle", "voiceFourStyle", "mergeDifferentlyDottedOn", "mergeDifferentlyHeadedOn",
    "RemoveEmptyStaves", "RemoveAllEmptyStaves", "newSpacingSection", "break", "arabicStringNumbers", "romanStringNumbers", "bassFigureExtendersOn",
    "predefinedFretboardsOn", "predefinedFretboardsOff", "tabFullNotation", "mm", "cm", "in", "pt", "sectionLabel",
];

/// commands with arguments we skip: s = scheme/string/number, p = property path, m = music
/// (returned), P = pitch, w = one bare token
const WITH_ARGS: &[(&str, &str)] = &[
    ("unfoldRepeats", "m"),
    ("compressMMRests", "m"),
    ("autoChange", "m"),
    ("articulate", "m"),
    ("killCues", "m"),
    ("displayLilyMusic", "m"),
    ("displayMusic", "m"),
    ("tag", "sm"),
    ("keepWithTag", "sm"),
    ("removeWithTag", "sm"),
    ("magnifyMusic", "sm"),
    ("magnifyStaff", "s"),
    ("shape", "sp"),
    ("transposition", "P"),
    ("accidentalStyle", "w"),
    ("barNumberCheck", "s"),
    ("bookOutputName", "s"),
    ("bookOutputSuffix", "s"),
    ("cueDuring", "ssm"),
    ("cueDuringWithClef", "sssm"),
    ("quoteDuring", "sm"),
    ("instrumentSwitch", "s"),
    ("overrideTimeSignatureSettings", "ssss"),
    ("applyContext", "s"),
    ("applyOutput", "ss"),
    ("applyMusic", "sm"),
    ("label", "s"),
    ("tocItem", "s"),
    ("offset", "ssp"),
    ("alterBroken", "ssp"),
    ("stringTuning", "s"),
    ("bendAfter", "s"),
    ("footnote", "sss"),
    ("ambitusAfter", "w"),
    ("styledNoteHeads", "ssm"),
    ("pageBreak", ""),
    ("set-global-staff-size", "s"),
    ("setBeatStructure", "s"),
    ("featherDurations", "sm"),
    ("partCombineForce", "s"),
    ("staffHighlight", "s"),
];

pub struct Parser<'d> {
    pub lx: Lexer,
    pub diags: &'d mut Diags,
    vars: HashMap<String, Val>,
    lang: Lang,
    last_dur: Dur,
    base_dir: Option<PathBuf>,
    include_depth: u32,
    pending_once: bool,
    depth: u32,
    warned: HashSet<String>,
    pub file: File,
}

type R<T> = Result<T, String>;

impl<'d> Parser<'d> {
    pub fn new(src: &str, base_dir: Option<PathBuf>, diags: &'d mut Diags) -> Parser<'d> {
        Parser {
            lx: Lexer::new(src),
            diags,
            vars: HashMap::new(),
            lang: Lang::Nederlands,
            last_dur: Dur { log: 2, dots: 0, mult: Frac::int(1) },
            base_dir,
            include_depth: 0,
            pending_once: false,
            depth: 0,
            warned: HashSet::new(),
            file: File::default(),
        }
    }

    fn err<T>(&self, msg: impl AsRef<str>) -> R<T> {
        Err(format!("line {}: {}", self.lx.line(), msg.as_ref()))
    }

    /// report once per distinct message
    fn note(&mut self, level: Level, msg: String) {
        if self.warned.insert(msg.clone()) {
            let line = self.lx.line();
            self.diags.at_line(level, line, msg);
        }
    }

    fn expect(&mut self, s: &str) -> R<()> {
        if self.lx.eat(s) {
            Ok(())
        } else {
            let found = match self.lx.raw() {
                Some(c) => format!("`{}`", c as char),
                None => "end of file".to_string(),
            };
            self.err(format!("expected `{}`, found {}", s, found))
        }
    }

    // ------------------------------------------------------------ top level

    pub fn parse_toplevel(&mut self, in_braces: bool) -> R<()> {
        loop {
            if self.lx.eof() {
                if in_braces {
                    return self.err("unclosed `{`");
                }
                return Ok(());
            }
            let c = self.lx.raw().unwrap();
            match c {
                b'}' if in_braces => {
                    self.lx.pos += 1;
                    return Ok(());
                }
                b'\\' => {
                    let save = self.lx.pos;
                    let cmd = self.lx.read_command();
                    match cmd.as_str() {
                        "version" => {
                            self.read_string_arg()?;
                        }
                        "language" => {
                            let l = self.read_string_arg()?;
                            self.set_language(&l);
                        }
                        "include" => {
                            let f = self.read_string_arg()?;
                            self.include(&f)?;
                        }
                        "header" => {
                            let a = self.parse_assignments()?;
                            self.file.header.extend(a);
                        }
                        "paper" => {
                            let a = self.parse_assignments()?;
                            self.file.paper.extend(a);
                        }
                        "layout" => {
                            self.lx.skip_ws();
                            let raw = self.lx.skip_braces()?;
                            self.file.layout_raw.push_str(&raw);
                            self.file.layout_raw.push('\n');
                        }
                        "midi" => {
                            self.lx.skip_ws();
                            self.lx.skip_braces()?;
                        }
                        "score" => {
                            let m = self.parse_score()?;
                            self.file.scores.push(m);
                        }
                        "book" | "bookpart" => {
                            self.expect("{")?;
                            self.parse_toplevel(true)?;
                        }
                        "markup" | "markuplist" => {
                            self.parse_mk()?;
                        }
                        "pointAndClickOff" | "pointAndClickOn" | "pageBreak" | "noPageBreak" => {}
                        _ => {
                            self.lx.pos = save;
                            let m = self.parse_music()?;
                            if m != Music::Nothing {
                                self.file.scores.push(m);
                            }
                        }
                    }
                }
                b'#' | b'$' => {
                    self.lx.pos += 1;
                    let s = self.lx.read_scheme()?;
                    self.toplevel_scheme(&s);
                }
                b'{' | b'<' => {
                    let m = self.parse_music()?;
                    self.file.scores.push(m);
                }
                b'"' => {
                    let name = self.lx.read_string()?;
                    if self.lx.eat("=") {
                        let v = self.parse_value()?;
                        self.vars.insert(name, v);
                    }
                }
                c if is_alpha(c) => {
                    let name = self.lx.read_ident(true);
                    let mut dotted = false;
                    while self.lx.raw() == Some(b'.') {
                        self.lx.pos += 1;
                        self.lx.read_ident(true);
                        dotted = true;
                    }
                    if self.lx.eat("=") {
                        let v = self.parse_value()?;
                        if !dotted {
                            self.vars.insert(name, v);
                        }
                    } else {
                        return self.err(format!("unexpected `{}` at top level", name));
                    }
                }
                c => return self.err(format!("unexpected `{}` at top level", c as char)),
            }
        }
    }

    fn toplevel_scheme(&mut self, s: &Sexp) {
        if let Sexp::List(items) = s {
            match items.first().and_then(|x| x.as_str()) {
                Some("set-global-staff-size") => self.file.staff_size = items.get(1).and_then(|x| x.as_f64()),
                Some("set-default-paper-size") | Some("set-paper-size") => {
                    self.file.paper_size = items.get(1).and_then(|x| x.as_str()).map(String::from)
                }
                _ => {}
            }
        }
    }

    fn read_string_arg(&mut self) -> R<String> {
        self.lx.skip_ws();
        match self.lx.raw() {
            Some(b'"') => self.lx.read_string(),
            Some(b'#') => {
                self.lx.pos += 1;
                let s = self.lx.read_scheme()?;
                Ok(s.as_str().unwrap_or("").to_string())
            }
            _ => Ok(self.lx.read_token()),
        }
    }

    fn set_language(&mut self, name: &str) {
        match lang_of(name) {
            Some(l) => self.lang = l,
            None => self.note(Level::Warning, format!("note-name language `{}` is not supported; reading as nederlands", name)),
        }
    }

    fn include(&mut self, name: &str) -> R<()> {
        let base = name.rsplit('/').next().unwrap_or(name);
        if let Some(l) = lang_of(base) {
            self.lang = l;
            return Ok(());
        }
        const STOCK: &[&str] = &[
            "articulate.ly", "gregorian.ly", "predefined-guitar-fretboards.ly", "predefined-ukulele-fretboards.ly", "lilypond-book-preamble.ly",
            "event-listener.ly", "arabic.ly", "makam.ly", "bagpipe.ly", "festival.ly", "base-tkit.ly", "satb.ly", "ssaattbb.ly", "piano-tkit.ly",
            "swing.ly", "hel-arabic.ly", "turkish-makam.ly", "chord-grid.ly",
        ];
        if STOCK.contains(&base) {
            if base.ends_with("satb.ly") || base.ends_with("ssaattbb.ly") {
                self.note(Level::Dropped, format!("\\include \"{}\": LilyPond's built-in score templates are not supported", name));
            }
            return Ok(());
        }
        let text = self.base_dir.as_ref().and_then(|d| std::fs::read(d.join(name)).ok());
        let text = match text {
            Some(t) => String::from_utf8_lossy(&t).into_owned(),
            None => {
                self.note(Level::Warning, format!("\\include \"{}\": file not found, skipped", name));
                return Ok(());
            }
        };
        if self.include_depth > 20 {
            return self.err("\\include nested too deeply");
        }
        let saved = std::mem::replace(&mut self.lx, Lexer::new(&text));
        self.include_depth += 1;
        let r = self.parse_toplevel(false).map_err(|e| format!("in \"{}\": {}", name, e));
        self.include_depth -= 1;
        self.lx = saved;
        r
    }

    /// `{ name = value ... }` blocks: \header, \paper, \with
    fn parse_assignments(&mut self) -> R<Vec<(String, Val)>> {
        self.expect("{")?;
        let mut out = vec![];
        loop {
            match self.lx.peek() {
                None => return self.err("unclosed `{`"),
                Some(b'}') => {
                    self.lx.pos += 1;
                    return Ok(out);
                }
                Some(b'#') | Some(b'$') => {
                    self.lx.pos += 1;
                    let s = self.lx.read_scheme()?;
                    self.toplevel_scheme_in_paper(&s);
                    out.push(("#".into(), Val::Scheme(s)));
                }
                Some(b'\\') => {
                    let cmd = self.lx.read_command();
                    match cmd.as_str() {
                        "consists" | "remove" | "accepts" | "denies" | "alias" | "type" | "name" | "description" | "defaultchild" => {
                            let a = self.read_string_arg()?;
                            out.push((cmd, Val::Str(a)));
                        }
                        "override" => {
                            let p = self.read_path()?;
                            self.expect("=")?;
                            let v = self.parse_value()?;
                            out.push((format!("override {}", p), v));
                        }
                        "revert" | "omit" | "hide" | "unset" => {
                            let p = self.read_path()?;
                            out.push((format!("{} {}", cmd, p), Val::Other));
                        }
                        "set" => {
                            let p = self.read_path()?;
                            self.expect("=")?;
                            let v = self.parse_value()?;
                            out.push((p.rsplit('.').next().unwrap_or("").to_string(), v));
                        }
                        "context" => {
                            self.lx.skip_ws();
                            let raw = self.lx.skip_braces()?;
                            out.push(("context".into(), Val::Str(raw)));
                        }
                        _ => out.push((cmd, Val::Other)),
                    }
                }
                Some(b'"') => {
                    let name = self.lx.read_string()?;
                    self.expect("=")?;
                    let v = self.parse_value()?;
                    out.push((name, v));
                }
                Some(c) if is_alpha(c) => {
                    let mut name = self.lx.read_ident(true);
                    while self.lx.raw() == Some(b'.') {
                        self.lx.pos += 1;
                        name.push('.');
                        name.push_str(&self.lx.read_ident(true));
                    }
                    self.expect("=")?;
                    let v = self.parse_value()?;
                    out.push((name, v));
                }
                Some(c) => return self.err(format!("unexpected `{}` in block", c as char)),
            }
        }
    }

    fn toplevel_scheme_in_paper(&mut self, s: &Sexp) {
        if let Sexp::List(items) = s {
            if items.first().and_then(|x| x.as_str()) == Some("set-paper-size") {
                self.file.paper_size = items.get(1).and_then(|x| x.as_str()).map(String::from);
            }
        }
    }

    fn parse_score(&mut self) -> R<Music> {
        self.expect("{")?;
        let mut items = vec![];
        loop {
            match self.lx.peek() {
                None => return self.err("unclosed \\score"),
                Some(b'}') => {
                    self.lx.pos += 1;
                    break;
                }
                _ => {}
            }
            match self.lx.peek_command().as_deref() {
                Some("header") => {
                    self.lx.read_command();
                    let a = self.parse_assignments()?;
                    for (k, v) in a {
                        let key = if k == "piece" { "title".to_string() } else { k };
                        if !self.file.header.iter().any(|(n, _)| *n == key) {
                            self.file.header.push((key, v));
                        }
                    }
                }
                Some("layout") => {
                    self.lx.read_command();
                    self.lx.skip_ws();
                    let raw = self.lx.skip_braces()?;
                    self.file.layout_raw.push_str(&raw);
                    self.file.layout_raw.push('\n');
                }
                Some("midi") => {
                    self.lx.read_command();
                    self.lx.skip_ws();
                    self.lx.skip_braces()?;
                }
                _ => {
                    let m = self.parse_music()?;
                    if m != Music::Nothing {
                        items.push(m);
                    }
                }
            }
        }
        Ok(if items.len() == 1 { items.pop().unwrap() } else { Music::Seq(items) })
    }

    // ------------------------------------------------------------ values

    fn parse_value(&mut self) -> R<Val> {
        match self.lx.peek() {
            None => self.err("value expected"),
            Some(b'"') => Ok(Val::Str(self.lx.read_string()?)),
            Some(b'#') | Some(b'$') => {
                self.lx.pos += 1;
                Ok(Val::Scheme(self.lx.read_scheme()?))
            }
            Some(c) if c.is_ascii_digit() || ((c == b'-' || c == b'.') && self.lx.at(1).map(|d| d.is_ascii_digit() || d == b'.').unwrap_or(false)) => {
                let neg = c == b'-';
                if neg {
                    self.lx.pos += 1;
                }
                let mut n = self.lx.read_number().unwrap_or(0.0);
                if self.lx.raw() == Some(b'/') && self.lx.at(1).map(|d| d.is_ascii_digit()).unwrap_or(false) {
                    self.lx.pos += 1;
                    let d = self.lx.read_number().unwrap_or(1.0);
                    if d != 0.0 {
                        n /= d;
                    }
                }
                if neg {
                    n = -n;
                }
                // a unit makes it millimetres
                match self.lx.peek_command().as_deref() {
                    Some("mm") => {
                        self.lx.read_command();
                    }
                    Some("cm") => {
                        self.lx.read_command();
                        n *= 10.0;
                    }
                    Some("in") => {
                        self.lx.read_command();
                        n *= 25.4;
                    }
                    Some("pt") => {
                        self.lx.read_command();
                        n *= 25.4 / 72.27;
                    }
                    _ => {}
                }
                Ok(Val::Num(n))
            }
            Some(b'\\') => {
                let cmd = self.lx.peek_command().unwrap_or_default();
                match cmd.as_str() {
                    "markup" => {
                        self.lx.read_command();
                        Ok(Val::Markup(self.parse_markup()?))
                    }
                    "markuplist" => {
                        self.lx.read_command();
                        self.parse_mk()?;
                        Ok(Val::Other)
                    }
                    "with" => {
                        self.lx.read_command();
                        self.parse_assignments()?;
                        Ok(Val::Other)
                    }
                    "paper" | "layout" | "midi" | "header" => {
                        self.lx.read_command();
                        self.lx.skip_ws();
                        self.lx.skip_braces()?;
                        Ok(Val::Other)
                    }
                    _ => {
                        if let Some(v) = self.vars.get(&cmd) {
                            if !matches!(v, Val::Music(_)) {
                                let v = v.clone();
                                self.lx.read_command();
                                return Ok(v);
                            }
                        }
                        if DYNAMICS.contains(&cmd.as_str()) || art_of(&cmd).is_some() {
                            // a post-event stored in a variable: not supported, skipped
                            self.lx.read_command();
                            return Ok(Val::Other);
                        }
                        Ok(Val::Music(Rc::new(self.parse_music()?)))
                    }
                }
            }
            Some(b'{') | Some(b'<') => Ok(Val::Music(Rc::new(self.parse_music()?))),
            Some(_) => Ok(Val::Str(self.lx.read_token())),
        }
    }

    /// `Context.Grob.property` paths, including old-style `#'property` parts
    fn read_path(&mut self) -> R<String> {
        self.lx.skip_ws();
        let mut out = String::new();
        if self.lx.raw() == Some(b'"') {
            out = self.lx.read_string()?;
        } else if self.lx.raw() != Some(b'#') {
            out = self.lx.read_ident(true);
        }
        loop {
            if self.lx.raw() == Some(b'.') {
                self.lx.pos += 1;
                out.push('.');
                if self.lx.raw().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    out.push_str(&self.lx.read_uint().unwrap_or(0).to_string());
                } else {
                    out.push_str(&self.lx.read_ident(true));
                }
                continue;
            }
            let save = self.lx.pos;
            self.lx.skip_ws();
            if self.lx.raw() == Some(b'#') {
                self.lx.pos += 1;
                let s = self.lx.read_scheme()?;
                match s.unquoted() {
                    Sexp::Sym(x) => {
                        if !out.is_empty() {
                            out.push('.');
                        }
                        out.push_str(x);
                    }
                    Sexp::List(items) => {
                        for i in items {
                            if let Some(x) = i.as_str() {
                                out.push('.');
                                out.push_str(x);
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
            self.lx.pos = save;
            break;
        }
        Ok(out)
    }

    /// one scheme-ish argument: #expr, string, number, markup
    fn skip_arg(&mut self) -> R<()> {
        match self.lx.peek() {
            Some(b'#') | Some(b'$') => {
                self.lx.pos += 1;
                self.lx.read_scheme()?;
            }
            Some(b'"') => {
                self.lx.read_string()?;
            }
            Some(b'\\') => {
                if self.lx.peek_command().as_deref() == Some("markup") {
                    self.lx.read_command();
                    self.parse_mk()?;
                } else if self.lx.peek_command().as_deref() == Some("default") {
                    self.lx.read_command();
                }
            }
            Some(c) if c.is_ascii_digit() || c == b'-' => {
                self.lx.read_token();
            }
            _ => {}
        }
        Ok(())
    }

    fn skip_scheme_args(&mut self) -> R<()> {
        loop {
            let save = self.lx.pos;
            self.lx.skip_ws();
            if self.lx.raw() == Some(b'#') {
                self.lx.pos += 1;
                self.lx.read_scheme()?;
            } else {
                self.lx.pos = save;
                return Ok(());
            }
        }
    }

    // ------------------------------------------------------------ markup

    pub fn parse_markup(&mut self) -> R<MarkupText> {
        let mk = self.parse_mk()?;
        let mut out = MarkupText::default();
        let text = flatten(&mk, &mut out);
        out.text = text.trim().to_string();
        Ok(out)
    }

    fn parse_mk(&mut self) -> R<Mk> {
        self.depth += 1;
        if self.depth > 250 {
            return self.err("markup nested too deeply");
        }
        let r = self.parse_mk_inner();
        self.depth -= 1;
        r
    }

    fn parse_mk_inner(&mut self) -> R<Mk> {
        match self.lx.peek() {
            None => Ok(Mk::List(vec![])),
            Some(b'{') => {
                self.lx.pos += 1;
                let mut items = vec![];
                loop {
                    match self.lx.peek() {
                        None => return self.err("unclosed markup"),
                        Some(b'}') => {
                            self.lx.pos += 1;
                            break;
                        }
                        _ => items.push(self.parse_mk()?),
                    }
                }
                Ok(Mk::List(items))
            }
            Some(b'"') => Ok(Mk::Word(self.lx.read_string()?)),
            Some(b'#') | Some(b'$') => {
                self.lx.pos += 1;
                let s = self.lx.read_scheme()?;
                Ok(match s {
                    Sexp::Str(t) => Mk::Word(t),
                    _ => Mk::List(vec![]),
                })
            }
            Some(b'\\') => {
                let cmd = self.lx.read_command();
                match cmd.as_str() {
                    "note" => {
                        self.lx.skip_ws();
                        let mut t = NoteType { log: 2, dots: 0 };
                        if self.lx.raw() == Some(b'{') {
                            self.lx.pos += 1;
                            self.lx.skip_ws();
                            let keep = self.last_dur.clone();
                            if let Some(d) = self.parse_dur() {
                                t = d.ntype();
                            }
                            self.last_dur = keep;
                            self.expect("}")?;
                        } else if self.lx.raw() == Some(b'#') {
                            self.lx.pos += 1;
                            if let Some(s) = self.lx.read_scheme()?.as_str() {
                                let dots = s.chars().filter(|c| *c == '.').count() as u8;
                                let n: u32 = s.trim_end_matches('.').parse().unwrap_or(4);
                                t = NoteType { log: n.max(1).trailing_zeros() as i8, dots };
                            }
                        }
                        self.skip_scheme_args()?;
                        Ok(Mk::Note(t))
                    }
                    "note-by-number" => {
                        let mut nums = vec![];
                        for _ in 0..3 {
                            self.lx.skip_ws();
                            if self.lx.raw() == Some(b'#') {
                                self.lx.pos += 1;
                                nums.push(self.lx.read_scheme()?.as_f64().unwrap_or(0.0));
                            }
                        }
                        Ok(Mk::Note(NoteType { log: *nums.first().unwrap_or(&2.0) as i8, dots: *nums.get(1).unwrap_or(&0.0) as u8 }))
                    }
                    "musicglyph" => {
                        self.lx.skip_ws();
                        let name = self.read_string_arg()?;
                        Ok(Mk::Glyph(name))
                    }
                    "segno" | "coda" | "varcoda" => Ok(Mk::Glyph(cmd)),
                    "flat" => Ok(Mk::Word("\u{266d}".into())),
                    "sharp" => Ok(Mk::Word("\u{266f}".into())),
                    "natural" => Ok(Mk::Word("\u{266e}".into())),
                    "hspace" | "vspace" | "null" | "draw-line" | "strut" | "draw-hline" | "path" | "draw-circle" | "triangle" | "beam" | "filled-box"
                    | "arrow-head" | "slashed-digit" | "backslashed-digit" | "fromproperty" | "draw-dashed-line" | "draw-dotted-line" => {
                        self.skip_scheme_args()?;
                        Ok(Mk::List(vec![]))
                    }
                    "char" | "lookup" | "postscript" | "fret-diagram" | "fret-diagram-terse" | "epsfile" | "verbatim-file" | "harp-pedal" => {
                        self.skip_scheme_args()?;
                        if self.lx.peek() == Some(b'"') {
                            self.lx.read_string()?;
                        }
                        self.skip_scheme_args()?;
                        Ok(Mk::List(vec![]))
                    }
                    "score" | "score-lines" => {
                        self.lx.skip_ws();
                        if self.lx.raw() == Some(b'{') {
                            self.lx.skip_braces()?;
                        }
                        Ok(Mk::List(vec![]))
                    }
                    "combine" | "fraction" | "put-adjacent" | "fill-with-pattern" => {
                        self.skip_scheme_args()?;
                        let a = self.parse_mk()?;
                        self.skip_scheme_args()?;
                        let b = self.parse_mk()?;
                        Ok(Mk::List(vec![a, b]))
                    }
                    _ => {
                        if let Some(v) = self.vars.get(&cmd) {
                            return Ok(match v {
                                Val::Markup(m) => Mk::Var(m.clone()),
                                Val::Str(s) => Mk::Word(s.clone()),
                                _ => Mk::List(vec![]),
                            });
                        }
                        self.skip_scheme_args()?;
                        match self.lx.peek() {
                            None | Some(b'}') => Ok(Mk::Cmd(cmd, None)),
                            _ => {
                                let arg = self.parse_mk()?;
                                Ok(Mk::Cmd(cmd, Some(Box::new(arg))))
                            }
                        }
                    }
                }
            }
            Some(_) => Ok(Mk::Word(self.lx.read_token())),
        }
    }

    // ------------------------------------------------------------ music

    /// one music expression, with any \addlyrics that follow it
    pub fn parse_music(&mut self) -> R<Music> {
        self.depth += 1;
        if self.depth > 250 {
            return self.err("music nested too deeply");
        }
        let m = self.parse_music_inner();
        self.depth -= 1;
        let m = m?;
        let mut lyr = vec![];
        while self.lx.peek_command().as_deref() == Some("addlyrics") {
            self.lx.read_command();
            lyr.push(self.parse_lyric_arg()?);
        }
        if lyr.is_empty() {
            Ok(m)
        } else {
            Ok(Music::AddLyrics(Box::new(m), lyr))
        }
    }

    /// the argument of \lyricsto / \addlyrics: braces mean lyric mode
    fn parse_lyric_arg(&mut self) -> R<Music> {
        if self.lx.peek() == Some(b'{') {
            self.lx.pos += 1;
            Ok(Music::Lyrics(self.parse_lyric_seq()?))
        } else {
            self.parse_music()
        }
    }

    fn parse_music_inner(&mut self) -> R<Music> {
        let c = match self.lx.peek() {
            None => return self.err("music expected, found end of file"),
            Some(c) => c,
        };
        match c {
            b'{' => {
                self.lx.pos += 1;
                let line = self.lx.line();
                let mut items = vec![];
                self.lx.hint = None;
                loop {
                    let next = self.lx.peek();
                    if let Some(h) = self.lx.hint.take() {
                        items.push(Music::BarNumber(h));
                    }
                    match next {
                        None => return Err(format!("line {}: unclosed `{{`", line)),
                        Some(b'}') => {
                            self.lx.pos += 1;
                            break;
                        }
                        _ => {
                            let m = self.parse_music()?;
                            if m != Music::Nothing {
                                items.push(m);
                            }
                        }
                    }
                }
                Ok(Music::Seq(items))
            }
            b'<' if self.lx.at(1) == Some(b'<') => {
                self.lx.pos += 2;
                let line = self.lx.line();
                let mut branches: Vec<Vec<Music>> = vec![vec![]];
                let mut voices = false;
                loop {
                    match self.lx.peek() {
                        None => return Err(format!("line {}: unclosed `<<`", line)),
                        Some(b'>') if self.lx.at(1) == Some(b'>') => {
                            self.lx.pos += 2;
                            break;
                        }
                        Some(b'\\') if self.lx.at(1) == Some(b'\\') => {
                            self.lx.pos += 2;
                            voices = true;
                            branches.push(vec![]);
                        }
                        _ => {
                            let m = self.parse_music()?;
                            if m != Music::Nothing {
                                branches.last_mut().unwrap().push(m);
                            }
                        }
                    }
                }
                if voices {
                    Ok(Music::Voices(branches.into_iter().map(|mut b| if b.len() == 1 { b.pop().unwrap() } else { Music::Seq(b) }).collect()))
                } else {
                    Ok(Music::Sim(branches.pop().unwrap()))
                }
            }
            b'<' => self.parse_chord(),
            b'\\' => self.parse_command(),
            b'|' => {
                self.lx.pos += 1;
                Ok(Music::Nothing)
            }
            b'#' | b'$' => {
                self.lx.pos += 1;
                self.lx.read_scheme()?;
                Ok(Music::Nothing)
            }
            b'"' => {
                self.lx.read_string()?;
                Ok(Music::Nothing)
            }
            c if c.is_ascii_digit() => {
                let line = self.lx.line() as u32;
                match self.parse_dur() {
                    Some(dur) => {
                        let posts = self.parse_posts()?;
                        Ok(Music::Ev(NoteEv { kind: EvKind::Again, dur, posts, line }))
                    }
                    None => {
                        let t = self.lx.read_token();
                        self.note(Level::Warning, format!("stray `{}` in music skipped", t));
                        Ok(Music::Nothing)
                    }
                }
            }
            c if is_alpha(c) => self.parse_word_event(),
            b'}' => self.err("unexpected `}`"),
            b'>' => self.err("unexpected `>`"),
            c => {
                // post-events after a closing brace (`{ ... } _\!`): they happen at this moment
                let line = self.lx.line() as u32;
                let save = self.lx.pos;
                let posts = self.parse_posts()?;
                if !posts.is_empty() && self.lx.pos > save {
                    return Ok(Music::Ev(NoteEv { kind: EvKind::Skip, dur: Dur { log: 2, dots: 0, mult: Frac::ZERO }, posts, line }));
                }
                self.lx.pos = save;
                self.lx.skip_ws();
                self.lx.pos += 1;
                self.note(Level::Warning, format!("stray `{}` in music skipped", c as char));
                Ok(Music::Nothing)
            }
        }
    }

    fn lookup_pitch(&self, w: &str) -> Option<(u8, i8)> {
        let step_of = |c: u8| -> Option<u8> {
            Some(match c {
                b'c' => 0,
                b'd' => 1,
                b'e' => 2,
                b'f' => 3,
                b'g' => 4,
                b'a' => 5,
                b'b' => 6,
                _ => return None,
            })
        };
        let b = w.as_bytes();
        if b.is_empty() {
            return None;
        }
        match self.lang {
            Lang::Nederlands => {
                let step = step_of(b[0])?;
                let alter = match &w[1..] {
                    "" => 0,
                    "is" => 1,
                    "isis" => 2,
                    "es" => -1,
                    "eses" => -2,
                    "s" if step == 2 || step == 5 => -1,
                    "ses" if step == 2 || step == 5 => -2,
                    _ => return None,
                };
                Some((step, alter))
            }
            Lang::English => {
                let step = step_of(b[0])?;
                let alter = match &w[1..] {
                    "" | "n" | "natural" => 0,
                    "s" | "sharp" => 1,
                    "ss" | "x" | "sharpsharp" => 2,
                    "f" | "flat" => -1,
                    "ff" | "flatflat" => -2,
                    _ => return None,
                };
                Some((step, alter))
            }
            Lang::Deutsch => {
                if w == "b" {
                    return Some((6, -1));
                }
                if w == "heses" || w == "bes" || w == "bess" {
                    return Some((6, -2));
                }
                let step = if b[0] == b'h' { 6 } else if b[0] == b'b' { return None } else { step_of(b[0])? };
                let alter = match &w[1..] {
                    "" => 0,
                    "is" | "iss" => 1,
                    "isis" | "ississ" => 2,
                    "es" | "ess" => -1,
                    "eses" | "essess" => -2,
                    "s" if step == 2 || step == 5 => -1,
                    "ses" | "sess" if step == 2 || step == 5 => -2,
                    _ => return None,
                };
                Some((step, alter))
            }
            Lang::Romance => {
                const BASES: &[(&str, u8)] = &[("sol", 4), ("do", 0), ("re", 1), ("ré", 1), ("mi", 2), ("fa", 3), ("la", 5), ("si", 6)];
                for (name, step) in BASES {
                    if let Some(rest) = w.strip_prefix(name) {
                        let alter = match rest {
                            "" => 0,
                            "d" | "s" => 1,
                            "dd" | "ss" | "x" => 2,
                            "b" => -1,
                            "bb" => -2,
                            _ => continue,
                        };
                        return Some((*step, alter));
                    }
                }
                None
            }
        }
    }

    /// pitch at the cursor (a word), with octave marks and ! ?
    fn parse_pitch(&mut self) -> R<Option<Pitch>> {
        self.lx.skip_ws();
        let save = self.lx.pos;
        let w = self.lx.read_word();
        let (step, mut alter) = match self.lookup_pitch(&w) {
            Some(x) => x,
            None => {
                self.lx.pos = save;
                return Ok(None);
            }
        };
        if self.lang == Lang::English && self.lx.raw() == Some(b'-') {
            for (suffix, a) in [("-sharpsharp", 2), ("-flatflat", -2), ("-sharp", 1), ("-flat", -1), ("-natural", 0)] {
                if self.lx.looking_at(suffix) {
                    self.lx.pos += suffix.len();
                    alter = a;
                    break;
                }
            }
        }
        let mut marks = 0;
        loop {
            match self.lx.raw() {
                Some(b'\'') => marks += 1,
                Some(b',') => marks -= 1,
                _ => break,
            }
            self.lx.pos += 1;
        }
        let (mut force_acc, mut caut_acc) = (false, false);
        loop {
            match self.lx.raw() {
                Some(b'!') => force_acc = true,
                Some(b'?') => caut_acc = true,
                _ => break,
            }
            self.lx.pos += 1;
        }
        // octave check
        if self.lx.raw() == Some(b'=') && matches!(self.lx.at(1), Some(b'\'') | Some(b',')) {
            self.lx.pos += 1;
            while matches!(self.lx.raw(), Some(b'\'') | Some(b',')) {
                self.lx.pos += 1;
            }
        }
        Ok(Some(Pitch { step, alter, marks, force_acc, caut_acc }))
    }

    /// duration at the cursor, if there is one; becomes the default for what follows
    fn parse_dur(&mut self) -> Option<Dur> {
        let save = self.lx.pos;
        let log: i8 = if self.lx.raw().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            match self.lx.read_uint() {
                Some(n) if n.is_power_of_two() && n <= 1024 => n.trailing_zeros() as i8,
                _ => {
                    self.lx.pos = save;
                    return None;
                }
            }
        } else if self.lx.looking_at("\\breve") {
            self.lx.pos += 6;
            -1
        } else if self.lx.looking_at("\\longa") {
            self.lx.pos += 6;
            -2
        } else if self.lx.looking_at("\\maxima") {
            self.lx.pos += 7;
            -3
        } else {
            return None;
        };
        let mut dots = 0u8;
        while self.lx.raw() == Some(b'.') {
            dots += 1;
            self.lx.pos += 1;
        }
        let mut mult = Frac::int(1);
        while self.lx.raw() == Some(b'*') {
            self.lx.pos += 1;
            let n = self.lx.read_uint().unwrap_or(1) as i64;
            let mut d = 1i64;
            if self.lx.raw() == Some(b'/') {
                self.lx.pos += 1;
                d = self.lx.read_uint().unwrap_or(1).max(1) as i64;
            }
            mult = mult * Frac::new(n, d);
        }
        let d = Dur { log, dots, mult };
        self.last_dur = d.clone();
        Some(d)
    }

    fn dur_or_last(&mut self) -> Dur {
        self.parse_dur().unwrap_or_else(|| self.last_dur.clone())
    }

    fn parse_word_event(&mut self) -> R<Music> {
        let line = self.lx.line() as u32;
        self.pending_once = false;
        let save = self.lx.pos;
        let w = self.lx.read_word();
        let kind = match w.as_str() {
            "r" => Some(EvKind::Rest),
            "s" => Some(EvKind::Skip),
            "R" => Some(EvKind::MeasureRest),
            "q" => Some(EvKind::ChordRepeat),
            _ => None,
        };
        if let Some(kind) = kind {
            let dur = self.dur_or_last();
            let posts = self.parse_posts()?;
            return Ok(Music::Ev(NoteEv { kind, dur, posts, line }));
        }
        self.lx.pos = save;
        match self.parse_pitch()? {
            Some(p) => {
                let dur = self.dur_or_last();
                let mut kind = EvKind::Notes(vec![ChordNote { pitch: p, tie: false }]);
                if self.lx.peek_command().as_deref() == Some("rest") {
                    self.lx.read_command();
                    kind = EvKind::Rest;
                }
                let posts = self.parse_posts()?;
                Ok(Music::Ev(NoteEv { kind, dur, posts, line }))
            }
            None => {
                let t = self.lx.read_token();
                self.note(Level::Dropped, format!("`{}` is not a note name here; skipped", t));
                Ok(Music::Nothing)
            }
        }
    }

    fn parse_chord(&mut self) -> R<Music> {
        let line = self.lx.line() as u32;
        self.lx.pos += 1;
        let mut notes: Vec<ChordNote> = vec![];
        let mut posts: Vec<Post> = vec![];
        loop {
            match self.lx.peek() {
                None => return Err(format!("line {}: unclosed chord `<`", line)),
                Some(b'>') => {
                    self.lx.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    let cmd = self.lx.read_command();
                    if cmd == "tweak" {
                        self.read_path()?;
                        self.skip_arg()?;
                    } else {
                        self.skip_scheme_args()?;
                    }
                }
                Some(c) if is_alpha(c) => match self.parse_pitch()? {
                    Some(p) => {
                        let mut tie = false;
                        for po in self.parse_posts()? {
                            match po {
                                Post::Tie => tie = true,
                                other => posts.push(other),
                            }
                        }
                        notes.push(ChordNote { pitch: p, tie });
                    }
                    None => {
                        let t = self.lx.read_word();
                        self.note(Level::Dropped, format!("`{}` in a chord is not a note name; skipped", t));
                    }
                },
                Some(c) => {
                    self.lx.pos += 1;
                    self.note(Level::Warning, format!("stray `{}` in a chord skipped", c as char));
                }
            }
        }
        let dur = self.dur_or_last();
        posts.extend(self.parse_posts()?);
        let kind = if notes.is_empty() { EvKind::Skip } else { EvKind::Notes(notes) };
        let dur = if kind == EvKind::Skip { Dur { log: 2, dots: 0, mult: Frac::ZERO } } else { dur };
        Ok(Music::Ev(NoteEv { kind, dur, posts, line }))
    }

    /// everything attached after a note: ties, slurs, beams, articulations, dynamics, text
    fn parse_posts(&mut self) -> R<Vec<Post>> {
        let mut out = vec![];
        let mut slur_id: Option<u8> = None;
        loop {
            let save = self.lx.pos;
            self.lx.skip_ws();
            let c = match self.lx.raw() {
                Some(c) => c,
                None => break,
            };
            let mut placement = None;
            match c {
                b'~' => {
                    self.lx.pos += 1;
                    out.push(Post::Tie);
                    continue;
                }
                b'(' => {
                    self.lx.pos += 1;
                    out.push(Post::SlurStart(slur_id.take().unwrap_or(1)));
                    continue;
                }
                b')' => {
                    self.lx.pos += 1;
                    out.push(Post::SlurStop(slur_id.take().unwrap_or(1)));
                    continue;
                }
                b'[' => {
                    self.lx.pos += 1;
                    out.push(Post::BeamStart);
                    continue;
                }
                b']' => {
                    self.lx.pos += 1;
                    out.push(Post::BeamStop);
                    continue;
                }
                b':' => {
                    self.lx.pos += 1;
                    let n = self.lx.read_uint().unwrap_or(8);
                    // kept as the subdivision (8 -> 3, 16 -> 4 ...); the interpreter turns it
                    // into strokes once it knows the note value
                    let sub = n.clamp(8, 256).trailing_zeros() as u8;
                    out.push(Post::Art(Art::Tremolo(sub.max(3)), None));
                    continue;
                }
                b'^' | b'_' | b'-' => {
                    placement = match c {
                        b'^' => Some(Placement::Above),
                        b'_' => Some(Placement::Below),
                        _ => None,
                    };
                    let next = self.lx.at(1);
                    let short = match next {
                        Some(b'.') => Some(Art::Staccato),
                        Some(b'>') => Some(Art::Accent),
                        Some(b'^') => Some(Art::StrongAccent),
                        Some(b'-') => Some(Art::Tenuto),
                        Some(b'!') => Some(Art::Staccatissimo),
                        Some(b'_') => Some(Art::DetachedLegato),
                        Some(b'+') => Some(Art::Stopped),
                        _ => None,
                    };
                    if let Some(a) = short {
                        self.lx.pos += 2;
                        out.push(Post::Art(a, placement));
                        continue;
                    }
                    match next {
                        Some(d) if d.is_ascii_digit() => {
                            self.lx.pos += 1;
                            let n = self.lx.read_uint().unwrap_or(0);
                            out.push(Post::Art(Art::Fingering(n.to_string()), placement));
                            continue;
                        }
                        Some(b'"') => {
                            self.lx.pos += 1;
                            let text = self.lx.read_string()?;
                            out.push(Post::Text { text, italic: false, bold: false, placement });
                            continue;
                        }
                        Some(b'(') | Some(b'[') | Some(b'~') | Some(b'\\') => {
                            self.lx.pos += 1;
                            if next != Some(b'\\') {
                                continue; // direction prefix on a slur/beam/tie: handled next round
                            }
                        }
                        _ => {
                            self.lx.pos = save;
                            break;
                        }
                    }
                }
                b'\\' => {}
                _ => {
                    self.lx.pos = save;
                    break;
                }
            }
            // a command
            let cmd_pos = self.lx.pos;
            let cmd = self.lx.read_command();
            let name = cmd.as_str();
            if DYNAMICS.contains(&name) {
                out.push(Post::Dynamic(cmd, placement));
            } else if let Some(a) = art_of(name) {
                let a = match a {
                    Art::Fermata { .. } => Art::Fermata { inverted: placement == Some(Placement::Below) },
                    a => a,
                };
                out.push(Post::Art(a, placement));
            } else if POST_IGNORED.contains(&name) {
                self.note(Level::Warning, format!("\\{} is not carried over", name));
            } else {
                match name {
                    "<" => out.push(Post::Wedge(Wedge::Cresc, placement)),
                    ">" => out.push(Post::Wedge(Wedge::Dim, placement)),
                    "!" => out.push(Post::Wedge(Wedge::Stop, placement)),
                    "cresc" | "crescendo" => out.push(Post::TextSpan("cresc.".into())),
                    "dim" | "diminuendo" => out.push(Post::TextSpan("dim.".into())),
                    "decresc" | "decrescendo" => out.push(Post::TextSpan("decresc.".into())),
                    "(" => out.push(Post::SlurStart(2)),
                    ")" => out.push(Post::SlurStop(2)),
                    "[" => out.push(Post::BeamStart),
                    "]" => out.push(Post::BeamStop),
                    "laissezVibrer" => out.push(Post::TieOut),
                    "repeatTie" => out.push(Post::TieIn),
                    "sustainOn" => out.push(Post::Pedal(Pedal::Start)),
                    "sustainOff" => out.push(Post::Pedal(Pedal::Stop)),
                    "=" => {
                        // \=1( : slur with an id
                        self.lx.skip_ws();
                        if self.lx.raw() == Some(b'"') {
                            self.lx.read_string()?;
                            slur_id = Some(3);
                        } else if self.lx.raw() == Some(b'#') {
                            self.lx.pos += 1;
                            slur_id = Some(self.lx.read_scheme()?.as_f64().unwrap_or(1.0) as u8);
                        } else {
                            slur_id = Some(self.lx.read_uint().unwrap_or(1).min(200) as u8);
                        }
                    }
                    "markup" => {
                        let m = self.parse_markup()?;
                        out.push(Post::Text { text: m.text, italic: m.italic, bold: m.bold, placement });
                    }
                    "tweak" => {
                        self.read_path()?;
                        self.skip_arg()?;
                    }
                    "bendAfter" => {
                        // a fall (negative) or a rise off the note
                        self.lx.skip_ws();
                        if self.lx.raw() == Some(b'#') {
                            self.lx.pos += 1;
                            let v = self.lx.read_scheme()?.as_f64().unwrap_or(-4.0);
                            out.push(Post::Art(if v < 0.0 { Art::Falloff } else { Art::Doit }, placement));
                        }
                    }
                    "shape" | "footnote" | "rightHandFinger" => {
                        self.skip_scheme_args()?;
                    }
                    "finger" => {
                        self.lx.skip_ws();
                        let t = if self.lx.peek_command().as_deref() == Some("markup") {
                            self.lx.read_command();
                            self.parse_markup()?.text
                        } else {
                            self.read_string_arg()?
                        };
                        out.push(Post::Art(Art::Fingering(t), placement));
                    }
                    _ => {
                        // a variable holding text or a dynamic?
                        match self.vars.get(name) {
                            Some(Val::Markup(m)) if placement.is_some() || c != b'\\' => {
                                let m = m.clone();
                                out.push(Post::Text { text: m.text, italic: m.italic, bold: m.bold, placement });
                            }
                            Some(Val::Str(s)) if placement.is_some() || c != b'\\' => {
                                let text = s.clone();
                                out.push(Post::Text { text, italic: false, bold: false, placement });
                            }
                            _ => {
                                // not a post-event: it starts the next music item
                                self.lx.pos = if c == b'\\' { save } else { cmd_pos };
                                if c != b'\\' {
                                    // a dangling direction prefix
                                    self.lx.pos = save;
                                    // step over the prefix so the caller makes progress
                                    self.lx.skip_ws();
                                    self.lx.pos += 1;
                                    continue;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn parse_command(&mut self) -> R<Music> {
        let line = self.lx.line() as u32;
        let start = self.lx.pos;
        let cmd = self.lx.read_command();
        let name = cmd.as_str();
        let once = self.pending_once;
        self.pending_once = false;
        match name {
            "relative" => {
                self.lx.skip_ws();
                let p = if self.lx.raw().map(is_alpha).unwrap_or(false) { self.parse_pitch()? } else { None };
                let m = self.parse_music()?;
                Ok(Music::Relative(p, Box::new(m)))
            }
            "fixed" => {
                let p = self.parse_pitch()?;
                let m = self.parse_music()?;
                match p {
                    Some(p) => Ok(Music::Fixed(p, Box::new(m))),
                    None => Ok(Music::Absolute(Box::new(m))),
                }
            }
            "absolute" => Ok(Music::Absolute(Box::new(self.parse_music()?))),
            "transpose" => {
                let a = self.parse_pitch()?;
                let b = self.parse_pitch()?;
                let m = self.parse_music()?;
                match (a, b) {
                    (Some(a), Some(b)) => Ok(Music::Transpose(a, b, Box::new(m))),
                    _ => Ok(m),
                }
            }
            "new" | "context" => {
                self.lx.skip_ws();
                let ctype = self.lx.read_ident(true);
                let mut cname = None;
                if self.lx.eat("=") {
                    self.lx.skip_ws();
                    cname = Some(if self.lx.raw() == Some(b'"') {
                        self.lx.read_string()?
                    } else if self.lx.raw() == Some(b'#') {
                        self.lx.pos += 1;
                        self.lx.read_scheme()?.as_str().unwrap_or("").to_string()
                    } else {
                        self.lx.read_ident(true)
                    });
                }
                let mut with = vec![];
                while self.lx.peek_command().as_deref() == Some("with") {
                    self.lx.read_command();
                    with.extend(self.parse_assignments()?);
                }
                let body = if ctype == "Lyrics" && self.lx.peek() == Some(b'{') && !self.brace_starts_with_command() {
                    // \new Lyrics { words }: read the braces as lyrics
                    self.lx.pos += 1;
                    Music::Lyrics(self.parse_lyric_seq()?)
                } else {
                    self.parse_music()?
                };
                Ok(Music::Context { new: name == "new", ctype, name: cname, with, body: Box::new(body) })
            }
            "tuplet" | "times" => {
                self.lx.skip_ws();
                let a = self.lx.read_uint().unwrap_or(3);
                self.expect("/")?;
                let b = self.lx.read_uint().unwrap_or(2);
                let (num, den) = if name == "tuplet" { (a, b) } else { (b, a) };
                self.lx.skip_ws();
                let mut span = None;
                if self.lx.raw().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    let keep = self.last_dur.clone();
                    span = self.parse_dur().map(|d| d.len());
                    self.last_dur = keep;
                }
                let body = self.parse_music()?;
                Ok(Music::Tuplet { num: num.max(1), den: den.max(1), span, body: Box::new(body) })
            }
            "scaleDurations" => {
                self.lx.skip_ws();
                let f = self.read_fraction()?;
                let body = self.parse_music()?;
                Ok(Music::Scale(f, Box::new(body)))
            }
            "grace" => Ok(Music::Grace { slash: false, slur: false, body: Box::new(self.parse_music()?) }),
            "appoggiatura" => Ok(Music::Grace { slash: false, slur: true, body: Box::new(self.parse_music()?) }),
            "acciaccatura" => Ok(Music::Grace { slash: true, slur: true, body: Box::new(self.parse_music()?) }),
            "slashedGrace" => Ok(Music::Grace { slash: true, slur: false, body: Box::new(self.parse_music()?) }),
            "afterGrace" => {
                self.lx.skip_ws();
                if self.lx.raw().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    self.read_fraction()?;
                } else {
                    self.skip_scheme_args()?;
                }
                let main = self.parse_music()?;
                let grace = self.parse_music()?;
                Ok(Music::AfterGrace(Box::new(main), Box::new(grace)))
            }
            "repeat" => {
                self.lx.skip_ws();
                let kind = match self.lx.read_word().as_str() {
                    "volta" => RepKind::Volta,
                    "unfold" => RepKind::Unfold,
                    "percent" => RepKind::Percent,
                    "tremolo" => RepKind::Tremolo,
                    "segno" => RepKind::Segno,
                    other => return self.err(format!("unknown repeat type `{}`", other)),
                };
                self.lx.skip_ws();
                let count = if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    self.lx.read_scheme()?.as_f64().unwrap_or(2.0) as u32
                } else {
                    self.lx.read_uint().unwrap_or(2)
                };
                let mut body = self.parse_music()?;
                let mut alts = vec![];
                if self.lx.peek_command().as_deref() == Some("alternative") {
                    self.lx.read_command();
                    alts = self.parse_alternatives()?;
                } else if let Music::Seq(items) = &mut body {
                    if let Some(Music::Alternative(_)) = items.last() {
                        if let Some(Music::Alternative(a)) = items.pop() {
                            alts = a;
                        }
                    }
                }
                Ok(Music::Repeat { kind, count: count.max(1), body: Box::new(body), alts })
            }
            "alternative" => Ok(Music::Alternative(self.parse_alternatives()?)),
            "volta" => {
                self.lx.skip_ws();
                let mut nums = vec![];
                if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    if let Sexp::List(items) = self.lx.read_scheme()?.unquoted() {
                        nums = items.iter().filter_map(|x| x.as_f64()).map(|x| x as u32).collect();
                    }
                } else {
                    while let Some(n) = self.lx.read_uint() {
                        nums.push(n);
                        if self.lx.raw() == Some(b',') {
                            self.lx.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                let m = self.parse_music()?;
                Ok(Music::Volta(nums, Box::new(m)))
            }
            "time" => {
                self.lx.skip_ws();
                if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    self.lx.read_scheme()?;
                    self.lx.skip_ws();
                }
                let mut n = self.lx.read_uint().unwrap_or(4);
                if self.lx.raw() == Some(b',') {
                    while self.lx.raw() == Some(b',') {
                        self.lx.pos += 1;
                        self.lx.read_uint();
                    }
                    self.lx.skip_ws();
                    n = self.lx.read_uint().unwrap_or(4);
                }
                self.expect("/")?;
                let d = self.lx.read_uint().unwrap_or(4);
                Ok(Music::Time { beats: n.to_string(), beat_type: d.max(1) })
            }
            "compoundMeter" => {
                self.lx.skip_ws();
                let mut beats = "4".to_string();
                let mut beat_type = 4;
                if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    let s = self.lx.read_scheme()?;
                    // ((3 2 8)) or (3 2 8)
                    let mut l = s.unquoted();
                    if let Sexp::List(items) = l {
                        if items.len() == 1 {
                            if let Sexp::List(_) = &items[0] {
                                l = &items[0];
                            }
                        }
                    }
                    if let Sexp::List(items) = l {
                        let nums: Vec<i64> = items.iter().filter_map(|x| x.as_f64()).map(|x| x as i64).collect();
                        if nums.len() >= 2 {
                            beat_type = *nums.last().unwrap() as u32;
                            beats = nums[..nums.len() - 1].iter().map(|x| x.to_string()).collect::<Vec<_>>().join("+");
                        }
                    }
                }
                Ok(Music::Time { beats, beat_type: beat_type.max(1) })
            }
            "key" => {
                let p = self.parse_pitch()?;
                let mode = match self.lx.peek_command() {
                    Some(m) if ["major", "minor", "ionian", "dorian", "phrygian", "lydian", "mixolydian", "aeolian", "locrian"].contains(&m.as_str()) => {
                        self.lx.read_command();
                        m
                    }
                    _ => "major".to_string(),
                };
                let p = match p {
                    Some(p) => p,
                    None => return self.err("\\key needs a pitch"),
                };
                const LOF: [i32; 7] = [0, 2, 4, -1, 1, 3, 5];
                let off = match mode.as_str() {
                    "minor" | "aeolian" => -3,
                    "dorian" => -2,
                    "phrygian" => -4,
                    "lydian" => 1,
                    "mixolydian" => -1,
                    "locrian" => -5,
                    _ => 0,
                };
                let fifths = LOF[p.step as usize] + 7 * p.alter as i32 + off;
                Ok(Music::Key { fifths: fifths.clamp(-12, 12) as i8, minor: mode == "minor" || mode == "aeolian" })
            }
            "clef" => {
                let n = self.read_string_arg()?;
                match clef_of(&n) {
                    Some((sign, cl, octave)) => Ok(Music::Clef { sign, line: cl, octave }),
                    None => {
                        self.note(Level::Warning, format!("clef `{}` not known; kept as treble", n));
                        Ok(Music::Clef { sign: 'G', line: 2, octave: 0 })
                    }
                }
            }
            "partial" => {
                self.lx.skip_ws();
                let keep = self.last_dur.clone();
                let d = self.parse_dur();
                self.last_dur = keep;
                match d {
                    Some(d) => Ok(Music::Partial(d.len())),
                    None => self.err("\\partial needs a duration"),
                }
            }
            "tempo" => {
                self.lx.skip_ws();
                let mut text = None;
                let mut unit = None;
                if self.lx.raw() == Some(b'"') {
                    text = Some(self.lx.read_string()?);
                } else if self.lx.peek_command().as_deref() == Some("markup") {
                    self.lx.read_command();
                    let m = self.parse_markup()?;
                    match (m.note, m.text.split_once('=')) {
                        (Some(nt), Some((before, after))) => {
                            unit = Some((nt, after.trim().to_string()));
                            let b = before.trim();
                            if !b.is_empty() {
                                text = Some(b.to_string());
                            }
                        }
                        _ => text = Some(m.text),
                    }
                }
                let save = self.lx.pos;
                self.lx.skip_ws();
                let keep = self.last_dur.clone();
                if let Some(d) = self.parse_dur() {
                    self.lx.skip_ws();
                    if self.lx.raw() == Some(b'=') {
                        self.lx.pos += 1;
                        self.lx.skip_ws();
                        let a = self.lx.pos;
                        while self.lx.raw().map(|c| c.is_ascii_digit() || c == b'-' || c == b'.').unwrap_or(false) {
                            self.lx.pos += 1;
                        }
                        let s = String::from_utf8_lossy(&self.lx.src[a..self.lx.pos]).into_owned();
                        unit = Some((d.ntype(), s));
                    } else {
                        self.lx.pos = save;
                    }
                } else {
                    self.lx.pos = save;
                }
                self.last_dur = keep;
                Ok(Music::Tempo { text: text.filter(|t| !t.is_empty()), unit })
            }
            "bar" => Ok(Music::Bar(self.read_string_arg()?)),
            "mark" => {
                self.lx.skip_ws();
                match self.lx.peek_command().as_deref() {
                    Some("default") => {
                        self.lx.read_command();
                        Ok(Music::Mark(MarkKind::Default))
                    }
                    Some("markup") => {
                        self.lx.read_command();
                        Ok(Music::Mark(MarkKind::Text(self.parse_markup()?)))
                    }
                    Some(v) => {
                        // a variable holding markup
                        let v = v.to_string();
                        match self.vars.get(&v).cloned() {
                            Some(Val::Markup(m)) => {
                                self.lx.read_command();
                                Ok(Music::Mark(MarkKind::Text(m)))
                            }
                            Some(Val::Str(s)) => {
                                self.lx.read_command();
                                Ok(Music::Mark(MarkKind::Text(MarkupText { text: s, ..Default::default() })))
                            }
                            _ => Ok(Music::Mark(MarkKind::Default)),
                        }
                    }
                    None => match self.lx.raw() {
                        Some(b'"') => Ok(Music::Mark(MarkKind::Text(MarkupText { text: self.lx.read_string()?, ..Default::default() }))),
                        Some(b'#') => {
                            self.lx.pos += 1;
                            let s = self.lx.read_scheme()?;
                            Ok(match (s.as_f64(), s.as_str()) {
                                (Some(n), _) => Music::Mark(MarkKind::Number(n as u32)),
                                (_, Some(t)) => Music::Mark(MarkKind::Text(MarkupText { text: t.to_string(), ..Default::default() })),
                                _ => Music::Mark(MarkKind::Default),
                            })
                        }
                        Some(c) if c.is_ascii_digit() => Ok(Music::Mark(MarkKind::Number(self.lx.read_uint().unwrap_or(1)))),
                        _ => Ok(Music::Mark(MarkKind::Default)),
                    },
                }
            }
            "break" => Ok(Music::Break),
            "pageBreak" => Ok(Music::PageBreak),
            "set" => {
                let p = self.read_path()?;
                self.expect("=")?;
                let val = self.parse_value()?;
                let (ctx, prop) = match p.rsplit_once('.') {
                    Some((c, p)) => (Some(c.to_string()), p.to_string()),
                    None => (None, p),
                };
                Ok(Music::Set { ctx, prop, val, line })
            }
            "unset" => {
                let p = self.read_path()?;
                let (ctx, prop) = match p.rsplit_once('.') {
                    Some((c, p)) => (Some(c.to_string()), p.to_string()),
                    None => (None, p),
                };
                Ok(Music::Unset { ctx, prop })
            }
            "override" | "overrideProperty" => {
                let p = self.read_path()?;
                self.expect("=")?;
                self.parse_value()?;
                if p.contains("TimeSignature.style") {
                    return Ok(Music::Cmd("numericTimeSignature".into()));
                }
                if p.contains("BreathingSign.text") {
                    return Ok(Music::Cmd("caesuraNext".into()));
                }
                if p.contains("Glissando.style") {
                    return Ok(Music::Cmd("wavyGlissando".into()));
                }
                Ok(Music::Nothing)
            }
            "revert" => {
                self.read_path()?;
                Ok(Music::Nothing)
            }
            "tweak" => {
                self.read_path()?;
                self.skip_arg()?;
                Ok(Music::Nothing)
            }
            "once" => {
                self.pending_once = true;
                Ok(Music::Nothing)
            }
            "omit" | "hide" => {
                let path = self.read_path()?;
                Ok(Music::Omit { once, path })
            }
            "ottava" => {
                self.lx.skip_ws();
                let n = if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    self.lx.read_scheme()?.as_f64().unwrap_or(0.0)
                } else {
                    let neg = self.lx.raw() == Some(b'-');
                    if neg {
                        self.lx.pos += 1;
                    }
                    let n = self.lx.read_uint().unwrap_or(0) as f64;
                    if neg {
                        -n
                    } else {
                        n
                    }
                };
                Ok(Music::Ottava(n as i8))
            }
            "jump" | "textEndMark" => Ok(Music::EndText(self.read_text_arg()?)),
            "textMark" | "sectionLabel" => Ok(Music::StartText(self.read_text_arg()?)),
            "segnoMark" | "codaMark" => {
                self.skip_arg()?;
                Ok(if name == "segnoMark" { Music::Segno } else { Music::Coda })
            }
            "inStaffSegno" => Ok(Music::Segno),
            "lyricmode" | "lyrics" => {
                let m = if self.lx.peek() == Some(b'{') {
                    self.lx.pos += 1;
                    Music::Lyrics(self.parse_lyric_seq()?)
                } else {
                    self.parse_music()?
                };
                if name == "lyrics" {
                    Ok(Music::Context { new: true, ctype: "Lyrics".into(), name: None, with: vec![], body: Box::new(m) })
                } else {
                    Ok(m)
                }
            }
            "lyricsto" => {
                let target = self.read_string_arg()?;
                let body = self.parse_lyric_arg()?;
                Ok(Music::LyricsTo(target, Box::new(body)))
            }
            "addlyrics" => {
                // no music before it that we could see: read and report
                self.parse_lyric_arg()?;
                self.note(Level::Dropped, "\\addlyrics without preceding music".into());
                Ok(Music::Nothing)
            }
            "notemode" => self.parse_music(),
            "chordmode" | "chords" => {
                let m = if self.lx.peek() == Some(b'{') {
                    self.lx.pos += 1;
                    let mut items = vec![];
                    self.parse_chord_seq(&mut items)?;
                    Music::Chords(items)
                } else {
                    self.parse_music()?
                };
                if name == "chords" {
                    Ok(Music::Context { new: true, ctype: "ChordNames".into(), name: None, with: vec![], body: Box::new(m) })
                } else {
                    Ok(m)
                }
            }
            "drummode" | "drums" | "figuremode" | "figures" => {
                self.lx.skip_ws();
                if self.lx.raw() == Some(b'{') {
                    self.lx.skip_braces()?;
                }
                Ok(Music::Unsupported(format!("\\{}", name), line))
            }
            "markup" | "markuplist" => {
                self.parse_mk()?;
                Ok(Music::Nothing)
            }
            "partCombine" | "partcombine" | "partCombineUp" | "partCombineDown" => {
                self.skip_scheme_args()?;
                let a = self.parse_music()?;
                let b = self.parse_music()?;
                Ok(Music::Voices(vec![a, b]))
            }
            "skip" => {
                self.lx.skip_ws();
                let dur = self.dur_or_last();
                let posts = self.parse_posts()?;
                Ok(Music::Ev(NoteEv { kind: EvKind::Skip, dur, posts, line }))
            }
            "after" => {
                self.lx.skip_ws();
                let keep = self.last_dur.clone();
                let d = self.parse_dur().map(|d| d.len()).unwrap_or(Frac::ZERO);
                self.last_dur = keep;
                let posts = self.parse_posts()?;
                let m = self.parse_music()?;
                Ok(Music::After(d, posts, Box::new(m)))
            }
            "change" => {
                self.lx.skip_ws();
                self.lx.read_ident(true);
                self.expect("=")?;
                self.read_string_arg()?;
                self.note(Level::Warning, "\\change Staff: cross-staff notes stay on their own staff".into());
                Ok(Music::Nothing)
            }
            "voices" => {
                self.lx.skip_ws();
                self.lx.read_token();
                self.parse_music()
            }
            "tupletSpan" => {
                self.lx.skip_ws();
                if self.lx.peek_command().as_deref() == Some("default") {
                    self.lx.read_command();
                } else {
                    let keep = self.last_dur.clone();
                    self.parse_dur();
                    self.last_dur = keep;
                }
                Ok(Music::Nothing)
            }
            "version" => {
                self.read_string_arg()?;
                Ok(Music::Nothing)
            }
            "language" => {
                let l = self.read_string_arg()?;
                self.set_language(&l);
                Ok(Music::Nothing)
            }
            "include" => {
                let f = self.read_string_arg()?;
                self.note(Level::Warning, format!("\\include \"{}\" inside music skipped", f));
                Ok(Music::Nothing)
            }
            "header" | "layout" | "midi" | "paper" | "with" => {
                self.lx.skip_ws();
                if self.lx.raw() == Some(b'{') {
                    self.lx.skip_braces()?;
                }
                Ok(Music::Nothing)
            }
            "barNumberCheck" => {
                // states the number of the bar that starts here
                self.lx.skip_ws();
                let mut val = Val::Other;
                if self.lx.raw() == Some(b'#') {
                    self.lx.pos += 1;
                    val = Val::Scheme(self.lx.read_scheme()?);
                }
                Ok(Music::Set { ctx: None, prop: "currentBarNumber".into(), val, line })
            }
            "\\" => self.err("`\\\\` outside `<< >>`"),
            _ if SIMPLE.contains(&name) => Ok(Music::Cmd(cmd)),
            _ if SILENT.contains(&name) => Ok(Music::Nothing),
            _ => {
                if let Some((_, spec)) = WITH_ARGS.iter().find(|(n, _)| *n == name) {
                    let mut music = Music::Nothing;
                    for ch in spec.chars() {
                        match ch {
                            's' => self.skip_arg()?,
                            'p' => {
                                self.read_path()?;
                            }
                            'P' => {
                                self.parse_pitch()?;
                            }
                            'w' => {
                                self.lx.skip_ws();
                                self.lx.read_token();
                            }
                            'm' => music = self.parse_music()?,
                            _ => {}
                        }
                    }
                    return Ok(music);
                }
                if let Some(v) = self.vars.get(name) {
                    return Ok(match v {
                        Val::Music(m) => Music::Var(m.clone()),
                        _ => Music::Nothing,
                    });
                }
                if DYNAMICS.contains(&name) || art_of(name).is_some() || matches!(name, "<" | ">" | "!" | "(" | ")" | "cresc" | "dim" | "decresc") {
                    // a post-event with nothing to attach to: put it on an empty chord
                    self.lx.pos = start;
                    let posts = self.parse_posts()?;
                    return Ok(Music::Ev(NoteEv { kind: EvKind::Skip, dur: Dur { log: 2, dots: 0, mult: Frac::ZERO }, posts, line }));
                }
                let quiet = ["Up", "Down", "Neutral", "On", "Off", "Solid", "Dashed", "Dotted", "Default"].iter().any(|s| name.ends_with(s))
                    || name.starts_with("partCombine");
                self.skip_scheme_args()?;
                if !quiet {
                    self.note(Level::Warning, format!("\\{} is not understood; skipped", name));
                }
                Ok(Music::Nothing)
            }
        }
    }

    /// does the `{` at the cursor start with a backslash command (so: not plain lyrics)?
    fn brace_starts_with_command(&mut self) -> bool {
        let save = self.lx.pos;
        self.lx.pos += 1;
        let r = match self.lx.peek_command().as_deref() {
            Some("lyricmode") | Some("lyricsto") | Some("new") | Some("context") => true,
            Some(v) => matches!(self.vars.get(v), Some(Val::Music(_))),
            None => false,
        };
        self.lx.pos = save;
        r
    }

    fn read_text_arg(&mut self) -> R<MarkupText> {
        self.lx.skip_ws();
        if self.lx.peek_command().as_deref() == Some("markup") {
            self.lx.read_command();
            self.parse_markup()
        } else if self.lx.raw() == Some(b'"') {
            Ok(MarkupText { text: self.lx.read_string()?, ..Default::default() })
        } else if let Some(v) = self.lx.peek_command() {
            let r = match self.vars.get(&v) {
                Some(Val::Markup(m)) => m.clone(),
                Some(Val::Str(s)) => MarkupText { text: s.clone(), ..Default::default() },
                _ => MarkupText::default(),
            };
            self.lx.read_command();
            Ok(r)
        } else {
            Ok(MarkupText::default())
        }
    }

    fn read_fraction(&mut self) -> R<Frac> {
        self.lx.skip_ws();
        if self.lx.raw() == Some(b'#') {
            self.lx.pos += 1;
            return Ok(match self.lx.read_scheme()?.unquoted() {
                Sexp::Rat(n, d) => Frac::new(*n, *d),
                Sexp::Num(n) => Frac::new((*n * 960.0).round() as i64, 960),
                _ => Frac::int(1),
            });
        }
        let n = self.lx.read_uint().unwrap_or(1) as i64;
        let mut d = 1;
        if self.lx.raw() == Some(b'/') {
            self.lx.pos += 1;
            d = self.lx.read_uint().unwrap_or(1).max(1) as i64;
        }
        Ok(Frac::new(n, d))
    }

    fn parse_alternatives(&mut self) -> R<Vec<Music>> {
        self.expect("{")?;
        let mut alts = vec![];
        loop {
            match self.lx.peek() {
                None => return self.err("unclosed \\alternative"),
                Some(b'}') => {
                    self.lx.pos += 1;
                    return Ok(alts);
                }
                _ => {
                    let m = self.parse_music()?;
                    if m != Music::Nothing {
                        alts.push(m);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ chord mode

    /// after `{` in chord mode, up to the matching `}`
    fn parse_chord_seq(&mut self, out: &mut Vec<(Option<crate::ir::Harmony>, Frac)>) -> R<()> {
        let line = self.lx.line();
        loop {
            let c = match self.lx.peek() {
                None => return Err(format!("line {}: unclosed `{{` in chord mode", line)),
                Some(c) => c,
            };
            match c {
                b'}' => {
                    self.lx.pos += 1;
                    return Ok(());
                }
                b'{' => {
                    self.lx.pos += 1;
                    self.parse_chord_seq(out)?;
                }
                b'|' => self.lx.pos += 1,
                b'#' | b'$' => {
                    self.lx.pos += 1;
                    self.lx.read_scheme()?;
                }
                b'\\' => {
                    let cmd = self.lx.read_command();
                    match cmd.as_str() {
                        "set" | "override" => {
                            self.read_path()?;
                            self.expect("=")?;
                            self.parse_value()?;
                        }
                        "unset" | "revert" | "omit" | "hide" => {
                            self.read_path()?;
                        }
                        "repeat" => {
                            self.lx.skip_ws();
                            let kind = self.lx.read_word();
                            self.lx.skip_ws();
                            let n = self.lx.read_uint().unwrap_or(1);
                            self.expect("{")?;
                            let mut items = vec![];
                            self.parse_chord_seq(&mut items)?;
                            for _ in 0..(if kind == "volta" { 1 } else { n }) {
                                out.extend(items.iter().cloned());
                            }
                        }
                        "skip" => {
                            self.lx.skip_ws();
                            let d = self.dur_or_last().len();
                            out.push((None, d));
                        }
                        _ => match self.vars.get(&cmd) {
                            Some(Val::Music(m)) => {
                                if let Music::Chords(items) = &**m {
                                    out.extend(items.iter().cloned());
                                }
                            }
                            _ => self.skip_scheme_args()?,
                        },
                    }
                }
                c if is_alpha(c) => {
                    let save = self.lx.pos;
                    let w = self.lx.read_word();
                    if matches!(w.as_str(), "r" | "s" | "R") {
                        let d = self.dur_or_last().len();
                        let h = if w == "s" { None } else { Some(crate::ir::Harmony { pos: Frac::ZERO, root: (0, 0), kind: "none".into(), bass: None, degrees: vec![] }) };
                        out.push((h, d));
                        continue;
                    }
                    self.lx.pos = save;
                    let p = match self.parse_pitch()? {
                        Some(p) => p,
                        None => {
                            let t = self.lx.read_token();
                            self.note(Level::Dropped, format!("`{}` in chord mode is not a chord; skipped", t));
                            continue;
                        }
                    };
                    let d = self.dur_or_last().len();
                    let mut modifier = String::new();
                    if self.lx.raw() == Some(b':') {
                        self.lx.pos += 1;
                        let a = self.lx.pos;
                        while let Some(ch) = self.lx.raw() {
                            if ch.is_ascii_whitespace() || matches!(ch, b'/' | b'{' | b'}' | b'|' | b'\\') {
                                break;
                            }
                            self.lx.pos += 1;
                        }
                        modifier = String::from_utf8_lossy(&self.lx.src[a..self.lx.pos]).into_owned();
                    }
                    let mut bass = None;
                    if self.lx.raw() == Some(b'/') {
                        self.lx.pos += 1;
                        if self.lx.raw() == Some(b'+') {
                            self.lx.pos += 1;
                        }
                        bass = self.parse_pitch()?.map(|b| (b.step, b.alter));
                    }
                    let (kind, degrees) = chord_kind(&modifier);
                    out.push((Some(crate::ir::Harmony { pos: Frac::ZERO, root: (p.step, p.alter), kind, bass, degrees }), d));
                }
                _ => {
                    self.lx.pos += 1;
                }
            }
        }
    }

    // ------------------------------------------------------------ lyrics

    /// after `{` in lyric mode, up to the matching `}`
    fn parse_lyric_seq(&mut self) -> R<Vec<LyItem>> {
        let line = self.lx.line();
        let mut out = vec![];
        loop {
            let c = match self.lx.peek() {
                None => return Err(format!("line {}: unclosed `{{` in lyrics", line)),
                Some(c) => c,
            };
            match c {
                b'}' => {
                    self.lx.pos += 1;
                    return Ok(out);
                }
                b'{' => {
                    self.lx.pos += 1;
                    out.extend(self.parse_lyric_seq()?);
                }
                b'|' => self.lx.pos += 1,
                b'"' => {
                    let text = self.lx.read_string()?;
                    let dur = self.lyric_dur();
                    out.push(LyItem::Syl { text: lyric_text(&text), dur });
                }
                b'#' | b'$' => {
                    self.lx.pos += 1;
                    self.lx.read_scheme()?;
                }
                b'<' | b'>' if self.lx.at(1) == Some(c) => self.lx.pos += 2,
                b'\\' => {
                    let cmd = self.lx.read_command();
                    match cmd.as_str() {
                        "skip" => {
                            self.lx.skip_ws();
                            let keep = self.last_dur.clone();
                            let d = self.parse_dur().map(|d| d.len());
                            self.last_dur = keep;
                            out.push(LyItem::Skip(d));
                        }
                        "set" => {
                            let p = self.read_path()?;
                            self.expect("=")?;
                            let v = self.parse_value()?;
                            if p.ends_with("stanza") {
                                out.push(LyItem::Stanza(v.text().unwrap_or_default()));
                            } else if p.ends_with("includeGraceNotes") {
                                out.push(LyItem::IncludeGrace(matches!(v, Val::Scheme(Sexp::Bool(true)))));
                            } else if p.ends_with("ignoreMelismata") {
                                out.push(LyItem::IgnoreMelismata(matches!(v, Val::Scheme(Sexp::Bool(true)))));
                            }
                        }
                        "unset" | "revert" | "omit" | "hide" => {
                            self.read_path()?;
                        }
                        "override" => {
                            self.read_path()?;
                            self.expect("=")?;
                            self.parse_value()?;
                        }
                        "tweak" => {
                            self.read_path()?;
                            self.skip_arg()?;
                        }
                        "markup" => {
                            let m = self.parse_markup()?;
                            let dur = self.lyric_dur();
                            out.push(LyItem::Syl { text: m.text, dur });
                        }
                        "repeat" => {
                            self.lx.skip_ws();
                            self.lx.read_word();
                            self.lx.skip_ws();
                            let n = self.lx.read_uint().unwrap_or(1);
                            self.expect("{")?;
                            let items = self.parse_lyric_seq()?;
                            for _ in 0..n {
                                out.extend(items.iter().cloned());
                            }
                        }
                        "lyricmode" => {}
                        "once" | "break" | "noBreak" | "pageBreak" | "bar" | "temporary" | "undo" => {
                            if cmd == "bar" {
                                self.read_string_arg()?;
                            }
                        }
                        _ => match self.vars.get(&cmd) {
                            Some(Val::Music(m)) => {
                                let mut items = vec![];
                                collect_lyrics(m, &mut items);
                                out.extend(items);
                            }
                            Some(Val::Str(s)) => {
                                let text = s.clone();
                                out.push(LyItem::Syl { text, dur: None });
                            }
                            Some(Val::Markup(m)) => {
                                let text = m.text.clone();
                                out.push(LyItem::Syl { text, dur: None });
                            }
                            _ => {
                                self.skip_scheme_args()?;
                                self.note(Level::Warning, format!("\\{} in lyrics is not understood; skipped", cmd));
                            }
                        },
                    }
                }
                _ => {
                    // a word: up to whitespace; trailing digits are a duration
                    let a = self.lx.pos;
                    let first_digit = c.is_ascii_digit();
                    while let Some(ch) = self.lx.raw() {
                        if ch.is_ascii_whitespace() || ch == b'{' || ch == b'}' || ch == b'\\' || (!first_digit && ch.is_ascii_digit()) {
                            break;
                        }
                        self.lx.pos += 1;
                    }
                    let w = String::from_utf8_lossy(&self.lx.src[a..self.lx.pos]).into_owned();
                    match w.as_str() {
                        "--" => out.push(LyItem::Hyphen),
                        "__" => out.push(LyItem::Extender),
                        "_" => {
                            let d = self.lyric_dur();
                            out.push(LyItem::Skip(d));
                        }
                        _ => {
                            let dur = self.lyric_dur();
                            out.push(LyItem::Syl { text: lyric_text(&w), dur });
                        }
                    }
                }
            }
        }
    }

    fn lyric_dur(&mut self) -> Option<Frac> {
        if self.lx.raw().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            let keep = self.last_dur.clone();
            let d = self.parse_dur().map(|d| d.len());
            self.last_dur = keep;
            d
        } else {
            None
        }
    }
}

/// A chord-mode modifier ("m7.5-", "maj7.11+", "sus4^3") as a MusicXML chord kind plus
/// added / removed degrees.
fn chord_kind(modifier: &str) -> (String, Vec<(u8, i8, char)>) {
    const KINDS: &[(&str, &str)] = &[
        ("maj13.11", "major-13th"),
        ("m13.11", "minor-13th"),
        ("13.11", "dominant-13th"),
        ("m7.5-", "half-diminished"),
        ("1.5", "power"),
        ("maj11", "major-11th"),
        ("maj13", "major-13th"),
        ("maj9", "major-ninth"),
        ("maj7", "major-seventh"),
        ("maj", "major-seventh"),
        ("m7+", "major-minor"),
        ("m11", "minor-11th"),
        ("m13", "minor-13th"),
        ("m9", "minor-ninth"),
        ("m7", "minor-seventh"),
        ("m6", "minor-sixth"),
        ("m5", "minor"),
        ("m", "minor"),
        ("dim7", "diminished-seventh"),
        ("dim", "diminished"),
        ("aug7", "augmented-seventh"),
        ("aug", "augmented"),
        ("sus2", "suspended-second"),
        ("sus4", "suspended-fourth"),
        ("sus", "suspended-fourth"),
        ("13", "dominant-13th"),
        ("11", "dominant-11th"),
        ("9", "dominant-ninth"),
        ("7", "dominant"),
        ("6", "major-sixth"),
        ("5", "major"),
    ];
    let (main, removed) = match modifier.split_once('^') {
        Some((a, b)) => (a, b),
        None => (modifier, ""),
    };
    let tokens: Vec<&str> = main.split('.').filter(|t| !t.is_empty()).collect();
    let mut kind = "major".to_string();
    let mut used = 0;
    for (base, k) in KINDS {
        let bt: Vec<&str> = base.split('.').collect();
        if tokens.len() >= bt.len() && tokens[..bt.len()] == bt[..] {
            kind = k.to_string();
            used = bt.len();
            break;
        }
    }
    let degree = |t: &str| -> Option<(u8, i8)> {
        let (num, alt) = if let Some(n) = t.strip_suffix('+') {
            (n, 1)
        } else if let Some(n) = t.strip_suffix('-') {
            (n, -1)
        } else {
            (t, 0)
        };
        num.parse::<u8>().ok().map(|n| (n, alt))
    };
    // a changed degree the chord already has is an alteration, anything else an addition
    let has = |n: u8| -> bool {
        let k = kind.as_str();
        match n {
            1 => true,
            3 => !k.starts_with("suspended") && k != "power",
            5 => true,
            7 => k.contains("seventh") || k == "dominant" || k == "half-diminished" || k == "major-minor" || k.contains("ninth") || k.contains("11th") || k.contains("13th"),
            9 => k.contains("ninth") || k.contains("11th") || k.contains("13th"),
            11 => k.contains("11th") || k.contains("13th"),
            13 => k.contains("13th"),
            _ => false,
        }
    };
    let mut degrees = vec![];
    for t in &tokens[used..] {
        if let Some((n, a)) = degree(t) {
            degrees.push((n, a, if a != 0 && has(n) { 'l' } else { 'a' }));
        }
    }
    for t in removed.split('.').filter(|t| !t.is_empty()) {
        if let Some((n, a)) = degree(t) {
            degrees.push((n, a, 's'));
        }
    }
    (kind, degrees)
}

/// LilyPond lyric text conventions: `~` is an elision tie, `_` a space
fn lyric_text(w: &str) -> String {
    w.replace('~', "\u{203f}").replace('_', " ")
}

/// all lyric items below a music expression
pub fn collect_lyrics(m: &Music, out: &mut Vec<LyItem>) {
    match m {
        Music::Lyrics(items) => out.extend(items.iter().cloned()),
        Music::Seq(items) | Music::Sim(items) => {
            for i in items {
                collect_lyrics(i, out);
            }
        }
        Music::Var(m) => collect_lyrics(m, out),
        Music::Context { body, with, .. } => {
            for (k, v) in with {
                if k == "includeGraceNotes" {
                    out.push(LyItem::IncludeGrace(matches!(v, Val::Scheme(Sexp::Bool(true)))));
                }
            }
            collect_lyrics(body, out)
        }
        Music::LyricsTo(_, body) => collect_lyrics(body, out),
        Music::Repeat { count, body, kind, .. } => {
            let mut items = vec![];
            collect_lyrics(body, &mut items);
            let n = if *kind == RepKind::Volta { 1 } else { *count };
            for _ in 0..n {
                out.extend(items.iter().cloned());
            }
        }
        Music::Set { prop, val, .. } if prop == "stanza" => out.push(LyItem::Stanza(val.text().unwrap_or_default())),
        _ => {}
    }
}

pub fn clef_of(name: &str) -> Option<(char, i8, i8)> {
    let (base, octave) = {
        let n = name.replace(['(', ')', '[', ']'], "");
        if let Some(b) = n.strip_suffix("_8") {
            (b.to_string(), -1)
        } else if let Some(b) = n.strip_suffix("^8") {
            (b.to_string(), 1)
        } else if let Some(b) = n.strip_suffix("_15") {
            (b.to_string(), -2)
        } else if let Some(b) = n.strip_suffix("^15") {
            (b.to_string(), 2)
        } else {
            (n, 0)
        }
    };
    let (sign, line) = match base.as_str() {
        "treble" | "violin" | "G" | "G2" => ('G', 2),
        "french" => ('G', 1),
        "bass" | "F" => ('F', 4),
        "varbaritone" => ('F', 3),
        "subbass" => ('F', 5),
        "alto" | "C" => ('C', 3),
        "tenor" => ('C', 4),
        "soprano" => ('C', 1),
        "mezzosoprano" => ('C', 2),
        "baritone" => ('C', 5),
        "percussion" => ('P', 2),
        "tab" | "moderntab" => ('T', 5),
        "GG" => return Some(('G', 2, -1)),
        "tenorG" => return Some(('G', 2, -1)),
        _ => return None,
    };
    Some((sign, line, octave))
}

// ---------------------------------------------------------------- markup tree

enum Mk {
    Word(String),
    List(Vec<Mk>),
    Cmd(String, Option<Box<Mk>>),
    Note(NoteType),
    Glyph(String),
    Var(MarkupText),
}

fn flatten(mk: &Mk, out: &mut MarkupText) -> String {
    fn join(items: &[Mk], sep: &str, out: &mut MarkupText) -> String {
        let parts: Vec<String> = items.iter().map(|i| flatten(i, out)).filter(|s| !s.is_empty()).collect();
        parts.join(sep)
    }
    match mk {
        Mk::Word(s) => s.clone(),
        Mk::List(items) => join(items, " ", out),
        Mk::Note(t) => {
            out.note = Some(*t);
            String::new()
        }
        Mk::Glyph(g) => {
            if g.contains("segno") {
                out.glyph = Some("segno".into());
            } else if g.contains("coda") {
                out.glyph = Some("coda".into());
            }
            String::new()
        }
        Mk::Var(m) => {
            out.italic |= m.italic;
            out.bold |= m.bold;
            out.boxed |= m.boxed;
            if m.note.is_some() {
                out.note = m.note;
            }
            if m.glyph.is_some() {
                out.glyph = m.glyph.clone();
            }
            m.text.clone()
        }
        Mk::Cmd(name, arg) => {
            match name.as_str() {
                "bold" => out.bold = true,
                "italic" => out.italic = true,
                "box" | "circle" | "rounded-box" => out.boxed = true,
                _ => {}
            }
            match (name.as_str(), arg.as_deref()) {
                (_, None) => String::new(),
                ("column" | "center-column" | "left-column" | "right-column" | "dir-column", Some(Mk::List(items))) => {
                    let mut lines = vec![];
                    for i in items {
                        let mut sub = MarkupText::default();
                        let t = flatten(i, &mut sub);
                        if t.is_empty() {
                            continue;
                        }
                        out.bold |= sub.bold;
                        out.italic |= sub.italic;
                        // a line that is italic only in part keeps its *spans*
                        let mut mixed: Option<String> = None;
                        if let Mk::Cmd(n, Some(a)) = i {
                            if let (true, Mk::List(parts)) = (n == "concat", &**a) {
                                let segs: Vec<(String, bool)> = parts
                                    .iter()
                                    .map(|p| {
                                        let mut s = MarkupText::default();
                                        let t = flatten(p, &mut s);
                                        (t, s.italic)
                                    })
                                    .filter(|(t, _)| !t.is_empty())
                                    .collect();
                                if segs.iter().any(|s| s.1) && segs.iter().any(|s| !s.1) {
                                    mixed = Some(segs.iter().map(|(t, it)| if *it { format!("*{}*", t) } else { t.clone() }).collect());
                                }
                            }
                        }
                        lines.push(match mixed {
                            Some(m) => m,
                            None if sub.italic => format!("*{}*", t),
                            None => t,
                        });
                    }
                    let text = lines.iter().map(|l| l.trim_matches('*').to_string()).collect::<Vec<_>>().join("\n");
                    out.lines = lines;
                    text
                }
                ("concat", Some(Mk::List(items))) => {
                    let parts: Vec<String> = items.iter().map(|i| flatten(i, out)).collect();
                    parts.concat()
                }
                (_, Some(a)) => flatten(a, out),
            }
        }
    }
}
