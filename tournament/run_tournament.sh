#!/usr/bin/env bash
# run_tournament.sh
#
# Runs HiveChess as the gauntlet challenger against a spread of CCRL-rated engines.
# Gauntlet format: HiveChess plays every other engine, opponents don't play each other.
#
# CCRL Blitz conditions:
#   Time control : 2'+1"  (120 s + 1 s / move)
#   Hash         : 128 MB per engine
#   Threads      : 1 per engine
#   Pondering    : OFF
#   No adjudication – engines play every game to completion
#
# Usage:
#   ./run_tournament.sh [OPTIONS]
#
# Options:
#   -c N    Concurrency (default: 200)
#   -r N    Rounds per pairing (default: 2 → 4 games each, 2 per color)
#   -t PATH Syzygy tablebase path (optional)
#   -o FILE Output PGN (default: results.pgn)
#   -h      Show this help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ENGINES_DIR="$SCRIPT_DIR/engines"
BOOKS_DIR="$SCRIPT_DIR/books"
BOOK="$BOOKS_DIR/ccrl.epd"
FASTCHESS="$REPO_ROOT/tools/fastchess"
OUTPUT_PGN="$SCRIPT_DIR/results.pgn"

TC="30+0.3"
HASH=128
THREADS=1
CONCURRENCY=200
ROUNDS=20
SYZYGY_PATH=""

while getopts "c:r:t:o:h" opt; do
    case $opt in
        c) CONCURRENCY="$OPTARG" ;;
        r) ROUNDS="$OPTARG" ;;
        t) SYZYGY_PATH="$OPTARG" ;;
        o) OUTPUT_PGN="$OPTARG" ;;
        h) grep '^#' "$0" | grep -v '#!/' | sed 's/^# \?//'; exit 0 ;;
        *) echo "Unknown option $opt"; exit 1 ;;
    esac
done

# ── Sanity checks ─────────────────────────────────────────────────────────────

[[ -x "$FASTCHESS" ]] || { echo "ERROR: fastchess not found at $FASTCHESS"; exit 1; }
[[ -f "$BOOK"      ]] || { echo "ERROR: Book not found at $BOOK"; exit 1; }

HIVECHESS="$ENGINES_DIR/hivechess"
[[ -x "$HIVECHESS" ]] || { echo "ERROR: hivechess not found at $HIVECHESS"; exit 1; }

# All opponents = every executable in engines/ except hivechess
mapfile -t OPPONENTS < <(
    find "$ENGINES_DIR" -maxdepth 1 \( -type f -o -type l \) -executable \
    | grep -v '/hivechess$' | sort
)

[[ ${#OPPONENTS[@]} -ge 1 ]] || { echo "ERROR: No opponent engines found in $ENGINES_DIR"; exit 1; }

# ── Summary ───────────────────────────────────────────────────────────────────

N_OPP=${#OPPONENTS[@]}
PAIRS=$N_OPP
TOTAL=$(( PAIRS * ROUNDS * 2 ))

echo "================================================================"
echo "  HiveChess Gauntlet  (CCRL Blitz conditions)"
echo "================================================================"
echo "  Challenger   : hivechess"
echo "  Opponents    : $N_OPP"
for o in "${OPPONENTS[@]}"; do printf "    %s\n" "$(basename "$o")"; done
echo "  Time control : $TC  (2'+1\")"
echo "  Hash         : ${HASH} MB"
echo "  Threads      : $THREADS"
echo "  Adjudication : none"
echo "  Rounds       : $ROUNDS (× 2 colors = $((ROUNDS*2)) games/opponent)"
echo "  Total games  : $TOTAL  ($PAIRS opponents × ${ROUNDS} rounds × 2 colors)"
echo "  Concurrency  : $CONCURRENCY"
echo "  Output PGN   : $OUTPUT_PGN"
echo "================================================================"
echo ""
read -r -p "Press ENTER to start or Ctrl-C to abort ..."
echo ""

# ── Build command ─────────────────────────────────────────────────────────────

CMD=("$FASTCHESS")

# HiveChess must be listed FIRST for gauntlet (-seeds 1)
CMD+=(-engine "cmd=$HIVECHESS" "name=hivechess"
      "option.Hash=$HASH" "option.Threads=$THREADS")

for bin in "${OPPONENTS[@]}"; do
    name="$(basename "$bin")"
    CMD+=(-engine "cmd=$bin" "name=$name"
          "option.Hash=$HASH")
    [[ -n "$SYZYGY_PATH" ]] && CMD+=("option.SyzygyPath=$SYZYGY_PATH")
done

CMD+=(-each "tc=$TC")
CMD+=(-tournament gauntlet -seeds 1)
CMD+=(-openings "file=$BOOK" "format=epd" "order=random")
CMD+=(-rounds "$ROUNDS" -repeat -concurrency "$CONCURRENCY")
CMD+=(-pgnout "file=$OUTPUT_PGN" "append=false")
CMD+=(-autosaveinterval 1)
CMD+=(-maxmoves 300)   # hard cap: 300-move game = draw (covers threefold/50-move bugs)
CMD+=(-output "format=cutechess")

# ── Run ───────────────────────────────────────────────────────────────────────

echo "Running fastchess..."
echo ""

"${CMD[@]}" | python3 "$SCRIPT_DIR/live_elo.py" --every 10

echo ""
echo "================================================================"
echo "  Done. PGN saved to: $OUTPUT_PGN"
echo "  Run: python3 $SCRIPT_DIR/calculate_elo.py $OUTPUT_PGN"
echo "================================================================"
