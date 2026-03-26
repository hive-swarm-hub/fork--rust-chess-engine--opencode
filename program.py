from auto import state

WORKDIR = "/home/tianhao/rust-chess-engine"

# ---------------------------------------------------------------------------
# Prompt templates — each becomes one agent turn via step()
# ---------------------------------------------------------------------------

SETUP_PROMPT = f"""\
Setup the workspace for the Rust Chess Engine optimization loop.

1. Verify Rust: `source "$HOME/.cargo/env" && rustc --version`
2. Verify tools: `ls {WORKDIR}/tools/stockfish {WORKDIR}/tools/cutechess-cli`
3. If `{WORKDIR}/results.tsv` does not exist, create it with header:
   commit\telo\tgames_played\tstatus\tdescription
4. If results.tsv has rows, find the highest ELO with status=keep. That is the current best.
5. Run `bash {WORKDIR}/prepare.sh` if tools are missing.
6. Read program.md for the full task specification.

Return the current best ELO (0 if no prior results) and whether setup succeeded.
"""

THINK_PROMPT = """\
PHASE: THINK — Gather context and plan the next experiment.

Goal: maximize ELO rating of the Rust chess engine at {workdir}.

1. Check hive for shared intelligence (skip if hive unavailable):
   - `hive task context` — leaderboard, feed, claims, skills
   - `hive run list --view deltas` — biggest improvements
   - `hive feed list --since 2h` — recent activity
   If any hive command fails, just continue — hive is optional.

2. Read your own experiment history: `{workdir}/results.tsv`

3. Read the current engine code under `engine/src/` to understand what's implemented.

4. If hive shows someone beat your best ({best_elo}), consider adopting their code:
   - `hive run view <sha>` to get fork URL + SHA
   - `git remote add <agent> <fork-url>` && `git fetch <agent>` && `git checkout <sha>`
   - Verify with eval before building on it.

Current best ELO: {best_elo}
Iteration: {iteration}

Strategies (from program.md roadmap):
- Phase 1 (->1800): TT, MVV-LVA, killer moves, history heuristic, null move, LMR, aspiration windows
- Phase 2 (->2400): PSTs, pawn structure, king safety, mobility, bishop pair
- Phase 3 (->2800+): NNUE, PVS, singular extensions, Syzygy, Lazy SMP
- Phase 4 (2800+): SPSA tuning, search param optimization

Pick ONE focused experiment. Form a specific hypothesis about why it should help.
"""

CLAIM_PROMPT = """\
PHASE: CLAIM — Announce your experiment so other agents don't duplicate it.

Run: `hive feed claim "<concise description of your experiment>"`

If hive is unavailable, skip this step.
"""

MODIFY_EVAL_PROMPT = """\
PHASE: MODIFY & EVAL — Implement and test your experiment.

Working directory: {workdir}

Allowed modifications:
- engine/src/main.rs and engine/src/*.rs — search, eval, move ordering, everything
- engine/Cargo.toml — add dependencies
- engine/build.rs — build scripts
- Data files under engine/ (NNUE weights, opening books, etc.)

Forbidden: eval/eval.sh, eval/compute_elo.py, eval/openings.epd, prepare.sh, tools/

Steps:
1. Implement your planned changes.
2. Compile check: `cd {workdir} && source "$HOME/.cargo/env" && cd engine && cargo build --release 2>&1`
   Fix any errors before proceeding.
3. Commit: `cd {workdir} && git add -A && git commit -m "<description>"`
4. Run eval: `cd {workdir} && bash eval/eval.sh > run.log 2>&1`
5. Extract: `grep "^elo:\\|^valid:" {workdir}/run.log`
   If empty: `tail -n 100 {workdir}/run.log` for errors.

Constraints: <=10000 lines, <=100MB binary, <=5min compile, <=30min eval.
Anti-cheat: no network, no subprocess spawning, no reading tools/eval/proc paths.
"""

RECORD_PROMPT = """\
PHASE: RECORD — Log result and decide keep vs revert.

Result: ELO={elo}, valid={valid}, crashed={crashed}
Previous best: {best_elo}

1. Append to {workdir}/results.tsv (tab-separated):
   <7-char sha>\t<elo or ERROR>\t<games>\t<keep|discard|crash>\t<description>

2. If ELO > {best_elo} AND valid=true: KEEP the commit.
   Otherwise: REVERT with `cd {workdir} && git reset --hard HEAD~1`

Return what you decided, the commit SHA, and the ELO.
"""

SUBMIT_PROMPT = """\
PHASE: SUBMIT — Push results to hive (skip if hive unavailable).

ELO={elo}, description="{description}", parent={parent_sha}

1. Push: `cd {workdir} && git push origin` (push even if reverted — failures teach too)
2. Submit: `hive run submit -m "{description}" --score {elo} --parent {parent_sha}`
   Use `--parent none` for the very first run.

If hive commands fail, log the error and continue.
"""

SHARE_PROMPT = """\
PHASE: SHARE — Post insights to the hive feed (skip if hive unavailable).

Experiment #{iteration}: {description}
Result: ELO={elo} (best: {best_elo}), outcome: {outcome}

Post to feed: `hive feed post "<detailed insight>" --task rust-chess-engine`
Include: what you tried, what happened, theories about why, ideas for next steps.

If hive is unavailable, skip.
"""

REFLECT_PROMPT = """\
PHASE: REFLECT — Strategic review after {n} experiments.

1. Read {workdir}/results.tsv — review all experiments.
2. Check hive leaderboard: `hive task context` (skip if unavailable).
3. Analyze:
   - What patterns are working? What keeps failing?
   - Is there a plateau? What's the bottleneck?
   - Should you try something radically different (e.g., NNUE, Lazy SMP)?
4. Return a strategic assessment and plan for next experiments.
"""


# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

async def main(step):
    state.set("status", "starting")
    state.set("task", "rust-chess-engine")
    state.set("workdir", WORKDIR)

    # --- Setup ---
    setup = await step(
        SETUP_PROMPT,
        schema={"best_elo": "float", "setup_ok": "bool", "message": "str"},
    )

    best_elo = setup["best_elo"]
    parent_sha = "none"
    iteration = 0

    state.update({"status": "running", "best_elo": best_elo})

    # --- Experiment loop (runs forever) ---
    while True:
        iteration += 1
        state.update({"iteration": iteration, "phase": "think", "best_elo": best_elo})

        # 1. THINK
        think = await step(
            THINK_PROMPT.format(
                workdir=WORKDIR, best_elo=best_elo, iteration=iteration
            ),
            schema={"plan": "str", "rationale": "str", "building_on": "str"},
        )
        state.update({"phase": "claim", "current_plan": think["plan"]})

        # 2. CLAIM
        await step(CLAIM_PROMPT, schema={"claimed": "str"})
        state.update({"phase": "modify_eval"})

        # 3. MODIFY & EVAL
        try:
            result = await step(
                MODIFY_EVAL_PROMPT.format(workdir=WORKDIR),
                schema={
                    "elo": "float or 0 if crashed",
                    "valid": "bool",
                    "games_played": "int",
                    "description": "str",
                    "crashed": "bool",
                },
            )
            elo = result["elo"]
            valid = result["valid"]
            crashed = result["crashed"]
            description = result["description"]
        except Exception as e:
            elo = 0
            valid = False
            crashed = True
            description = f"step failed: {e}"

        state.update({
            "phase": "record",
            "last_elo": elo,
            "last_valid": valid,
            "last_crashed": crashed,
        })

        # 4. RECORD & DECIDE
        try:
            decision = await step(
                RECORD_PROMPT.format(
                    elo=elo if not crashed else "ERROR",
                    valid=valid,
                    crashed=crashed,
                    best_elo=best_elo,
                    workdir=WORKDIR,
                ),
                schema={"kept": "bool", "commit_sha": "str", "final_elo": "float"},
            )

            if decision["kept"] and valid and not crashed and elo > best_elo:
                outcome = "keep"
                best_elo = elo
                parent_sha = decision["commit_sha"]
            elif crashed:
                outcome = "crash"
            else:
                outcome = "discard"
        except Exception:
            outcome = "crash"

        state.update({"phase": "submit", "best_elo": best_elo, "outcome": outcome})

        # 5. SUBMIT
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

        # 6. SHARE
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

        # 7. REFLECT every 5 iterations
        if iteration % 5 == 0:
            state.update({"phase": "reflect"})
            await step(
                REFLECT_PROMPT.format(n=iteration, workdir=WORKDIR),
                schema={"assessment": "str", "next_strategy": "str"},
            )

        state.update({"phase": "loop_complete", "iterations_done": iteration})
