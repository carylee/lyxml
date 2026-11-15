//! Tests of the LilyPond reader against the fixtures in tests/ly (each of which compiles
//! cleanly with LilyPond 2.24) and against small inline snippets.

use super::read_ly;
use crate::diag::{Diags, Level};
use crate::ir::*;

fn read(src: &str) -> (Score, Diags) {
    let mut d = Diags::default();
    let s = read_ly(src, None, &mut d).expect("parses");
    (s, d)
}

fn q(n: i64) -> Frac {
    Frac::new(n, 4)
}

/// all events of one (staff, voice-rank) of a part, with their bar index
fn voice(p: &Part, staff: u32, rank: usize) -> Vec<(usize, &Event)> {
    let mut ids: Vec<u32> = vec![];
    for m in &p.measures {
        for v in &m.voices {
            if v.staff == staff && !ids.contains(&v.voice) {
                ids.push(v.voice);
            }
        }
    }
    ids.sort();
    let id = ids[rank];
    let mut out = vec![];
    for (mi, m) in p.measures.iter().enumerate() {
        for v in &m.voices {
            if v.staff == staff && v.voice == id {
                out.extend(v.events.iter().map(|e| (mi, e)));
            }
        }
    }
    out
}

fn pitches(evs: &[(usize, &Event)]) -> Vec<String> {
    evs.iter()
        .map(|(_, e)| {
            if e.is_rest() {
                "r".to_string()
            } else {
                e.notes()
                    .iter()
                    .map(|n| {
                        let mut s = String::new();
                        s.push(['C', 'D', 'E', 'F', 'G', 'A', 'B'][n.step as usize]);
                        match n.alter {
                            1 => s.push('#'),
                            2 => s.push_str("##"),
                            -1 => s.push('b'),
                            -2 => s.push_str("bb"),
                            _ => {}
                        }
                        s.push_str(&n.octave.to_string());
                        s
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            }
        })
        .collect()
}

fn words(evs: &[(usize, &Event)], line: u32) -> String {
    let mut out = String::new();
    for (_, e) in evs {
        for l in e.lyrics.iter().filter(|l| l.line == line) {
            out.push_str(&l.text);
            match l.syllabic {
                Syllabic::Begin | Syllabic::Middle => out.push('-'),
                _ => out.push(' '),
            }
        }
    }
    out.trim().to_string()
}

fn no_drops(d: &Diags) {
    assert_eq!(d.n_dropped(), 0, "{}", d.to_text());
}

#[test]
fn hymn_satb() {
    let (s, d) = read(include_str!("../../tests/ly/hymn.ly"));
    no_drops(&d);
    assert_eq!(s.header.title.as_deref(), Some("Evening Hymn"));
    assert_eq!(s.header.poet.as_deref(), Some("Anon."));
    assert_eq!(s.parts.len(), 2);
    assert!(matches!(s.part_list[0], PartListItem::GroupStart { symbol: GroupSymbol::Bracket, barline: Some(false), .. }));
    let sa = &s.parts[0];
    assert_eq!(sa.name, "S A");
    assert_eq!(sa.measures.len(), 7);
    // pickup
    assert!(sa.measures[0].implicit);
    assert_eq!(sa.measures[0].number, "0");
    assert_eq!(sa.measures[0].len, q(1));
    assert_eq!(sa.measures[1].number, "1");
    assert_eq!(sa.measures[6].len, q(3));
    assert_eq!(sa.measures[6].right.style.as_deref(), Some("light-heavy"));
    assert!(sa.measures[0].attrs.iter().any(|a| a.kind == Attr::Key { fifths: 1, minor: false }));
    let sop = voice(sa, 1, 0);
    let alto = voice(sa, 1, 1);
    assert_eq!(sop.len(), 23);
    assert_eq!(alto.len(), 23);
    assert_eq!(pitches(&sop)[..9], ["G4", "G4", "F#4", "E4", "D4", "G4", "A4", "B4", "B4"]);
    assert_eq!(pitches(&alto)[..5], ["D4", "D4", "D4", "B3", "A3"]);
    assert!(sop[7].1.arts.contains(&Art::Fermata { inverted: false }));
    // two verses on the sopranos; the slurred pair takes one syllable
    assert_eq!(words(&sop, 1), "Now all the woods are sleep-ing, and night and still-ness creep-ing on vil-lage, field and town.");
    assert_eq!(words(&sop, 2), "The sun has gone in glo-ry; the stars now tell their sto-ry, and qui-et set-tles down.");
    assert_eq!(sop[0].1.lyrics[0].stanza.as_deref(), Some("1."));
    assert_eq!(sop[0].1.lyrics[1].stanza.as_deref(), Some("2."));
    assert!(sop[18].1.lyrics.is_empty(), "second note of the slur has no syllable");
    assert!(alto.iter().all(|(_, e)| e.lyrics.is_empty()));
    // men: bass clef, relative octaves
    let tb = &s.parts[1];
    assert!(tb.measures[0].attrs.iter().any(|a| matches!(a.kind, Attr::Clef { sign: 'F', line: 4, .. })));
    assert_eq!(pitches(&voice(tb, 1, 1))[..8], ["G3", "G3", "D3", "E3", "F#3", "B2", "D3", "G2"]);
}

#[test]
fn piano_two_staves() {
    let (s, d) = read(include_str!("../../tests/ly/piano.ly"));
    no_drops(&d);
    assert_eq!(s.parts.len(), 1);
    let p = &s.parts[0];
    assert_eq!(p.staves, 2);
    assert_eq!(p.name, "Piano");
    assert_eq!(p.measures.len(), 7);
    assert!(p.measures.iter().all(|m| m.len == q(3)));
    let up = voice(p, 1, 0);
    let up2 = voice(p, 1, 1);
    let low = voice(p, 2, 0);
    assert_eq!(pitches(&up2), ["D4", "D4", "C#4"]);
    // triplet
    let trip: Vec<&Event> = up.iter().filter(|(mi, e)| *mi == 1 && e.tuplet.is_some()).map(|x| x.1).collect();
    assert_eq!(trip.len(), 3);
    assert!(trip.iter().all(|e| e.dur == Frac::new(1, 12) && e.tuplet == Some(Tuplet { actual: 3, normal: 2 })));
    assert!(trip[0].tuplet_start && trip[2].tuplet_stop);
    // grace notes: an acciaccatura with its slur, then two plain ones
    let bar3: Vec<&Event> = up.iter().filter(|(mi, _)| *mi == 2).map(|x| x.1).collect();
    assert_eq!(bar3.len(), 6);
    assert_eq!(bar3[0].grace, Some(Grace { slash: true }));
    assert_eq!(bar3[0].slur_starts, vec![1]);
    assert_eq!(bar3[1].slur_stops, vec![1]);
    assert!(bar3[1].arts.contains(&Art::Staccato));
    assert_eq!(bar3[2].grace, Some(Grace { slash: false }));
    assert_eq!(bar3[3].grace, Some(Grace { slash: false }));
    assert_eq!(bar3[3].pos, q(1));
    assert!(bar3[4].arts.contains(&Art::Accent));
    assert!(bar3[5].arts.contains(&Art::Tenuto) && bar3[5].arts.contains(&Art::Fermata { inverted: false }));
    // chords, key change, whole-bar rest
    assert_eq!(pitches(&up.iter().filter(|(mi, _)| *mi == 3).cloned().collect::<Vec<_>>()), ["D4+F#4+A4", "E4+G4+B4"]);
    assert!(p.measures[3].attrs.iter().any(|a| a.kind == Attr::Key { fifths: 2, minor: false }));
    assert!(p.measures[0].attrs.iter().any(|a| a.kind == Attr::Key { fifths: -1, minor: true }));
    assert!(matches!(up.last().unwrap().1.kind, EventKind::Rest { whole_measure: true }));
    // left hand: `q` repeats the chord, tie carried over the bar
    let lp = pitches(&low);
    assert_eq!(lp[lp.len() - 3..], ["D2+D3", "D2+D3", "D2"]);
    let tied = low[low.len() - 3].1;
    assert!(tied.notes().iter().all(|n| n.tie_start));
    assert!(low[low.len() - 2].1.notes().iter().all(|n| n.tie_stop));
    // directions
    let kinds: Vec<&DirKind> = p.measures.iter().flat_map(|m| m.directions.iter().map(|d| &d.kind)).collect();
    assert!(kinds.contains(&&DirKind::Tempo { text: Some("Andante".into()), metronome: Some((NoteType { log: 2, dots: 0 }, "72".into())) }));
    assert!(kinds.contains(&&DirKind::Dynamic("p".into())));
    assert!(kinds.contains(&&DirKind::Ottava(1)) && kinds.contains(&&DirKind::Ottava(0)));
    assert!(kinds.contains(&&DirKind::Pedal(Pedal::Start)) && kinds.contains(&&DirKind::Pedal(Pedal::Stop)));
    let wedges: Vec<Wedge> = kinds.iter().filter_map(|k| if let DirKind::Wedge(w) = k { Some(*w) } else { None }).collect();
    assert_eq!(wedges, [Wedge::Cresc, Wedge::Stop, Wedge::Dim, Wedge::Stop]);
    let pedal_staff: Vec<u32> = p.measures[0].directions.iter().filter(|d| matches!(d.kind, DirKind::Pedal(_))).map(|d| d.staff).collect();
    assert_eq!(pedal_staff, [2, 2]);
}

#[test]
fn song_with_repeat_and_endings() {
    let (s, d) = read(include_str!("../../tests/ly/song.ly"));
    no_drops(&d);
    let p = &s.parts[0];
    assert_eq!(p.name, "Voice");
    assert_eq!(p.abbr.as_deref(), Some("V."));
    assert_eq!(p.measures.len(), 10);
    let m = &p.measures;
    assert!(m[0].implicit && m[0].len == q(1));
    assert!(m[1].left.repeat);
    assert_eq!(m[4].left.ending_start.as_ref().map(|e| e.number.as_str()), Some("1"));
    assert!(m[4].right.repeat && m[4].right.ending_stop.is_some());
    assert_eq!(m[5].left.ending_start.as_ref().map(|e| e.number.as_str()), Some("2"));
    assert!(m[5].right.ending_stop.is_some() && !m[5].right.repeat);
    assert!(m[6].new_system);
    assert!(m[6].directions.iter().any(|d| d.kind == DirKind::Rehearsal("A".into())));
    assert_eq!(m[9].right.style.as_deref(), Some("light-heavy"));
    let v = voice(p, 1, 0);
    assert_eq!(
        words(&v, 1),
        "Come and sit be-side the fi-re, tell a sto-ry. Come ry. Night is fall-ing soft-ly o-ver."
    );
    // melismas: the slur and the ties take no syllables; extenders are kept
    let tell = v.iter().find(|(_, e)| e.lyrics.iter().any(|l| l.text == "tell")).unwrap().1;
    assert!(tell.lyrics[0].extend);
    let n_syll: usize = v.iter().map(|(_, e)| e.lyrics.len()).sum();
    let n_notes = v.iter().filter(|(_, e)| !e.is_rest()).count();
    assert_eq!(n_syll, 22);
    assert_eq!(n_notes, 25);
}

#[test]
fn variables_transpose_and_layout() {
    let (s, d) = read(include_str!("../../tests/ly/variables.ly"));
    no_drops(&d);
    assert_eq!(s.header.title.as_deref(), Some("Variations"));
    assert_eq!(s.header.subtitle.as_deref(), Some("on three notes"));
    assert_eq!(s.header.composer_lines, ["Music by A. Tester", "*arr. B. Tester*"]);
    assert_eq!(s.header.composer.as_deref(), Some("Music by A. Tester"));
    assert_eq!(s.header.copyright.as_deref(), Some("Public domain"));
    assert_eq!(s.layout.staff_size, Some(18.0));
    assert_eq!(s.layout.page_width_mm, Some(210.0));
    assert_eq!(s.layout.first_page, Some(3));
    assert!(s.layout.hide_empty);
    assert_eq!(s.layout.bar_numbers, Some(false));
    assert_eq!(s.parts.len(), 2);
    let fl = &s.parts[0];
    assert_eq!(fl.abbr.as_deref(), Some("Fl."));
    assert_eq!(s.parts[1].display_name.as_deref(), Some("Cello\n(or Bassoon)"));
    assert_eq!(fl.measures.len(), 8);
    let v = voice(fl, 1, 0);
    let ps = pitches(&v);
    assert_eq!(ps[..6], ["C5", "D5", "E5", "E5", "D5", "C5"]);
    // \repeat unfold 2 { \motif }
    assert_eq!(ps[6..12], ["C5", "D5", "E5", "C5", "D5", "E5"]);
    // \transpose c d
    assert_eq!(ps[12..18], ["D3", "E3", "F#3", "F#3", "E3", "D3"]);
    // meter change with a double bar before it
    assert_eq!(fl.measures[3].right.style.as_deref(), Some("light-light"));
    assert!(fl.measures[4].attrs.iter().any(|a| matches!(&a.kind, Attr::Time { beats, beat_type: 4, .. } if beats == "3")));
    assert_eq!(fl.measures[4].len, q(3));
    assert!(fl.measures[0].attrs.iter().any(|a| matches!(&a.kind, Attr::Time { symbol: Some(TimeSymbol::Cut), .. })));
    assert!(fl.measures[3].directions.iter().any(|d| d.kind == DirKind::Words { text: "dolce".into(), italic: true, bold: false }));
    // cello: R2.*2 becomes two whole-bar rests
    let vc = voice(&s.parts[1], 1, 0);
    let rests: Vec<usize> = vc.iter().filter(|(_, e)| matches!(e.kind, EventKind::Rest { whole_measure: true })).map(|x| x.0).collect();
    assert_eq!(rests, [5, 6]);
    assert_eq!(pitches(&vc)[..3], ["C3", "G2", "C3"]);
}

#[test]
fn english_names_and_roadmap() {
    let (s, d) = read(include_str!("../../tests/ly/english.ly"));
    no_drops(&d);
    let p = &s.parts[0];
    assert_eq!(p.measures.len(), 8);
    assert!(p.measures[0].attrs.iter().any(|a| a.kind == Attr::Key { fifths: -2, minor: false }));
    let v = voice(p, 1, 0);
    let ps = pitches(&v);
    assert_eq!(ps[..6], ["Bb4", "C5", "D5", "Eb5", "F5", "C#5"]);
    assert_eq!(ps[7..10], ["Bb3+D4+F4", "Bb3+D4+F4", "F#4"]);
    assert_eq!(ps[13..17], ["F##4", "Gb4", "F4", "E4"]);
    // forced and cautionary accidentals
    assert!(v[15].1.notes()[0].acc_shown && !v[15].1.notes()[0].acc_caut);
    assert!(v[16].1.notes()[0].acc_caut);
    // a tie on one note of a chord
    let chord = v[17].1.notes();
    assert_eq!(chord.iter().map(|n| n.tie_start).collect::<Vec<_>>(), [true, false, false]);
    assert!(v[18].1.notes()[0].tie_stop);
    assert!(v[12].1.arts.contains(&Art::Breath));
    // roadmap
    let seg = p.measures[1].directions.iter().find(|d| d.kind == DirKind::Segno).unwrap();
    assert_eq!(seg.pos, Frac::new(3, 8));
    let ds = p.measures[6].directions.iter().find(|d| matches!(&d.kind, DirKind::Words { text, .. } if text == "D.S. al Fine")).unwrap();
    assert_eq!(ds.pos, p.measures[6].len);
    assert_eq!(p.measures[6].right.style.as_deref(), Some("light-light"));
    assert!(p.measures[7].directions.iter().any(|d| matches!(&d.kind, DirKind::Words { text, .. } if text == "Fine")));
    assert_eq!(p.measures[7].right.style.as_deref(), Some("light-heavy"));
    assert!(p.measures[0].directions.iter().any(|d| d.kind == DirKind::Tempo { text: None, metronome: Some((NoteType { log: 2, dots: 1 }, "60".into())) }));
}

#[test]
fn relative_octaves() {
    let (s, _) = read("\\relative { c' e g c  b, f' a, d''4 <c, e g>2 f }");
    let v = voice(&s.parts[0], 1, 0);
    assert_eq!(pitches(&v), ["C4", "E4", "G4", "C5", "B3", "F4", "A3", "D6", "C5+E5+G5", "F5"]);
    let (s, _) = read("\\relative c'' { c4 \\relative c { c d } g }  \\fixed c' { c g, }");
    let v = voice(&s.parts[0], 1, 0);
    assert_eq!(pitches(&v)[..4], ["C5", "C3", "D3", "G4"]);
}

#[test]
fn deutsch_and_italiano() {
    let (s, _) = read("\\language \"deutsch\" { h4 b as es fis }");
    assert_eq!(pitches(&voice(&s.parts[0], 1, 0)), ["B3", "Bb3", "Ab3", "Eb3", "F#3"]);
    let (s, _) = read("\\include \"italiano.ly\" { do'4 re' mib' fad' sol' sib' }");
    assert_eq!(pitches(&voice(&s.parts[0], 1, 0)), ["C4", "D4", "Eb4", "F#4", "G4", "Bb4"]);
}

#[test]
fn durations_and_bars() {
    let (s, _) = read("{ \\time 3/4 c'4. d'8 e'4 | f'2 g'8. a'16 | b'\\breve*3/8 | c''4 4 4 }");
    let p = &s.parts[0];
    assert_eq!(p.measures.len(), 4);
    let v = voice(p, 1, 0);
    let durs: Vec<Frac> = v.iter().map(|x| x.1.dur).collect();
    assert_eq!(durs[..7], [Frac::new(3, 8), Frac::new(1, 8), q(1), q(2), Frac::new(3, 16), Frac::new(1, 16), q(3)]);
    assert_eq!(pitches(&v)[7..], ["C5", "C5", "C5"]);
    assert_eq!(v[0].1.ntype, Some(NoteType { log: 2, dots: 1 }));
}

#[test]
fn note_across_barline_is_split() {
    let (s, d) = read("{ \\time 2/4 c'4 d'2 e'4 }");
    let p = &s.parts[0];
    assert_eq!(p.measures.len(), 2);
    let v = voice(p, 1, 0);
    assert_eq!(pitches(&v), ["C4", "D4", "D4", "E4"]);
    assert!(v[1].1.notes()[0].tie_start && v[2].1.notes()[0].tie_stop);
    assert!(d.list.iter().any(|x| x.level == Level::Warning));
}

#[test]
fn volta_with_new_syntax_and_three_times() {
    let src = "{ \\repeat volta 3 { c'1 \\alternative { \\volta 1,2 { d'1 } \\volta 3 { e'1 } } } f'1 \\repeat volta 4 { g'1 } }";
    let (s, _) = read(src);
    let m = &s.parts[0].measures;
    assert_eq!(m.len(), 5);
    assert_eq!(m[1].left.ending_start.as_ref().unwrap().number, "1, 2");
    assert!(m[1].right.repeat);
    assert_eq!(m[2].left.ending_start.as_ref().unwrap().number, "3");
    assert!(m[4].left.repeat && m[4].right.repeat);
    assert_eq!(m[4].right.times, Some(4));
}

#[test]
fn literal_repeat_commands_and_mid_bar_barline() {
    let src = "{ \\time 4/4 c'1 \\set Score.repeatCommands = #'((volta \"1.\")) d'1 \\bar \":|.\" \\set Score.repeatCommands = #'((volta #f) (volta \"2.\")) e'1 \
               \\set Score.repeatCommands = #'((volta #f)) f'2 \\bar \"||\" g'2 a'1 }";
    let (s, _) = read(src);
    let m = &s.parts[0].measures;
    assert_eq!(m.len(), 6);
    assert_eq!(m[1].left.ending_start.as_ref().unwrap().text.as_deref(), Some("1."));
    assert!(m[1].right.repeat && m[1].right.ending_stop.is_some());
    assert_eq!(m[2].left.ending_start.as_ref().unwrap().number, "2");
    // the bar split by a bar line in the middle: two short bars, the second not counted
    assert_eq!((m[3].len, m[4].len), (q(2), q(2)));
    assert_eq!(m[3].right.style.as_deref(), Some("light-light"));
    assert!(m[4].implicit);
    assert_eq!(m[5].number, "5");
}

#[test]
fn melisma_rules() {
    // ties, slurs and manual beams make melismas; phrasing slurs do not
    let src = "<< \\new Voice = \"v\" { c'4~ c'8[ d'8] e'4( f') g'\\( a'\\) } \\new Lyrics \\lyricsto \"v\" { a b c d } >>";
    let (s, d) = read(src);
    no_drops(&d);
    let v = voice(&s.parts[0], 1, 0);
    let has: Vec<bool> = v.iter().map(|x| !x.1.lyrics.is_empty()).collect();
    assert_eq!(has, [true, false, false, true, false, true, true]);
    // with melismaBusyProperties emptied every note takes a syllable or `_`
    let src = "<< \\new Voice = \"v\" { \\set melismaBusyProperties = #'() c'4~ c'4 e'4( f') } \\new Lyrics \\lyricsto \"v\" { a _ b c } >>";
    let (s, _) = read(src);
    let v = voice(&s.parts[0], 1, 0);
    let has: Vec<bool> = v.iter().map(|x| !x.1.lyrics.is_empty()).collect();
    assert_eq!(has, [true, false, true, true]);
}

#[test]
fn words_are_never_dropped_silently() {
    let (_, d) = read("<< \\new Voice = \"v\" { c'4 d' } \\new Lyrics \\lyricsto \"v\" { one two three four } >>");
    assert_eq!(d.n_dropped(), 1);
    let (_, d) = read("<< \\new Voice = \"v\" { c'4 d' } \\new Lyrics \\lyricsto \"w\" { one two } >>");
    assert_eq!(d.n_dropped(), 1);
    let (_, d) = read("<< \\new FiguredBass \\figuremode { <6>1 } \\new Staff { c'1 } >>");
    assert!(d.n_dropped() >= 1);
}

#[test]
fn syntax_errors_have_line_numbers() {
    let mut d = Diags::default();
    let e = read_ly("\\score {\n  { c'4 d'\n", None, &mut d).unwrap_err();
    assert!(e.starts_with("line "), "{}", e);
    let e = read_ly("{ c'4 <e' g' }", None, &mut d).unwrap_err();
    assert!(e.contains("line 1"), "{}", e);
    assert!(read_ly("", None, &mut d).is_err());
    // garbage must not panic
    for junk in ["}}}}", "<< \\\\ >>", "\\relative", "{ c'4 \\tuplet }", "#(", "\\new Staff = ", "{ \\repeat volta }", "x = { c \\x }", "{ <<<>>> }"] {
        let _ = read_ly(junk, None, &mut d);
    }
}

#[test]
fn overrides_and_scheme_are_skipped() {
    let src = "#(define (f x) (* x 2))\n\
               { \\override Staff.NoteHead.color = #(rgb-color 1 0 0) \\once \\override Stem #'thickness = #2.0 c'4 \
                 \\tweak NoteHead.color #red d'4 \\set Staff.midiInstrument = \"flute\" \\stemUp e'4 \\revert Staff.NoteHead.color f'4 }";
    let (s, d) = read(src);
    no_drops(&d);
    assert_eq!(pitches(&voice(&s.parts[0], 1, 0)), ["C4", "D4", "E4", "F4"]);
}

#[test]
fn chord_symbols() {
    let src = "<< \\new ChordNames \\chordmode { c2 g:7 | a:m7.5-/c d4:sus4 r4 | ees1:maj7.11+^5 } \\new Staff { c'1 c'1 c'1 } >>";
    let (s, d) = read(src);
    no_drops(&d);
    let m = &s.parts[0].measures;
    let kinds: Vec<(&str, (u8, i8), Frac)> = m.iter().flat_map(|m| m.harmonies.iter().map(|h| (h.kind.as_str(), h.root, h.pos))).collect();
    assert_eq!(
        kinds,
        [
            ("major", (0, 0), q(0)),
            ("dominant", (4, 0), q(2)),
            ("half-diminished", (5, 0), q(0)),
            ("suspended-fourth", (1, 0), q(2)),
            ("none", (0, 0), q(3)),
            ("major-seventh", (2, -1), q(0))
        ]
    );
    assert_eq!(m[1].harmonies[0].bass, Some((0, 0)));
    assert_eq!(m[2].harmonies[0].degrees, [(11, 1, 'a'), (5, 0, 's')]);
}

#[test]
fn invisible_bar_line_splits_a_bar() {
    let (s, _) = read("{ \\time 4/4 \\partial 4. e'4 e'8 | f'4 g'4 \\bar \"\" a'4 b'4 | c''4 d''4 r2 }");
    let m = &s.parts[0].measures;
    assert_eq!(m.iter().map(|m| m.len).collect::<Vec<_>>(), [Frac::new(3, 8), q(2), q(2), q(4)]);
    assert_eq!(m[1].right.style.as_deref(), Some("none"));
    assert!(m[0].implicit && !m[1].implicit && m[2].implicit && !m[3].implicit);
    assert_eq!(m[3].number, "2");
}

#[test]
fn fast_enough() {
    // about 3000 notes with lyrics
    let mut src = String::from("\\score { << \\new Staff \\new Voice = \"v\" \\relative c' { \\time 4/4 ");
    for _ in 0..375 {
        src.push_str("c8 d e f g( a) b c | ");
    }
    src.push_str("} \\new Lyrics \\lyricsto \"v\" { ");
    for _ in 0..375 {
        src.push_str("la la la la la __ la la ");
    }
    src.push_str("} >> }");
    let mut d = Diags::default();
    let _ = read_ly(&src, None, &mut d).unwrap(); // warm up
    let t = std::time::Instant::now();
    let s = read_ly(&src, None, &mut d).unwrap();
    let dt = t.elapsed();
    assert_eq!(s.parts[0].measures.len(), 375);
    // generous for unoptimised test builds; the release binary takes about a millisecond
    assert!(dt.as_millis() < 500, "took {:?}", dt);
}
