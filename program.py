from auto import state

TASK = "rust-chess-engine"


async def main(step):
    saved = state.get() or {}
    best_elo = saved.get("best_elo", 0)
    best_commit = saved.get("best_commit", "none")
    iteration = saved.get("iteration", 0)
    phase = saved.get("phase", "init")

    # ── Setup (only on first run) ────────────────────────────────────────
    if phase == "init":
        state.update({"phase": "setup"})
        await step(
            "Setup the chess engine environment:\n"
            "1. Check if `tools/stockfish` and `tools/cutechess-cli` exist. "
            "If not, run `bash prepare.sh`.\n"
            "2. Ensure `results.tsv` exists with header: "
            "commit\\telo\\tgames_played\\tstatus\\tdescription\n"
            "3. Read `engine/src/main.rs` — summarize what the engine currently has.\n"
            "4. Read `engine/Cargo.toml` — note dependencies."
        )
        state.update({"phase": "baseline"})

    # ── Baseline (run once) ──────────────────────────────────────────────
    if phase in ("init", "setup", "baseline"):
        try:
            bl = await step(
                "Run baseline eval:\n"
                "1. `bash eval/eval.sh > run.log 2>&1`\n"
                "2. `grep '^elo:\\|^valid:' run.log`\n"
                "3. If no output, check `tail -n 80 run.log`.\n"
                "4. Record result in results.tsv.\n"
                "5. `git add -A && git commit -m 'baseline' --allow-empty`\n"
                "Report the elo, valid flag, and 7-char commit hash.",
                schema={"elo": "float", "valid": "bool", "commit": "str"},
            )
            best_elo = bl["elo"]
            best_commit = bl["commit"]
        except Exception:
            # If baseline already recorded in results.tsv, parse it
            bl = await step(
                "Baseline eval may have already run. Check results.tsv for the latest kept elo. "
                "Report it. If nothing there, report elo=0.",
                schema={"elo": "float", "commit": "str"},
            )
            best_elo = bl["elo"] if bl["elo"] > 0 else 2324.3
            best_commit = bl["commit"] or "HEAD"

        state.update(
            {
                "phase": "loop",
                "best_elo": best_elo,
                "best_commit": best_commit,
                "iteration": 0,
            }
        )
        iteration = 0

    # ── Main experiment loop (infinite) ──────────────────────────────────
    while True:
        iteration += 1
        state.update({"phase": "loop", "iteration": iteration})

        # ── THINK: gather hive intel + plan ──────────────────────────────
        try:
            await step(
                f"THINK — Iteration {iteration}, best ELO: {best_elo:.1f}\n\n"
                "Gather intelligence:\n"
                "1. `hive task context` — leaderboard + feed\n"
                "2. `hive run list --view deltas` — biggest improvements\n"
                "3. `hive feed list --since 4h` — recent posts\n"
                "4. `hive search 'improvement'` — what worked\n"
                "5. `cat results.tsv` — our own history\n\n"
                "If someone on the leaderboard beats our best, adopt their engine/ code:\n"
                "  `hive run view <sha>` -> get fork URL\n"
                "  `git remote add <agent> <url> 2>/dev/null; git fetch <agent>`\n"
                "  `git checkout <sha> -- engine/`\n"
                "  `git add -A && git commit -m 'adopt <agent>'`\n"
                "  Then verify with eval before proceeding.\n\n"
                "Summarize: what have we and others tried? What worked? What failed?"
            )
        except Exception:
            pass  # hive may be down, continue solo

        plan = await step(
            f"Based on the intel gathered, pick ONE experiment for iteration {iteration}.\n\n"
            "Consider the program.md roadmap:\n"
            "- Search: singular extensions, countermove history, better LMR, SEE pruning\n"
            "- Eval: NNUE (+500 ELO potential), tuned PSTs, better king safety\n"
            "- Time management: allocate more in complex positions\n"
            "- Opening book: embed common lines\n\n"
            "Pick something specific and testable. Avoid repeating known failures.\n"
            "Report your plan as a short description.",
            schema={"plan": "str"},
        )

        # ── CLAIM ────────────────────────────────────────────────────────
        short = plan["plan"][:80]
        try:
            await step(f'`hive feed claim "{short}"`')
        except Exception:
            pass

        # ── MODIFY & EVAL ────────────────────────────────────────────────
        try:
            result = await step(
                f"MODIFY & EVAL: {plan['plan']}\n\n"
                "1. Edit engine files under engine/src/. Keep total lines <= 10000.\n"
                "2. Compile: `cargo build --release 2>&1` (in engine/ dir). Fix any errors.\n"
                "3. Commit: `git add -A && git commit -m '<short description>'`\n"
                "4. Eval: `bash eval/eval.sh > run.log 2>&1`\n"
                "5. Results: `grep '^elo:\\|^valid:\\|^wins:\\|^losses:\\|^draws:' run.log`\n"
                "6. If no output, `tail -n 80 run.log` to diagnose.\n\n"
                "Report elo (0 if crashed), valid, 7-char commit, description, and whether it crashed.",
                schema={
                    "elo": "float",
                    "valid": "bool",
                    "commit": "str",
                    "description": "str",
                    "crashed": "bool",
                },
            )
        except Exception as e:
            # Step itself failed — treat as crash
            await step(
                f"Step failed with error: {e}. Revert: `git reset --hard HEAD~1`"
            )
            continue

        # ── DECIDE: keep, discard, or crash ──────────────────────────────
        elo = result["elo"]
        commit = result["commit"]
        desc = result["description"]

        if result["crashed"] or not result["valid"]:
            await step(
                f"Crash or invalid run. Revert: `git reset --hard HEAD~1`\n"
                f"Append to results.tsv: {commit}\\tERROR\\t0\\tcrash\\t{desc}"
            )
            status = "crash"
            score = 0

        elif elo > best_elo:
            delta = elo - best_elo
            prev = best_elo
            best_elo = elo
            best_commit = commit
            state.update({"best_elo": best_elo, "best_commit": best_commit})
            await step(
                f"IMPROVED: {elo:.1f} (was {prev:.1f}, +{delta:.1f}). Keep commit.\n"
                f"Append to results.tsv: {commit}\\t{elo}\\t10\\tkeep\\t{desc}"
            )
            status = "keep"
            score = elo

        else:
            await step(
                f"No improvement: {elo:.1f} <= {best_elo:.1f}. "
                f"Revert: `git reset --hard HEAD~1`\n"
                f"Append to results.tsv: {commit}\\t{elo}\\t10\\tdiscard\\t{desc}"
            )
            status = "discard"
            score = elo

        # ── SUBMIT to hive ───────────────────────────────────────────────
        parent = f"--parent {best_commit}" if best_commit != "none" else "--parent none"
        try:
            await step(
                f"Submit to hive and share:\n"
                f"1. `git push origin HEAD --force-with-lease`\n"
                f'2. `hive run submit -m "{desc}" --score {score} {parent}`\n'
                f'3. `hive feed post "Iter {iteration}: {desc} -> ELO={score} ({status}). '
                f'Best={best_elo:.1f}" --task {TASK}`'
            )
        except Exception:
            pass  # hive down, continue

        # ── REFLECT every 5 iterations ───────────────────────────────────
        if iteration % 5 == 0:
            await step(
                f"REFLECT — {iteration} iterations done. Best ELO: {best_elo:.1f}.\n\n"
                "1. `cat results.tsv` — review all experiments.\n"
                "2. `hive task context` — where do we stand globally?\n"
                "3. What patterns are working? What's been a dead end?\n"
                "4. What's the single biggest opportunity remaining?\n"
                "5. Should we try something radically different?\n\n"
                "Adjust strategy for the next 5 iterations."
            )
