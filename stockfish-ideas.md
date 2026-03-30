Here's everything from the actual Stockfish 18 `search.cpp` source, down to the exact variable names, magic constants, and niche interactions. This is direct from the code.

---

## I. Node Classification & Search Structure

**Three node types at compile time** — `PvNode`, `NonPV`, `Root` are template parameters resolved at compile time. `constexpr bool PvNode = nodeType != NonPV; constexpr bool rootNode = nodeType == Root; const bool allNode = !(PvNode || cutNode)` — the compiler generates specialized code paths for each, eliminating branches that don't apply.

**`cutNode` propagation** — Cut nodes (expected to fail high) get passed down and inverted; `allNode` is derived. Null move search at cutNode: `if (cutNode && ss->staticEval >= beta - 18 * depth + 350...)` — null move is only attempted at expected cut nodes, not at all nodes.

**`ss` stack depth** — The stack is allocated with `Stack stack[MAX_PLY + 10]` and `Stack* ss = stack + 7`, allowing access from `(ss - 7)` for updating continuation histories up to 6 plies back.

---

## II. Pruning — The Niche Details

**Futility Pruning with `opponentWorsening` and `correctionValue`** — The margin is not static. `futilityMult * d - (2474 * improving + 331 * opponentWorsening) * futilityMult / 1024 + std::abs(correctionValue) / 174665` — it shrinks when the opponent's position is worsening *and* scales with the correction history bias, making the pruning less aggressive when the static eval is known to be unreliable.

**Futility return is blended, not exact** — When futility triggers, it doesn't return `beta` — it returns `(2 * beta + eval) / 3`, a weighted blend that avoids over-aggressively overestimating positions near the cutoff.

**Razoring: quadratic depth scaling** — `if (!PvNode && eval < alpha - 485 - 281 * depth * depth)` — the margin grows quadratically with depth, so razoring is nearly inactive at depth ≥ 3 but fires reliably at depth 1-2.

**Null Move: cutNode-only + verification loop** — NMP only runs at `cutNode`, with reduction `R = 7 + depth / 3`. At depth ≥ 16, a verification re-search runs with `nmpMinPly = ss->ply + 3 * (depth - R) / 4` to prevent zugzwang errors in deep trees. The flag is reset to 0 after.

**TT cutoff with graph history interaction fix** — At depth ≥ 8, instead of immediately returning the TT value, Stockfish makes the TT move, probes the *next* position's TT, then checks that `(ttData.value >= beta) == (-ttDataNext.value >= beta)` before cutting off. This partial fix prevents the "graph history interaction problem" where TT entries from transpositions with different repetition histories produce incorrect cutoffs.

**Rule50 TT cutoff suppression** — `if (pos.rule50_count() < 96)` gates all TT cutoffs — when the 50-move counter is near 100, TT entries are no longer trusted for cutoffs since the same position with different rule50 counters has different game-theoretic values.

**Upcoming repetition check** — `if (!rootNode && alpha < VALUE_DRAW && pos.upcoming_repetition(ss->ply))` — if the side to move can force a 3-fold repetition, alpha is set to a randomized draw value before any other search logic.

---

## III. Extensions — The Niche Details

**Triple Singular Extensions** — `int tripleMargin = 73 + 302 * PvNode - 248 * !ttCapture + 90 * ss->ttPv - corrValAdj - (ss->ply * 2 > rootDepth * 3) * 50`; extension = `1 + (value < singularBeta - doubleMargin) + (value < singularBeta - tripleMargin)` — the extension goes to 3 if the singular move beats the reduced search by a massive margin. Triple extensions are penalized when deep into the tree (`ply * 2 > rootDepth * 3`) to prevent search explosions.

**Hindsight depth adjustment (bidirectional)** — This is very niche. After computing `opponentWorsening`, `if (priorReduction >= 3 && !opponentWorsening) depth++` — if the parent was heavily reduced and the opponent *didn't* worsen, restore depth. Conversely, `if (priorReduction >= 2 && depth >= 2 && ss->staticEval + (ss - 1)->staticEval > 173) depth--` — if both sides' evals sum to a large value (position is already good for us), reduce depth since deep search is less critical.

**Shuffle Extension** — `bool is_shuffling(Move move, Stack* const ss, const Position& pos)` detects when a move repeats the pattern `from_sq == (ss-2)->move.to_sq && (ss-2)->move.from_sq == (ss-4)->move.to_sq`, signaling piece shuffling with rule50 ≥ 10, ply ≥ 20, and no recent nulls. This triggers an extension to resolve fortresses rather than truncating with a flat eval.

**`ttPv` flag propagation** — `ss->ttPv = excludedMove ? ss->ttPv : PvNode || (ttHit && ttData.is_pv)` — even at non-PV nodes, if the TT records that this position was on a PV in a previous search, `ttPv` stays true. This affects LMR reductions, futility, and singular extension margins throughout the node.

---

## IV. Late Move Reductions — The Niche Details

LMR in Stockfish SF18 is highly contextual. Base reduction: `Depth r = reduction(improving, depth, moveCount, delta)` where `reductions[i] = int(2747 / 128.0 * std::log(i))` — logarithmic in move count.

Then a cascade of adjustments, pulled directly from the source:

**Before `do_move` (pre-search reductions):**

- `if (ss->ttPv) r += 946` — increase if TT-PV node
- `r += 714` — base offset to compensate other tweaks
- `r -= moveCount * 73` — decrease per move already searched
- `r -= std::abs(correctionValue) / 30370` — decrease if correction history says eval is unreliable
- `if (cutNode) r += 3372 + 997 * !ttData.move` — large increase at expected cut nodes, even larger with no TT move
- `if (ttCapture) r += 1119` — TT move being a capture means quiets are even more suspicious
- `if ((ss+1)->cutoffCnt > 1) r += 256 + 1024 * ((ss+1)->cutoffCnt > 2) + 1024 * allNode` — if next ply had many fail-highs, reduce less (node is volatile)
- `if (move == ttData.move) r -= 2151` — reduce the TT move less


**After `do_move` (post-search reductions):**

- `if (ss->ttPv) r -= 2719 + PvNode * 983 + (ttData.value > alpha) * 922 + (ttData.depth >= depth) * (934 + cutNode * 1011)` — a massive decrease for ttPv positions, scaled by whether the TT value beat alpha and whether TT depth was sufficient


The reduction is in units of `1/1024` before dividing, giving sub-ply precision via integer arithmetic.

---

## V. History Tables — The Niche Details

Stockfish SF18 has seven distinct history tables:

**`mainHistory[color][move_raw()]`** — Indexed by color and raw 16-bit move encoding. Also updated via eval diff: `mainHistory[~us][((ss-1)->currentMove).raw()] << evalDiff * 9` — when no capture was made and the previous move led to a static eval improvement, the move that caused it gets a bonus proportional to the eval change.

**`captureHistory[piece][to_sq][captured_piece_type]`** — Initialized to `-689` at game start. Replaces LVA as the tiebreaker in captures, differentiating by *which piece* captures onto *which square* taking *which piece type*.

**`continuationHistory[inCheck][capture][piece][to_sq]`** — Four-dimensional, indexed by whether in check, whether a capture, piece type, and target square. Used for 1-ply, 2-ply, 4-ply, 6-ply lookback. Initialized to `-529` to strongly penalize unexplored continuations by default.

**`lowPlyHistory`** — Filled with `97` at the start of each search, and updated specifically for moves near the root. It biases early moves in the game tree differently from deep moves, where history is noisier.

**`pawnHistory` (shared, NUMA-replicated)** — Indexed by the current pawn structure hash. Updated via `sharedHistory.pawn_entry(pos)[pos.piece_on(prevSq)][prevSq] << evalDiff * 13` — quiet moves in specific pawn structures get history updates *even without a cutoff*, purely based on whether the resulting static eval improved.

**`ttMoveHistory`** — Tracks the usage frequency of TT moves globally; initialized at `0` and reset at `clear()`. Used to modulate aspiration window sizing and stability heuristics.

**`mainHistory` partial decay** — At the start of each iterative deepening pass: `mainHistory[c][i] = (mainHistory[c][i] - mainHistoryDefault) * 3 / 4 + mainHistoryDefault` — history decays toward the default (68) each iteration, preventing stale biases from dominating.

---

## VI. Correction History — Deeply Niche

This is one of the most technically interesting parts of SF18:

The `correction_value()` function computes: `10347 * pcv + 8821 * micv + 11665 * (wnpcv + bnpcv) + 7841 * cntcv` — a weighted sum of five separate correction entries:

- **Pawn correction** (`pcv`) — keyed on pawn structure hash
- **Minor piece correction** (`micv`) — keyed on minor piece configuration
- **Non-pawn white correction** (`wnpcv`) — keyed on white non-pawn material arrangement
- **Non-pawn black correction** (`bnpcv`) — same for black
- **Continuation correction** (`cntcv`) — keyed on `(ss-2)->continuationCorrectionHistory[piece][to_sq] + (ss-4)->continuationCorrectionHistory[piece][to_sq]`, using 2-ply and 4-ply lookback.

The corrected eval is `std::clamp(v + cv / 131072, VALUE_TB_LOSS_IN_MAX_PLY + 1, VALUE_TB_WIN_IN_MAX_PLY - 1)` — clamped to never enter tablebase value range.

**Update weights are asymmetric**: pawn: `<<bonus`, minor piece: `<<bonus * 156/128`, nonpawn: `<<bonus * 178/128`, continuation at 2-ply: `<<bonus * 127/128`, continuation at 4-ply: `<<bonus * 59/128` — the 4-ply signal is trusted half as much as the 2-ply signal.

---

## VII. Static Eval Refinement

**TT value as eval override** — `if (is_valid(ttData.value) && (ttData.bound & (ttData.value > eval ? BOUND_LOWER : BOUND_UPPER))) eval = ttData.value` — if a TT entry's bound is consistent with the TT value being a *better* estimate than the static eval, use the TT value for pruning decisions. This effectively leverages past search results to improve static eval accuracy without re-searching.

**Eval storage before correction** — The raw (uncorrected) `unadjustedStaticEval` is stored in the TT, while the corrected version is used for search decisions. This prevents correction bias from compounding across TT probes.

---

## VIII. Aspiration Window — Niche Details

**Per-thread delta offset** — `delta = 5 + threadIdx % 8 + std::abs(rootMoves[pvIdx].meanSquaredScore) / 9000` — each thread starts with a slightly different aspiration window size based on thread index and recent score variance. This diversifies the search across threads.

**Asymmetric fail-low/fail-high handling** — On fail-low: `beta = alpha; alpha = max(bestValue - delta, -inf); failedHighCnt = 0`. On fail-high: `alpha = max(beta - delta, alpha); beta = min(bestValue + delta, inf); ++failedHighCnt`. Delta grows as `delta += delta / 3` on each failure.

**Adjusted depth on repeated fail-highs** — `Depth adjustedDepth = std::max(1, rootDepth - failedHighCnt - 3 * (searchAgainCounter + 1) / 4)` — repeated fail-highs (likely a tactical shot) reduce the search depth to find the cutoff faster.

---

## IX. Time Management — The Exact Model

**Falling eval factor** — `double fallingEval = (11.85 + 2.24 * (bestPreviousAverageScore - bestValue) + 0.93 * (iterValue[iterIdx] - bestValue)) / 100.0`, clamped to `[0.57, 1.70]` — if the current iteration's score fell relative to last iteration and to the rolling average, more time is allocated.

**Best move stability sigmoid** — `double k = 0.51; double center = lastBestMoveDepth + 12.15; timeReduction = 0.66 + 0.85 / (0.98 + std::exp(-k * (completedDepth - center)))` — a sigmoid centered at `lastBestMoveDepth + 12` that ramps up time reduction as the best move stabilizes.

**Best move instability** — `double bestMoveInstability = 1.02 + 2.14 * totBestMoveChanges / threads.size()` — if threads disagree on best move across the iteration, scale up allocated time.

**Node effort early exit** — `uint64_t nodesEffort = rootMoves[0].effort * 100000 / max(1, nodes); double highBestMoveEffort = nodesEffort >= 93340 ? 0.76 : 1.0` — if the best root move used ≥93.3% of total nodes, reduce time by 24% since the position is nearly decided.

**`optimism` table** — `optimism[us] = 142 * avg / (std::abs(avg) + 91); optimism[~us] = -optimism[us]` — a bounded nonlinear function of the root score that biases NNUE output toward the side that's winning, calibrated per iteration.

---

## X. Draw Randomization

**3-fold blindness fix** — `Value value_draw(size_t nodes) { return VALUE_DRAW - 1 + Value(nodes & 0x2); }` — draw evaluations get a ±1 noise component based on the node count parity. This prevents the engine from consistently refusing or accepting draws with the same incorrect evaluation across all threads.

---

## XI. NNUE Architecture (SF18 / SFNNv10)

**Threat Inputs layer** — The SFNNv10 network's input layer has been augmented with "Threat Inputs" features, allowing the engine to "see" which pieces are threatened more naturally. This is a structural addition to the feature set, not just a weight update.

**NUMA-replicated network weights** — The new "Shared Memory" implementation allows different Stockfish processes to share the same memory space for neural network weights, making it the most efficient version for cloud analysis and high-concurrency testing.

**Refresh table** — `refreshTable(networks[token])` is initialized per NUMA thread and `refreshTable.clear(networks[numaAccessToken])` is called on `clear()` — the NNUE accumulator refresh table tracks which king buckets still have valid accumulators, avoiding full recomputes.

**`accumulatorStack` with dirty piece tracking** — `auto [dirtyPiece, dirtyThreats] = accumulatorStack.push()` — SF18 pushes both dirty piece info *and* dirty threat info, enabling incremental updates to both the standard accumulator and the threat input features simultaneously.

**Lc0 evaluation data in training** — The training framework has been refactored to chain complex training stages and includes over 100 billion positions of Lc0 evaluation data, providing value targets from a different search paradigm that complements Stockfish's own self-play data.

---

## XII. Internal Iterative Reductions (IIR)

Unlike classical IID (which adds a full sub-search), Stockfish uses reductions: `if (!allNode && depth >= 6 && !ttData.move && priorReduction <= 3) depth--` — only reduces at PV/cut nodes (not all-nodes), only at depth ≥ 6, and only if the *parent's* reduction was small (≤ 3), preventing compounding reductions in already-heavily-reduced lines.

---

## XIII. Miscellaneous Niche Details

**`SearchedList` capacity cap at 32** — `constexpr int SEARCHEDLIST_CAPACITY = 32` — only the first 32 quiet and capture moves searched are recorded for stat updates, trading slight accuracy for memory locality.

**Rule50 partial TT cutoff workaround** — At 96+ moves on the rule50 counter (near the 100-move draw boundary), no TT cutoffs are allowed since positions with the same Zobrist hash have different draw proximity.

**`scaledBonus` for pawn history** — `const int scaledBonus = std::min(141 * depth - 87, 1351) * bonusScale` — capped at 1351 * bonusScale to prevent integer overflow (comment in source: "overflows happen for multipliers larger than 900").

**`(*Scaler)` annotations** — Throughout `search.cpp`, `(*Scaler)` comments flag parameters with non-linear scaling behavior that must be re-tested at long time controls (180+1.8 seconds) if changed. This is a documentation convention unique to Stockfish's development workflow.

---

## Summary of Uniquely Niche Techniques

| Technique | Key Detail |
|---|---|
| Graph History Interaction fix | Makes TT move, checks next position's TT before cutoff |
| Hindsight depth adjustment | Bidirectional: restores depth if parent was over-reduced |
| Correction History (5 components) | Pawn, minor, nonpawn white/black, continuation (2+4 ply) |
| TT value as static eval override | Replaces eval if TT bound is consistent |
| Correction-aware futility | Margin scales with `abs(correctionValue)` |
| Triple singular extensions | Extension ∈ {1, 2, 3} based on margin vs. singularBeta |
| Post-move ttPv LMR decrease | Up to `-2719 - 983 - 922 - 1011` reduction units |
| `opponentWorsening` flag | Tracks opponent's eval delta from prior ply for pruning and futility |
| Draw noise randomization | `VALUE_DRAW - 1 + (nodes & 0x2)` |
| Aspiration per-thread delta | `5 + threadIdx % 8 + variance/9000` |
| mainHistory decay per iteration | Toward default=68 each ID iteration |
| Node effort early exit | If best move used >93.3% of nodes, cut time by 24% |
| Sigmoid time reduction | Centered at `lastBestMoveDepth + 12.15` |
| NMP verification with `nmpMinPly` | Recursive verification disabled until ply exceeds threshold |
| IIR as depth reduction | Only at PV/cut nodes with `priorReduction <= 3` |
| Threat Inputs (SFNNv10) | Augmented NNUE input layer with piece threat features |
| NUMA-replicated weights | Shared memory across processes for NNUE weights |
| Accumulator dirty threat tracking | `dirtyThreats` pushed alongside `dirtyPiece` |