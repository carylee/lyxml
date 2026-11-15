#!/bin/bash
# Run tests/midi_check.py over scores in parallel. usage: tests/midi_all.sh [FILES...]
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HERE/../target/release/lyxml"
OUT="${CLAUDE_JOB_DIR:-/tmp}/tmp/midi"
mkdir -p "$OUT"
if [ $# -eq 0 ]; then
  # scores to check: the files given, else $LYXML_CORPUS (a list of files or globs), else tests/data
  set -- ${LYXML_CORPUS:-"$(dirname "$0")"/data/*}
fi
one() { f="$1"; b="$(basename "$(dirname "$f")")-$(basename "${f%.*}")"; echo "$b: $(python3 -I "$HERE/midi_check.py" "$BIN" "$f" "$OUT/$b" 2>&1 | tail -1)"; }
export -f one; export BIN OUT HERE
printf '%s\n' "$@" | xargs -P 8 -I{} bash -c 'one "$@"' _ {} | sort > "$OUT/results.txt"
grep -c ": OK" "$OUT/results.txt" | sed 's/^/OK: /'
grep -v ": OK" "$OUT/results.txt"
