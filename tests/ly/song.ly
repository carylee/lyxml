\version "2.24.0"
% Lead vocal: pickup, \repeat volta with two endings, melismas by slur and tie,
% \lyricsto with hyphens and extenders, a rehearsal mark and a line break.

\header {
  title = "Round the Fire"
  composer = "Test"
}

melody = \relative c' {
  \key f \major
  \time 4/4
  \partial 4
  c4 |
  \repeat volta 2 {
    f4 f g a | bes2 a4 g | a4( g) f e |
  }
  \alternative {
    { f2. c4 | }
    { f2. r4 | }
  }
  \break
  \mark \default
  c'2 a4 f | g2 ~ g8 f e d | c1 ~ | c2. r4 \bar "|."
}

words = \lyricmode {
  Come and sit be -- side the fi -- re, tell __ a sto -- ry. Come
  ry.
  Night is fall -- ing soft -- ly o -- ver. __
}

\score {
  <<
    \new Staff \with { instrumentName = "Voice" shortInstrumentName = "V." } {
      \new Voice = "lead" { \autoBeamOff \melody }
    }
    \new Lyrics \lyricsto "lead" \words
  >>
  \layout { }
}
