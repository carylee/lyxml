\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  title = "Old Hundredth"
  poet = "William Kethe, 1561"
  composer = \markup \right-column { "Genevan Psalter, 1551" }
  tagline = ##f
}

\paper {
  #(set-paper-size "letter")
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \autoLineBreaksOff
  \time 4/4 s1 | % 1
  s1 | % 2
  \break s1 \bar "||" % 3
}

SAVoiceOneMusic = {
  \voiceOne \key g \major \clef "treble" g'2\mf g'4 fis'4 | % 1
  e'4 d'4 g'4 a'4 | % 2
  b'1\fermata % 3
}

SAVoiceOneMusicVerseOne = \lyricmode {
  All peo -- ple % 1
  that on earth do % 2
  dwell, % 3
}

SAVoiceTwoMusic = {
  \voiceTwo d'2 d'4 d'4 | % 1
  c'4 d'4 d'4 fis'4 | % 2
  d'1_\fermata % 3
}

TBVoiceOneMusic = {
  \voiceOne \key g \major \clef "bass" b2 b4 a4 | % 1
  g4 g4 b4 d'4 | % 2
  g1\fermata % 3
}

TBVoiceOneMusicVerseOne = \lyricmode {
  All peo -- ple % 1
  that on earth do % 2
  dwell, % 3
}

TBVoiceTwoMusic = {
  \voiceTwo g2 g4 d4 | % 1
  c4 b,4 g,4 d4 | % 2
  g,1_\fermata % 3
}

\score {
  <<
    \new ChoirStaff <<
      \new Staff = "P1-1" \with { instrumentName = "SA" } <<
        \Structure
        \new Voice = "SAVoiceOneMusic" { \set melismaBusyProperties = #'() \SAVoiceOneMusic }
        \new Voice = "SAVoiceTwoMusic" { \SAVoiceTwoMusic }
      >>
      \new Lyrics \lyricsto "SAVoiceOneMusic" { \SAVoiceOneMusicVerseOne }
      \new Staff = "P2-1" \with { instrumentName = "TB" } <<
        \new Voice = "TBVoiceOneMusic" { \set melismaBusyProperties = #'() \TBVoiceOneMusic }
        \new Voice = "TBVoiceTwoMusic" { \TBVoiceTwoMusic }
      >>
      \new Lyrics \lyricsto "TBVoiceOneMusic" { \TBVoiceOneMusicVerseOne }
    >>
  >>
  \layout {
    \context { \Lyrics \override LyricHyphen.minimum-distance = #1.2 }
    \context { \Score \override TimeSignature.style = #'numbered }
  }
}
