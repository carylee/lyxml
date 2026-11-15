#!/bin/bash
# MusicXML -> model -> MusicXML must compare identical (checks reader, writer and diff).
BIN="$(dirname "$0")/../target/release/lyxml"
OUT="${CLAUDE_JOB_DIR:-/tmp}/tmp/xmlrt"
mkdir -p "$OUT"
if [ $# -eq 0 ]; then
  # scores to check: the files given, else $LYXML_CORPUS (a list of files or globs), else tests/data
  set -- ${LYXML_CORPUS:-"$(dirname "$0")"/data/*}
fi
ok=0; bad=0
for f in "$@"; do
  b="$(basename "$(dirname "$f")")-$(basename "${f%.*}")"
  "$BIN" "$f" --to xml -o "$OUT/$b.musicxml" -q || { echo "FAIL $b"; bad=$((bad+1)); continue; }
  if "$BIN" diff "$f" "$OUT/$b.musicxml" > "$OUT/$b.diff" 2>&1; then ok=$((ok+1)); else bad=$((bad+1)); echo "DIFF $b ($(wc -l < "$OUT/$b.diff") lines)"; fi
done
echo "identical: $ok, different: $bad"
