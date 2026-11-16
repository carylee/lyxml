#!/usr/bin/env python3
"""Check a conversion against LilyPond's own reading of it.

usage: midi_check.py LYXML SCORE.(mxl|musicxml) [WORKDIR]

Converts SCORE to LilyPond with --midi, compiles it, and compares every note LilyPond
plays (onset and pitch, from the MIDI file) with the notes in the MusicXML (`lyxml notes`).
Prints one line: OK or the differences. Grace notes are ignored on both sides.
"""
import collections, os, struct, subprocess, sys, tempfile
from fractions import Fraction


def midi_notes(path):
    """-> list of tracks, each a Counter of (onset in whole notes, pitch)."""
    data = open(path, "rb").read()
    assert data[:4] == b"MThd"
    division = struct.unpack(">H", data[12:14])[0]
    pos, tracks = 14, []
    while pos < len(data):
        if data[pos:pos + 4] != b"MTrk":
            break
        length = struct.unpack(">I", data[pos + 4:pos + 8])[0]
        chunk = data[pos + 8:pos + 8 + length]
        pos += 8 + length
        t, i, status, notes, name = 0, 0, 0, collections.Counter(), ""
        while i < len(chunk):
            delta = 0
            while True:
                b = chunk[i]; i += 1
                delta = (delta << 7) | (b & 0x7F)
                if not b & 0x80:
                    break
            t += delta
            b = chunk[i]
            if b & 0x80:
                status = b; i += 1
            if status == 0xFF:
                meta = chunk[i]
                i += 1
                n = 0
                while True:
                    c = chunk[i]; i += 1
                    n = (n << 7) | (c & 0x7F)
                    if not c & 0x80:
                        break
                if meta == 3:
                    name = chunk[i:i + n].decode("utf-8", errors="replace")
                i += n
            elif status in (0xF0, 0xF7):
                n = 0
                while True:
                    c = chunk[i]; i += 1
                    n = (n << 7) | (c & 0x7F)
                    if not c & 0x80:
                        break
                i += n
            else:
                kind = status & 0xF0
                if kind in (0xC0, 0xD0):
                    i += 1
                else:
                    a, v = chunk[i], chunk[i + 1]; i += 2
                    if kind == 0x90 and v > 0:
                        notes[(t, a)] += 1
        if notes:
            tracks.append((name, notes))
    return tracks, division


def main():
    lyxml, score = sys.argv[1], sys.argv[2]
    work = sys.argv[3] if len(sys.argv) > 3 else tempfile.mkdtemp()
    os.makedirs(work, exist_ok=True)
    ly = os.path.join(work, "m.ly")
    subprocess.run([lyxml, score, "-o", ly, "--midi", "-q"], check=True)
    r = subprocess.run([os.environ.get("LILYPOND", "lilypond"), "-dno-print-pages", "-o", os.path.join(work, "m"), ly], capture_output=True, text=True)
    midi = os.path.join(work, "m.midi")
    if r.returncode != 0 or not os.path.exists(midi):
        print("LILYPOND FAILED"); return 1
    want = collections.defaultdict(collections.Counter)
    graces = collections.defaultdict(collections.Counter)
    for line in subprocess.run([lyxml, "notes", score, "-q"], capture_output=True, text=True, check=True).stdout.splitlines():
        part, staff, onset, pitch = line.split()
        if onset == "grace":
            graces[(int(part), int(staff))][int(pitch)] += 1
            continue
        want[(int(part), int(staff))][(Fraction(onset), int(pitch))] += 1
    keys = sorted(want)
    want = [want[k] for k in keys]
    graces = [graces[k] for k in keys]
    tracks, division = midi_notes(midi)
    # staff tracks carry the staff's name; a chord-symbol track has none and is not compared
    named = [notes for name, notes in tracks if name]
    got = named if named else [notes for _, notes in tracks]
    # MIDI ticks cannot hold every tuplet exactly: allow one tick
    want = [collections.Counter({(round(o * division * 4), p): n for (o, p), n in w.items()}) for w in want]
    near = lambda c, k: any(c.get((k[0] + d, k[1]), 0) for d in (0, -1, 1))
    if len(want) != len(got):
        print(f"staves with notes: {len(want)} in MusicXML, {len(got)} in MIDI"); return 1
    missing = extra = 0
    examples = []
    for i, (w, g) in enumerate(zip(want, got)):
        # LilyPond merges unisons between voices of one staff into one MIDI note
        for key, n in w.items():
            if not near(g, key):
                missing += n
                examples.append(f"staff#{i + 1} missing tick {key[0]} pitch {key[1]}")
        for key, n in g.items():
            if not near(w, key):
                if graces[i][key[1]] >= n:      # LilyPond plays grace notes; the listing has no time for them
                    graces[i][key[1]] -= n
                    continue
                extra += n
                examples.append(f"staff#{i + 1} extra tick {key[0]} pitch {key[1]}")
    total = sum(sum(w.values()) for w in want)
    if missing == 0 and extra == 0:
        print(f"OK {total} notes"); return 0
    print(f"DIFF {total} notes: {missing} missing, {extra} extra; " + "; ".join(examples[:4])); return 1


if __name__ == "__main__":
    sys.exit(main())
