#!/usr/bin/env python3
"""Robustness check for the LilyPond reader: truncated and mutated inputs must never
crash or hang lyxml (a clean error exit is fine).

usage: lyfuzz.py LYXML FILE.ly... [--rounds N]
"""
import random
import subprocess
import sys


def main():
    args = sys.argv[1:]
    rounds = 150
    if "--rounds" in args:
        i = args.index("--rounds")
        rounds = int(args[i + 1])
        del args[i:i + 2]
    binary, files = args[0], args[1:]
    rng = random.Random(7)
    junk = ["{", "}", "<<", ">>", "<", ">", "\\", "\\\\", "#(", ")", '"', "~", "(", "[", "|", "%{", "\\relative", "\\tuplet 3/2",
            "\\repeat volta 2", "\\new Staff", "=", "'", ",", "4.", "*2/3", "\\lyricsto", "--", "__", "\\markup {", "#'(", "\\set", "\\times 2/3"]
    bad = 0
    total = 0
    for f in files:
        src = open(f, encoding="utf-8", errors="replace").read()
        for r in range(rounds):
            kind = r % 3
            if kind == 0:
                s = src[: rng.randrange(len(src) + 1)]
            elif kind == 1:
                p = rng.randrange(len(src) + 1)
                s = src[:p] + rng.choice(junk) + src[p:]
            else:
                a = rng.randrange(len(src) + 1)
                b = min(len(src), a + rng.randrange(1, 40))
                s = src[:a] + src[b:]
            total += 1
            try:
                p = subprocess.run([binary, "-", "--from", "ly", "--to", "xml", "-q"], input=s.encode(), stdout=subprocess.DEVNULL,
                                   stderr=subprocess.PIPE, timeout=10)
            except subprocess.TimeoutExpired:
                bad += 1
                open(f"/tmp/lyfuzz-hang-{bad}.ly", "w").write(s)
                print(f"HANG {f} round {r}")
                continue
            if p.returncode not in (0, 1, 2):
                bad += 1
                print(f"CRASH {f} round {r}: exit {p.returncode}: {p.stderr.decode(errors='replace')[:200]}")
                open(f"/tmp/lyfuzz-crash-{bad}.ly", "w").write(s)
    print(f"{total} inputs, {bad} crashes or hangs")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
