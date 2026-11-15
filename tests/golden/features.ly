\version "2.24.0"
% Written by lyxml from MusicXML. One bar per line; `% N` is the source's bar number.

\pointAndClickOff

\header {
  title = "Feature test"
  tagline = ##f
}

\paper {
  #(set-paper-size "letter")
}

% Bar lines, repeats, breaks, tempo and rehearsal marks: one skip per bar.
Structure = {
  \autoLineBreaksOff
  \time 4/4 \tempo "Moderato" s1 | % 1
  s1 | % 2
  \break \once \omit Score.TimeSignature \time 3/4 s2. | % 3
  \once \omit Score.TimeSignature \time 4/4 s1 | % 4
  \time 6/8 s2. | % 5
  \bar "||" s2. | % 6
  s2. \bar "|." % 7
}

PartOneMusic = {
  \key bes \major \clef "treble" R1 | % 1
  R1 | % 2
  r8^\markup { \italic "Récit." }\fermata d''16 d''16 d''8 bes'16 c''16 d''16-> d''16 \breathe d''16-- d''16 | % 3
  g'8 g'16[ bes'16] \slashedGrace { a'16 } bes'16([ c''16)] d''4\trill ees''8\mordent f''8\turn r8 | % 4
  r4. r4. | % 5
  r4. r4. | % 6
  bes'4. r4. % 7
}

PartOneMusicVerseOne = \lyricmode {
  Puis -- que me voi -- là Prin -- ce pen -- dant % 3
  Je suis en -- _ cor bien as -- _ sez % 4
  fin % 7
}

PRVoiceOneMusic = {
  \oneVoice \key bes \major \clef "treble" bes'8\sf[ \tuplet 7/4 { c''32 d''32 ees''32 f''32 g''32 a''32 bes''32] } \once \omit TupletNumber \once \omit TupletBracket \tuplet 6/4 { d''16([ ees''16 f''16 g''16 a''16 bes''16)] } <d'' f'' bes''>4-. r4 | % 1
  \voiceOne \ottava #1 c'''64[ d'''64 ees'''32 f'''16 g'''8] \ottava #0 a'''4 r2 | % 2
  \oneVoice <f' bes' d''>2~:32\p_\markup { \italic "Tremolo." } <f' bes' d''>4 | % 3
  <ees' g' c''>8\=2([ c''8] ees'16[ g'16 c''16 ees''16] f''4 g''16[ a''16 bes''8\=2)] | % 4
  d''8\>[ e''8 f''8] g''8[ a''8 bes''8] | % 5
  r4.\! r4. | % 6
  <d'' f'' bes''>4. r4. % 7
}

PRVoiceTwoMusic = {
  s1 | % 1
  \voiceTwo r2 bes'2 | % 2
  s2. | % 3
  s1 | % 4
  s2. | % 5
  s2. | % 6
  s2. % 7
}

PLMusic = {
  \key bes \major \clef "bass" <bes, f>4\sustainOn-. r4 bes,4\sustainOff r4 | % 1
  bes,2 d'4 \clef "treble" ees'4 | % 2
  \clef "bass" <bes, f>2~:32 <bes, f>4 | % 3
  ees4 bes,4 f4 bes,4 | % 4
  g4. r4. | % 5
  a4. r4. | % 6
  bes,4. r4. % 7
}

\score {
  <<
    \new Staff = "P1-1" \with { instrumentName = "Voice" shortInstrumentName = "V." } <<
      \Structure
      \new Voice = "PartOneMusic" { \set melismaBusyProperties = #'() \autoBeamOff \PartOneMusic }
    >>
    \new Lyrics \lyricsto "PartOneMusic" { \set includeGraceNotes = ##t \PartOneMusicVerseOne }
    \new GrandStaff \with { instrumentName = "PIANO" } <<
      \new Staff = "P2-1" <<
        \new Voice = "PRVoiceOneMusic" { \autoBeamOff \PRVoiceOneMusic }
        \new Voice = "PRVoiceTwoMusic" { \autoBeamOff \PRVoiceTwoMusic }
      >>
      \new Staff = "P3-1" <<
        \new Voice = "PLMusic" { \autoBeamOff \PLMusic }
      >>
    >>
  >>
  \layout {
    indent = 16.5\mm
    short-indent = 6.8\mm
    \context { \Lyrics \override LyricHyphen.minimum-distance = #1.2 }
    \context { \Staff \RemoveAllEmptyStaves }
    \context { \Score \omit BarNumber }
  }
}
