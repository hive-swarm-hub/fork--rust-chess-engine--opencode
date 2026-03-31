#!/usr/bin/env python3
"""
live_elo.py — pipe fastchess stdout through this to get a live ELO table.

Usage:
    fastchess [...] | python3 live_elo.py [--every N]

Prints updated standings every N completed games (default: 20).
Passes all fastchess output through unchanged.
"""

import sys
import re
import math
import collections
import argparse
import os

# ── CCRL Blitz ELO seeds (March 2026) ────────────────────────────────────────
CCRL = {
    "stockfish":  3792, "stormphrax": 3750, "viridithas": 3742,
    "koivisto":   3689, "tcheran":    3634, "blackmarlin": 3629,
    "akimbo":     3621, "patricia":   3542, "carp":        3529,
    "blackcore":  3444, "avalanche":  3396, "frozenight":  3367,
    "nalwald":    3346, "stockdory":  3400, "wahoo":       3085,
    "inanis":     3084, "4ku":        3061, "aurora":      2873,
    "apotheosis": 2748, "tantabus":   2553, "tofiks":      1781,
}

# ── Argument parsing ──────────────────────────────────────────────────────────
parser = argparse.ArgumentParser(add_help=False)
parser.add_argument("--every", type=int, default=20)
args, _ = parser.parse_known_args()
UPDATE_EVERY = args.every

# ── State ─────────────────────────────────────────────────────────────────────
scores   = collections.defaultdict(float)
games    = collections.defaultdict(int)
vs       = collections.defaultdict(lambda: collections.defaultdict(float))
vs_games = collections.defaultdict(lambda: collections.defaultdict(int))
completed = 0

# regex: "Finished game N (A vs B): result {..."
FINISHED_RE = re.compile(
    r'Finished game \d+ \((.+?) vs (.+?)\): (1-0|0-1|1/2-1/2)'
)

# ── ELO helpers ───────────────────────────────────────────────────────────────
def expected(ra, rb):
    return 1.0 / (1.0 + 10 ** ((rb - ra) / 400.0))

def estimate_elo(engine, iterations=400):
    known = [(opp, vs[engine][opp], vs_games[engine][opp])
             for opp in vs[engine]
             if opp in CCRL and vs_games[engine][opp] > 0]
    if not known:
        return None
    r = float(sum(CCRL[opp] for opp, _, _ in known) / len(known))
    for _ in range(iterations):
        num = den = 0.0
        for opp, sc, g in known:
            e = expected(r, CCRL[opp])
            num += sc - g * e
            # derivative of expected score wrt r
            den += g * e * (1 - e) * math.log(10) / 400.0
        if abs(den) < 1e-9:
            break
        r += num / den
    return round(r)

def clear_lines(n):
    """Move cursor up n lines and clear them (ANSI)."""
    if n > 0 and sys.stdout.isatty():
        sys.stdout.write(f"\033[{n}A\033[J")

LAST_TABLE_LINES = 0

def print_standings():
    global LAST_TABLE_LINES

    all_engines = set(scores.keys()) | set(CCRL.keys())
    # only show engines that have played at least 1 game
    active = [e for e in all_engines if games[e] > 0]
    if not active:
        return

    est = {}
    for eng in active:
        if eng in CCRL:
            # for seeded engines, show their perf rating if they've played enough,
            # else show the seed
            perf = estimate_elo(eng)
            est[eng] = perf if perf is not None else CCRL[eng]
        else:
            e = estimate_elo(eng)
            est[eng] = e if e is not None else "?"

    ranked = sorted(active, key=lambda e: scores[e] / max(games[e], 1), reverse=True)

    lines = []
    lines.append(f"\n  ── Live standings ({completed} games complete) ──")
    lines.append(f"  {'Rank':<5} {'Engine':<22} {'Score':>7} {'G':>4} {'%':>6}  {'Est.Elo':>8}  {'Seed':>6}")
    lines.append("  " + "─" * 62)
    for rank, eng in enumerate(ranked, 1):
        g   = games[eng]
        sc  = scores[eng]
        pct = 100 * sc / g if g else 0.0
        elo = est.get(eng, "?")
        seed = CCRL.get(eng, "---")
        lines.append(f"  {rank:<5} {eng:<22} {sc:>7.1f} {g:>4} {pct:>5.1f}%  {str(elo):>8}  {str(seed):>6}")
    lines.append("")

    table = "\n".join(lines)

    clear_lines(LAST_TABLE_LINES)
    print(table, flush=True)
    LAST_TABLE_LINES = len(lines) + 1   # +1 for the leading \n

# ── Main loop ─────────────────────────────────────────────────────────────────
# Lines to suppress — fastchess debug/engine-comms noise
SUPPRESS_PREFIXES = ("Info;", "Position;", "Moves;")

for raw in sys.stdin:
    # suppress engine debug spam, pass everything else through
    if not raw.startswith(SUPPRESS_PREFIXES):
        sys.stdout.write(raw)
        sys.stdout.flush()

    m = FINISHED_RE.search(raw)
    if not m:
        continue

    white, black, result = m.group(1), m.group(2), m.group(3)
    if result == "1-0":
        sw, sb = 1.0, 0.0
    elif result == "0-1":
        sw, sb = 0.0, 1.0
    else:
        sw, sb = 0.5, 0.5

    scores[white] += sw;  scores[black] += sb
    games[white]  += 1;   games[black]  += 1
    vs[white][black] += sw;  vs_games[white][black] += 1
    vs[black][white] += sb;  vs_games[black][white] += 1
    completed += 1

    if completed % UPDATE_EVERY == 0:
        print_standings()

# Final standings
print_standings()
