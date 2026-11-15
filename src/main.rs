//! lyxml: convert between MusicXML (.musicxml, .xml, .mxl) and LilyPond (.ly).

mod diag;
mod diff;
mod ir;
mod ly_write;
mod lyread;
mod mxml_read;
mod mxml_write;
mod transform;

use diag::Diags;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const USAGE: &str = "\
lyxml - convert between MusicXML and LilyPond

usage:
  lyxml IN [-o OUT] [options]      convert; direction follows the file types
  lyxml diff A B [--notes|--words] compare two scores (any format) musically;
                                   --notes ignores the words, --words shows only them
  lyxml notes IN                   list every sounding note: part staff onset pitch
  lyxml dump IN                    print the parsed score model (debugging)

IN / OUT:
  .musicxml .xml .mxl  <->  .ly     `-` reads stdin / writes stdout
  --from xml|ly      input format when it cannot be told from the name or content
  --to ly|xml|mxl    output format when there is no -o name to tell it

diagnostics:
  --json             diagnostics as JSON on stderr
  --strict           exit 2 when anything in the source was dropped
  -q, --quiet        no diagnostics on stderr

LilyPond output:
  --paper NAME       letter, a4, octavo, remarkable, or WxH in mm (default: the file's page size)
  --staff-size PT    staff height in points (default: the file's scaling)
  --hide-empty       hide staves that rest for a whole system
  --hide-empty-staves NAME,NAME   hide only these staves (part id, name or short name)
                     on systems where they rest
  --bar-numbers all|system|none   on every bar, at each system start, or not at all
                     (--no-bar-numbers = none; default: what the file says, else system)
  --breaks keep|pages|ignore   the file's system and page breaks: all (default), only
                     the page breaks (LilyPond lays out the rest), or none
  --page-breaks N,N  start a new page at these bar numbers as well
  --hide-shared-lyrics   words that a staff shares with the staff above print once,
                     under the upper staff (bar by bar, verse by verse)
  --beams file|auto  explicit beams from the file, or LilyPond's own (default: file when it has any)
  --curly-quotes     typographic quotes in the words (\" opening a syllable -> “, else ”)
  --title-style plain|smallcaps   smallcaps: the octavo title block (large small-caps
                     title on its own line, credits below it, footnote above the copyright)
  --text-font NAME   serif text font family, e.g. Times
  --midi             add a \\midi block
  --ly-version V     \\version to write (default 2.24.0)
";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Xml,
    Mxl,
    Ly,
}

fn format_of_name(name: &str) -> Option<Format> {
    let l = name.to_ascii_lowercase();
    if l.ends_with(".mxl") {
        Some(Format::Mxl)
    } else if l.ends_with(".musicxml") || l.ends_with(".xml") {
        Some(Format::Xml)
    } else if l.ends_with(".ly") || l.ends_with(".ily") {
        Some(Format::Ly)
    } else {
        None
    }
}

fn sniff(data: &[u8]) -> Format {
    if data.starts_with(b"PK") {
        return Format::Mxl;
    }
    let head = &data[..data.len().min(2000)];
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with("<?xml") || t.starts_with("<!DOCTYPE") || t.starts_with("<score-") {
        Format::Xml
    } else {
        Format::Ly
    }
}

fn read_input(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = vec![];
        std::io::stdin().read_to_end(&mut buf).map_err(|e| format!("stdin: {}", e))?;
        Ok(buf)
    } else {
        std::fs::read(path).map_err(|e| format!("{}: {}", path, e))
    }
}

fn load(path: &str, from: Option<Format>, diags: &mut Diags) -> Result<(ir::Score, Format), String> {
    let data = read_input(path)?;
    let fmt = from.or_else(|| format_of_name(path)).unwrap_or_else(|| sniff(&data));
    let fmt = if fmt != Format::Ly && data.starts_with(b"PK") { Format::Mxl } else { fmt };
    let score = match fmt {
        Format::Xml | Format::Mxl => mxml_read::read_musicxml(&data, diags)?,
        Format::Ly => {
            let src = String::from_utf8_lossy(&data);
            let base: Option<PathBuf> = if path == "-" { None } else { Path::new(path).parent().map(|p| p.to_path_buf()) };
            lyread::read_ly(&src, base.as_deref(), diags)?
        }
    };
    Ok((score, fmt))
}

fn fail(msg: &str) -> ! {
    eprintln!("lyxml: {}", msg);
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", USAGE);
        return;
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("lyxml {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut pos: Vec<String> = vec![];
    let mut out: Option<String> = None;
    let mut from: Option<Format> = None;
    let mut to: Option<Format> = None;
    let (mut json, mut strict, mut quiet) = (false, false, false);
    let mut breaks_mode = String::from("keep");
    let mut page_breaks: Vec<String> = vec![];
    let mut hide_shared = false;
    let mut diff_mode = "all";
    let mut curly = false;
    let mut opts = ly_write::LyOptions::default();
    let mut i = 0;
    let parse_fmt = |s: &str| match s {
        "xml" | "musicxml" => Format::Xml,
        "mxl" => Format::Mxl,
        "ly" | "lilypond" => Format::Ly,
        _ => fail(&format!("unknown format `{}` (xml, mxl, ly)", s)),
    };
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| fail(&format!("{} needs a value", a)))
        };
        match a {
            "-o" | "--output" => out = Some(val()),
            "--from" => from = Some(parse_fmt(&val())),
            "--to" => to = Some(parse_fmt(&val())),
            "--json" => json = true,
            "--strict" => strict = true,
            "-q" | "--quiet" => quiet = true,
            "--paper" => opts.paper = Some(val()),
            "--staff-size" => opts.staff_size = Some(val().parse().unwrap_or_else(|_| fail("--staff-size needs a number"))),
            "--hide-empty" => opts.hide_empty = Some(true),
            "--no-bar-numbers" => opts.bar_numbers = Some(false),
            "--bar-numbers" => match val().as_str() {
                "all" => {
                    opts.bar_numbers = Some(true);
                    opts.bar_numbers_all = Some(true);
                }
                "system" => {
                    opts.bar_numbers = Some(true);
                    opts.bar_numbers_all = Some(false);
                }
                "none" => opts.bar_numbers = Some(false),
                _ => fail("--bar-numbers all|system|none"),
            },
            "--breaks" => {
                breaks_mode = val();
                if !["keep", "pages", "ignore"].contains(&breaks_mode.as_str()) {
                    fail("--breaks keep|pages|ignore");
                }
            }
            "--page-breaks" => page_breaks = val().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "--hide-shared-lyrics" => hide_shared = true,
            "--curly-quotes" => curly = true,
            "--hide-empty-staves" => opts.hide_staves = val().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "--title-style" => match val().as_str() {
                "smallcaps" => opts.title_smallcaps = true,
                "plain" => opts.title_smallcaps = false,
                _ => fail("--title-style plain|smallcaps"),
            },
            "--text-font" => opts.text_font = Some(val()),
            "--notes" => diff_mode = "notes",
            "--words" => diff_mode = "words",
            "--beams" => {
                opts.beams = match val().as_str() {
                    "file" => Some(true),
                    "auto" => Some(false),
                    _ => fail("--beams file|auto"),
                }
            }
            "--midi" => opts.midi = true,
            "--ly-version" => opts.version = val(),
            "-" => pos.push(a.to_string()),
            _ if a.starts_with('-') => fail(&format!("unknown option {} (see --help)", a)),
            _ => pos.push(a.to_string()),
        }
        i += 1;
    }
    let mut diags = Diags::default();
    let report = |diags: &Diags| {
        if json {
            eprint!("{}", diags.to_json());
        } else if !quiet {
            eprint!("{}", diags.to_text());
        }
    };

    match pos.first().map(|s| s.as_str()) {
        Some("diff") => {
            if pos.len() != 3 {
                fail("usage: lyxml diff A B");
            }
            let (a, _) = load(&pos[1], None, &mut diags).unwrap_or_else(|e| fail(&e));
            let (b, _) = load(&pos[2], None, &mut diags).unwrap_or_else(|e| fail(&e));
            let (mut a, mut b) = (a, b);
            let all = if diff_mode == "notes" { vec![] } else { diff::diff(&a, &b) };
            // without the words: what differs in the music itself
            for s in [&mut a, &mut b] {
                for e in s.parts.iter_mut().flat_map(|p| p.measures.iter_mut()).flat_map(|m| m.voices.iter_mut()).flat_map(|v| v.events.iter_mut()) {
                    e.lyrics.clear();
                }
            }
            let music = diff::diff(&a, &b);
            let lines: Vec<String> = match diff_mode {
                "notes" => music,
                // lines that only exist because of the words
                "words" => all.into_iter().filter(|l| !music.contains(l)).collect(),
                _ => all,
            };
            let mut so = std::io::stdout().lock();
            for l in &lines {
                let _ = writeln!(so, "{}", l);
            }
            if lines.is_empty() {
                let _ = writeln!(so, "identical: {} parts, {} bars", a.parts.len(), a.parts.first().map(|p| p.measures.len()).unwrap_or(0));
            }
            std::process::exit(if lines.is_empty() { 0 } else { 1 });
        }
        Some("notes") => {
            // every sounding note: part, staff, onset (whole notes from the start), MIDI pitch
            if pos.len() != 2 {
                fail("usage: lyxml notes IN");
            }
            let (s, _) = load(&pos[1], from, &mut diags).unwrap_or_else(|e| fail(&e));
            let n = s.parts.iter().map(|p| p.measures.len()).max().unwrap_or(0);
            let mut so = std::io::stdout().lock();
            for (pi, p) in s.parts.iter().enumerate() {
                let mut start = ir::Frac::ZERO;
                // pitches held over by a tie, per voice: (end of the tied note, pitch)
                let mut held: std::collections::HashMap<(u32, u32), Vec<(ir::Frac, i32)>> = Default::default();
                for i in 0..n {
                    if let Some(m) = p.measures.get(i) {
                        for v in &m.voices {
                            let h = held.entry((v.staff, v.voice)).or_default();
                            for e in &v.events {
                                if e.grace.is_some() {
                                    for note in e.notes() {
                                        let _ = writeln!(so, "{} {} grace {}", pi + 1, v.staff, note.midi());
                                    }
                                    continue;
                                }
                                let at = start + e.pos;
                                let before: Vec<(ir::Frac, i32)> = std::mem::take(h);
                                for note in e.notes() {
                                    if note.tie_start {
                                        h.push((at + e.dur, note.midi()));
                                    }
                                    // a tie arriving from nowhere (second ending) is struck again
                                    if !(note.tie_stop && before.contains(&(at, note.midi()))) {
                                        let at = start + e.pos;
                                        let _ = writeln!(so, "{} {} {}/{} {}", pi + 1, v.staff, at.n, at.d, note.midi());
                                    }
                                }
                            }
                        }
                    }
                    start += s.parts.iter().filter_map(|p| p.measures.get(i)).map(|m| m.len).max().unwrap_or(ir::Frac::ZERO);
                }
            }
        }
        Some("dump") => {
            if pos.len() != 2 {
                fail("usage: lyxml dump IN");
            }
            let (s, _) = load(&pos[1], from, &mut diags).unwrap_or_else(|e| fail(&e));
            println!("{:#?}", s);
            report(&diags);
        }
        Some(first) => {
            let input = if first == "convert" { pos.get(1).cloned().unwrap_or_else(|| fail("no input file")) } else { first.to_string() };
            let (mut score, in_fmt) = load(&input, from, &mut diags).unwrap_or_else(|e| fail(&e));
            for b in transform::apply_breaks(&mut score, &breaks_mode, &page_breaks) {
                diags.warn(None, Some(&b), "--page-breaks: no bar with this number");
            }
            if hide_shared {
                transform::hide_shared_lyrics(&mut score);
            }
            if curly {
                transform::curly_quotes(&mut score);
            }
            let out_fmt = to
                .or_else(|| out.as_deref().and_then(format_of_name))
                .unwrap_or(if in_fmt == Format::Ly { Format::Xml } else { Format::Ly });
            let bytes: Vec<u8> = match out_fmt {
                Format::Ly => ly_write::write_ly(&score, &opts, &mut diags).into_bytes(),
                Format::Xml => mxml_write::write_musicxml(&score, &mut diags).into_bytes(),
                Format::Mxl => mxml_write::write_mxl(&mxml_write::write_musicxml(&score, &mut diags)).unwrap_or_else(|e| fail(&e)),
            };
            match out.as_deref() {
                None | Some("-") => {
                    let _ = std::io::stdout().lock().write_all(&bytes);
                }
                Some(p) => std::fs::write(p, &bytes).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))),
            }
            report(&diags);
            if strict && diags.n_dropped() > 0 {
                std::process::exit(2);
            }
        }
        None => fail("no input file (see --help)"),
    }
}
