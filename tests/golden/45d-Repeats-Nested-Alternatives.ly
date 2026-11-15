\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  tagline = ##f
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \time 4/4 s1 | % 1
  \set Score.repeatCommands = #'((volta "1.")) s1 | % 2
  \bar ":|." \set Score.repeatCommands = #'((volta #f) (volta "2.")) s1 | % 3
  s1 | % 4
  s1 | % 5
  \once \override Score.VoltaBracket.edge-height = #'(2.0 . 0) \set Score.repeatCommands = #'((volta #f) (volta "3.")) s1 | % 6
  \set Score.repeatCommands = #'((volta #f)) s1 | % 7
  s1 | % 8
  s1 | % 9
  \set Score.repeatCommands = #'((volta "1.")) s1 | % 10
  \set Score.repeatCommands = #'((volta #f) (volta "5.")) s1 | % 11
  \bar ":|." \set Score.repeatCommands = #'((volta #f)) s1 \bar "|." % 12
}

MusicXMLPartMusic = {
  \key c \major \clef "treble" R1 | % 1
  R1 | % 2
  R1 | % 3
  R1 | % 4
  R1 | % 5
  R1 | % 6
  R1 | % 7
  R1 | % 8
  R1 | % 9
  R1 | % 10
  R1 | % 11
  R1 % 12
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
