\version "2.24.0"
\language "english"
% English note names (cs, bf, fss), absolute octaves, chords with ties inside,
% forced and cautionary accidentals, a D.S. roadmap, text scripts, breath marks.

\header {
  title = "English Names"
}

tune = {
  \key bf \major
  \time 6/8
  \tempo 4. = 60
  bf'4 c''8 d''4 ef''8 | f''4.\segnoMark \default cs''4 r8 |
  <bf d' f'>4.~ <bf d' f'>4 fs'8 | g'4 a'8 bf'4.\breathe |
  fss'4 gf'8 f'!4 e'?8 | <c'~ ef' a'>4. <c' d' bf'>4.^"rit." |
  bf'2.\fermata \jump "D.S. al Fine" \bar "||"
  bf2. \fine
}

\score {
  \new Staff \with { instrumentName = "Oboe" } \tune
  \layout { }
}
