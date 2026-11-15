\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  title = "Notation extras"
  copyright = \markup \column { \fontsize #-1 \justify { "✻" "A" "long" "footnote" "that" "has" "to" "wrap" "across" "the" "width" "of" "the" "page," "with" "an" \italic { "italic" "span" } "in" "the" "middle" "of" "it," "so" "that" "it" "is" "certainly" "longer" "than" "one" "line" "of" "text" "at" "this" "size." } \vspace #1 \fill-line { \center-column { "© 2026 Nobody" } } }
  tagline = ##f
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \time 4/4 s1 | % 1
  s1 \bar "|." % 2
}

VoiceVoiceOneMusic = {
  \voiceOne \key c \major \clef "treble" \once \slurDashed e''4^\markup { "Solo" }^\f( d''4) \once \tieDotted c''4~ c''4 \once \override BreathingSign.text = \markup \musicglyph "scripts.caesura.straight" \breathe | % 1
  \oneVoice \once \set doubleSlurs = ##t <a'~ cis''>2^\markup { \italic "dolce" }( <a' d''>2) % 2
}

VoiceVoiceTwoMusic = {
  \voiceTwo \xNote c'2\mf d'2 | % 1
  s1 % 2
}

CymbalsMusic = {
  \clef "percussion" \set Staff.middleCPosition = #-6 \xNote b'2 r2 | % 1
  R1 % 2
}

\score {
  <<
    \new Staff = "P1-1" \with { instrumentName = "Voice" } <<
      \Structure
      \new Voice = "VoiceVoiceOneMusic" { \VoiceVoiceOneMusic }
      \new Voice = "VoiceVoiceTwoMusic" { \VoiceVoiceTwoMusic }
    >>
    \new Staff = "P2-1" \with { \override StaffSymbol.line-count = #1 instrumentName = "Cymbals" } <<
      \new Voice = "CymbalsMusic" { \CymbalsMusic }
    >>
  >>
  \layout {
    indent = 16.3\mm
    \context { \Score \override BarNumber.break-visibility = ##(#f #t #t) barNumberVisibility = #all-bar-numbers-visible \override TimeSignature.style = #'numbered }
  }
}
