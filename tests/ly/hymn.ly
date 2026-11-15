\version "2.24.0"
% SATB hymn on two staves: \relative, a shared global, two verses with \addlyrics.

\header {
  title = "Evening Hymn"
  composer = "Trad."
  poet = "Anon."
  tagline = ##f
}

global = {
  \key g \major
  \time 4/4
  \partial 4
}

soprano = \relative c'' {
  g4 | g fis e d | g a b\fermata b |
  b b a g | c b a\fermata g |
  a( b) c b | a2 g4 \bar "|."
}

alto = \relative c' {
  d4 | d d b a | d d d d |
  d d d d | e d d d |
  d2 e4 d | d( c) b
}

tenor = \relative c' {
  b4 | b a g fis | g fis g g |
  g g fis g | g g fis b |
  a( g) g g | g( fis) g
}

bass = \relative c' {
  g4 | g d e fis | b, d g, g' |
  g g d e | c g d' g |
  fis( g) c, g | d'2 g,4
}

\score {
  \new ChoirStaff <<
    \new Staff = "women" \with { instrumentName = "S A" } <<
      \new Voice = "sopranos" { \voiceOne \global \soprano }
      \new Voice = "altos" { \voiceTwo \global \alto }
    >>
    \addlyrics {
      \set stanza = "1."
      Now all the woods are sleep -- ing, and
      night and still -- ness creep -- ing on
      vil -- lage, field and town.
    }
    \addlyrics {
      \set stanza = "2."
      The sun has gone in glo -- ry; the
      stars now tell their sto -- ry, and
      qui -- et set -- tles down.
    }
    \new Staff = "men" \with { instrumentName = "T B" } <<
      \clef bass
      \new Voice = "tenors" { \voiceOne \global \tenor }
      \new Voice = "basses" { \voiceTwo \global \bass }
    >>
  >>
  \layout { }
}
