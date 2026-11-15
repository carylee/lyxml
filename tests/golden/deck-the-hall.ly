\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  title = "Deck the Hall"
  poet = "Thomas Oliphant, 1862"
  composer = \markup \right-column { "Welsh traditional" \italic "arr. for the lyxml tests" }
  tagline = ##f
}

\paper {
  #(set-paper-size "letter")
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \autoLineBreaksOff
  \autoPageBreaksOff
  \time 4/4 \tempo "Allegro" 4 = 132 s1 | % 1
  s1 | % 2
  s1 | % 3
  \set Score.repeatCommands = #'((volta "1.")) s1 | % 4
  \bar ":|." \set Score.repeatCommands = #'((volta #f) (volta "2.")) s1 | % 5
  \break \bar "||" \set Score.repeatCommands = #'((volta #f)) s1 | % 6
  s1 | % 7
  s1 | % 8
  s1 | % 9
  \pageBreak s1 | % 10
  s1 | % 11
  s1 | % 12
  s1 \bar "|." % 13
}

SopranoMusic = {
  \key f \major \clef "treble" c''4.\f bes'8 a'4 g'4 | % 1
  f'4 g'4 a'4 f'4 | % 2
  g'8 a'8 bes'8 g'8 a'4. g'8 | % 3
  f'4 e'4 f'2 | % 4
  f'4 e'4 f'2 | % 5
  g'4.\mf\< a'8 bes'4 g'4 | % 6
  a'4. bes'8 c''4 g'4\! | % 7
  a'8 b'8 c''4 d''8 e''8 f''4 | % 8
  e''4 d''4 c''2 | % 9
  c''4.\f bes'8 a'4 g'4 | % 10
  f'4 g'4 a'4 f'4 | % 11
  d''8^\markup { \italic "rit." } d''8 d''8 d''8 c''4. bes'8 | % 12
  a'4 g'4 f'2\fermata % 13
}

SopranoMusicVerseOne = \lyricmode {
  \set stanza = "1." Deck the hall with % 1
  boughs of hol -- ly, % 2
  Fa la la la la, la % 3
  la la la. % 4
  _ _ _ % 5
  Don we now our % 6
  gay ap -- par -- el, % 7
  Fa la la, la la la, % 8
  la la la. % 9
  Troll the an -- cient % 10
  Yule -- tide car -- ol, % 11
  Fa la la la la, la % 12
  la la la. % 13
}

SopranoMusicVerseTwo = \lyricmode {
  \set stanza = "2." ’Tis the sea -- son % 1
  to be jol -- ly, % 2
  Fa la la la la, la % 3
  _ _ _ % 4
  la la la. % 5
}

AltoMusic = {
  \key f \major \clef "treble" a'4. g'8 f'4 e'4 | % 1
  c'4 e'4 f'4 c'4 | % 2
  e'8 f'8 g'8 e'8 f'4. e'8 | % 3
  c'4 c'4 c'2 | % 4
  c'4 c'4 c'2 | % 5
  e'4. f'8 g'4 e'4 | % 6
  f'4. f'8 f'4 e'4 | % 7
  f'8 f'8 g'4 a'8 g'8 a'4 | % 8
  g'4 f'4 e'2 | % 9
  a'4. g'8 f'4 e'4 | % 10
  c'4 e'4 f'4 c'4 | % 11
  f'8 f'8 f'8 f'8 f'4. f'8 | % 12
  f'4 e'4 c'2\fermata % 13
}

AltoMusicVerseOne = \lyricmode {
  \set stanza = "1." Deck the hall with % 1
  boughs of hol -- ly, % 2
  Fa la la la la, la % 3
  la la la. % 4
  _ _ _ % 5
  Don we now our % 6
  gay ap -- par -- el, % 7
  Fa la la, la la la, % 8
  la la la. % 9
  Troll the an -- cient % 10
  Yule -- tide car -- ol, % 11
  Fa la la la la, la % 12
  la la la. % 13
}

AltoMusicVerseTwo = \lyricmode {
  \set stanza = "2." ’Tis the sea -- son % 1
  to be jol -- ly, % 2
  Fa la la la la, la % 3
  _ _ _ % 4
  la la la. % 5
}

TenorMusic = {
  \key f \major \clef "treble_8" c'4. c'8 c'4 bes4 | % 1
  a4 c'4 c'4 a4 | % 2
  c'8 c'8 c'8 c'8 c'4. bes8 | % 3
  a4 g4 a2 | % 4
  a4 g4 a2 | % 5
  c'4. c'8 c'4 c'4 | % 6
  c'4. d'8 c'4 c'4 | % 7
  c'8 d'8 e'4 f'8 e'8 d'4 | % 8
  c'4 b4 c'2 | % 9
  c'4. c'8 c'4 bes4 | % 10
  a4 c'4 c'4 a4 | % 11
  bes8 bes8 bes8 bes8 a4. d'8 | % 12
  c'4 bes4 a2\fermata % 13
}

TenorMusicVerseOne = \lyricmode {
  \set stanza = "1." Deck the hall with % 1
  boughs of hol -- ly, % 2
  Fa la la la la, la % 3
  la la la. % 4
  _ _ _ % 5
  Don we now our % 6
  gay ap -- par -- el, % 7
  Fa la la, la la la, % 8
  la la la. % 9
  Troll the an -- cient % 10
  Yule -- tide car -- ol, % 11
  Fa la la la la, la % 12
  la la la. % 13
}

TenorMusicVerseTwo = \lyricmode {
  \set stanza = "2." ’Tis the sea -- son % 1
  to be jol -- ly, % 2
  Fa la la la la, la % 3
  _ _ _ % 4
  la la la. % 5
}

BassMusic = {
  \key f \major \clef "bass" f4. f8 f4 c4 | % 1
  f4 c4 f4 f4 | % 2
  c8 c8 c8 c8 f4. c8~ | % 3
  c4 c4 f2 | % 4
  c4\repeatTie c4 f2 | % 5
  c4. c8 c4 c4 | % 6
  f4. f8 a4 c'4 | % 7
  f8 d8 c4 f8 c8 d4 | % 8
  g4 g,4 c2 | % 9
  f4. f8 f4 c4 | % 10
  f4 c4 f4 f4 | % 11
  bes,8 bes,8 bes,8 bes,8 c4. c8 | % 12
  c4 c4 f,2\fermata % 13
}

\score {
  <<
    \new ChoirStaff <<
      \new Staff = "P1-1" \with { instrumentName = "Soprano" } <<
        \Structure
        \new Voice = "SopranoMusic" { \set melismaBusyProperties = #'() \SopranoMusic }
      >>
      \new Lyrics \lyricsto "SopranoMusic" { \SopranoMusicVerseOne }
      \new Lyrics \lyricsto "SopranoMusic" { \SopranoMusicVerseTwo }
      \new Staff = "P2-1" \with { instrumentName = "Alto" } <<
        \new Voice = "AltoMusic" { \set melismaBusyProperties = #'() \AltoMusic }
      >>
      \new Lyrics \lyricsto "AltoMusic" { \AltoMusicVerseOne }
      \new Lyrics \lyricsto "AltoMusic" { \AltoMusicVerseTwo }
      \new Staff = "P3-1" \with { instrumentName = "Tenor" } <<
        \new Voice = "TenorMusic" { \set melismaBusyProperties = #'() \TenorMusic }
      >>
      \new Lyrics \lyricsto "TenorMusic" { \TenorMusicVerseOne }
      \new Lyrics \lyricsto "TenorMusic" { \TenorMusicVerseTwo }
      \new Staff = "P4-1" \with { instrumentName = "Bass" } <<
        \new Voice = "BassMusic" { \BassMusic }
      >>
    >>
  >>
  \layout {
    indent = 16.3\mm
    \context { \Lyrics \override LyricHyphen.minimum-distance = #1.2 }
    \context { \Score \omit BarNumber }
  }
}
