# Rust Chess Engine — Maximize ELO

Build and iteratively improve a UCI chess engine in Rust. Your engine plays a parallel SPRT (Sequential Probability Ratio Test) gauntlet against Stockfish at calibrated strength levels and CCRL-rated reference engines. The score is your estimated ELO rating — higher is better.

**Baseline:** ~2435 ELO (ported from [github.com/deedy/chess](https://github.com/deedy/chess) — TT, LMR, null move, PVS, killer moves, SEE, PSTs, king safety)
**Target:** As high as possible (2718 = Deedy's benchmark, 3723 = Stormphrax ceiling)
**Ceiling:** 3723 ELO (Stormphrax, strongest CCRL-rated opponent)

## Quick Start

```bash
bash prepare.sh          # Install Rust, Stockfish, fastchess, reference engines
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
- `tools/` — Stockfish, fastchess, reference engine binaries

## How Evaluation Works

```
cargo build --release
       |
       v
Parallel gauntlet via fastchess (40 moves / 2 minutes)
       |
       +--> vs Stockfish UCI_LimitStrength (5 levels: 2600-3000)
       +--> vs CCRL-rated engines (Blunder, Inanis, Mantissa, Stormphrax)
       |
       v
SPRT (Sequential Probability Ratio Test)
Terminates early once a result is statistically significant.
```

## Gauntlet Opponents

| Opponent | Rating | Source |
|----------|--------|--------|
| SF 1000-2000 | 1000-2000 | Stockfish UCI_LimitStrength |
| Blunder 6.1 | 2105 | CCRL Blitz verified |
| Blunder 8.5 | 2667 | CCRL Blitz verified |
| Inanis 1.6 | 3085 | CCRL Blitz verified |
| Mantissa 3.7 | 3317 | CCRL Blitz verified |
| Stormphrax 7.0 | 3723 | CCRL Blitz verified |
| SF depth 4-10 | ~1500-2700 | Cross-validation |

## Improvement Roadmap

| Phase | ELO Range | Key Techniques |
|-------|-----------|----------------|
| Baseline | ~2435 | TT, LMR, null move, PVS, killer moves, SEE, PSTs, king safety |
| Core | 1500-2000 | Transposition table, MVV-LVA, killer moves, null move pruning, LMR |
| Evaluation | 2000-2400 | Piece-square tables, pawn structure, king safety, mobility |
| Advanced | 2400-2800 | NNUE evaluation, PVS, singular extensions, Syzygy tablebases |
| Elite | 2800+ | Multi-threaded search, SPSA tuning, NNUE self-play training |

## Anti-Cheat

The eval enforces:
- SHA-256 checksums on all tool binaries (no tampering with opponents)
- Source scan for network access, process spawning, protected path reads
- Git diff check on protected files
- Cross-validation between opponent types (detects reward hacking)
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
    openings.epd       # High-quality Drawkiller opening book (15,962 positions)
  tools/               # Created by prepare.sh
    stockfish
    fastchess
    blunder-6, blunder-8, inanis, mantissa, stormphrax
```
