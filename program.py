from auto import state


async def main(step):
    # -- Setup --
    state.set("phase", "setup")

    await step(
        "Setup the environment:\n"
        "1. Run `bash prepare.sh` to install Rust, Stockfish, cutechess-cli.\n"
        "2. Verify `tools/stockfish` and `tools/cutechess-cli` exist.\n"
        "3. Create `results.tsv` with header: commit\\telo\\tgames_played\\tstatus\\tdescription\n"
        "4. Read `engine/src/main.rs` to understand the current engine.\n"
        "5. Read `engine/Cargo.toml`.\n"
        "Report what the engine currently implements (search, eval features)."
    )

    # -- Baseline --
    state.set("phase", "baseline")

    baseline = await step(
        "Run the baseline eval:\n"
        "1. `bash eval/eval.sh > run.log 2>&1`\n"
        "2. Read run.log — extract elo and valid fields.\n"
        "3. If valid=true, record in results.tsv: <commit>\\t<elo>\\t<games>\\tkeep\\tbaseline\n"
        "4. `git add -A && git commit -m 'baseline'` if needed.\n"
        "Report the baseline ELO.",
        schema={"elo": "float", "valid": "bool", "commit": "str"},
    )

    best_elo = baseline["elo"]
    best_commit = baseline["commit"]
    state.update({"best_elo": best_elo, "best_commit": best_commit, "iteration": 0})

    # -- Check hive state --
    await step(
        "Check hive shared state:\n"
        "1. Run `hive task context` to see leaderboard + feed + claims + skills.\n"
        "2. Run `hive run list --view deltas` to see biggest improvements.\n"
        "3. Run `hive feed list --since 24h` for recent activity.\n"
        "Summarize: what approaches have been tried? What's the current best? "
        "What hasn't been explored yet?"
    )

    # -- Main experiment loop --
    for i in range(100):
        state.update({"phase": "experiment", "iteration": i + 1})

        # 1. THINK
        plan = await step(
            f"THINK — Iteration {i + 1}. Current best ELO: {best_elo}.\n"
            "1. Review results.tsv for your experiment history.\n"
            "2. Run `hive run list` to check if someone beat you. If so, adopt their code:\n"
            "   `hive run view <sha>` -> `git remote add <agent> <fork-url>` -> fetch+checkout.\n"
            "3. Run `hive feed list --since 1h` and `hive search` for relevant insights.\n"
            "4. Study the engine code for improvement opportunities.\n"
            "5. Consult the roadmap in program.md.\n\n"
            "Form a specific hypothesis for the next experiment. Consider:\n"
            "- Search: singular extensions, multi-cut, countermove history, better LMR\n"
            "- Eval: NNUE, better PSTs, mobility, pawn structure\n"
            "- Opening book, time management, Lazy SMP\n"
            "- Combining two ideas that each helped independently\n\n"
            "Report your plan.",
            schema={"plan": "str", "adopted_other_code": "bool"},
        )

        if plan["adopted_other_code"]:
            # Re-baseline after adopting
            re = await step(
                "You adopted another agent's code. Run eval to verify:\n"
                "`bash eval/eval.sh > run.log 2>&1`\n"
                "Report the ELO.",
                schema={"elo": "float", "valid": "bool"},
            )
            if re["valid"] and re["elo"] > best_elo:
                best_elo = re["elo"]
                state.update({"best_elo": best_elo})
                await step(
                    f"Verified. New baseline from other agent: {best_elo}. "
                    f"Post verification: `hive feed post '[VERIFY] score={best_elo} PASS'`"
                )

        # 2. CLAIM
        await step(
            f"Claim your experiment so others don't duplicate:\n"
            f'`hive feed claim "{plan["plan"][:80]}"`'
        )

        # 3. MODIFY & EVAL
        result = await step(
            f"MODIFY & EVAL — Implement: {plan['plan']}\n"
            "1. Edit engine source files (engine/src/main.rs or new files).\n"
            "2. Make sure total lines under engine/src/ <= 10000.\n"
            "3. `git add -A && git commit -m '<short description>'`\n"
            "4. `bash eval/eval.sh > run.log 2>&1`\n"
            "5. Extract results: `grep '^elo:\\|^valid:' run.log`\n"
            "6. If empty or valid=false, check `tail -n 100 run.log` for errors.\n"
            "Report the ELO, validity, and commit hash.",
            schema={
                "elo": "float",
                "valid": "bool",
                "commit": "str",
                "description": "str",
                "crashed": "bool",
            },
        )

        state.update({"last_elo": result["elo"], "last_valid": result["valid"]})

        # 4. Decide keep/discard
        if result["crashed"] or not result["valid"]:
            status = "crash"
            await step(
                "Experiment crashed or invalid. Check `tail -n 100 run.log` for the error. "
                "Revert: `git reset --hard HEAD~1`. "
                f"Record in results.tsv: {result['commit']}\\tERROR\\t0\\tcrash\\t{result['description']}"
            )
        elif result["elo"] > best_elo:
            status = "keep"
            best_elo = result["elo"]
            best_commit = result["commit"]
            state.update({"best_elo": best_elo, "best_commit": best_commit})
            await step(
                f"ELO improved! {result['elo']} > previous {best_elo - (result['elo'] - best_elo):.1f}. Keep it.\n"
                f"Record in results.tsv: {result['commit']}\\t{result['elo']}\\t10\\tkeep\\t{result['description']}"
            )
        else:
            status = "discard"
            await step(
                f"No improvement ({result['elo']} <= {best_elo}). Revert: `git reset --hard HEAD~1`.\n"
                f"Record in results.tsv: {result['commit']}\\t{result['elo']}\\t10\\tdiscard\\t{result['description']}"
            )

        # 5. SUBMIT to hive
        score = result["elo"] if not result["crashed"] else 0
        await step(
            f"Submit to hive and share:\n"
            "1. `git push origin main` (or current branch)\n"
            f'2. `hive run submit -m "{result["description"]}" --score {score} --parent {best_commit}`\n'
            f'3. `hive feed post "Iter {i + 1}: {result["description"]} -> ELO {result["elo"]} ({status})" --task rust-chess-engine`\n'
            "If the result was interesting (surprising failure, new insight), write a detailed post."
        )

        # 6. Periodic reflection
        if (i + 1) % 5 == 0:
            await step(
                f"REFLECT after {i + 1} iterations. Best ELO: {best_elo}.\n"
                "1. Review results.tsv — what patterns are working?\n"
                "2. `hive task context` — any new breakthroughs from other agents?\n"
                "3. What's the biggest bottleneck? What radical ideas haven't been tried?\n"
                "Adjust strategy for the next 5 iterations."
            )

    state.set("phase", "done")
