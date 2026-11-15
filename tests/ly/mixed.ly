\version "2.24.0"
% A grab bag of constructs: nested tuplets, \afterGrace, \times, a free bar with
% \cadenzaOn, tremolo repeats, \partial in the middle, octave checks, a temporary
% second voice, \after, nested \relative inside << >>, and a Dynamics context.

\header {
  title = "Mixed Bag"
}

rh = \relative c'' {
  \key a \minor
  \time 2/4
  \tuplet 3/2 { c8 d \tuplet 3/2 { e16 f e } } d8 c |
  \afterGrace b2\trill { a16 b } |
  \times 2/3 { c4 b8 } \repeat tremolo 4 a32 gis8 |
  \cadenzaOn a8[ b c d e f] \cadenzaOff \bar "|"
  e=''4.\startTextSpan d8\stopTextSpan |
  << { e4 d } \\ { c8 b a gis } >> |
  \after 4 \f a2\< |
  \time 3/8
  a8 c e |
  \partial 8 a8 \bar "||"
  \time 2/4
  <a, c e>2\arpeggio \bar "|."
}

lh = \relative c {
  \clef bass
  \key a \minor
  \time 2/4
  a4 <c e> | e, <b' d> | a4 e |
  \cadenzaOn a,2. \cadenzaOff
  << { c'4 b } { e,2 } >> |
  a,4 e' | a,2 |
  \time 3/8
  a4. | a8
  \time 2/4
  a2
}

\score {
  \new PianoStaff <<
    \new Staff = "rh" \rh
    \new Dynamics { s2\p s2 s2\< s2.\! s2\mp }
    \new Staff = "lh" \lh
  >>
  \layout { }
}
