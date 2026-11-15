#!/bin/bash
# MusicXML -> LilyPond -> MusicXML must compare identical (no LilyPond run: fast).
# usage: tests/lyrt.sh [FILES...]
BIN="$(dirname "$0")/../target/release/lyxml"
OUT="${CLAUDE_JOB_DIR:-/tmp}/tmp/lyrt"
mkdir -p "$OUT"
if [ $# -eq 0 ]; then
  set -- /home/sprite/src/caroling/media/musicxml/*.mxl /home/sprite/src/caroling-print/print/out/*.mxl \
         /home/sprite/src/score-transcription-skill/tests/golden/*.musicxml /home/sprite/scores/barigoule/barigoule.mxl
fi
ok=0; bad=0
for f in "$@"; do
  b="$(basename "$(dirname "$f")")-$(basename "${f%.*}")"
  "$BIN" "$f" -o "$OUT/$b.ly" -q || { echo "FAIL(to ly) $b"; bad=$((bad+1)); continue; }
  "$BIN" "$OUT/$b.ly" -o "$OUT/$b.musicxml" 2> "$OUT/$b.back.txt" || { echo "FAIL(read ly) $b: $(head -1 "$OUT/$b.back.txt")"; bad=$((bad+1)); continue; }
  if "$BIN" diff "$f" "$OUT/$b.musicxml" > "$OUT/$b.diff" 2>&1; then ok=$((ok+1)); else bad=$((bad+1)); echo "DIFF $b ($(wc -l < "$OUT/$b.diff") lines, $(wc -l < "$OUT/$b.back.txt") reader diagnostics)"; fi
done
echo "identical: $ok, different: $bad"
