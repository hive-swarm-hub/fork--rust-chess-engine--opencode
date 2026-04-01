#!/usr/bin/env python3
"""
calculate_elo.py — compute HiveChess performance ELO from a gauntlet PGN.

Usage:
    python3 calculate_elo.py results.pgn [--challenger hivechess]
"""

import sys
import re
import math
import argparse
import collections

# CCRL Blitz ELO seeds (March 2026)
CCRL = {
    "stockfish":  3792, "stormphrax": 3750, "viridithas": 3742,
    "lizard":     3740, "koivisto":   3689, "tcheran":    3634,
    "blackmarlin": 3629, "akimbo":    3621, "patricia":   3542,
    "carp":        3529, "blackcore": 3444, "avalanche":  3396,
    "frozenight":  3367, "nalwald":   3346, "stockdory":  3400,
    "wahoo":       3085, "inanis":    3084, "4ku":        3061,
    "aurora":      2873, "apotheosis": 2748, "tantabus":  2553,
    "oxidation":   2362, "tofiks":    1781,
}

def parse_pgn(path):
    scores   = collections.defaultdict(float)
    games    = collections.defaultdict(int)
    vs       = collections.defaultdict(lambda: collections.defaultdict(float))
    vs_games = collections.defaultdict(lambda: collections.defaultdict(int))
    vs_wdl   = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0, 0]))

    with open(path) as f:
        white = black = result = None
        for line in f:
            line = line.strip()
            m = re.match(r'\[White "(.+?)"\]', line)
            if m: white = m.group(1)
            m = re.match(r'\[Black "(.+?)"\]', line)
            if m: black = m.group(1)
            m = re.match(r'\[Result "(.+?)"\]', line)
            if m: result = m.group(1)
            if white and black and result:
                if result == "1-0":
                    sw, sb = 1.0, 0.0
                    vs_wdl[white][black][0] += 1  # white win
                    vs_wdl[black][white][2] += 1  # black loss
                elif result == "0-1":
                    sw, sb = 0.0, 1.0
                    vs_wdl[black][white][0] += 1  # black win
                    vs_wdl[white][black][2] += 1  # white loss
                elif result == "1/2-1/2":
                    sw, sb = 0.5, 0.5
                    vs_wdl[white][black][1] += 1
                    vs_wdl[black][white][1] += 1
                else:
                    white = black = result = None
                    continue
                scores[white] += sw;  scores[black] += sb
                games[white]  += 1;   games[black]  += 1
                vs[white][black] += sw;  vs_games[white][black] += 1
                vs[black][white] += sb;  vs_games[black][white] += 1
                white = black = result = None

    return scores, games, vs, vs_games, vs_wdl

def expected(ra, rb):
    return 1.0 / (1.0 + 10 ** ((rb - ra) / 400.0))

def performance_elo(engine, vs, vs_games, seed_elos, iterations=1000):
    """Newton's method to find r such that sum of expected scores = actual score."""
    known = [
        (opp, vs[engine][opp], vs_games[engine][opp])
        for opp in vs[engine]
        if opp in seed_elos and vs_games[engine][opp] > 0
    ]
    if not known:
        return None, 0

    total_games = sum(g for _, _, g in known)
    total_score = sum(sc for _, sc, _ in known)

    # start from weighted average of opponent ratings
    r = sum(seed_elos[opp] * g for opp, _, g in known) / total_games

    for _ in range(iterations):
        grad = hess = 0.0
        for opp, sc, g in known:
            e = expected(r, seed_elos[opp])
            grad += sc - g * e
            hess -= g * e * (1 - e) * math.log(10) / 400.0
        if abs(hess) < 1e-12:
            break
        step = grad / hess
        r -= step
        if abs(step) < 0.001:
            break

    return round(r), total_games

def error_bar(n_games, score_pct, z=1.96):
    """95% confidence interval on ELO using normal approximation."""
    if score_pct <= 0 or score_pct >= 1:
        return float('inf')
    variance = score_pct * (1 - score_pct) / n_games
    # propagate through ELO formula: d(ELO)/d(pct) = 400*log10(e) / (pct*(1-pct))
    dpct = score_pct * (1 - score_pct)
    dElo_dpct = 400 * math.log10(math.e) / dpct
    return round(z * math.sqrt(variance) * abs(dElo_dpct))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("pgn", help="Path to PGN file")
    parser.add_argument("--challenger", default="hivechess")
    parser.add_argument("--output", help="Save report to this file")
    args = parser.parse_args()

    scores, games, vs, vs_games, vs_wdl = parse_pgn(args.pgn)
    challenger = args.challenger

    out = open(args.output, "w") if args.output else None
    def emit(line=""):
        print(line)
        if out: out.write(line + "\n")

    if challenger not in games:
        print(f"ERROR: '{challenger}' not found in PGN.")
        sys.exit(1)

    emit(f"\n{'='*65}")
    emit(f"  Performance ELO Report  —  {challenger}")
    emit(f"{'='*65}\n")

    # Per-opponent breakdown
    emit(f"  {'Opponent':<20} {'CCRL':>6}  {'Score':>10}  {'G':>4}  {'%':>6}")
    emit("  " + "─" * 55)

    opponents = sorted(
        vs[challenger].keys(),
        key=lambda o: CCRL.get(o, 0),
        reverse=True
    )

    for opp in opponents:
        g    = vs_games[challenger][opp]
        sc   = vs[challenger][opp]
        pct  = 100 * sc / g if g else 0
        seed = CCRL.get(opp, "?")
        emit(f"  {opp:<20} {str(seed):>6}  {sc:>5.1f}/{g:<4}  {g:>4}  {pct:>5.1f}%")

    # Overall performance ELO
    elo, n = performance_elo(challenger, vs, vs_games, CCRL)
    total_sc = scores[challenger]
    total_g  = games[challenger]
    overall_pct = total_sc / total_g if total_g else 0

    if elo is not None:
        ci = error_bar(n, overall_pct)
        emit(f"\n  {'─'*55}")
        emit(f"  Overall score   : {total_sc:.1f} / {total_g}  ({100*overall_pct:.1f}%)")
        emit(f"  Performance ELO : {elo}  (±{ci} at 95% CI)")
        emit(f"  Games vs known  : {n}")
    else:
        emit(f"\n  Could not estimate ELO (no games vs CCRL-seeded opponents)")

    # Head-to-head W/D/L table
    emit(f"\n  {'─'*55}")
    emit(f"  {'Opponent':<20}  {'W':>4}  {'D':>4}  {'L':>4}")
    emit("  " + "─" * 40)
    total_w = total_d = total_l = 0
    for opp in opponents:
        g = vs_games[challenger][opp]
        if g == 0: continue
        w, d, l = vs_wdl[challenger][opp]
        total_w += w; total_d += d; total_l += l
        emit(f"  {opp:<20}  {w:>4}  {d:>4}  {l:>4}")
    emit("  " + "─" * 40)
    emit(f"  {'Total':<20}  {total_w:>4}  {total_d:>4}  {total_l:>4}")
    emit()

    if out:
        out.close()

if __name__ == "__main__":
    main()
