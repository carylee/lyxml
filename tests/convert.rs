//! End-to-end tests of the binary.
//!
//! - every file in tests/data converts to LilyPond identical to tests/golden/NAME.ly
//!   (run with UPDATE_GOLDEN=1 to rewrite the goldens after an intended change);
//! - MusicXML -> MusicXML through the model compares identical;
//! - output is deterministic; diagnostics and exit codes behave.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lyxml"))
}

fn data_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    v.sort();
    v
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lyxml-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

#[test]
fn golden_lilypond() {
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let mut failed = vec![];
    for f in data_files() {
        let out = bin().arg(&f).args(["--to", "ly", "-q"]).output().unwrap();
        assert!(out.status.success(), "{} failed: {}", f.display(), String::from_utf8_lossy(&out.stderr));
        let got = String::from_utf8(out.stdout).unwrap();
        let golden = golden_dir.join(format!("{}.ly", f.file_stem().unwrap().to_string_lossy()));
        if update {
            std::fs::write(&golden, &got).unwrap();
        } else {
            let want = std::fs::read_to_string(&golden).unwrap_or_default();
            if want != got {
                failed.push(f.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    assert!(failed.is_empty(), "LilyPond output changed for {:?} (UPDATE_GOLDEN=1 to accept)", failed);
}

#[test]
fn deterministic() {
    for f in data_files() {
        let a = bin().arg(&f).args(["--to", "ly", "-q"]).output().unwrap().stdout;
        let b = bin().arg(&f).args(["--to", "ly", "-q"]).output().unwrap().stdout;
        assert_eq!(a, b, "{}", f.display());
    }
}

#[test]
fn musicxml_round_trip() {
    for (i, f) in data_files().into_iter().enumerate() {
        let out = tmp(&format!("rt{}.mxl", i));
        let st = bin().arg(&f).arg("-o").arg(&out).arg("-q").status().unwrap();
        assert!(st.success());
        let d = bin().arg("diff").arg(&f).arg(&out).output().unwrap();
        assert!(d.status.success(), "{}:\n{}", f.display(), String::from_utf8_lossy(&d.stdout));
    }
}

#[test]
fn lilypond_round_trip() {
    // MusicXML -> LilyPond -> MusicXML keeps notes, words and structure
    for (i, f) in data_files().into_iter().enumerate() {
        if f.to_string_lossy().contains("Chordnames") {
            continue; // chord symbols are not read back from LilyPond
        }
        let ly = tmp(&format!("rt{}.ly", i));
        assert!(bin().arg(&f).arg("-o").arg(&ly).arg("-q").status().unwrap().success());
        let d = bin().arg("diff").arg(&f).arg(&ly).output().unwrap();
        assert!(d.status.success(), "{}:\n{}", f.display(), String::from_utf8_lossy(&d.stdout));
    }
}

#[test]
fn stdin_stdout_and_json() {
    use std::io::Write;
    let f = data_files().into_iter().find(|p| p.to_string_lossy().ends_with("old_hundredth.musicxml")).unwrap();
    let mut child = bin()
        .args(["-", "--to", "ly", "--json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&std::fs::read(&f).unwrap()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("\\version"));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("{\"dropped\":0,"), "{}", err);
}

#[test]
fn strict_fails_on_loss() {
    let src = r#"<?xml version="1.0"?><score-partwise><part-list><score-part id="P1"><part-name>X</part-name></score-part></part-list>
<part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes>
<figured-bass><figure><figure-number>6</figure-number></figure></figured-bass>
<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note></measure></part></score-partwise>"#;
    let p = tmp("fb.musicxml");
    std::fs::write(&p, src).unwrap();
    let st = bin().arg(&p).args(["--to", "ly", "--strict", "-q"]).output().unwrap();
    assert_eq!(st.status.code(), Some(2));
    let ok = bin().arg(&p).args(["--to", "ly", "-q"]).output().unwrap();
    assert!(ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stdout).contains("c'1"));
}

#[test]
fn extender_stops_where_hidden_words_begin() {
    // lower staff: "Oo" with an extender in bar 1, then the same word as the upper staff in
    // bar 2. With the shared word hidden, the extender must not run on: an empty syllable
    // takes the hidden word's place.
    let note = |step: &str, lyric: &str| {
        format!("<note><pitch><step>{}</step><octave>4</octave></pitch><duration>4</duration><type>whole</type>{}</note>", step, lyric)
    };
    let part = |id: &str, l1: &str| {
        format!(
            "<part id=\"{}\"><measure number=\"1\"><attributes><divisions>1</divisions></attributes>{}</measure><measure number=\"2\">{}</measure></part>",
            id,
            note("C", l1),
            note("D", "<lyric number=\"1\"><syllabic>single</syllabic><text>snow</text></lyric>")
        )
    };
    let src = format!(
        "<?xml version=\"1.0\"?><score-partwise><part-list><score-part id=\"P1\"><part-name>Up</part-name></score-part><score-part id=\"P2\"><part-name>Low</part-name></score-part></part-list>{}{}</score-partwise>",
        part("P1", "<lyric number=\"1\"><syllabic>single</syllabic><text>Let</text></lyric>"),
        part("P2", "<lyric number=\"1\"><syllabic>single</syllabic><text>Oo</text><extend/></lyric>")
    );
    let p = tmp("ext.musicxml");
    std::fs::write(&p, src).unwrap();
    let out = bin().arg(&p).args(["--to", "ly", "-q", "--hide-shared-lyrics"]).output().unwrap();
    let ly = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(ly.contains("Oo __"), "{}", ly);
    assert!(ly.contains("\"\" % 2"), "{}", ly);
    assert!(ly.contains("Let") && ly.matches("snow").count() == 1, "{}", ly);
}

#[test]
fn quotes_in_words_survive() {
    // a straight double quote in lyric text crashed musicxml2ly
    let src = r#"<?xml version="1.0"?><score-partwise><part-list><score-part id="P1"><part-name>X</part-name></score-part></part-list>
<part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes>
<note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><type>half</type><lyric number="1"><syllabic>begin</syllabic><text>"Mer</text></lyric></note>
<note><pitch><step>D</step><octave>4</octave></pitch><duration>2</duration><type>half</type><lyric number="1"><syllabic>end</syllabic><text>ry."</text></lyric></note>
</measure></part></score-partwise>"#;
    let p = tmp("q.musicxml");
    std::fs::write(&p, src).unwrap();
    let out = bin().arg(&p).args(["--to", "ly", "-q"]).output().unwrap();
    let ly = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(ly.contains(r#""\"Mer" -- "ry.\"""#), "{}", ly);
}
