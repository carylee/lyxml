#!/bin/bash
# Hand-written LilyPond fixtures: each must compile cleanly, and LilyPond's MIDI of the
# fixture must equal its MIDI of fixture -> MusicXML -> LilyPond (an oracle outside lyxml).
# usage: tests/lyfix.sh [FILES...]    (default: tests/ly/*.ly)
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HERE/../target/release/lyxml"
OUT="${CLAUDE_JOB_DIR:-/tmp}/tmp/lyfix"
mkdir -p "$OUT"
[ $# -eq 0 ] && set -- "$HERE"/ly/*.ly
bad=0
for f in "$@"; do
  b="$(basename "${f%.ly}")"
  # the fixture itself, with a \midi block added to its score
  sed -e 's/\\layout { }/\\layout { } \\midi { }/' -e 's/\\repeat percent/\\repeat unfold/' "$f" > "$OUT/$b.orig.ly"
  "${LILYPOND:-lilypond}" -dno-point-and-click -o "$OUT/$b.orig" "$OUT/$b.orig.ly" > "$OUT/$b.orig.log" 2>&1 || { echo "FAIL $b: fixture does not compile"; bad=1; continue; }
  w=$(grep -ci "warning\|error" "$OUT/$b.orig.log")
  "$BIN" "$f" -o "$OUT/$b.musicxml" 2> "$OUT/$b.diag.txt" || { echo "FAIL $b: $(head -1 "$OUT/$b.diag.txt")"; bad=1; continue; }
  "$BIN" "$OUT/$b.musicxml" --midi -o "$OUT/$b.back.ly" -q
  "${LILYPOND:-lilypond}" -dno-point-and-click -o "$OUT/$b.back" "$OUT/$b.back.ly" > "$OUT/$b.back.log" 2>&1 || { echo "FAIL $b: round-tripped file does not compile"; bad=1; continue; }
  if r=$(python3 -I "$HERE/midicmp.py" "$OUT/$b.orig.midi" "$OUT/$b.back.midi"); then
    echo "ok   $b: fixture warnings=$w, reader diagnostics=$(wc -l < "$OUT/$b.diag.txt"), midi $r"
  else
    echo "DIFF $b"; echo "$r" | head -12; bad=1
  fi
done
exit $bad
