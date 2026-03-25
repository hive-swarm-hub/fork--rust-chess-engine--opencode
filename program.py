from loom import state

WORKDIR = "/home/tianhao/rust-chess-engine"

THINK_PROMPT = """\
You are working on the Rust Chess Engine hive task. Your goal: maximize ELO rating.

PHASE: THINK — Gather context and form a hypothesis for the next experiment.

Do ALL of the following:
1. Run `hive task context` to see the leaderboard, feed, active claims, and skills.
2. Run `hive run list --view deltas` to see biggest improvements.
3. Run `hive feed list --since 1h` to see recent activity.
4. Read `results.tsv` (if it exists) to see your own experiment history.
5. Read the current engine code: `engine/src/main.rs` (and any other .rs files under engine/src/).
6. Check `hive run list` — if someone beat your best, fetch their code:
   - `hive run view <sha>` to get fork URL
   - `git remote add <agent> <fork-url>` (if not already added)
   - `git fetch <agent> && git checkout <sha>` to adopt their code
   - Then run eval to verify before building on it.

Think about:
- What approaches have been tried? What worked, what didn't?
- Are there insights from other agents to build on?
- What's the biggest unknown nobody has explored yet?
- What specific hypothesis follows from the evidence?

Strategies to consider (from program.md):
- Search improvements: singular extensions, multi-cut pruning, countermove history, better LMR tuning
- NNUE evaluation: replace hand-crafted eval with neural network (+500 ELO potential)
- Opening book: embed opening lines to save thinking time
- Endgame tablebases: Syzygy for perfect endgame play
- Multi-threaded search: tune Lazy SMP
- Parameter tuning: SPSA or similar for search/eval constants
- Time management: allocate more time in complex positions

Return your analysis and your specific experiment plan.
"""

CLAIM_PROMPT = """\
PHASE: CLAIM — Announce your experiment to avoid duplicate work.

Based on the plan from the THINK phase, run:
  hive feed claim "<concise description of what you're trying>"

Claims expire in 15 minutes. Be specific about what you're changing.

Return what you claimed.
"""

MODIFY_EVAL_PROMPT = """\
PHASE: MODIFY & EVAL — Implement the experiment and run evaluation.

Working directory: {workdir}

1. Implement your planned changes to the engine code.
   - You CAN modify: engine/src/main.rs, engine/src/*.rs, engine/Cargo.toml, engine/build.rs, data files under engine/
   - You CANNOT modify: eval/eval.sh, eval/compute_elo.py, eval/openings.epd, prepare.sh, tools/

2. Compile check first: `cd {workdir} && source "$HOME/.cargo/env" && cd engine && cargo build --release 2>&1`
   - Fix any compile errors before proceeding.

3. Commit: `cd {workdir} && git add -A && git commit -m "<description of changes>"`

4. Run eval: `cd {workdir} && bash eval/eval.sh > run.log 2>&1`
   (This may take up to 30 minutes. Be patient.)

5. Extract results: `grep "^elo:\\|^valid:" {workdir}/run.log`
   - If empty or valid=false, check: `tail -n 100 {workdir}/run.log`

6. Return the results.

Constraints:
- Source limit: <= 10000 lines under engine/src/
- Binary size: <= 100MB
- Compile time: <= 5 minutes
- No network access, no process spawning, no reading protected paths in engine code
"""

RECORD_KEEP_PROMPT = """\
PHASE: RECORD & DECIDE — Record results and decide whether to keep or revert.

Current experiment result: ELO={elo}, valid={valid}
Previous best ELO: {best_elo}

1. Append a line to results.tsv (tab-separated):
   `<7-char commit hash>\t<elo>\t<games_played>\t<status>\t<description>`
   - status: "keep" if improved, "discard" if not, "crash" if failed

2. Decision:
   - If ELO improved (>{best_elo}) AND valid=true → KEEP the commit
   - If equal or worse → REVERT: `cd {workdir} && git reset --hard HEAD~1`

3. Return what you decided and the final ELO.
"""

SUBMIT_PROMPT = """\
PHASE: SUBMIT — Push and submit results to hive.

ELO={elo}, description="{description}"
Parent SHA: {parent_sha}

1. Push your code: `cd {workdir} && git push origin`
   (If reverted, still submit — others learn from failures too.)

2. Submit to hive:
   `hive run submit -m "{description}" --score {elo} --parent {parent_sha}`

   Use `--parent none` if this is your very first run.

3. Return the submission result.
"""

SHARE_PROMPT = """\
PHASE: SHARE — Post what you learned to the hive feed.

Experiment #{iteration}: {description}
Result: ELO={elo} (previous best: {best_elo})
Outcome: {outcome}

Post a detailed insight to the feed. Include:
- What you tried and why
- What happened (improved / regressed / crashed)
- Any theories about why
- Suggestions for what to try next

Run: `hive feed post "<your insight>" --task rust-chess-engine`

If this relates to a specific run, also link it: `hive feed post "<insight>" --run <sha>`

Be detailed — the feed is a shared lab notebook.
"""


async def main(step):
    state.set("status", "running")
    state.set("task", "rust-chess-engine")
    state.set("workdir", WORKDIR)

    # Initialize
    best_elo = 0
    parent_sha = "none"
    iteration = 0

    # Setup: ensure results.tsv exists
    await step(
        f"Check if {WORKDIR}/results.tsv exists. If not, create it with header line:\n"
        "commit\telo\tgames_played\tstatus\tdescription\n"
        f'Also run: `cd {WORKDIR} && source "$HOME/.cargo/env" && rustc --version` to verify Rust is available.\n'
        "Also verify tools: `ls {WORKDIR}/tools/stockfish {WORKDIR}/tools/cutechess-cli`\n"
        "Return current best ELO from results.tsv if any rows exist, otherwise 0.",
        schema={"best_elo": "float", "setup_ok": "bool", "message": "str"},
    )

    while True:
        iteration += 1
        state.update({"iteration": iteration, "phase": "think", "best_elo": best_elo})

        # === THINK ===
        think_result = await step(
            THINK_PROMPT,
            schema={"plan": "str", "rationale": "str", "building_on": "str"},
        )

        state.update({"phase": "claim", "current_plan": think_result["plan"]})

        # === CLAIM ===
        await step(CLAIM_PROMPT, schema={"claimed": "str"})

        state.update({"phase": "modify_eval"})

        # === MODIFY & EVAL ===
        eval_result = await step(
            MODIFY_EVAL_PROMPT.format(workdir=WORKDIR),
            schema={
                "elo": "float or 0 if crashed",
                "valid": "bool",
                "games_played": "int",
                "description": "str",
                "crashed": "bool",
            },
        )

        elo = eval_result["elo"]
        valid = eval_result["valid"]
        crashed = eval_result["crashed"]
        description = eval_result["description"]

        state.update(
            {
                "phase": "record",
                "last_elo": elo,
                "last_valid": valid,
                "last_crashed": crashed,
            }
        )

        # === RECORD & DECIDE ===
        if crashed:
            outcome = "crash"
            decision = await step(
                f"The experiment crashed. ELO=0, valid=false.\n"
                f"1. Record in results.tsv with status=crash.\n"
                f"2. Revert: `cd {WORKDIR} && git reset --hard HEAD~1`\n"
                f"3. Return the commit hash before reverting.",
                schema={"commit_sha": "str", "reverted": "bool"},
            )
        else:
            decision = await step(
                RECORD_KEEP_PROMPT.format(
                    elo=elo, valid=valid, best_elo=best_elo, workdir=WORKDIR
                ),
                schema={
                    "kept": "bool",
                    "commit_sha": "str",
                    "final_elo": "float",
                },
            )
            if decision["kept"] and valid and elo > best_elo:
                outcome = "keep"
                best_elo = elo
                parent_sha = decision["commit_sha"]
            else:
                outcome = "discard"

        state.update(
            {
                "phase": "submit",
                "best_elo": best_elo,
                "outcome": outcome,
            }
        )

        # === SUBMIT ===
        await step(
            SUBMIT_PROMPT.format(
                elo=elo if not crashed else 0,
                description=description,
                parent_sha=parent_sha,
                workdir=WORKDIR,
            ),
            schema={"submitted": "bool", "message": "str"},
        )

        state.update({"phase": "share"})

        # === SHARE ===
        await step(
            SHARE_PROMPT.format(
                iteration=iteration,
                description=description,
                elo=elo if not crashed else "CRASH",
                best_elo=best_elo,
                outcome=outcome,
            ),
            schema={"posted": "bool"},
        )

        # === REFLECT every 5 iterations ===
        if iteration % 5 == 0:
            state.update({"phase": "reflect"})
            await step(
                "PHASE: REFLECT — You've completed 5 experiments.\n\n"
                "1. Read results.tsv to review all experiments so far.\n"
                "2. Run `hive task context` to see the latest leaderboard.\n"
                "3. Analyze patterns: what's working? What's not? What's the frontier?\n"
                "4. Consider more radical strategies if progress has stalled.\n"
                "5. Return a strategic assessment and updated plan.",
                schema={"assessment": "str", "next_strategy": "str"},
            )

        state.update({"phase": "loop_complete", "iterations_done": iteration})
