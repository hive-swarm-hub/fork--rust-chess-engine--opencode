# Rust Chess Engine — Maximize ELO Rating

Improve a UCI chess engine in Rust to maximize ELO rating. Engine plays a parallel SPRT (Sequential Probability Ratio Test) gauntlet vs Stockfish (5 levels: 2600-3000). Baseline: ~2435 ELO (ported from deedy/chess).

**Baseline engine** ported from [github.com/deedy/chess](https://github.com/deedy/chess) (Deedy Das's vibecoded engine, UCI wrapper added for standalone operation). Current baseline: ~2435 ELO.

## Setup

1. **Read the in-scope files**:
   - `engine/src/main.rs` — the main engine file you modify. A minimal UCI chess engine.
   - `engine/Cargo.toml` — Rust project config. You may add dependencies.
   - `eval/eval.sh` — compiles and runs the gauntlet. Do not modify.
   - `eval/compute_elo.py` — computes ELO from game results. Do not modify.
   - `eval/openings.epd` — fallback opening positions. Do not modify.
   - `prepare.sh` — installs Rust, Stockfish, fastchess. Do not modify.
2. **Run prepare**: `bash prepare.sh` to install dependencies and download the Drawkiller opening book.
3. **Verify setup**: Check that `tools/stockfish` and `tools/fastchess` exist.
4. **Run baseline**: `bash eval/eval.sh > run.log 2>&1` to establish the starting ELO.

## The benchmark

The challenge: build the strongest chess engine you can in Rust, measured by ELO rating in a gauntlet tournament against Stockfish.

- **Metric**: `elo` — estimated ELO rating from gauntlet results. **Higher is better.**
- **Source limit**: Total lines under `engine/src/` must be <= 10000 lines
- **Binary size limit**: Compiled engine binary must be <= 100MB
- **Compile time limit**: `cargo build --release` must complete within 5 minutes
- **Eval time limit**: Gauntlet tournament must complete within 30 minutes
- **Baseline**: ~2435 ELO (Deedy's engine with TT, LMR, null move, PVS, PSTs, king safety)

### Gauntlet setup

The eval uses a parallel SPRT methodology:

- **Opponents**: Stockfish with `UCI_LimitStrength` at 5 levels: 2600, 2700, 2800, 2900, 3000.
- **Anchor center**: 2800
- **Time control**: 40 moves in 2 minutes (40/120) for engine, 40 moves in 24 seconds for Stockfish.
- **SPRT**: Sequential Probability Ratio Test for fast, statistically significant results.
- **Opening Book**: 15,962-position Drawkiller EPD suite.

### ELO reference points

| ELO   | Level                          |
|-------|--------------------------------|
| 2435  | Baseline (this engine, HCE)    |
| 2718  | Deedy's Lichess rating         |
| 3000  | Strong engine                  |
| 3600+ | Stockfish / Viridithas (SOTA)  |

### Strategies to improve

The eval is fixed — agents improve the **engine code** under `engine/`. Possible strategies:

- **Search improvements**: Singular extensions, multi-cut pruning, countermove history, better LMR tuning
- **NNUE evaluation**: Replace hand-crafted eval with a trained neural network (biggest single gain, +500 ELO)
- **Opening book**: Embed opening lines in the engine to save thinking time in the first moves
- **Endgame tablebases**: Integrate Syzygy tablebases for perfect endgame play
- **Multi-threaded search**: Tune Lazy SMP parallelism for the time control
- **Parameter tuning**: Use SPSA or similar to optimize search/eval constants
- **Time management**: Allocate more time in complex middlegame positions, less in simple endgames

## Experimentation

**What you CAN modify:**
- `engine/src/main.rs` — search, evaluation, move ordering, everything
- `engine/src/*.rs` — you may create additional source files and modules
- `engine/Cargo.toml` — add dependencies (NNUE crates, bitboard libraries, etc.)
- `engine/build.rs` — add build scripts if needed (e.g., for embedding NNUE weights)
- Any data files under `engine/` (e.g., NNUE weight files, opening books for the engine)

**What you CANNOT modify:**
- `eval/eval.sh`, `eval/compute_elo.py`, `eval/openings.epd`
- `prepare.sh`
- `tools/` directory (Stockfish, fastchess binaries)

**Anti-cheat rules (enforced by eval.sh):**
- **No network access**: Engine source must not contain TCP, HTTP, or socket code. No calling external APIs for moves.
- **No process spawning**: Engine must not spawn Stockfish or any other engine as a subprocess.
- **No reading protected paths**: Engine must not access `tools/`, `eval/`, or system paths like `/proc/`.
- **Tool integrity**: SHA-256 checksums of Stockfish and fastchess are verified before each eval. Tampering = invalid.
- **Protected files**: Git diff is checked — modifications to eval scripts, prepare.sh, or tools invalidate the run.

The engine must play chess on its own. Strength must come from search + evaluation, not from gaming the eval infrastructure.

**The goal: maximize ELO.** Higher is better. Every improvement counts.
