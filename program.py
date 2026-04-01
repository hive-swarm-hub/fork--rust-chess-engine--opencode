from auto import state

TASK = "rust-chess-engine"


async def main(step):
    saved = state.get() or {}
    best_elo = saved.get("best_elo", 2881.7)
    best_commit = saved.get("best_commit", "1824564")
    iteration = saved.get("iteration", 68)
    phase = saved.get("phase", "loop")

    if phase in ("init", "setup", "baseline"):
        state.update({"phase": "loop", "best_elo": best_elo, "best_commit": best_commit, "iteration": 0})
        iteration = 0

    while True:
        iteration += 1
        state.update({"phase": "loop", "iteration": iteration})

        # ══ STEP 1: DEEP RESEARCH ══
        research = await step(
            f"DEEP RESEARCH — Iteration {iteration}, best ELO: {best_elo:.1f}\n\n"
            "Do BROAD research using multiple sources. Use sub-agents for parallel work.\n\n"
            "1. **Hive intelligence**: `hive task context`, `hive run list`, `hive feed list --since 4h`\n"
            "   If someone beats our best, fetch and study their code.\n\n"
            "2. **Web research** (WebSearch/WebFetch):\n"
            "   - Chess engine optimization techniques\n"
            "   - Small fast NNUE implementations\n"
            "   - Rust-specific optimizations for chess\n"
            "   - Look at open-source engines (Clockwork HCE, Inanis, etc.)\n\n"
            "3. **Code analysis** (sub-agents):\n"
            "   - Read our engine for missed optimization opportunities\n"
            "   - Study competitor code diffs\n"
            "   - Check for Rust crates that could help\n\n"
            "4. **History**: `cat logs/results.tsv` — what's been tried\n\n"
            "KEY CONTEXT:\n"
            "- Best: contempt=0 + 1/15 time + 50ms min = 2881.7\n"
            "- True strength ~2650 (±300 variance on 10 games)\n"
            "- NNUE too slow without SIMD (no AVX2, Apple Silicon/Rosetta)\n"
            "- Recapture extension at depth>=3 was too aggressive (0W/1D/9L)\n"
            "- All aggressive pruning changes are catastrophic\n\n"
            "Synthesize findings into a concrete, novel experiment.",
            schema={"findings": "str", "novel_idea": "str", "confidence": "str"},
        )

        # ══ STEP 2: PLAN ══
        plan = await step(
            f"Research: {research['findings'][:400]}\n"
            f"Idea: {research['novel_idea'][:300]}\n\n"
            f"Design a concrete experiment for iteration {iteration}.\n"
            "AVOID: aggressive pruning, pure variance sampling, repeated experiments, NNUE without speed fix.\n"
            "Report your plan.",
            schema={"plan": "str"},
        )

        # ══ STEP 3: CLAIM ══
        try:
            await step(f'`hive feed claim "{plan["plan"][:80]}"`')
        except Exception:
            pass

        # ══ STEP 4: IMPLEMENT & EVAL ══
        try:
            result = await step(
                f"IMPLEMENT & EVAL: {plan['plan']}\n\n"
                "1. Edit engine/src/. Keep <= 10000 lines.\n"
                "2. Compile: `cd engine && cargo build --release 2>&1`\n"
                "3. Commit: `git add -A && git commit -m '<desc>'`\n"
                "4. Eval: `ulimit -n 65536; bash eval/eval.sh > logs/run.log 2>&1`\n"
                "5. Results: `grep '^elo:\\|^valid:\\|^wins:\\|^losses:\\|^draws:' logs/run.log`\n"
                "6. If no output: `tail -n 80 logs/run.log`\n"
                "7. Check PGN: `head -30 eval/games.pgn`\n\n"
                "Report elo, valid, commit, description, crashed.",
                schema={"elo": "float", "valid": "bool", "commit": "str", "description": "str", "crashed": "bool"},
            )
        except Exception as e:
            await step(f"Failed: {e}. Revert: `git reset --hard HEAD~1`")
            continue

        elo = result["elo"]
        commit = result["commit"]
        desc = result["description"]

        if result["crashed"] or not result["valid"]:
            await step(f"Crash. Revert: `git reset --hard HEAD~1`\nAppend to logs/results.tsv: {commit}\\tERROR\\t0\\tcrash\\t{desc}")
            status, score = "crash", 0
        elif elo > best_elo:
            delta = elo - best_elo
            prev = best_elo
            best_elo = elo
            best_commit = commit
            state.update({"best_elo": best_elo, "best_commit": best_commit})
            await step(f"IMPROVED: {elo:.1f} (+{delta:.1f}). Keep.\nAppend: {commit}\\t{elo}\\t10\\tkeep\\t{desc}")
            status, score = "keep", elo
        else:
            await step(f"No improvement: {elo:.1f} <= {best_elo:.1f}. Revert: `git reset --hard HEAD~1`\nAppend: {commit}\\t{elo}\\t10\\tdiscard\\t{desc}")
            status, score = "discard", elo

        # ══ STEP 6: SUBMIT ══
        try:
            await step(
                f"Submit:\n1. `git push origin HEAD --force-with-lease`\n"
                f'2. `hive run submit -m "{desc[:80]}" --score {score} --parent {best_commit} --tldr "{desc[:60]}, ELO={score}"`\n'
                f'3. `hive feed post "Iter {iteration}: {desc[:80]} -> ELO={score} ({status}). Best={best_elo:.1f}" --task {TASK}`'
            )
        except Exception:
            pass

        if iteration % 5 == 0:
            await step(
                f"DEEP REFLECT — {iteration} iters. Best: {best_elo:.1f}.\n"
                "1. `cat logs/results.tsv` — review all\n2. `hive task context`\n"
                "3. Web search for new techniques\n4. Study competitor code\n"
                "5. What's the most promising unexplored direction?\nAdjust strategy."
            )
