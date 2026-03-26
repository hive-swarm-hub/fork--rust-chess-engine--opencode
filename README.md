# Rust Chess Engine — Maximize ELO

Build and iteratively improve a UCI chess engine in Rust. Your engine plays a parallel SPRT (Sequential Probability Ratio Test) gauntlet against Stockfish levels. The score is your estimated ELO rating — higher is better.

**Baseline:** ~2435 ELO (ported from [github.com/deedy/chess](https://github.com/deedy/chess) — TT, LMR, null move, PVS, killer moves, SEE, PSTs, king safety)
**Anchor Center:** 2800 ELO

## Quick Start

```bash
bash prepare.sh          # Install Rust, Stockfish, fastchess
bash eval/eval.sh        # Compile engine + run parallel SPRT gauntlet + compute ELO
```

## What You Modify

Everything under `engine/` is fair game:

```
engine/
  Cargo.toml        # Add dependencies (NNUE crates, bitboard libs, etc.)
  src/main.rs       # Search, evaluation, move ordering — the engine itself
  src/*.rs          # Create additional modules as needed
```

## What You Cannot Modify

- `eval/` — Evaluation scripts (gauntlet runner, ELO computation)
- `prepare.sh` — Setup script
- `tools/` — Stockfish, fastchess binaries

## How Evaluation Works

```
cargo build --release
       |
       v
Parallel gauntlet via fastchess (40 moves / 2 minutes)
       |
       +--> vs Stockfish UCI_LimitStrength (5 levels: 2600-3000)
       |
       v
SPRT (Sequential Probability Ratio Test)
Terminates early once a result is statistically significant.
```

## Gauntlet Opponents

| Opponent | Rating | Source |
|----------|--------|--------|
| SF 2600 | 2600 | Stockfish UCI_LimitStrength |
| SF 2700 | 2700 | Stockfish UCI_LimitStrength |
| SF 2800 | 2800 | Stockfish UCI_LimitStrength |
| SF 2900 | 2900 | Stockfish UCI_LimitStrength |
| SF 3000 | 3000 | Stockfish UCI_LimitStrength |

## Improvement Roadmap

| Phase | ELO Range | Key Techniques |
|-------|-----------|----------------|
| Baseline | ~2435 | TT, LMR, null move, PVS, killer moves, SEE, PSTs, king safety |
| Core | 1500-2400 | Transposition table, MVV-LVA, killer moves, null move pruning, LMR |
| Evaluation | 2400-3000 | Piece-square tables, pawn structure, king safety, mobility |
| Elite | 3000+ | NNUE evaluation, Singular extensions, Multi-threaded search |

## Anti-Cheat

The eval enforces:
- SHA-256 checksums on all tool binaries (no tampering with opponents)
- Source scan for network access, process spawning, protected path reads
- Git diff check on protected files
- 15,962-position Drawkiller opening book (no memorization)

## File Structure

```
rust_chess_engine/
  program.md           # Full task spec (agent reads this)
  prepare.sh           # One-time setup
  requirements.txt     # Python dependencies
  gen_openings.py      # Opening book generator (fallback)
  engine/              # YOUR CODE — modify freely
    Cargo.toml
    src/main.rs
  eval/                # READ ONLY
    eval.sh            # Compile + gauntlet + scoring
    compute_elo.py     # MLE ELO estimation
    openings.epd       # Fallback opening book (30 positions)
  data/                # Created by prepare.sh
    openings.epd       # High-quality Drawkiller opening book
  tools/               # Created by prepare.sh
    stockfish
    fastchess
```
