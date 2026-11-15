\version "2.24.0"
% Variables everywhere: motifs reused inside \relative, \transpose, \repeat unfold,
% a markup variable, a paper block, a staff size, overrides that must be skipped.

#(set-global-staff-size 18)

\paper {
  #(set-paper-size "a4")
  first-page-number = 3
  top-margin = 12\mm
  ragged-last = ##t
}

\header {
  title = \markup { \bold "Variations" }
  subtitle = "on three notes"
  composer = \markup \right-column { "Music by A. Tester" \italic "arr. B. Tester" }
  copyright = "Public domain"
}

dolce = \markup { \italic "dolce" }
motif = { c8 d e4 }
answer = { e8 d c4 }
phrase = { \motif \answer }
bassline = { c2 g, | c1 }

flute = \relative c'' {
  \key c \major
  \time 2/2
  \override Staff.TimeSignature.color = #red
  \once \override NoteHead.color = #blue
  \phrase |
  \repeat unfold 2 { \motif } |
  \transpose c d { \phrase } |
  c1^\dolce \bar "||"
  \time 3/4
  \tweak color #red c4 r r |
  \repeat percent 2 { c8 e g e c4 } |
  c2. \bar "|."
}

cello = {
  \clef bass
  \key c \major
  \time 2/2
  \bassline |
  \fixed c { \bassline } |
  \time 3/4
  c2. | R2.*2 | c,2.
}

\score {
  \new StaffGroup <<
    \new Staff \with { instrumentName = "Flute" shortInstrumentName = "Fl." } \flute
    \new Staff \with { instrumentName = \markup \center-column { "Cello" "(or Bassoon)" } } \cello
  >>
  \layout {
    \context {
      \Staff
      \RemoveAllEmptyStaves
    }
    \context {
      \Score
      \omit BarNumber
    }
  }
  \midi { }
}
