---
title: "Building a 3,200 ELO Chess Engine with Hive"
author: "Pinak Paliwal & Hive Team"
author_line: "Pinak Paliwal & Hive Team"
date: "2026-03-31"
citation_key: "hive2026chess"
github: "https://github.com/hive-swarm-hub/fork--rust-chess-engine--opencode"
---
<aside>

## TL;DR

Hive improved a Rust chess engine from **2,324 to 3,208 ELO** (+884 points) in 2-3 weeks. It ran 100+ experiments, found that NNUE evaluation was worth ~+500 ELO over the hand-crafted baseline, and picked up another ~+380 from search improvements and correction history.

The gauntlet runs in 8-15 minutes with parallel SPRT. Full iteration (read code, write change, test) is 1-4 hours depending on complexity.

👨‍💻 [Code](https://github.com/hive-swarm-hub/fork--rust-chess-engine--opencode)

</aside>

## The Starting Point

The engine started as a Rust port of [github.com/deedy/chess](https://github.com/deedy/chess), Deedy Das's Python chess engine. The original had the basics—alpha-beta search, piece-square tables, transposition table, null move pruning, LMR, killer moves, SEE—and was ported to Rust to get compiled-language speeds and UCI support for tournament play. 

First gauntlet result: **2,324 ELO**.

## From 2,324 to 3,208 ELO in Three Weeks

The engine started at 2,324 ELO—alpha-beta with a hand-crafted eval, transposition table, null move pruning, LMR. Decent but nothing special. I pointed Hive at it and told it to maximize ELO. Three weeks later: **3,208 ELO**, SPRT-confirmed over 201 games against Stockfish 2600-3000.

The loop is: Hive reads the code, proposes a change, compiles, runs a gauntlet, keeps it if ELO goes up, reverts if not. With little/no knowledge of what makes a chess engine good required on our end, Hive was able to optimize it to become a top 150 engine.

## Iteration Speed

The key bottleneck with human created engines is the testing. Some of the top engines have massive cpu farms dedicated to testing ideas. The full `lscpu` output is provided at the end of this blog for proper benchmarking.

The gauntlet uses **fastchess** with SPRT (`elo0=0 elo1=35`, α=β=0.05). SPRT stops as soon as the result is statistically clear—sometimes at 80 games, sometimes 200. With `nproc * 90%` concurrency (216 threads on the eval machine), each run takes **8 minutes**. The full agent cycle including reading the code, planning, implementing, and running is 1-4 hours depending on complexity.

> **Eval machine**: AMD EPYC 7B12, 2× 60-core / 120-thread sockets (240 logical CPUs), x86_64. AVX2 available but not used by the engine's NNUE inference (nnue-rs is scalar).

Compare: the LMP change (restrict late move pruning to cut nodes only) took Hive about 2 hours total. It found +9.2 ELO and moved on. A human doing the same would spend a day implementing, whereas it is extremely quick here.

The bad experiments, such as check extensions lost **-611 ELO** because it caused unbounded depth explosions, get reverted in the same time. There is little wasted time, allowing the engine to become stronger, faster.

## The Evaluation Script

The gauntlet evolved a lot over the three weeks. It started as a basic cutechess-cli script running a fixed number of games against SF 2200. By the end it was running in parallel and ensuring statistical significance. 

**Throughout — anti-cheat hardening.** SHA-256 checksums on the Stockfish and fastchess binaries. A git diff check that fails the run if eval.sh or compute_elo.py was modified. A source scan that rejects any engine Rust code containing network calls, filesystem paths outside `engine/`, or subprocess spawning. This was added after realizing the eval loop is an attack surface if you're running untrusted agents.

**March 25 — basic setup.** Single-threaded cutechess-cli, fixed game count, anchor at SF 2200. Each run took 2-4 minutes. The engine quickly saturated at this level, so the anchor moved to SF 2800.

**March 26 — parallel SPRT.** Switched to fastchess for true parallel execution. Added SPRT so the gauntlet stops early instead of running the full game count. Spread opponents across 5 SF levels (2600-3000). Added adjudication—resign at +600cp for 3 moves, draw at <10cp for 8 moves past move 34. This dropped run time from hours to 8 minutes. SPRT is also used by most of the chess engine community, as it guarantees that the engine improvements are statistically significant. 

**March 30 — equal time control.** The original script gave the engine 5× more time than Stockfish (engine 40/120, SF 40/24). Switched to equal time (40/120 both sides) to get numbers more similar to the official CCRL list.

**March 31 — H2H validation.** Added a second stage: if the gauntlet ELO beats the stored best, run a direct head-to-head against the previous best binary using a tighter SPRT (`elo0=0 elo1=20`, up to 1000 games). Only update the "best" if H2H confirms improvement. This caught several cases where gauntlet variance gave a false positive (due to statistical variance). For ideas that only slightly boosted Elo (such as only +20 Elo), this helped ensure that this was not just random chance.

Final system: compile (~10s) → gauntlet (8 min) → H2H if new best (15 min) → save or revert.

## What Hive Found

### NNUE: the big one

The engine used a hand-crafted eval—piece-square tables, king safety, mobility bonuses. This was a good starting point.

The first NNUE experiment used a **21 MB HalfKP network** (SF12 era, via the `nnue-rs` crate). In a single 10-game test against the HCE baseline (~2465 true ELO), it scored 2969—about +500 ELO. Noisy from 10 games, but it was a clear improvement over the original.

The current engine uses the **Reckless v58 network** (`v58.nnue`, 61 MB): king-bucketed with 10 input buckets and 8 output buckets, 768→16→32→1 hidden layers, and 66,864 threat input features. It's implemented from scratch in `reckless_nnue.rs`—no external crate.

We also tried the HalfKAv2 net from SF18. It was 10× slower per eval without AVX2 intrinsics, dropping search depth from 14-16 to 9-11 and losing ~700 ELO.

Interestingly, two changes that did nothing on the HCE base—eval cache (2M entries) and IIR—each gave ~100-150 ELO when retested on the NNUE base. The eval is more expensive now, so caching and reducing redundant re-evaluations actually matter.

### Correction history

Even with NNUE, the engine's static eval has systematic biases. Correction history (a Stockfish SF18 technique) tracks the difference between what the eval predicted and what the search actually found, bucketed by pawn structure and material configuration. Those corrections get applied before pruning decisions.

Porting this from the Reckless v58 reference implementation, along with several search refinements, gave about +207 ELO over the ~2918 SPRT baseline.

### LMP at cut nodes only

The original late move pruning code applied at all nodes. Hive noticed that PV nodes—the main line—need accurate move counts, not early cutoffs. Restricting LMP to cut nodes only gave +9.2 ELO (SPRT-confirmed, 201 games).

### IIR instead of IID

Without a TT move, the standard approach is Internal Iterative Deepening—a full sub-search to find a candidate. Hive tried just reducing depth by 1 instead (IIR). On the NNUE base, this gave +150 ELO. Simpler and faster.

## What Didn't Work

A lot of ideas failed:

- **Check extensions**: -611 ELO. Some positions went to depth 40+ before returning.
- **Hindsight extensions** (extend when parent over-reduced): -138 ELO. Same problem.
- **Triple singular extensions**: -121 ELO. Needed depth capping that wasn't there.
- **LMR alpha-raises**: -74 ELO. Reduced moves that needed full search.
- **Draw noise randomization**: -54 ELO. The alternating ±1 introduced inconsistency.
- **NMP at cut nodes only**: -36 ELO. Turns out null-move pruning at PV nodes is valuable.
- **History-aware LMP**: -497 ELO (catastrophic). Negative history scores caused massive over-pruning.

The value here is speed. Each of these was tried, measured, and reverted in under half an hour. Without fast feedback, any of them could have been incorrectly dragging the engine performance down.

## The Tournament System

The eval.sh gauntlet measures improvement over time but uses the same fixed set of SF-limited opponents. For a real-world strength estimate, there's a separate tournament system (`tournament/run_tournament.sh`) that plays HiveChess against actual CCRL-rated engines under CCRL Blitz conditions (2'+1", 128 MB hash, 1 thread, no adjudication).

The full field (23 opponents, CCRL Blitz ELO from March 2026 list):

| Engine | Version | CCRL Blitz | Notes |
|--------|---------|-----------|-------|
| Stockfish | 17.1 | 3792 | 8CPU |
| Stormphrax | 7.0.0 | 3750 | 8CPU |
| Viridithas | 19.0.1 | 3742 | |
| Lizard | 11.2 | 3740 | 8CPU |
| Koivisto | 9.0 | 3689 | 8CPU |
| Tcheran | 11.0 | 3634 | |
| Black Marlin | 9.0 | 3629 | 8CPU |
| akimbo | 1.0.0 | 3621 | 8CPU |
| Patricia | 5.0 | 3542 | |
| Carp | 3.0.1 | 3529 | |
| BlackCore | 6.0 | 3444 | |
| StockDory | Starfish 0.1 | 3400 | |
| Avalanche | 2.1.0 | 3396 | |
| Frozenight | 6.0.0 | 3367 | |
| Nalwald | 19 | 3346 | |
| Wahoo | 4.0.0 | 3085 | |
| Inanis | 1.6.0 | 3084 | |
| 4ku | 5.1 | 3061 | |
| Aurora | 1.26.0 | 2873 | |
| Apotheosis | 4.0.1 | 2748 | |
| Tantabus | 2.0.0 | 2553 | |
| Oxidation | 0.7.1 | 2362 | |
| Tofiks | 1.3.0 | 1781 | |

Fastchess runs these at up to 200 concurrency, feeding results live to `live_elo.py` which prints a running ELO every 10 games. The UHO Lichess opening book is used instead of the training openings. This gives an independent sanity check on whether the gauntlet numbers are real, or if the engine is overfitting to strong openings.

## Why Chess Works Well for This

ELO is verifiable, with a clear win/lose signal. With SPRT, as other chess engine creators have found out, you can get statistical significance if a change makes the engine better or worse. The test suite (Stockfish at different strength levels) is reproducible and freely available. And the techniques themselves are well-documented: Stockfish, Ethereal, and others are open source with extensive writeups. Hive isn't discovering new chess theory (yet), it's applying and tuning known techniques. In the future, we see the opportunity for quick tuning of the parameters within a chess engine, with verifiable outcomes. In the future, as the LLMs improve, we forsee the possibility of new ideas being discovered, implemented and distributed via LLMs.

That combination—fast feedback, objective score, documented solution space—is what makes autonomous optimization work here. The same setup would apply to compiler flag tuning, database index selection, API timeout parameters, anything where you can express "better" as a number and run the test in under an hour.

## Conclusion

2,324 → 3,208 ELO in three weeks. The biggest single gain was utilizing the Reckless v58 NNUE. Correction history and search tuning added another ~+380. The failed experiments(and there were a lot of them) cost nothing because the eval loop was fast enough to treat each one as disposable. With claude code running different ideas, the cost of testing out a new idea (either generated from an llm, or a human idea) becomes little.

---

## Technical Appendix

### ELO Breakdown


| Change                                     | ELO Δ   | Notes                                                |
| -------------------------------------------- | ---------- | ------------------------------------------------------ |
| NNUE evaluation                            | ~+500    | vs true HCE baseline (~2465); noisy 10-game estimate |
| Eval cache + IIR on NNUE base              | ~+300    | both neutral on HCE base; unlocked by NNUE           |
| Reckless v58 (correction history + search) | ~+207    | vs ~2918 SPRT baseline                               |
| LMP at cut nodes only                      | +9.2     | SPRT-confirmed, 201 games                            |
| **Total**                                  | **+884** | 2324 → 3208                                         |

### Key Parameters

```rust
LMR_DIVISOR: 1.58
NMP_REDUCTION: 3 + depth/4
LMP_MARGIN: 2 + depth² + depth/2
SEE_THRESHOLD: -12 * depth²
```

### NNUE Network

- File: `engine/src/v58.nnue` (Reckless v58)
- Size: 61 MB
- Architecture: king-bucketed (10 input buckets, 8 output buckets), 768→16→32→1 with threat inputs (66,864 threat features)
- Inference: custom scalar implementation in `reckless_nnue.rs`

The original `nn.nnue` (21 MB HalfKP SF12, via `nnue-rs`) was used in early NNUE experiments but is no longer active.

### Memory Usage

- Transposition table: 64 MB
- Eval cache: 16 MB (2M entries)
- History tables: 20 MB
- NNUE accumulator: 196 KB
- **Total: ~120 MB**

### Eval Machine (`lscpu`)

```
Architecture:            x86_64
  CPU op-mode(s):        32-bit, 64-bit
  Address sizes:         48 bits physical, 48 bits virtual
  Byte Order:            Little Endian
CPU(s):                  240
  On-line CPU(s) list:   0-239
Vendor ID:               AuthenticAMD
  Model name:            AMD EPYC 7B12
    CPU family:          23
    Model:               49
    Thread(s) per core:  2
    Core(s) per socket:  60
    Socket(s):           2
    Stepping:            0
    BogoMIPS:            4499.99
    Flags:               fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat
                         pse36 clflush mmx fxsr sse sse2 ht syscall nx mmxext fxsr_opt
                         pdpe1gb rdtscp lm constant_tsc rep_good nopl nonstop_tsc cpuid
                         extd_apicid tsc_known_freq pni pclmulqdq ssse3 fma cx16 sse4_1
                         sse4_2 x2apic movbe popcnt aes xsave avx f16c rdrand hypervisor
                         lahf_lm cmp_legacy cr8_legacy abm sse4a misalignsse 3dnowprefetch
                         osvw topoext ssbd ibrs ibpb stibp vmmcall fsgsbase tsc_adjust bmi1
                         avx2 smep bmi2 rdseed adx smap clflushopt clwb sha_ni xsaveopt
                         xsavec xgetbv1 clzero xsaveerptr arat npt nrip_save umip rdpid

Virtualization features:
  Hypervisor vendor:     KVM
  Virtualization type:   full
Caches (sum of all):
  L1d:                   3.8 MiB (120 instances)
  L1i:                   3.8 MiB (120 instances)
  L2:                    60 MiB (120 instances)
  L3:                    480 MiB (30 instances)
NUMA:
  NUMA node(s):          2
  NUMA node0 CPU(s):     0-59,120-179
  NUMA node1 CPU(s):     60-119,180-239
Vulnerabilities:
  Itlb multihit:         Not affected
  L1tf:                  Not affected
  Mds:                   Not affected
  Meltdown:              Not affected
  Mmio stale data:       Not affected
  Retbleed:              Mitigation; untrained return thunk; SMT enabled with STIBP protection
  Spec store bypass:     Mitigation; Speculative Store Bypass disabled via prctl
  Spectre v1:            Mitigation; usercopy/swapgs barriers and __user pointer sanitization
  Spectre v2:            Mitigation; Retpolines, IBPB conditional, STIBP always-on, RSB filling, PBRSB-eIBRS Not affected
  Srbds:                 Not affected
  Tsx async abort:       Not affected
```

### Tournament Results (CCRL Blitz conditions, 30 games per opponent)

```
=================================================================
  Performance ELO Report  —  hivechess
=================================================================

  Opponent               CCRL       Score     G       %
  ───────────────────────────────────────────────────────
  stockfish              3792    5.0/30      30   16.7%
  stormphrax             3750    3.5/30      30   11.7%
  viridithas             3742    3.5/30      30   11.7%
  lizard                 3740    5.0/30      30   16.7%
  koivisto               3689    6.5/30      30   21.7%
  tcheran                3634    4.5/30      30   15.0%
  blackmarlin            3629    6.5/30      30   21.7%
  akimbo                 3621    8.0/30      30   26.7%
  patricia               3542    8.5/30      30   28.3%
  carp                   3529    6.0/30      30   20.0%
  blackcore              3444   11.5/30      30   38.3%
  stockdory              3400   12.5/30      30   41.7%
  avalanche              3396   13.0/30      30   43.3%
  frozenight             3367   13.5/30      30   45.0%
  nalwald                3346   15.5/30      30   51.7%
  wahoo                  3085   17.0/30      30   56.7%
  inanis                 3084   21.0/30      30   70.0%
  4ku                    3061   20.0/30      30   66.7%
  aurora                 2873   25.5/30      30   85.0%
  apotheosis             2748   25.5/30      30   85.0%
  tantabus               2553   25.0/30      30   83.3%
  oxidation              2362   29.0/30      30   96.7%
  tofiks                 1781   27.5/30      30   91.7%

  ───────────────────────────────────────────────────────
  Overall score   : 313.5 / 690  (45.4%)
  Performance ELO : 3312  (±26 at 95% CI)
  Games vs known  : 690

  ───────────────────────────────────────────────────────
  Opponent                 W     D     L
  ────────────────────────────────────────
  stockfish                0    10    20
  stormphrax               0     7    23
  viridithas               0     7    23
  lizard                   0    10    20
  koivisto                 2     9    19
  tcheran                  1     7    22
  blackmarlin              1    11    18
  akimbo                   1    14    15
  patricia                 1    15    14
  carp                     0    12    18
  blackcore                4    15    11
  stockdory                4    17     9
  avalanche                3    20     7
  frozenight               5    17     8
  nalwald                  5    21     4
  wahoo                   13     8     9
  inanis                  16    10     4
  4ku                     12    16     2
  aurora                  22     7     1
  apotheosis              23     5     2
  tantabus                21     8     1
  oxidation               28     2     0
  tofiks                  27     1     2
  ────────────────────────────────────────
  Total                  189   249   252
```
