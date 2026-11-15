//! Diagnostics: everything a conversion could not carry over is recorded here,
//! with the part and bar where it happened.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// something in the source was left out of the output
    Dropped,
    /// carried over, but approximated or suspicious
    Warning,
    Info,
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub level: Level,
    pub part: Option<String>,
    pub measure: Option<String>,
    /// source line (LilyPond input)
    pub line: Option<usize>,
    pub msg: String,
}

#[derive(Default, Debug)]
pub struct Diags {
    pub list: Vec<Diag>,
}

impl Diags {
    pub fn push(&mut self, level: Level, part: Option<&str>, measure: Option<&str>, msg: impl Into<String>) {
        let d = Diag { level, part: part.map(String::from), measure: measure.map(String::from), line: None, msg: msg.into() };
        // the same complaint about the same place is reported once
        if !self.list.iter().any(|x| x.level == d.level && x.part == d.part && x.measure == d.measure && x.msg == d.msg) {
            self.list.push(d);
        }
    }
    pub fn dropped(&mut self, part: Option<&str>, measure: Option<&str>, msg: impl Into<String>) {
        self.push(Level::Dropped, part, measure, msg);
    }
    pub fn warn(&mut self, part: Option<&str>, measure: Option<&str>, msg: impl Into<String>) {
        self.push(Level::Warning, part, measure, msg);
    }
    /// diagnostics tied to a line of LilyPond source
    pub fn at_line(&mut self, level: Level, line: usize, msg: impl Into<String>) {
        self.list.push(Diag { level, part: None, measure: None, line: Some(line), msg: msg.into() });
    }
    pub fn n_dropped(&self) -> usize {
        self.list.iter().filter(|d| d.level == Level::Dropped).count()
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for d in &self.list {
            let lv = match d.level {
                Level::Dropped => "dropped",
                Level::Warning => "warning",
                Level::Info => "info",
            };
            out.push_str(lv);
            out.push_str(": ");
            if let Some(l) = d.line {
                out.push_str(&format!("line {}: ", l));
            }
            if let Some(p) = &d.part {
                out.push_str(&format!("part {}", p));
                out.push_str(if d.measure.is_some() { ", " } else { ": " });
            }
            if let Some(m) = &d.measure {
                out.push_str(&format!("bar {}: ", m));
            }
            out.push_str(&d.msg);
            out.push('\n');
        }
        out
    }

    pub fn to_json(&self) -> String {
        fn esc(s: &str) -> String {
            let mut o = String::with_capacity(s.len() + 2);
            o.push('"');
            for c in s.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    '\n' => o.push_str("\\n"),
                    '\t' => o.push_str("\\t"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        }
        let items: Vec<String> = self
            .list
            .iter()
            .map(|d| {
                let lv = match d.level {
                    Level::Dropped => "dropped",
                    Level::Warning => "warning",
                    Level::Info => "info",
                };
                let opt = |o: &Option<String>| o.as_deref().map(esc).unwrap_or_else(|| "null".into());
                format!(
                    "{{\"level\":\"{}\",\"part\":{},\"bar\":{},\"line\":{},\"message\":{}}}",
                    lv,
                    opt(&d.part),
                    opt(&d.measure),
                    d.line.map(|l| l.to_string()).unwrap_or_else(|| "null".into()),
                    esc(&d.msg)
                )
            })
            .collect();
        format!("{{\"dropped\":{},\"diagnostics\":[{}]}}\n", self.n_dropped(), items.join(","))
    }
}
