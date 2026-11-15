//! Character-level scanner for LilyPond source. The parser drives it: LilyPond's lexing
//! depends on the mode (notes, lyrics, markup, Scheme), so there is no fixed token stream.

#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    Num(f64),
    /// exact rational n/d
    Rat(i64, i64),
    Str(String),
    Sym(String),
    Bool(bool),
    List(Vec<Sexp>),
    Quote(Box<Sexp>),
    /// something we do not look into (embedded LilyPond, vectors, chars)
    Other,
}

impl Sexp {
    /// strip quotes
    pub fn unquoted(&self) -> &Sexp {
        match self {
            Sexp::Quote(b) => b.unquoted(),
            s => s,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self.unquoted() {
            Sexp::Num(n) => Some(*n),
            Sexp::Rat(n, d) if *d != 0 => Some(*n as f64 / *d as f64),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self.unquoted() {
            Sexp::Str(s) | Sexp::Sym(s) => Some(s),
            _ => None,
        }
    }
}

pub struct Lexer {
    pub src: Vec<u8>,
    pub pos: usize,
    /// collect `% N` end-of-line comments (the bar numbers lyxml's own writer leaves)
    pub hints_on: bool,
    pub hint: Option<String>,
    hint_pos: usize,
    scheme_depth: u32,
    /// (position, its line number)
    line_cache: std::cell::Cell<(usize, usize)>,
}

pub fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic() || c >= 0x80
}

impl Lexer {
    pub fn new(src: &str) -> Lexer {
        let s = src.strip_prefix('\u{feff}').unwrap_or(src);
        Lexer { src: s.as_bytes().to_vec(), pos: 0, hints_on: s.contains("Written by lyxml"), hint: None, hint_pos: 0, scheme_depth: 0, line_cache: std::cell::Cell::new((0, 1)) }
    }

    pub fn line(&self) -> usize {
        self.line_at(self.pos)
    }
    pub fn line_at(&self, pos: usize) -> usize {
        // counted from the last answer, so asking at every note stays linear overall
        let pos = pos.min(self.src.len());
        let (p0, l0) = self.line_cache.get();
        let line = if pos >= p0 {
            l0 + self.src[p0..pos].iter().filter(|&&c| c == b'\n').count()
        } else {
            l0 - self.src[pos..p0].iter().filter(|&&c| c == b'\n').count()
        };
        self.line_cache.set((pos, line));
        line
    }

    pub fn at(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }
    /// current byte, without skipping anything
    pub fn raw(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
    pub fn eof(&mut self) -> bool {
        self.skip_ws();
        self.pos >= self.src.len()
    }

    /// skip whitespace and comments
    pub fn skip_ws(&mut self) {
        loop {
            match self.raw() {
                Some(c) if c.is_ascii_whitespace() => self.pos += 1,
                Some(b'%') => {
                    if self.at(1) == Some(b'{') {
                        self.pos += 2;
                        while self.pos < self.src.len() && !(self.src[self.pos] == b'%' && self.at(1) == Some(b'}')) {
                            self.pos += 1;
                        }
                        self.pos = (self.pos + 2).min(self.src.len());
                    } else {
                        let a = self.pos;
                        while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                            self.pos += 1;
                        }
                        if self.hints_on && a >= self.hint_pos {
                            let t = String::from_utf8_lossy(&self.src[a + 1..self.pos]).trim().to_string();
                            if !t.is_empty() && t.len() <= 12 && !t.contains(' ') {
                                self.hint = Some(t);
                            }
                            self.hint_pos = a + 1;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// next significant byte
    pub fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.raw()
    }

    pub fn looking_at(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s.as_bytes())
    }

    /// skip whitespace, then consume `s` if it is next
    pub fn eat(&mut self, s: &str) -> bool {
        self.skip_ws();
        if self.looking_at(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn slice(&self, a: usize, b: usize) -> String {
        String::from_utf8_lossy(&self.src[a..b]).into_owned()
    }

    /// a run of letters (ASCII or non-ASCII)
    pub fn read_word(&mut self) -> String {
        let a = self.pos;
        while self.pos < self.src.len() && is_alpha(self.src[self.pos]) {
            self.pos += 1;
        }
        self.slice(a, self.pos)
    }

    /// identifier: letters with single `-` or `_` between letters; digits allowed after the first letter
    /// when `digits` is set (context names, property names)
    pub fn read_ident(&mut self, digits: bool) -> String {
        let a = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if is_alpha(c) || (digits && c.is_ascii_digit() && self.pos > a) {
                self.pos += 1;
            } else if (c == b'-' || c == b'_') && self.pos > a && self.at(1).map(is_alpha).unwrap_or(false) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.slice(a, self.pos)
    }

    /// everything up to whitespace or a brace
    pub fn read_token(&mut self) -> String {
        let a = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c.is_ascii_whitespace() || c == b'{' || c == b'}' {
                break;
            }
            self.pos += 1;
        }
        self.slice(a, self.pos)
    }

    pub fn read_uint(&mut self) -> Option<u32> {
        let a = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if a == self.pos {
            return None;
        }
        self.slice(a, self.pos).parse().ok()
    }

    /// unsigned decimal number
    pub fn read_number(&mut self) -> Option<f64> {
        let a = self.pos;
        while self.pos < self.src.len() && (self.src[self.pos].is_ascii_digit() || self.src[self.pos] == b'.') {
            self.pos += 1;
        }
        if a == self.pos {
            return None;
        }
        self.slice(a, self.pos).parse().ok()
    }

    /// at `"`: read a string literal
    pub fn read_string(&mut self) -> Result<String, String> {
        let start_line = self.line();
        self.pos += 1;
        let mut out: Vec<u8> = vec![];
        loop {
            match self.raw() {
                None => return Err(format!("line {}: unterminated string", start_line)),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.raw() {
                        Some(b'n') => out.push(b'\n'),
                        Some(b't') => out.push(b'\t'),
                        Some(c) => out.push(c),
                        None => {}
                    }
                    self.pos += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// at `\`: read a command name (without the backslash). Symbol commands (`\\`, `\<`, `\!` ...)
    /// come back as that one character.
    pub fn read_command(&mut self) -> String {
        self.pos += 1;
        match self.raw() {
            Some(c) if is_alpha(c) => self.read_ident(false),
            Some(c) => {
                self.pos += 1;
                (c as char).to_string()
            }
            None => String::new(),
        }
    }

    /// the command name that comes next, if a command is next; nothing is consumed
    pub fn peek_command(&mut self) -> Option<String> {
        self.skip_ws();
        if self.raw() != Some(b'\\') {
            return None;
        }
        let save = self.pos;
        let c = self.read_command();
        self.pos = save;
        Some(c)
    }

    // ------------------------------------------------------------ Scheme

    fn skip_scheme_ws(&mut self) {
        loop {
            match self.raw() {
                Some(c) if c.is_ascii_whitespace() => self.pos += 1,
                Some(b';') => {
                    while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// read one Scheme datum; call with `pos` just after the `#` or `$`
    pub fn read_scheme(&mut self) -> Result<Sexp, String> {
        self.scheme_depth += 1;
        if self.scheme_depth > 250 {
            self.scheme_depth = 0;
            return Err(format!("line {}: Scheme expression nested too deeply", self.line()));
        }
        let r = self.read_scheme_inner();
        self.scheme_depth = self.scheme_depth.saturating_sub(1);
        r
    }

    fn read_scheme_inner(&mut self) -> Result<Sexp, String> {
        match self.raw() {
            None => Ok(Sexp::Other),
            Some(b'\'') | Some(b'`') => {
                self.pos += 1;
                Ok(Sexp::Quote(Box::new(self.read_scheme()?)))
            }
            Some(b',') => {
                self.pos += 1;
                if self.raw() == Some(b'@') {
                    self.pos += 1;
                }
                self.read_scheme()
            }
            Some(b'(') => {
                let line = self.line();
                self.pos += 1;
                let mut items = vec![];
                loop {
                    self.skip_scheme_ws();
                    match self.raw() {
                        None => return Err(format!("line {}: unclosed Scheme expression", line)),
                        Some(b')') => {
                            self.pos += 1;
                            break;
                        }
                        _ => items.push(self.read_scheme()?),
                    }
                }
                Ok(Sexp::List(items))
            }
            Some(b'"') => Ok(Sexp::Str(self.read_string()?)),
            Some(b'#') => {
                // #t #f #{ ... #} #( vector #\char #:keyword
                match self.at(1) {
                    Some(b'{') => {
                        let line = self.line();
                        self.pos += 2;
                        let mut depth = 1;
                        while self.pos < self.src.len() {
                            if self.looking_at("#{") {
                                depth += 1;
                                self.pos += 2;
                            } else if self.looking_at("#}") {
                                depth -= 1;
                                self.pos += 2;
                                if depth == 0 {
                                    return Ok(Sexp::Other);
                                }
                            } else {
                                self.pos += 1;
                            }
                        }
                        Err(format!("line {}: unclosed #{{", line))
                    }
                    Some(b'(') => {
                        self.pos += 1;
                        self.read_scheme()?;
                        Ok(Sexp::Other)
                    }
                    Some(b'\\') => {
                        self.pos += 3;
                        while self.pos < self.src.len() && self.src[self.pos].is_ascii_alphanumeric() {
                            self.pos += 1;
                        }
                        Ok(Sexp::Other)
                    }
                    _ => {
                        self.pos += 1;
                        let a = self.read_atom();
                        Ok(match a.as_str() {
                            "t" | "true" => Sexp::Bool(true),
                            "f" | "false" => Sexp::Bool(false),
                            _ => Sexp::Other,
                        })
                    }
                }
            }
            Some(_) => {
                let a = self.read_atom();
                if a.is_empty() {
                    // a stray delimiter: step over it so we always make progress
                    self.pos += 1;
                    return Ok(Sexp::Other);
                }
                if let Ok(n) = a.parse::<f64>() {
                    return Ok(Sexp::Num(n));
                }
                if let Some((n, d)) = a.split_once('/') {
                    if let (Ok(n), Ok(d)) = (n.parse::<i64>(), d.parse::<i64>()) {
                        return Ok(Sexp::Rat(n, d));
                    }
                }
                Ok(Sexp::Sym(a))
            }
        }
    }

    fn read_atom(&mut self) -> String {
        let a = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c.is_ascii_whitespace() || matches!(c, b'(' | b')' | b'"' | b';' | b'{' | b'}') {
                break;
            }
            self.pos += 1;
        }
        self.slice(a, self.pos)
    }

    /// at `{`: skip to the matching `}` (strings and comments respected); returns the inside text
    pub fn skip_braces(&mut self) -> Result<String, String> {
        let line = self.line();
        let a = self.pos + 1;
        let mut depth = 0;
        loop {
            match self.raw() {
                None => return Err(format!("line {}: unclosed {{", line)),
                Some(b'"') => {
                    self.read_string()?;
                }
                Some(b'%') => self.skip_ws(),
                Some(b'{') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(b'}') => {
                    depth -= 1;
                    self.pos += 1;
                    if depth == 0 {
                        return Ok(self.slice(a, self.pos - 1));
                    }
                }
                Some(_) => self.pos += 1,
            }
        }
    }
}
