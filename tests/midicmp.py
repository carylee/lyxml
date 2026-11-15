#!/usr/bin/env python3
"""Compare two MIDI files as sets of (onset, pitch, length) notes, ignoring tracks and channels.

usage: midicmp.py A.midi B.midi      exit 0 when the notes are the same

Used as an oracle that does not depend on lyxml's own reader: LilyPond renders the
original .ly and the .ly that came back through MusicXML; both must sound the same.
"""
import sys
from collections import Counter


def notes(path):
    data = open(path, "rb").read()
    assert data[:4] == b"MThd"
    division = int.from_bytes(data[12:14], "big")
    pos = 14
    out = Counter()
    while pos < len(data):
        assert data[pos:pos + 4] == b"MTrk", pos
        length = int.from_bytes(data[pos + 4:pos + 8], "big")
        p, end = pos + 8, pos + 8 + length
        pos = end
        t, status, on = 0, 0, {}
        while p < end:
            delta = 0
            while True:
                b = data[p]; p += 1
                delta = (delta << 7) | (b & 0x7F)
                if not b & 0x80:
                    break
            t += delta
            if data[p] & 0x80:
                status = data[p]; p += 1
            if status == 0xFF:
                p += 1
                n = 0
                while True:
                    b = data[p]; p += 1
                    n = (n << 7) | (b & 0x7F)
                    if not b & 0x80:
                        break
                p += n
            elif status in (0xF0, 0xF7):
                n = 0
                while True:
                    b = data[p]; p += 1
                    n = (n << 7) | (b & 0x7F)
                    if not b & 0x80:
                        break
                p += n
            else:
                kind = status & 0xF0
                if kind in (0xC0, 0xD0):
                    p += 1
                else:
                    a, b = data[p], data[p + 1]; p += 2
                    if kind == 0x90 and b > 0:
                        on.setdefault(a, []).append(t)
                    elif kind == 0x80 or (kind == 0x90 and b == 0):
                        if on.get(a):
                            t0 = on[a].pop(0)
                            out[(round(t0 * 96 / division), a, round((t - t0) * 96 / division))] += 1
    return out


def main():
    a, b = notes(sys.argv[1]), notes(sys.argv[2])
    if a == b:
        print(f"same: {sum(a.values())} notes")
        return 0
    for k in sorted((a - b).elements())[:15]:
        print("only in first: ", k)
    for k in sorted((b - a).elements())[:15]:
        print("only in second:", k)
    print(f"notes: {sum(a.values())} | {sum(b.values())}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
