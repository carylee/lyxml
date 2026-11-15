#!/bin/bash
# Convert every local score to LilyPond, compile it, and report failures and warnings.
# usage: tests/corpus.sh [OUTDIR] [FILES...]     (default: $LYXML_CORPUS, else tests/data)
BIN="$(dirname "$0")/../target/release/lyxml"
OUT="${1:-${CLAUDE_JOB_DIR:-/tmp}/tmp/corpus}"
shift
mkdir -p "$OUT"
if [ $# -eq 0 ]; then
  # scores to check: the files given, else $LYXML_CORPUS (a list of files or globs), else tests/data
  set -- ${LYXML_CORPUS:-"$(dirname "$0")"/data/*}
fi
one() {
  f="$1"; b="$(basename "$(dirname "$f")")-$(basename "${f%.*}")"; d="$OUT/$b"; mkdir -p "$d"
  if ! "$BIN" "$f" -o "$d/s.ly" 2> "$d/diag.txt"; then echo "CONVERT-FAIL $b"; return; fi
  if ! timeout 300 "${LILYPOND:-lilypond}" -dno-point-and-click -o "$d/s" "$d/s.ly" > "$d/ly.log" 2>&1; then echo "LILY-FAIL $b"; fi
  w=$(grep -c "warning" "$d/ly.log"); e=$(grep -c "error" "$d/ly.log"); dr=$(grep -c "^dropped" "$d/diag.txt")
  if "$BIN" "$d/s.ly" -o "$d/back.musicxml" 2> "$d/back.txt"; then
    if "$BIN" diff "$f" "$d/back.musicxml" > "$d/diff.txt" 2>&1; then rt=ok; else rt="DIFF($(wc -l < "$d/diff.txt"))"; fi
  else rt=noback; fi
  echo "$b warnings=$w errors=$e dropped=$dr roundtrip=$rt"
}
export -f one; export BIN OUT
printf '%s\n' "$@" | xargs -P 8 -I{} bash -c 'one "$@"' _ {} | sort > "$OUT/results.txt"
cat "$OUT/results.txt"
# fail on a conversion or LilyPond failure, a LilyPond error, or a round trip that differs
! grep -q -E "FAIL|errors=[1-9]|roundtrip=(DIFF|noback)" "$OUT/results.txt" 
