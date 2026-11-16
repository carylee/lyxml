# lyxml

Fast, faithful conversion between **MusicXML** (`.musicxml`, `.xml`, `.mxl`) and
**LilyPond** (`.ly`), in both directions, as one static binary.

```
lyxml score.mxl -o score.ly          # MusicXML -> LilyPond
lyxml score.ly  -o score.musicxml    # LilyPond -> MusicXML (.mxl for compressed)
lyxml diff a.mxl b.ly                # compare two scores musically, any formats
cat score.musicxml | lyxml - --to ly > score.ly
```

It replaces `musicxml2ly` plus the pile of regex repairs that used to follow it.

## Why

`musicxml2ly` invents repeats around second endings, hangs on some numbered endings,
crashes on a `"` in the words, loses page breaks, text tempos, short staff names and
the 8 of a tenor clef, makes up verse numbers, and runs extender lines through rests.
`lyxml` is built on two rules:

1. **Never invent structure.** Repeat signs, volta brackets, double bars, system and
   page breaks are written exactly where the source has them, as literal bar lines and
   brackets. Nothing is inferred, so nothing can be inferred wrongly.
2. **Never drop anything silently.** Every element that is not carried over is reported
   with its part and bar (`--json` for machines, `--strict` to fail on any loss).

## What the LilyPond looks like

- one bar per line, with a bar check and the source's bar number: `c'4 d'4 e'2 | % 12`
- absolute pitches; one variable per voice (`SopranoMusic`, `PianoUpperVoiceOneMusic`)
  and per verse (`SopranoMusicVerseOne`)
- a `Structure` variable of skips that carries what belongs to the whole score: time
  signatures, pickups, bar lines, repeats and endings, breaks, tempo, rehearsal marks,
  segno / coda / D.S. text, bar numbers
- words aligned explicitly: melisma guessing is off, every note takes a syllable or
  `_`, so a word can never slide onto the wrong note; extender lines stop at rests
- the same input always gives byte-identical output, so results can be kept as goldens

Carried over from MusicXML: notes, rests, chords, ties (including ties into second
endings and laissez-vibrer), tuplets, grace and after-grace notes, several voices per
staff, multi-staff parts, clefs (with octave marks), keys, time signatures (hidden,
common/cut, additive), pickups, split and free bars, printed bar numbers, slurs,
explicit beams, articulations, ornaments, fermatas, fingerings, tremolo, breath marks,
dynamics, hairpins (also on rests and inside long notes), text, tempo and metronome
marks, rehearsal marks, segno/coda, ottava, pedal, lyrics with verses, hyphens,
extenders, elisions and verse numbers, staff names and short names, brackets and braces,
title block and credits, page size, staff size, first page number, hidden empty staves.

Chord symbols are carried over as `\chordmode`.

`lyxml diff A B` compares notes, rhythm, words, bar lines, endings, breaks and marks;
`--notes` leaves the words out, `--words` shows only word differences. `lyxml notes IN`
lists every sounding note (used by `tests/midi_check.py`, which compares a conversion
with LilyPond's own MIDI reading of it).

Not carried over (each reported when met): figured bass, tablature
details, transposition of transposing parts (written pitches are kept), two-note
tremolo, cross-staff beams, microtones, a few rare technical marks.

## LilyPond input

The reader parses the LilyPond language rather than one tool's output: variables,
`\relative`, `\fixed`, `\transpose`, `\repeat volta` / `\alternative`, `<< \\ >>`,
`\new Staff/Voice/PianoStaff/ChoirStaff/Lyrics`, `\lyricsto`, `\addlyrics`, tuplets,
grace notes, chords, markup, `\include` and the common note-name languages. Scheme code
is skipped (and reported) except for the handful of forms that carry score content.

## Options

```
--from xml|ly, --to ly|xml|mxl   formats, when the file names do not tell
--json                           diagnostics as JSON on stderr
--strict                         exit 2 when anything was dropped
-q                               no diagnostics
--paper letter|a4|octavo|remarkable|WxH(mm)
--staff-size PT   --hide-empty   --no-bar-numbers
--breaks keep|pages|ignore       the file's breaks: all, only page breaks, or none
--page-breaks 10,20              start a new page at these bar numbers as well
--hide-shared-lyrics             words a staff shares with the staff above print once
--title-style smallcaps          title in large small capitals
--text-font Times                serif text font
--beams file|auto                the file's beams, or LilyPond's
--midi                           add a \midi block
```

Page size, staff size, first page number, `hide-empty` and `bar-numbers` are read from
the file (`<defaults>`, `<miscellaneous-field>`) when the flags are absent.

## Install

Download a binary for Linux, macOS or Windows from the
[releases page](https://github.com/carylee/lyxml/releases), or build it:

```
cargo install --git https://github.com/carylee/lyxml     # needs a Rust toolchain
```

The LilyPond it writes is checked with LilyPond 2.24 and 2.26; the same file compiles
on both.

## Build

```
cargo build --release        # target/release/lyxml
cargo test
```

Exit codes: 0 success, 1 unusable input or bad arguments, 2 something was dropped
under `--strict`, 3 an internal error (please report it with the input file).

## Checks

- `tests/corpus.sh` converts every local score to LilyPond, compiles it with LilyPond,
  converts the LilyPond back and compares with `lyxml diff`.
- `tests/xmlrt.sh` checks MusicXML -> model -> MusicXML.
- `tests/ly/` holds hand-written LilyPond fixtures for the reader.
- `tests/lyrt.sh` checks MusicXML -> LilyPond -> MusicXML; `tests/midi_all.sh` compares
  each conversion with LilyPond's own MIDI reading of it.

- `tests/xmlfuzz.py` and `tests/lyfuzz.py` feed damaged MusicXML and LilyPond to the
  binary: it must never crash or hang.

The scripts run on `tests/data` by default. Pass your own files as arguments
(`tests/lyrt.sh path/to/*.mxl`) or list them in `LYXML_CORPUS`; set `LILYPOND` to test
against another LilyPond (`LILYPOND=/opt/lilypond-2.26.0/bin/lilypond tests/corpus.sh out`).
All of this runs in CI on every push.

## Test music

Everything under `tests/` is public domain or written for these tests:

- `deck-the-hall` — Welsh traditional tune, words by Thomas Oliphant (1862), arranged
  for the tests as an SATB open score with a repeat, two endings and two verses
  (`tests/ly/deck-the-hall.ly` is the source; `tests/data/deck-the-hall.mxl` is lyxml's
  conversion of it).
- `old_hundredth` — the Genevan psalm tune (1551).
- `features` — a notation feature test, including bars from Viardot's *Cendrillon* (1904).
- `notation-extras` and the other files in `tests/ly/` — invented test material.
- `45d`, `46d`, `61k`, `71a` — from the unofficial MusicXML test suite (MIT licence),
  as distributed with LilyPond.

## Licence

MIT; see `LICENSE`.
