\version "2.24.0"
% Piano: two staves, two voices in the right hand, tuplets, grace notes, chords,
% dynamics, hairpins, pedal, ottava, a tempo mark and a key change.

\header {
  title = "Little Study"
  composer = "Test"
}

upper = \relative c'' {
  \clef treble
  \key d \minor
  \time 3/4
  \tempo "Andante" 4 = 72
  <<
    { a4\p( bes a) | \tuplet 3/2 { g8 a g } f4 e }
    \\
    { d2. | d2 cis4 }
  >>
  \acciaccatura e8 d4-. \grace { e16 f } g4-> a--\fermata |
  \key d \major
  <d, fis a>2\mf\< <e g b>4 |
  \ottava #1 d''8\f\! cis b a \tuplet 3/2 { g fis e } \ottava #0 |
  d2.\> |
  R2.\! \bar "|."
}

lower = \relative c {
  \clef bass
  \key d \minor
  \time 3/4
  d4\sustainOn a' d\sustainOff | bes,4 d a |
  d,4 r <a' d f> |
  \key d \major
  d,8 a' d fis a4 |
  <d,, d'>2. ~ |
  q2. |
  d2.
}

\score {
  \new PianoStaff \with { instrumentName = "Piano" } <<
    \new Staff = "up" \upper
    \new Staff = "down" \lower
  >>
  \layout { }
}
