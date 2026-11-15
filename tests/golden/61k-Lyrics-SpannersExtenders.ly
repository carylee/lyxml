\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  tagline = ##f
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \time 4/4 s1 | % 1
  s1 | % 2
  s1 \bar "|." % 3
}

MusicXMLPartMusic = {
  \key c \major \clef "treble" c''4 c''4 c''4 c''4 | % 1
  c''4 c''4 c''4 c''4 | % 2
  c''4 c''4 c''4 r4 % 3
}

MusicXMLPartMusicVerseOne = \lyricmode {
  A __ _ b -- _ % 1
  _ CC __ _ _ % 2
  e __ _ _ % 3
}

\score {
  <<
    \new Staff = "P1-1" <<
      \Structure
      \new Voice = "MusicXMLPartMusic" { \set melismaBusyProperties = #'() \MusicXMLPartMusic }
    >>
    \new Lyrics \lyricsto "MusicXMLPartMusic" { \MusicXMLPartMusicVerseOne }
  >>
  \layout {
    \context { \Lyrics \override LyricHyphen.minimum-distance = #1.2 }
  }
}
