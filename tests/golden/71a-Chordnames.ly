\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  tagline = ##f
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \time 4/4 s1 | % 1
  s1 \bar "|." % 2
}

MusicXMLPartMusic = {
  \key c \major \clef "treble" a'4 a'4 a'4 a'4 | % 1
  a'4 a'4 a'4 a'4 % 2
}

MusicXMLPartChords = \chordmode {
  c4 c4:maj7.11+ b4:7.5+.9+ ees4:5.2 | % 1
  g4:m dis4:maj7 a4:dim7 a4:aug % 2
}

\score {
  <<
    \new ChordNames { \MusicXMLPartChords }
    \new Staff = "P1-1" \with { instrumentName = "MusicXML Part" } <<
      \Structure
      \new Voice = "MusicXMLPartMusic" { \MusicXMLPartMusic }
    >>
  >>
  \layout {
    indent = 27.7\mm
  }
}
