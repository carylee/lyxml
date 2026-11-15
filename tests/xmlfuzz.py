#!/usr/bin/env python3
"""Robustness check for the MusicXML reader and both writers: damaged but mostly
well-formed MusicXML must never crash or hang lyxml (a clean error exit is fine).

usage: xmlfuzz.py LYXML FILE.(musicxml|xml|mxl)... [--rounds N] [--seed N] [--keep DIR]

Run it against a debug build (`cargo build`) as well: that one also traps arithmetic
overflow, which a release build lets wrap.

Each round damages a score in one of several ways (odd numbers, missing or repeated
elements, swapped start/stop marks, stray chord and grace flags, cut-off text), then
runs three conversions on it: to LilyPond, to MusicXML, and the LilyPond back to
MusicXML. Exit codes 0, 1 and 2 are fine; anything else, or a timeout, is a failure and
the input is kept in DIR.
"""
import os, random, re, subprocess, sys, tempfile, zipfile

NUMERIC = ["duration", "divisions", "octave", "fifths", "beats", "beat-type", "staff", "voice", "staves", "actual-notes", "normal-notes",
           "alter", "line", "clef-octave-change", "per-minute", "staff-lines", "display-octave", "tenths", "millimeters"]
ODD = ["0", "-1", "-7", "1", "3", "7", "64", "99999999", "4294967296", "3.7", "0.001", "", "x", "1e9", "-0"]
SMALL = ["duration", "type", "pitch", "voice", "staff", "divisions", "time-modification", "attributes", "key", "time", "clef", "notations",
         "lyric", "syllabic", "text", "dot", "tie", "rest", "backup", "forward", "barline", "direction", "print", "part-name", "score-part",
         "part-group", "beam", "tuplet", "slur", "tied", "ending", "repeat", "harmony", "root", "kind"]
INSERT = ["<chord/>", "<grace/>", "<grace slash=\"yes\"/>", "<cue/>", "<rest measure=\"yes\"/>", "<dot/><dot/><dot/><dot/><dot/>",
          "<time-modification><actual-notes>0</actual-notes><normal-notes>0</normal-notes></time-modification>",
          "<time-modification><actual-notes>1000003</actual-notes><normal-notes>7</normal-notes></time-modification>",
          "<notations><tuplet type=\"start\"/><tuplet type=\"start\"/><slur type=\"stop\"/><tied type=\"stop\"/></notations>",
          "<lyric number=\"0\"><text></text></lyric>", "<lyric><syllabic>middle</syllabic><text>\"\\{}%~_</text><extend/></lyric>",
          "<backup><duration>999999</duration></backup>", "<forward><duration>999999</duration></forward>",
          "<attributes><divisions>0</divisions></attributes>", "<attributes><time><beats>0</beats><beat-type>0</beat-type></time></attributes>",
          "<attributes><time><beats>3+2+</beats><beat-type>8</beat-type></time></attributes>",
          "<attributes><staves>40</staves></attributes>", "<staff>9</staff>", "<voice>4000000000</voice>",
          "<barline location=\"left\"><ending number=\"\" type=\"start\"/><repeat direction=\"backward\"/></barline>",
          "<direction><direction-type><wedge type=\"stop\"/></direction-type></direction>",
          "<direction><direction-type><metronome><beat-unit>eighth</beat-unit><per-minute></per-minute></metronome></direction-type></direction>",
          "<harmony><root><root-step>H</root-step></root><kind>none</kind></harmony>", "<print new-page=\"yes\"/>"]


def load(path):
    if path.endswith(".mxl"):
        z = zipfile.ZipFile(path)
        name = [n for n in z.namelist() if not n.startswith("META-INF") and n != "mimetype"][0]
        return z.read(name).decode("utf-8", errors="replace")
    return open(path, encoding="utf-8", errors="replace").read()


def damage(src, rng):
    kind = rng.randrange(9)
    if kind == 0:      # an odd number somewhere
        tag = rng.choice(NUMERIC)
        hits = list(re.finditer(r"<%s>([^<]*)</%s>" % (tag, tag), src))
        if hits:
            m = rng.choice(hits)
            return src[:m.start(1)] + rng.choice(ODD) + src[m.end(1):]
    if kind == 1:      # an element removed
        tag = rng.choice(SMALL)
        hits = list(re.finditer(r"<%s\b[^>]*?(/>|>.*?</%s>)" % (tag, tag), src, re.S))
        if hits:
            m = rng.choice(hits)
            return src[:m.start()] + src[m.end():]
    if kind == 2:      # every element of one kind removed
        tag = rng.choice(SMALL)
        return re.sub(r"<%s\b[^>]*?(/>|>.*?</%s>)" % (tag, tag), "", src, flags=re.S)
    if kind == 3:      # a note, measure, barline or direction repeated
        tag = rng.choice(["note", "measure", "barline", "direction", "attributes", "backup"])
        hits = list(re.finditer(r"<%s\b[^>]*>.*?</%s>" % (tag, tag), src, re.S))
        if hits:
            m = rng.choice(hits)
            return src[:m.end()] + m.group(0) * rng.choice([1, 2, 5]) + src[m.end():]
    if kind == 4:      # start and stop marks swapped or broken
        hits = list(re.finditer(r'type="(start|stop|begin|end|continue|forward|backward)"', src))
        if hits:
            m = rng.choice(hits)
            return src[:m.start(1)] + rng.choice(["start", "stop", "continue", "discontinue", "", "x"]) + src[m.end(1):]
    if kind == 5:      # something stray inside a note or a measure
        hits = list(re.finditer(r"<note\b[^>]*>|<measure\b[^>]*>", src))
        if hits:
            m = rng.choice(hits)
            return src[:m.end()] + rng.choice(INSERT) + src[m.end():]
    if kind == 6:      # an attribute value changed
        hits = list(re.finditer(r'(number|implicit|id|location|placement|line-type|symbol)="([^"]*)"', src))
        if hits:
            m = rng.choice(hits)
            return src[:m.start(2)] + rng.choice(["", "0", "-3", "yes", "99999999999", "P1", "middle", "x y", "1, 2", "\u00e9"]) + src[m.end(2):]
    if kind == 7:      # measures shuffled between parts: lengths stop agreeing
        hits = list(re.finditer(r"<measure\b[^>]*>.*?</measure>", src, re.S))
        if len(hits) > 2:
            a, b = rng.sample(hits, 2)
            if a.start() > b.start():
                a, b = b, a
            return src[:a.start()] + b.group(0) + src[a.end():b.start()] + a.group(0) + src[b.end():]
    # cut off, or a slice removed
    a = rng.randrange(len(src) + 1)
    if rng.random() < 0.5:
        return src[:a]
    return src[:a] + src[min(len(src), a + rng.randrange(1, 200)):]


def run(binary, args, data):
    try:
        p = subprocess.run([binary] + args, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)
    except subprocess.TimeoutExpired:
        return None, b"", b"timeout"
    return p.returncode, p.stdout, p.stderr


def main():
    args = sys.argv[1:]
    rounds, keep = 200, tempfile.gettempdir()
    seed = 11
    for flag in ("--rounds", "--keep", "--seed"):
        if flag in args:
            i = args.index(flag)
            if flag == "--rounds":
                rounds = int(args[i + 1])
            elif flag == "--seed":
                seed = int(args[i + 1])
            else:
                keep = args[i + 1]
            del args[i:i + 2]
    binary, files = args[0], args[1:]
    os.makedirs(keep, exist_ok=True)
    rng = random.Random(seed)
    total = bad = 0
    for f in files:
        src = load(f)
        for r in range(rounds):
            s = damage(src, rng)
            if rng.random() < 0.3:           # two kinds of damage at once
                s = damage(s, rng)
            data = s.encode("utf-8", errors="replace")
            total += 1
            steps = [("to ly", ["-", "--from", "xml", "--to", "ly", "-q"], data), ("to xml", ["-", "--from", "xml", "--to", "xml", "-q"], data)]
            failed = None
            ly = b""
            for name, a, d in steps:
                code, out, err = run(binary, a, d)
                if code is None or code not in (0, 1, 2):
                    failed = (name, code, err)
                    break
                if name == "to ly":
                    ly = out
            if failed is None and ly:
                code, out, err = run(binary, ["-", "--from", "ly", "--to", "xml", "-q"], ly)
                if code is None or code not in (0, 1, 2):
                    failed = ("ly back to xml", code, err)
            if failed:
                bad += 1
                path = os.path.join(keep, "xmlfuzz-%d.musicxml" % bad)
                open(path, "wb").write(data)
                print("FAIL %s round %d: %s: exit %s: %s -> %s" % (os.path.basename(f), r, failed[0], failed[1],
                                                                  failed[2].decode(errors="replace").strip()[:160], path))
    print("%d inputs, %d crashes or hangs" % (total, bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
