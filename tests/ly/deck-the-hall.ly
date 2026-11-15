\version "2.24.0"
% Deck the Hall: Welsh traditional tune (Nos Galan), words by Thomas Oliphant (1862).
% Public domain. Arranged for the lyxml tests as an open score: four staves, tenor in
% octave-treble clef, a repeat with first and second endings sung to two verses, a tie
% into the second ending, system and page breaks, credit lines, no bar numbers.

\header {
  title = "Deck the Hall"
  poet = "Thomas Oliphant, 1862"
  composer = \markup \right-column { "Welsh traditional" \italic "arr. for the lyxml tests" }
  tagline = ##f
}

\paper {
  #(set-paper-size "letter")
}

sopranoMusic = {
  \key f \major \time 4/4 \tempo "Allegro" 4 = 132
  \repeat volta 2 {
    c''4.\f bes'8 a'4 g' |
    f'4 g' a' f' |
    g'8 a' bes' g' a'4. g'8 |
  }
  \alternative {
    { f'4 e' f'2 | }
    { f'4 e' f'2 | }
  }
  \bar "||" \break
  g'4.\mf\< a'8 bes'4 g' |
  a'4. bes'8 c''4 g'\! |
  a'8 b' c''4 d''8 e'' f''4 |
  e''4 d'' c''2 |
  \pageBreak
  c''4.\f bes'8 a'4 g' |
  f'4 g' a' f' |
  d''8^\markup \italic "rit." d'' d'' d'' c''4. bes'8 |
  a'4 g' f'2\fermata |
  \bar "|."
}

altoMusic = {
  \key f \major
  \repeat volta 2 {
    a'4. g'8 f'4 e' |
    c'4 e' f' c' |
    e'8 f' g' e' f'4. e'8 |
  }
  \alternative {
    { c'4 c' c'2 | }
    { c'4 c' c'2 | }
  }
  e'4. f'8 g'4 e' |
  f'4. f'8 f'4 e' |
  f'8 f' g'4 a'8 g' a'4 |
  g'4 f' e'2 |
  a'4. g'8 f'4 e' |
  c'4 e' f' c' |
  f'8 f' f' f' f'4. f'8 |
  f'4 e' c'2\fermata |
}

tenorMusic = {
  \clef "treble_8" \key f \major
  \repeat volta 2 {
    c'4. c'8 c'4 bes |
    a4 c' c' a |
    c'8 c' c' c' c'4. bes8 |
  }
  \alternative {
    { a4 g a2 | }
    { a4 g a2 | }
  }
  c'4. c'8 c'4 c' |
  c'4. d'8 c'4 c' |
  c'8 d' e'4 f'8 e' d'4 |
  c'4 b c'2 |
  c'4. c'8 c'4 bes |
  a4 c' c' a |
  bes8 bes bes bes a4. d'8 |
  c'4 bes a2\fermata |
}

bassMusic = {
  \clef bass \key f \major
  \repeat volta 2 {
    f4. f8 f4 c |
    f4 c f f |
    c8 c c c f4. c8~ |
  }
  \alternative {
    { c4 c f2 | }
    { c4\repeatTie c f2 | }
  }
  c4. c8 c4 c |
  f4. f8 a4 c' |
  f8 d c4 f8 c d4 |
  g4 g, c2 |
  f4. f8 f4 c |
  f4 c f f |
  bes,8 bes, bes, bes, c4. c8 |
  c4 c f,2\fermata |
}

verseOne = \lyricmode {
  \set stanza = "1."
  Deck the hall with boughs of hol -- ly,
  Fa la la la la, la la la la.
  _ _ _
  Don we now our gay ap -- par -- el,
  Fa la la, la la la, la la la.
  Troll the an -- cient Yule -- tide car -- ol,
  Fa la la la la, la la la la.
}

verseTwo = \lyricmode {
  \set stanza = "2."
  "’Tis" the sea -- son to be jol -- ly,
  Fa la la la la, la
  _ _ _
  la la la.
}

\score {
  \new ChoirStaff <<
    \new Staff \with { instrumentName = "Soprano" } \new Voice = "sop" \sopranoMusic
    \new Lyrics \lyricsto "sop" \verseOne
    \new Lyrics \lyricsto "sop" \verseTwo
    \new Staff \with { instrumentName = "Alto" } \new Voice = "alt" \altoMusic
    \new Lyrics \lyricsto "alt" \verseOne
    \new Lyrics \lyricsto "alt" \verseTwo
    \new Staff \with { instrumentName = "Tenor" } \new Voice = "ten" \tenorMusic
    \new Lyrics \lyricsto "ten" \verseOne
    \new Lyrics \lyricsto "ten" \verseTwo
    \new Staff \with { instrumentName = "Bass" } \new Voice = "bas" \bassMusic
  >>
  \layout {
    \context { \Score \omit BarNumber }
  }
  \midi { }
}
