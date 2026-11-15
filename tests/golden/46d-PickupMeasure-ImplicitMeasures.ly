\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  tagline = ##f
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \time 4/4 \partial 4. s4. | % 0
  s2 % 1
  \bar "" s2 | % X1
  s2. \bar "|." % 2
}

MusicXMLPartMusic = {
  \key c \major \clef "treble" e'4 e'8 | % 0
  f'4 g'4 % 1
  a'4 b'4 | % X1
  c''4 d''4 r4 % 2
}

\score {
  <<
    \new Staff = "P1-1" \with { instrumentName = "MusicXML Part" } <<
      \Structure
      \new Voice = "MusicXMLPartMusic" { \MusicXMLPartMusic }
    >>
  >>
  \layout {
    indent = 27.7\mm
  }
}
