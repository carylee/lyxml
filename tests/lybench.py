#!/usr/bin/env python3
"""Speed and deep-nesting checks for the LilyPond reader.

usage: lybench.py LYXML [WORKDIR]
"""
import os
import subprocess
import sys
import time


def main():
    binary = sys.argv[1]
    work = sys.argv[2] if len(sys.argv) > 2 else "/tmp"
    ok = True
    # pathological nesting must be refused, not overflow the stack
    deep = {
        "braces": "{" * 100000 + "c" + "}" * 100000,
        "scheme": "x = #" + "(" * 100000,
        "markup": "\\markup " + "{ \\bold " * 50000,
        "chords": "{ " + "<< " * 50000 + "c" + " >>" * 50000 + " }",
    }
    for name, src in deep.items():
        p = subprocess.run([binary, "-", "--from", "ly", "--to", "xml", "-q"], input=src.encode(), capture_output=True)
        good = p.returncode == 1 and b"line" in p.stderr
        ok &= good
        print(f"deep {name}: exit {p.returncode} {'ok' if good else 'UNEXPECTED'}: {p.stderr.decode()[:70].strip()}")
    # a 400-bar voice with words: about 6000 notes
    s = '\\version "2.24.0"\n\\score { << \\new Staff \\new Voice = "v" \\relative c\' { \\time 4/4 \\key d \\major\n'
    for _ in range(400):
        s += "  d8 e fis g a( b) cis d | <d, fis a>4 r8 a' \\tuplet 3/2 { g8 fis e } d4\\mf |\n"
    s += '}\n \\new Lyrics \\lyricsto "v" {\n'
    for _ in range(400):
        s += "  la la la la la __ la la  doo -- bee doo wah wah \n"
    s += "} >> }\n"
    path = os.path.join(work, "big.ly")
    open(path, "w").write(s)
    out = os.path.join(work, "big.musicxml")
    best = 9.0
    for _ in range(5):
        t = time.perf_counter()
        p = subprocess.run([binary, path, "-o", out], capture_output=True)
        best = min(best, time.perf_counter() - t)
    notes = open(out).read().count("<note")
    print(f"big.ly: {len(s) // 1024} KiB, {notes} notes, exit {p.returncode}, diagnostics: {p.stderr.decode().strip() or 'none'}")
    print(f"read + write MusicXML, whole process: {best * 1000:.1f} ms")
    ok &= p.returncode == 0 and best < 0.05
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
