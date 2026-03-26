// HiveChess — UCI chess engine
// Ported from github.com/deedy/chess (Rust search engine)
// Added UCI protocol for standalone operation

use std::io::{self, BufRead, Write};

fn send(msg: &str) {
    println!("{}", msg);
    io::stdout().flush().ok();
}

fn main() {
    let stdin = io::stdin();
    let mut engine = RustAlphaBetaEngine::new(64);
    let mut root_fen = String::from("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    let mut move_history: Vec<String> = Vec::new();
    let mut current_board = Board::default();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() { continue; }

        match tokens[0] {
            "uci" => {
                send("id name HiveChess 0.2");
                send("id author HiveAgent");
                send("uciok");
            }
            "isready" => send("readyok"),
            "ucinewgame" => {
                engine = RustAlphaBetaEngine::new(64);
            }
            "position" => {
                let mut idx = 1;
                if tokens.get(idx) == Some(&"startpos") {
                    current_board = Board::default();
                    root_fen = current_board.to_string();
                    idx = 2;
                } else if tokens.get(idx) == Some(&"fen") {
                    idx += 1;
                    let mut fen_parts = Vec::new();
                    while idx < tokens.len() && tokens[idx] != "moves" {
                        fen_parts.push(tokens[idx]);
                        idx += 1;
                    }
                    root_fen = fen_parts.join(" ");
                    current_board = Board::from_str(&root_fen).unwrap_or_default();
                }
                move_history.clear();
                if tokens.get(idx) == Some(&"moves") {
                    idx += 1;
                    let mut b = Board::from_str(&root_fen).unwrap_or_default();
                    while idx < tokens.len() {
                        move_history.push(tokens[idx].to_string());
                        if let Ok(mv) = ChessMove::from_str(tokens[idx]) {
                            b = b.make_move_new(mv);
                        }
                        idx += 1;
                    }
                    current_board = b;
                }
            }
            "go" => {
                let side = current_board.side_to_move();
                let mut time_ms: u64 = 10000;
                let mut inc_ms: u64 = 0;
                let mut movestogo: u32 = 30;
                let mut max_depth: Option<i32> = None;
                let mut movetime: Option<u64> = None;

                let mut i = 1;
                while i < tokens.len() {
                    match tokens[i] {
                        "wtime" => { if let Some(t) = tokens.get(i+1).and_then(|t| t.parse().ok()) { if side == Color::White { time_ms = t; } } i += 2; }
                        "btime" => { if let Some(t) = tokens.get(i+1).and_then(|t| t.parse().ok()) { if side == Color::Black { time_ms = t; } } i += 2; }
                        "winc"  => { if let Some(t) = tokens.get(i+1).and_then(|t| t.parse().ok()) { if side == Color::White { inc_ms = t; } } i += 2; }
                        "binc"  => { if let Some(t) = tokens.get(i+1).and_then(|t| t.parse().ok()) { if side == Color::Black { inc_ms = t; } } i += 2; }
                        "movestogo" => { movestogo = tokens.get(i+1).and_then(|t| t.parse().ok()).unwrap_or(30); i += 2; }
                        "depth" => { max_depth = tokens.get(i+1).and_then(|t| t.parse().ok()); i += 2; }
                        "movetime" => { movetime = tokens.get(i+1).and_then(|t| t.parse().ok()); i += 2; }
                        "infinite" => { time_ms = 999_999_999; i += 1; }
                        _ => { i += 1; }
                    }
                }

                let alloc_ms = if let Some(mt) = movetime {
                    mt
                } else {
                    // Use 1/20 of remaining + increment, cap at 1/3 remaining
                    let base = time_ms / 20;
                    let with_inc = base + inc_ms;
                    with_inc.min(time_ms / 3).max(100)
                };

                let history = if move_history.is_empty() { None } else { Some(move_history.clone()) };
                match engine.choose_move(&root_fen, max_depth, Some(alloc_ms), history) {
                    Ok(res) => {
                        let pv = res.pv_uci.join(" ");
                        send(&format!("info depth {} score cp {} nodes {} pv {}", res.depth, res.score, res.nodes, pv));
                        send(&format!("bestmove {}", res.move_uci));
                    }
                    Err(_) => {
                        if let Some(mv) = MoveGen::new_legal(&current_board).next() {
                            send(&format!("bestmove {}", mv));
                        } else {
                            send("bestmove 0000");
                        }
                    }
                }
            }
            "quit" => break,
            _ => {}
        }
    }
}

// =============================================================================
// Engine code below (ported from github.com/deedy/chess rust/src/lib.rs)
// =============================================================================

use std::array;
use std::collections::HashMap;
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

use chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves,
    BitBoard, Board, BoardStatus, ChessMove, Color, File, MoveGen, Piece, Rank, Square,
};

const INFINITY: i32 = 1_000_000;
const MATE_SCORE: i32 = 100_000;
const DRAW_SCORE: i32 = 0;
const ASPIRATION_WINDOW: i32 = 40;
const HISTORY_SIZE: usize = 1 << 16;
const KILLER_PLY_CAPACITY: usize = 128;
const MAX_ROOT_THREADS: usize = 8;
const MAX_TT_ENTRIES: usize = 2_000_000;
const MAX_EVAL_CACHE_ENTRIES: usize = 500_000;
const MAX_PAWN_CACHE_ENTRIES: usize = 200_000;
const HISTORY_LIMIT: i32 = 32_000;
const CAPTURE_HISTORY_PIECES: usize = 6;
const CAPTURE_HISTORY_LIMIT: i32 = 16_000;
const COUNTERMOVE_BONUS: i32 = 200_000;
const MAX_TIME_SEARCH_DEPTH: i32 = 64;
const PHASE_TOTAL: i32 = 24;
const SEE_PRUNE_MARGIN: i32 = 80;

const EXACT: u8 = 0;
const LOWER_BOUND: u8 = 1;
const UPPER_BOUND: u8 = 2;

const PAWN: i32 = 100;
const KNIGHT: i32 = 320;
const BISHOP: i32 = 330;
const ROOK: i32 = 500;
const QUEEN: i32 = 900;

const BISHOP_PAIR_BONUS: i32 = 30;
const CASTLED_KING_BONUS: i32 = 18;
const DOUBLED_PAWN_PENALTY: i32 = 14;
const ISOLATED_PAWN_PENALTY: i32 = 11;
const TEMPO_BONUS: i32 = 10;
const ROOK_OPEN_FILE_BONUS: i32 = 18;
const ROOK_SEMI_OPEN_FILE_BONUS: i32 = 10;
const ROOK_SEVENTH_RANK_BONUS: i32 = 14;
const KNIGHT_OUTPOST_BONUS: i32 = 18;
const CENTER_PAWN_BONUS: i32 = 10;
const UNDEVELOPED_MINOR_PENALTY: i32 = 8;

const MOBILITY_KNIGHT: i32 = 4;
const MOBILITY_BISHOP: i32 = 5;
const MOBILITY_ROOK: i32 = 2;
const MOBILITY_QUEEN: i32 = 1;

const PASSED_PAWN_BONUS: [i32; 8] = [0, 8, 12, 20, 35, 60, 90, 0];
const ENDGAME_PASSED_PAWN_BONUS: [i32; 8] = [0, 0, 4, 8, 16, 32, 56, 0];
const SUPPORTED_PASSED_PAWN_BONUS: [i32; 8] = [0, 0, 3, 6, 12, 20, 32, 0];
const REVERSE_FUTILITY_MARGIN: [i32; 4] = [0, 85, 150, 235];
const FUTILITY_MARGIN: [i32; 4] = [0, 100, 170, 260];
const RAZOR_MARGIN: [i32; 3] = [0, 250, 380];

const PAWN_TABLE: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 5, 10, 10, -20, -20, 10, 10, 5, 5, -5, -10, 0, 0, -10, -5, 5, 0, 0, 0,
    20, 20, 0, 0, 0, 5, 5, 10, 25, 25, 10, 5, 5, 10, 10, 20, 30, 30, 20, 10, 10, 50, 50, 50, 50,
    50, 50, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0,
];

const KNIGHT_TABLE: [i32; 64] = [
    -50, -40, -30, -30, -30, -30, -40, -50, -40, -20, 0, 0, 0, 0, -20, -40, -30, 0, 10, 15, 15, 10,
    0, -30, -30, 5, 15, 20, 20, 15, 5, -30, -30, 0, 15, 20, 20, 15, 0, -30, -30, 5, 10, 15, 15, 10,
    5, -30, -40, -20, 0, 5, 5, 0, -20, -40, -50, -40, -30, -30, -30, -30, -40, -50,
];

const BISHOP_TABLE: [i32; 64] = [
    -20, -10, -10, -10, -10, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 10, 10, 5, 0,
    -10, -10, 5, 5, 10, 10, 5, 5, -10, -10, 0, 10, 10, 10, 10, 0, -10, -10, 10, 10, 10, 10, 10, 10,
    -10, -10, 5, 0, 0, 0, 0, 5, -10, -20, -10, -10, -10, -10, -10, -10, -20,
];

const ROOK_TABLE: [i32; 64] = [
    0, 0, 0, 5, 5, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0,
    0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, 5, 10, 10, 10, 10, 10, 10, 5, 0, 0,
    0, 0, 0, 0, 0, 0,
];

const QUEEN_TABLE: [i32; 64] = [
    -20, -10, -10, -5, -5, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 5, 5, 5, 0, -10,
    -5, 0, 5, 5, 5, 5, 0, -5, 0, 0, 5, 5, 5, 5, 0, -5, -10, 5, 5, 5, 5, 5, 0, -10, -10, 0, 5, 0, 0,
    0, 0, -10, -20, -10, -10, -5, -5, -10, -10, -20,
];

const KING_MIDGAME_TABLE: [i32; 64] = [
    -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40,
    -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -20, -30, -30, -40, -40, -30,
    -30, -20, -10, -20, -20, -20, -20, -20, -20, -10, 20, 20, 0, 0, 0, 0, 20, 20, 20, 30, 10, 0, 0,
    10, 30, 20,
];

const KING_ENDGAME_TABLE: [i32; 64] = [
    -50, -40, -30, -20, -20, -30, -40, -50, -30, -20, -10, 0, 0, -10, -20, -30, -30, -10, 20, 30,
    30, 20, -10, -30, -30, -10, 30, 40, 40, 30, -10, -30, -30, -10, 30, 40, 40, 30, -10, -30, -30,
    -10, 20, 30, 30, 20, -10, -30, -30, -30, 0, 0, 0, 0, -30, -30, -50, -30, -30, -30, -30, -30,
    -30, -50,
];

#[derive(Clone, Copy)]
struct TTEntry {
    depth: i32,
    score: i32,
    flag: u8,
    best_move: Option<ChessMove>,
}

#[derive(Clone, Copy)]
struct PawnCacheEntry {
    white_structure_mg: i32,
    black_structure_mg: i32,
    white_structure_eg: i32,
    black_structure_eg: i32,
    white_center_mg: i32,
    black_center_mg: i32,
    white_files: [u8; 8],
    black_files: [u8; 8],
}

#[derive(Clone, Copy)]
struct ScoredMove {
    chess_move: ChessMove,
    score: i32,
}

#[derive(Clone)]
struct RepetitionTracker {
    counts: HashMap<u64, u8>,
}

impl RepetitionTracker {
    fn new(root_hash: u64) -> Self {
        let mut counts = HashMap::with_capacity(64);
        counts.insert(root_hash, 1);
        Self { counts }
    }

    fn count(&self, hash: u64) -> u8 {
        *self.counts.get(&hash).unwrap_or(&0)
    }

    fn push(&mut self, hash: u64) {
        let count = self.counts.entry(hash).or_insert(0);
        *count = count.saturating_add(1);
    }

    fn pop(&mut self, hash: u64) {
        let Some(count) = self.counts.get_mut(&hash) else {
            return;
        };
        if *count <= 1 {
            self.counts.remove(&hash);
        } else {
            *count -= 1;
        }
    }
}

struct RawSearchResult {
    move_uci: String,
    score: i32,
    depth: i32,
    nodes: u64,
    pv_uci: Vec<String>,
}

struct RootChunkResult {
    best_move: Option<ChessMove>,
    best_score: i32,
    nodes: u64,
    tt: HashMap<u64, TTEntry>,
}

struct RustAlphaBetaEngine {
    depth: i32,
    threads: usize,
    nodes: u64,
    tt: HashMap<u64, TTEntry>,
    killer_moves: Vec<[Option<ChessMove>; 2]>,
    history_heuristic: Vec<i32>,
    capture_history: Vec<i16>,
    countermove: Vec<Option<ChessMove>>,
    move_stack: Vec<Option<ChessMove>>,
    eval_cache: HashMap<u64, i32>,
    pawn_cache: HashMap<u64, PawnCacheEntry>,
    deadline: Option<Instant>,
    stopped: bool,
}

impl RustAlphaBetaEngine {
    fn new(depth: i32) -> Self {
        Self {
            depth,
            threads: available_root_threads(),
            nodes: 0,
            tt: HashMap::new(),
            killer_moves: vec![[None, None]; KILLER_PLY_CAPACITY],
            history_heuristic: vec![0; HISTORY_SIZE],
            capture_history: vec![0; HISTORY_SIZE * CAPTURE_HISTORY_PIECES],
            countermove: vec![None; HISTORY_SIZE],
            move_stack: vec![None; KILLER_PLY_CAPACITY],
            eval_cache: HashMap::new(),
            pawn_cache: HashMap::new(),
            deadline: None,
            stopped: false,
        }
    }

    fn choose_move(
        &mut self,
        fen: &str,
        depth: Option<i32>,
        movetime_ms: Option<u64>,
        moves_uci: Option<Vec<String>>,
    ) -> Result<RawSearchResult, String> {
        self.trim_caches();

        let mut board = Board::from_str(fen).map_err(|error| {
            format!("invalid FEN for Rust search backend: {error}")
        })?;
        let mut repetition = RepetitionTracker::new(board_hash(&board));

        if let Some(history) = moves_uci {
            for move_uci in history {
                let chess_move = ChessMove::from_str(&move_uci).map_err(|error| {
                    format!("invalid move history for Rust search backend: {error}")
                })?;
                if !move_is_legal(&board, chess_move) {
                    return Err(format!("illegal move history for Rust search backend: {move_uci}"));
                }
                board = board.make_move_new(chess_move);
                repetition.push(board_hash(&board));
            }
        }

        if !matches!(board.status(), BoardStatus::Ongoing) {
            return Err(String::from("Cannot search a finished game."));
        }

        let time_budget = movetime_ms.filter(|ms| *ms > 0).map(Duration::from_millis);
        let requested_depth = depth.unwrap_or(self.depth);
        let target_depth = if requested_depth <= 0 && time_budget.is_some() {
            MAX_TIME_SEARCH_DEPTH
        } else {
            requested_depth.max(1)
        };
        self.nodes = 0;
        self.stopped = false;
        let search_start = Instant::now();
        self.deadline = time_budget.map(|budget| search_start + budget);

        let mut best_move: Option<ChessMove> = None;
        let mut best_score = 0;
        let mut completed_depth = 0;
        let mut last_iteration_nodes: Option<u64> = None;
        let mut previous_iteration_nodes: Option<u64> = None;
        let mut last_iteration_time: Option<Duration> = None;
        let mut previous_iteration_time: Option<Duration> = None;

        for current_depth in 1..=target_depth {
            if completed_depth > 0
                && time_budget.is_some()
                && should_skip_next_iteration(
                    search_start,
                    time_budget.unwrap(),
                    current_depth,
                    last_iteration_nodes,
                    previous_iteration_nodes,
                    last_iteration_time,
                    previous_iteration_time,
                    best_score,
                )
            {
                break;
            }

            self.ensure_ply_capacity(current_depth as usize + 16);
            let nodes_before = self.nodes;
            let iteration_start = Instant::now();
            let search = self.search_root(
                &board,
                current_depth,
                (completed_depth > 0).then_some(best_score),
                &mut repetition,
            );
            let Some((score, candidate)) = search else {
                break;
            };
            let iteration_nodes = self.nodes.saturating_sub(nodes_before);
            let iteration_time = iteration_start.elapsed();
            previous_iteration_nodes = last_iteration_nodes;
            previous_iteration_time = last_iteration_time;
            last_iteration_nodes = Some(iteration_nodes);
            last_iteration_time = Some(iteration_time);
            best_score = score;
            best_move = Some(candidate);
            completed_depth = current_depth;

            if best_score.abs() >= MATE_SCORE - 512 {
                break;
            }
        }

        self.deadline = None;

        let chosen = best_move
            .or_else(|| MoveGen::new_legal(&board).next())
            .ok_or_else(|| {
                String::from("No legal moves available for an ongoing position.")
            })?;

        Ok(RawSearchResult {
            move_uci: chosen.to_string(),
            score: best_score,
            depth: completed_depth.max(1),
            nodes: self.nodes,
            pv_uci: self.extract_principal_variation(board, completed_depth.max(1)),
        })
    }
}

impl RustAlphaBetaEngine {
    fn search_root(
        &mut self,
        board: &Board,
        depth: i32,
        previous_score: Option<i32>,
        repetition: &mut RepetitionTracker,
    ) -> Option<(i32, ChessMove)> {
        let mut alpha = -INFINITY;
        let mut beta = INFINITY;
        if let Some(score) = previous_score.filter(|_| depth >= 3) {
            alpha = score - ASPIRATION_WINDOW;
            beta = score + ASPIRATION_WINDOW;
        }

        loop {
            let Some((score, best_move)) =
                self.search_root_window(board, depth, alpha, beta, repetition)
            else {
                break;
            };

            if alpha != -INFINITY && score <= alpha {
                alpha = -INFINITY;
                beta = INFINITY;
                continue;
            }
            if beta != INFINITY && score >= beta {
                alpha = -INFINITY;
                beta = INFINITY;
                continue;
            }
            return Some((score, best_move));
        }

        None
    }

    fn search_root_window(
        &mut self,
        board: &Board,
        depth: i32,
        alpha: i32,
        beta: i32,
        repetition: &mut RepetitionTracker,
    ) -> Option<(i32, ChessMove)> {
        let tt_move = self
            .tt
            .get(&board_hash(board))
            .and_then(|entry| entry.best_move);
        let mut root_moves = self.scored_moves(board, tt_move, 0, false);
        if root_moves.is_empty() {
            return None;
        }
        root_moves.sort_unstable_by(|left, right| right.score.cmp(&left.score));

        if !self.should_parallelize_root(depth, root_moves.len()) {
            let score = self.negamax(board, depth, alpha, beta, 0, repetition)?;
            let best_move = self
                .tt
                .get(&board_hash(board))
                .and_then(|entry| entry.best_move)
                .or_else(|| MoveGen::new_legal(board).next())?;
            return Some((score, best_move));
        }

        self.search_root_parallel(board, depth, alpha, beta, repetition, root_moves)
    }

    fn search_root_parallel(
        &mut self,
        board: &Board,
        depth: i32,
        alpha: i32,
        beta: i32,
        repetition: &mut RepetitionTracker,
        root_moves: Vec<ScoredMove>,
    ) -> Option<(i32, ChessMove)> {
        let first_move = root_moves[0].chess_move;
        let first_child = board.make_move_new(first_move);
        let first_hash = board_hash(&first_child);
        repetition.push(first_hash);
        let mut best_score =
            -self.negamax(&first_child, depth - 1, -beta, -alpha, 1, repetition)?;
        repetition.pop(first_hash);
        let mut best_move = first_move;
        let current_alpha = alpha.max(best_score);

        if current_alpha >= beta || root_moves.len() == 1 {
            self.store_root_result(board, depth, best_score, alpha, beta, best_move);
            return Some((best_score, best_move));
        }

        let remaining: Vec<ChessMove> = root_moves
            .into_iter()
            .skip(1)
            .map(|entry| entry.chess_move)
            .collect();
        let worker_count = self.threads.min(remaining.len()).max(1);
        let mut move_groups = vec![Vec::new(); worker_count];
        for (index, chess_move) in remaining.into_iter().enumerate() {
            move_groups[index % worker_count].push(chess_move);
        }

        let board_copy = *board;
        let repetition_base = repetition.clone();
        let chunk_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(move_groups.len());
            for group in move_groups {
                if group.is_empty() {
                    continue;
                }
                let board = board_copy;
                let repetition = repetition_base.clone();
                let mut worker = self.worker_clone();
                handles.push(scope.spawn(move || {
                    worker.search_root_chunk(board, depth, current_alpha, beta, repetition, group)
                }));
            }

            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let chunk = handle.join().ok()??;
                results.push(chunk);
            }
            Some(results)
        })?;

        for chunk in chunk_results {
            self.nodes = self.nodes.saturating_add(chunk.nodes);
            self.merge_tt(chunk.tt);
            if let Some(chess_move) = chunk.best_move {
                if chunk.best_score > best_score {
                    best_score = chunk.best_score;
                    best_move = chess_move;
                }
            }
        }

        self.store_root_result(board, depth, best_score, alpha, beta, best_move);
        Some((best_score, best_move))
    }

    fn search_root_chunk(
        &mut self,
        board: Board,
        depth: i32,
        alpha: i32,
        beta: i32,
        repetition: RepetitionTracker,
        moves: Vec<ChessMove>,
    ) -> Option<RootChunkResult> {
        let mut best_move = None;
        let mut best_score = -INFINITY;
        let mut local_alpha = alpha;

        for chess_move in moves {
            if self.should_stop() {
                break;
            }

            let child = board.make_move_new(chess_move);
            let child_hash = board_hash(&child);
            let mut local_repetition = repetition.clone();
            local_repetition.push(child_hash);

            let mut score = -self.negamax(
                &child,
                depth - 1,
                -local_alpha - 1,
                -local_alpha,
                1,
                &mut local_repetition,
            )?;
            if score > local_alpha {
                score = -self.negamax(
                    &child,
                    depth - 1,
                    -beta,
                    -local_alpha,
                    1,
                    &mut local_repetition,
                )?;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(chess_move);
            }
            if score > local_alpha {
                local_alpha = score;
            }
        }

        Some(RootChunkResult {
            best_move,
            best_score,
            nodes: self.nodes,
            tt: std::mem::take(&mut self.tt),
        })
    }

    fn should_parallelize_root(&self, depth: i32, move_count: usize) -> bool {
        self.threads > 1 && self.deadline.is_some() && depth >= 6 && move_count >= 4
    }

    fn worker_clone(&self) -> Self {
        Self {
            depth: self.depth,
            threads: 1,
            nodes: 0,
            tt: self.tt.clone(),
            killer_moves: self.killer_moves.clone(),
            history_heuristic: self.history_heuristic.clone(),
            capture_history: self.capture_history.clone(),
            countermove: self.countermove.clone(),
            move_stack: self.move_stack.clone(),
            eval_cache: self.eval_cache.clone(),
            pawn_cache: self.pawn_cache.clone(),
            deadline: self.deadline,
            stopped: false,
        }
    }

    fn merge_tt(&mut self, other: HashMap<u64, TTEntry>) {
        for (key, entry) in other {
            self.merge_tt_entry(key, entry);
        }
    }

    fn merge_tt_entry(&mut self, key: u64, entry: TTEntry) {
        match self.tt.get(&key) {
            Some(existing) if existing.depth > entry.depth => {}
            _ => {
                self.tt.insert(key, entry);
            }
        }
    }

    fn store_root_result(
        &mut self,
        board: &Board,
        depth: i32,
        score: i32,
        alpha_original: i32,
        beta_original: i32,
        best_move: ChessMove,
    ) {
        let flag = if score <= alpha_original {
            UPPER_BOUND
        } else if score >= beta_original {
            LOWER_BOUND
        } else {
            EXACT
        };
        self.merge_tt_entry(
            board_hash(board),
            TTEntry {
                depth,
                score,
                flag,
                best_move: Some(best_move),
            },
        );
    }

    fn negamax(
        &mut self,
        board: &Board,
        depth: i32,
        mut alpha: i32,
        mut beta: i32,
        ply: usize,
        repetition: &mut RepetitionTracker,
    ) -> Option<i32> {
        if self.should_stop() {
            return None;
        }
        self.nodes += 1;

        if let Some(terminal_score) = self.terminal_score(board, ply, repetition) {
            return Some(terminal_score);
        }

        let in_check_now = in_check(board);
        let mut effective_depth = depth;
        if in_check_now {
            effective_depth += 1;
        }

        if effective_depth <= 0 {
            return self.quiescence(board, alpha, beta, ply, repetition);
        }

        let alpha_original = alpha;
        let beta_original = beta;
        let tt_key = board_hash(board);
        let tt_entry = self.tt.get(&tt_key).copied();
        let mut tt_move = tt_entry.and_then(|entry| entry.best_move);

        if let Some(entry) = tt_entry {
            if entry.depth >= effective_depth {
                match entry.flag {
                    EXACT => return Some(entry.score),
                    LOWER_BOUND => alpha = alpha.max(entry.score),
                    UPPER_BOUND => beta = beta.min(entry.score),
                    _ => {}
                }
                if alpha >= beta {
                    return Some(entry.score);
                }
            }
        }

        if tt_move.is_none() && effective_depth >= 6 && !in_check_now {
            let iid_depth = if effective_depth >= 8 {
                effective_depth - 3
            } else {
                effective_depth - 2
            };
            if iid_depth > 0 {
                let _ = self.negamax(board, iid_depth, alpha, beta, ply, repetition);
                tt_move = self.tt.get(&tt_key).and_then(|entry| entry.best_move);
            }
        }

        let static_eval = if !in_check_now {
            Some(self.evaluate(board))
        } else {
            None
        };

        if let Some(eval) = static_eval {
            if effective_depth <= 3
                && eval >= beta + REVERSE_FUTILITY_MARGIN[effective_depth as usize]
                && beta < MATE_SCORE - 1_000
            {
                return Some(eval);
            }
            if effective_depth <= 2
                && eval + RAZOR_MARGIN[effective_depth as usize] <= alpha
                && alpha > -MATE_SCORE + 1_000
            {
                return self.quiescence(board, alpha, beta, ply, repetition);
            }
        }

        if effective_depth >= 3
            && !in_check_now
            && has_non_pawn_material(board, board.side_to_move())
            && beta < MATE_SCORE - 1_000
        {
            if let Some(null_board) = board.null_move() {
                let reduction = 2 + effective_depth / 3;
                let null_hash = board_hash(&null_board);
                repetition.push(null_hash);
                let search = self.negamax(
                    &null_board,
                    effective_depth - 1 - reduction,
                    -beta,
                    -beta + 1,
                    ply + 1,
                    repetition,
                );
                repetition.pop(null_hash);
                let score = -search?;
                if score >= beta {
                    return Some(score);
                }
            }
        }

        let mut best_move: Option<ChessMove> = None;
        let mut best_score = -INFINITY;
        let mut move_picker = self.scored_moves(board, tt_move, ply, false);
        let mut searched_quiets: Vec<ChessMove> = Vec::with_capacity(16);
        let mut searched_captures: Vec<(ChessMove, Piece)> = Vec::with_capacity(8);

        for index in 0..move_picker.len() {
            if self.should_stop() {
                return None;
            }

            let chess_move = pick_next_move(&mut move_picker, index)?;
            let move_count = index + 1;
            let child = board.make_move_new(chess_move);
            let child_hash = board_hash(&child);
            let is_capture_move = is_capture(board, chess_move);
            let capture_victim = if is_capture_move {
                Some(victim_piece(board, chess_move).unwrap_or(Piece::Pawn))
            } else {
                None
            };
            let is_quiet = !is_capture_move && chess_move.get_promotion().is_none();
            let gives_check_move = gives_check(board, chess_move);

            if is_quiet && !in_check_now && !gives_check_move {
                if let Some(eval) = static_eval {
                    if effective_depth <= 3
                        && move_count > 1
                        && eval + FUTILITY_MARGIN[effective_depth as usize] <= alpha
                    {
                        continue;
                    }
                }
                if move_count > late_move_pruning_limit(effective_depth) {
                    continue;
                }
                searched_quiets.push(chess_move);
            }
            if let Some(victim) = capture_victim {
                searched_captures.push((chess_move, victim));
            }

            repetition.push(child_hash);

            let score = (|| -> Option<i32> {
                if move_count == 1 {
                    return Some(-self.negamax(
                        &child,
                        effective_depth - 1,
                        -beta,
                        -alpha,
                        ply + 1,
                        repetition,
                    )?);
                }

                let mut search_depth = effective_depth - 1;
                if effective_depth >= 4
                    && move_count > 3
                    && is_quiet
                    && !in_check_now
                    && !gives_check_move
                {
                    let mut reduction = late_move_reduction(effective_depth, move_count);
                    if self.is_countermove(chess_move, ply) {
                        reduction = (reduction - 1).max(0);
                    }
                    search_depth = (search_depth - reduction).max(0);
                }

                self.ensure_ply_capacity(ply + 2);
                self.move_stack[ply] = Some(chess_move);
                let mut score = -self.negamax(
                    &child,
                    search_depth,
                    -alpha - 1,
                    -alpha,
                    ply + 1,
                    repetition,
                )?;
                if score > alpha && search_depth != effective_depth - 1 {
                    score = -self.negamax(
                        &child,
                        effective_depth - 1,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        repetition,
                    )?;
                }
                if score > alpha && score < beta {
                    score = -self.negamax(
                        &child,
                        effective_depth - 1,
                        -beta,
                        -alpha,
                        ply + 1,
                        repetition,
                    )?;
                }
                Some(score)
            })();
            repetition.pop(child_hash);
            let score = score?;

            if score > best_score {
                best_score = score;
                best_move = Some(chess_move);
            }
            if score > alpha {
                alpha = score;
            }
            if alpha >= beta {
                let bonus = effective_depth * effective_depth;
                if is_quiet {
                    self.record_killer(chess_move, ply);
                    self.update_history(chess_move, bonus);
                    if ply > 0 {
                        if let Some(prev_move) = self.move_stack.get(ply - 1).copied().flatten() {
                            self.countermove[move_key(prev_move) as usize] = Some(chess_move);
                        }
                    }
                    for previous in searched_quiets.iter().copied() {
                        if previous != chess_move {
                            self.update_history(previous, -bonus);
                        }
                    }
                } else if let Some(victim) = capture_victim {
                    self.update_capture_history(chess_move, victim, bonus);
                    for (previous, previous_victim) in searched_captures.iter().copied() {
                        if previous != chess_move {
                            self.update_capture_history(previous, previous_victim, -bonus);
                        }
                    }
                }
                break;
            }
        }

        if best_move.is_none() {
            return Some(
                self.terminal_score(board, ply, repetition)
                    .unwrap_or(DRAW_SCORE),
            );
        }

        let flag = if best_score <= alpha_original {
            UPPER_BOUND
        } else if best_score >= beta_original {
            LOWER_BOUND
        } else {
            EXACT
        };
        let new_entry = TTEntry {
            depth: effective_depth,
            score: best_score,
            flag,
            best_move,
        };
        match self.tt.get(&tt_key) {
            Some(existing) if existing.depth > new_entry.depth => {}
            _ => {
                self.tt.insert(tt_key, new_entry);
            }
        }
        Some(best_score)
    }

    fn quiescence(
        &mut self,
        board: &Board,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        repetition: &mut RepetitionTracker,
    ) -> Option<i32> {
        if self.should_stop() {
            return None;
        }
        self.nodes += 1;

        if let Some(terminal_score) = self.terminal_score(board, ply, repetition) {
            return Some(terminal_score);
        }

        let in_check_now = in_check(board);
        let stand_pat = self.evaluate(board);
        if !in_check_now {
            if stand_pat >= beta {
                return Some(stand_pat);
            }
            alpha = alpha.max(stand_pat);
        }

        let mut move_picker = self.scored_moves(board, None, ply, !in_check_now);
        for index in 0..move_picker.len() {
            let chess_move = pick_next_move(&mut move_picker, index)?;
            if !in_check_now {
                if let Some(victim) = victim_piece(board, chess_move) {
                    if stand_pat + piece_value(victim) + 200 < alpha {
                        continue;
                    }
                }
                if is_capture(board, chess_move)
                    && static_exchange_eval(board, chess_move) < -SEE_PRUNE_MARGIN
                {
                    continue;
                }
            }

            let child = board.make_move_new(chess_move);
            let child_hash = board_hash(&child);
            repetition.push(child_hash);
            let search = self.quiescence(&child, -beta, -alpha, ply + 1, repetition);
            repetition.pop(child_hash);
            let score = -search?;
            if score >= beta {
                return Some(score);
            }
            alpha = alpha.max(score);
        }

        Some(alpha)
    }

    fn should_stop(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        if self.nodes & 1023 != 0 {
            return false;
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.stopped = true;
                return true;
            }
        }
        false
    }

    fn evaluate(&mut self, board: &Board) -> i32 {
        let board_key = board_hash(board);
        if let Some(score) = self.eval_cache.get(&board_key) {
            return *score;
        }

        let phase = game_phase(board);
        let endgame = phase <= 6;
        let mut white_mg = 0;
        let mut black_mg = 0;
        let mut white_eg = 0;
        let mut black_eg = 0;

        for square in piece_squares(board, Color::White, Piece::Pawn) {
            let pst = PAWN_TABLE[square_index(square)];
            white_mg += PAWN + pst;
            white_eg += PAWN + pst / 2;
        }
        for square in piece_squares(board, Color::Black, Piece::Pawn) {
            let pst = PAWN_TABLE[mirror_index(square_index(square))];
            black_mg += PAWN + pst;
            black_eg += PAWN + pst / 2;
        }

        for square in piece_squares(board, Color::White, Piece::Knight) {
            let pst = KNIGHT_TABLE[square_index(square)];
            white_mg += KNIGHT + pst;
            white_eg += KNIGHT + pst / 4;
        }
        for square in piece_squares(board, Color::Black, Piece::Knight) {
            let pst = KNIGHT_TABLE[mirror_index(square_index(square))];
            black_mg += KNIGHT + pst;
            black_eg += KNIGHT + pst / 4;
        }

        for square in piece_squares(board, Color::White, Piece::Bishop) {
            let pst = BISHOP_TABLE[square_index(square)];
            white_mg += BISHOP + pst;
            white_eg += BISHOP + pst / 2;
        }
        for square in piece_squares(board, Color::Black, Piece::Bishop) {
            let pst = BISHOP_TABLE[mirror_index(square_index(square))];
            black_mg += BISHOP + pst;
            black_eg += BISHOP + pst / 2;
        }

        for square in piece_squares(board, Color::White, Piece::Rook) {
            let pst = ROOK_TABLE[square_index(square)];
            white_mg += ROOK + pst;
            white_eg += ROOK + pst / 3;
        }
        for square in piece_squares(board, Color::Black, Piece::Rook) {
            let pst = ROOK_TABLE[mirror_index(square_index(square))];
            black_mg += ROOK + pst;
            black_eg += ROOK + pst / 3;
        }

        for square in piece_squares(board, Color::White, Piece::Queen) {
            let pst = QUEEN_TABLE[square_index(square)];
            white_mg += QUEEN + pst;
            white_eg += QUEEN + pst / 4;
        }
        for square in piece_squares(board, Color::Black, Piece::Queen) {
            let pst = QUEEN_TABLE[mirror_index(square_index(square))];
            black_mg += QUEEN + pst;
            black_eg += QUEEN + pst / 4;
        }

        white_mg += king_position_score(board, Color::White, false);
        black_mg += king_position_score(board, Color::Black, false);
        white_eg += king_position_score(board, Color::White, true);
        black_eg += king_position_score(board, Color::Black, true);

        let pawn_entry = self.pawn_entry(board);
        white_mg += pawn_entry.white_structure_mg + pawn_entry.white_center_mg;
        black_mg += pawn_entry.black_structure_mg + pawn_entry.black_center_mg;
        white_eg += pawn_entry.white_structure_eg;
        black_eg += pawn_entry.black_structure_eg;

        let white_mobility = mobility_score(board, Color::White);
        let black_mobility = mobility_score(board, Color::Black);
        white_mg += white_mobility;
        black_mg += black_mobility;
        white_eg += white_mobility / 2;
        black_eg += black_mobility / 2;

        let white_rook_activity = rook_activity_score(
            board,
            Color::White,
            &pawn_entry.white_files,
            &pawn_entry.black_files,
        );
        let black_rook_activity = rook_activity_score(
            board,
            Color::Black,
            &pawn_entry.black_files,
            &pawn_entry.white_files,
        );
        white_mg += white_rook_activity;
        black_mg += black_rook_activity;
        white_eg += white_rook_activity;
        black_eg += black_rook_activity;

        let white_outposts = knight_outpost_score(board, Color::White);
        let black_outposts = knight_outpost_score(board, Color::Black);
        white_mg += white_outposts;
        black_mg += black_outposts;
        white_eg += white_outposts / 3;
        black_eg += black_outposts / 3;

        white_mg += development_score(board, Color::White, endgame);
        black_mg += development_score(board, Color::Black, endgame);

        if piece_squares(board, Color::White, Piece::Bishop).len() >= 2 {
            white_mg += BISHOP_PAIR_BONUS;
            white_eg += BISHOP_PAIR_BONUS + 6;
        }
        if piece_squares(board, Color::Black, Piece::Bishop).len() >= 2 {
            black_mg += BISHOP_PAIR_BONUS;
            black_eg += BISHOP_PAIR_BONUS + 6;
        }

        white_mg += king_safety_score(
            board,
            Color::White,
            phase,
            &pawn_entry.white_files,
            &pawn_entry.black_files,
        );
        black_mg += king_safety_score(
            board,
            Color::Black,
            phase,
            &pawn_entry.black_files,
            &pawn_entry.white_files,
        );

        let white_score = tapered_score(white_mg, white_eg, phase);
        let black_score = tapered_score(black_mg, black_eg, phase);

        let score = if board.side_to_move() == Color::White {
            white_score - black_score + TEMPO_BONUS
        } else {
            black_score - white_score + TEMPO_BONUS
        };

        if self.eval_cache.len() >= MAX_EVAL_CACHE_ENTRIES {
            self.eval_cache.clear();
        }
        self.eval_cache.insert(board_key, score);
        score
    }

    fn terminal_score(
        &self,
        board: &Board,
        ply: usize,
        repetition: &RepetitionTracker,
    ) -> Option<i32> {
        match board.status() {
            BoardStatus::Checkmate => Some(-MATE_SCORE + ply as i32),
            BoardStatus::Stalemate => Some(DRAW_SCORE),
            BoardStatus::Ongoing => {
                if repetition.count(board_hash(board)) >= 3 {
                    Some(DRAW_SCORE)
                } else {
                    None
                }
            }
        }
    }

    fn scored_moves(
        &self,
        board: &Board,
        tt_move: Option<ChessMove>,
        ply: usize,
        captures_only: bool,
    ) -> Vec<ScoredMove> {
        MoveGen::new_legal(board)
            .filter(|candidate| {
                !captures_only
                    || is_capture(board, *candidate)
                    || candidate.get_promotion().is_some()
            })
            .map(|candidate| ScoredMove {
                chess_move: candidate,
                score: self.move_order_score(board, candidate, tt_move, ply),
            })
            .collect()
    }

    fn move_order_score(
        &self,
        board: &Board,
        chess_move: ChessMove,
        tt_move: Option<ChessMove>,
        ply: usize,
    ) -> i32 {
        if tt_move == Some(chess_move) {
            return 10_000_000;
        }

        let mut score = self.history_heuristic[move_key(chess_move) as usize];
        let is_quiet = !is_capture(board, chess_move) && chess_move.get_promotion().is_none();

        if let Some(promotion) = chess_move.get_promotion() {
            score += 800_000 + piece_value(promotion);
        }

        if !is_quiet {
            let victim = victim_piece(board, chess_move).unwrap_or(Piece::Pawn);
            let attacker = board
                .piece_on(chess_move.get_source())
                .unwrap_or(Piece::Pawn);
            score += 500_000 + 16 * piece_value(victim) - piece_value(attacker);
            score += i32::from(self.capture_history[capture_history_key(chess_move, victim)]);
        }

        if let Some(killers) = self.killer_moves.get(ply) {
            if killers[0] == Some(chess_move) {
                score += 300_000;
            } else if killers[1] == Some(chess_move) {
                score += 290_000;
            }
        }

        if self.is_countermove(chess_move, ply) {
            score += COUNTERMOVE_BONUS;
        }

        if gives_check(board, chess_move) {
            score += 50_000;
        }

        if is_castling(board, chess_move) {
            score += 10_000;
        }

        score
    }

    fn record_killer(&mut self, chess_move: ChessMove, ply: usize) {
        self.ensure_ply_capacity(ply + 1);
        let killers = &mut self.killer_moves[ply];
        if killers[0] == Some(chess_move) {
            return;
        }
        killers[1] = killers[0];
        killers[0] = Some(chess_move);
    }

    fn update_history(&mut self, chess_move: ChessMove, delta: i32) {
        let history = &mut self.history_heuristic[move_key(chess_move) as usize];
        *history = (*history + delta).clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
    }

    fn update_capture_history(&mut self, chess_move: ChessMove, victim: Piece, delta: i32) {
        let history = &mut self.capture_history[capture_history_key(chess_move, victim)];
        let updated = i32::from(*history) + delta;
        *history = updated.clamp(-CAPTURE_HISTORY_LIMIT, CAPTURE_HISTORY_LIMIT) as i16;
    }

    fn is_countermove(&self, chess_move: ChessMove, ply: usize) -> bool {
        if ply == 0 {
            return false;
        }

        self.move_stack
            .get(ply - 1)
            .copied()
            .flatten()
            .and_then(|prev_move| {
                self.countermove
                    .get(move_key(prev_move) as usize)
                    .copied()
                    .flatten()
            }) == Some(chess_move)
    }

    fn ensure_ply_capacity(&mut self, size: usize) {
        if self.killer_moves.len() < size {
            self.killer_moves.resize(size, [None, None]);
        }
        if self.move_stack.len() < size {
            self.move_stack.resize(size, None);
        }
    }

    fn pawn_entry(&mut self, board: &Board) -> PawnCacheEntry {
        let pawn_hash = board.get_pawn_hash();
        if let Some(entry) = self.pawn_cache.get(&pawn_hash) {
            return *entry;
        }

        let entry = analyze_pawns(board);
        if self.pawn_cache.len() >= MAX_PAWN_CACHE_ENTRIES {
            self.pawn_cache.clear();
        }
        self.pawn_cache.insert(pawn_hash, entry);
        entry
    }

    fn trim_caches(&mut self) {
        if self.tt.len() > MAX_TT_ENTRIES {
            self.tt.clear();
        }
        if self.eval_cache.len() > MAX_EVAL_CACHE_ENTRIES {
            self.eval_cache.clear();
        }
        if self.pawn_cache.len() > MAX_PAWN_CACHE_ENTRIES {
            self.pawn_cache.clear();
        }
    }

    fn extract_principal_variation(&self, mut board: Board, depth: i32) -> Vec<String> {
        let mut line = Vec::new();

        for _ in 0..depth {
            let Some(entry) = self.tt.get(&board_hash(&board)) else {
                break;
            };
            let Some(best_move) = entry.best_move else {
                break;
            };
            if !move_is_legal(&board, best_move) {
                break;
            }
            line.push(best_move.to_string());
            board = board.make_move_new(best_move);
        }

        line
    }
}

fn board_hash(board: &Board) -> u64 {
    board.get_hash()
}

fn available_root_threads() -> usize {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(MAX_ROOT_THREADS))
        .unwrap_or(1)
}

fn square_index(square: Square) -> usize {
    square.to_index()
}

fn mirror_index(index: usize) -> usize {
    index ^ 56
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => PAWN,
        Piece::Knight => KNIGHT,
        Piece::Bishop => BISHOP,
        Piece::Rook => ROOK,
        Piece::Queen => QUEEN,
        Piece::King => 0,
    }
}

fn move_key(chess_move: ChessMove) -> u16 {
    let source = square_index(chess_move.get_source()) as u16;
    let dest = square_index(chess_move.get_dest()) as u16;
    let promotion = match chess_move.get_promotion() {
        Some(Piece::Knight) => 1,
        Some(Piece::Bishop) => 2,
        Some(Piece::Rook) => 3,
        Some(Piece::Queen) => 4,
        _ => 0,
    };
    (source << 10) | (dest << 4) | promotion
}

fn capture_piece_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
}

fn capture_history_key(chess_move: ChessMove, victim: Piece) -> usize {
    move_key(chess_move) as usize * CAPTURE_HISTORY_PIECES + capture_piece_index(victim)
}

fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

fn piece_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
}

fn remove_piece(
    piece_occ: &mut [BitBoard; 6],
    color_occ: &mut [BitBoard; 2],
    color: Color,
    piece: Piece,
    square: Square,
) {
    let bit = BitBoard::from_square(square);
    piece_occ[piece_index(piece)] ^= bit;
    color_occ[color_index(color)] ^= bit;
}

fn add_piece(
    piece_occ: &mut [BitBoard; 6],
    color_occ: &mut [BitBoard; 2],
    color: Color,
    piece: Piece,
    square: Square,
) {
    let bit = BitBoard::from_square(square);
    piece_occ[piece_index(piece)] |= bit;
    color_occ[color_index(color)] |= bit;
}

fn attackers_to_square(
    square: Square,
    occupied: BitBoard,
    piece_occ: &[BitBoard; 6],
    color_occ: &[BitBoard; 2],
) -> BitBoard {
    let pawns = piece_occ[piece_index(Piece::Pawn)];
    let rooks = piece_occ[piece_index(Piece::Rook)] | piece_occ[piece_index(Piece::Queen)];
    let bishops = piece_occ[piece_index(Piece::Bishop)] | piece_occ[piece_index(Piece::Queen)];
    let knights = piece_occ[piece_index(Piece::Knight)];
    let kings = piece_occ[piece_index(Piece::King)];
    let white_pawns = pawns & color_occ[color_index(Color::White)];
    let black_pawns = pawns & color_occ[color_index(Color::Black)];

    get_rook_moves(square, occupied) & rooks
        | get_bishop_moves(square, occupied) & bishops
        | get_knight_moves(square) & knights
        | get_king_moves(square) & kings
        | get_pawn_attacks(square, Color::Black, white_pawns)
        | get_pawn_attacks(square, Color::White, black_pawns)
}

fn least_valuable_attacker(attackers: BitBoard, piece_occ: &[BitBoard; 6]) -> Option<(Square, Piece)> {
    for piece in [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
        Piece::King,
    ] {
        let matches = attackers & piece_occ[piece_index(piece)];
        if matches != BitBoard(0) {
            return Some((matches.to_square(), piece));
        }
    }
    None
}

fn static_exchange_eval(board: &Board, chess_move: ChessMove) -> i32 {
    let source = chess_move.get_source();
    let dest = chess_move.get_dest();
    let side = board.side_to_move();
    let moving_piece = match board.piece_on(source) {
        Some(piece) => piece,
        None => return 0,
    };
    let captured_piece = match victim_piece(board, chess_move) {
        Some(piece) => piece,
        None => return 0,
    };

    let mut piece_occ = [
        *board.pieces(Piece::Pawn),
        *board.pieces(Piece::Knight),
        *board.pieces(Piece::Bishop),
        *board.pieces(Piece::Rook),
        *board.pieces(Piece::Queen),
        *board.pieces(Piece::King),
    ];
    let mut color_occ = [*board.color_combined(Color::White), *board.color_combined(Color::Black)];
    let mut gains = [0; 32];
    gains[0] = piece_value(captured_piece);

    remove_piece(&mut piece_occ, &mut color_occ, side, moving_piece, source);
    if is_en_passant_capture(board, chess_move) {
        let captured_square = Square::make_square(source.get_rank(), dest.get_file());
        remove_piece(
            &mut piece_occ,
            &mut color_occ,
            !side,
            Piece::Pawn,
            captured_square,
        );
    } else {
        remove_piece(&mut piece_occ, &mut color_occ, !side, captured_piece, dest);
    }

    let placed_piece = chess_move.get_promotion().unwrap_or(moving_piece);
    if let Some(promotion) = chess_move.get_promotion() {
        gains[0] += piece_value(promotion) - piece_value(Piece::Pawn);
    }
    add_piece(&mut piece_occ, &mut color_occ, side, placed_piece, dest);

    let mut occupied = color_occ[0] | color_occ[1];
    let mut occupant_color = side;
    let mut occupant_piece = placed_piece;
    let mut current_side = !side;
    let mut depth = 0usize;

    loop {
        let attackers =
            attackers_to_square(dest, occupied, &piece_occ, &color_occ) & color_occ[color_index(current_side)];
        if attackers == BitBoard(0) {
            break;
        }
        let Some((attacker_square, attacker_piece)) = least_valuable_attacker(attackers, &piece_occ) else {
            break;
        };

        depth += 1;
        gains[depth] = piece_value(attacker_piece) - gains[depth - 1];
        if gains[depth].max(-gains[depth - 1]) < 0 {
            break;
        }

        remove_piece(&mut piece_occ, &mut color_occ, occupant_color, occupant_piece, dest);
        remove_piece(
            &mut piece_occ,
            &mut color_occ,
            current_side,
            attacker_piece,
            attacker_square,
        );
        add_piece(
            &mut piece_occ,
            &mut color_occ,
            current_side,
            attacker_piece,
            dest,
        );
        occupied = color_occ[0] | color_occ[1];
        occupant_color = current_side;
        occupant_piece = attacker_piece;
        current_side = !current_side;
    }

    while depth > 0 {
        depth -= 1;
        gains[depth] = -gains[depth + 1].max(-gains[depth]);
    }

    gains[0]
}

fn pick_next_move(moves: &mut [ScoredMove], start: usize) -> Option<ChessMove> {
    if start >= moves.len() {
        return None;
    }

    let mut best_index = start;
    for index in (start + 1)..moves.len() {
        if moves[index].score > moves[best_index].score {
            best_index = index;
        }
    }
    moves.swap(start, best_index);
    Some(moves[start].chess_move)
}

fn late_move_reduction(depth: i32, move_count: usize) -> i32 {
    let mut reduction = 1;
    if depth >= 5 && move_count > 6 {
        reduction += 1;
    }
    if depth >= 6 && move_count > 12 {
        reduction += 1;
    }
    reduction
}

fn late_move_pruning_limit(depth: i32) -> usize {
    match depth {
        d if d <= 1 => 9,
        2 => 14,
        3 => 22,
        _ => usize::MAX,
    }
}

fn projected_next_iteration_time(
    depth: i32,
    last_time: Duration,
    previous_time: Option<Duration>,
    last_nodes: u64,
    previous_nodes: Option<u64>,
) -> Duration {
    let mut factor: f64 = match depth {
        0..=2 => 1.3,
        3 => 1.45,
        4 => 1.65,
        5 => 1.82,
        _ => 1.95,
    };

    if let Some(previous) = previous_time {
        if previous.as_secs_f64() > 0.0 {
            let ratio = (last_time.as_secs_f64() / previous.as_secs_f64()).clamp(1.15, 3.2);
            factor = factor.max((0.55 * factor + 0.45 * ratio).clamp(1.15, 3.2));
        }
    }
    if let Some(previous) = previous_nodes {
        if previous > 0 {
            let ratio = (last_nodes as f64 / previous as f64).clamp(1.15, 3.2);
            factor = factor.max((0.55 * factor + 0.45 * ratio).clamp(1.15, 3.2));
        }
    }

    Duration::from_secs_f64((last_time.as_secs_f64() * factor.clamp(1.15, 3.2)).max(0.001))
}

fn should_skip_next_iteration(
    search_start: Instant,
    budget: Duration,
    next_depth: i32,
    last_nodes: Option<u64>,
    previous_nodes: Option<u64>,
    last_time: Option<Duration>,
    previous_time: Option<Duration>,
    best_score: i32,
) -> bool {
    if best_score.abs() >= MATE_SCORE - 512 {
        return true;
    }

    let Some(last_time) = last_time else {
        return false;
    };
    let Some(last_nodes) = last_nodes else {
        return false;
    };

    let elapsed = search_start.elapsed();
    if elapsed >= budget {
        return true;
    }
    let remaining = budget.saturating_sub(elapsed);
    if remaining.as_secs_f64() <= 0.0 {
        return true;
    }

    if next_depth <= 4 {
        return false;
    }

    if elapsed.mul_f64(2.0) <= budget {
        return false;
    }

    if next_depth <= 6 && last_time <= remaining {
        return false;
    }

    let projected = projected_next_iteration_time(
        next_depth,
        last_time,
        previous_time,
        last_nodes,
        previous_nodes,
    );
    let threshold = if next_depth >= 7 {
        1.02
    } else if next_depth >= 6 {
        1.12
    } else {
        1.2
    };
    projected.as_secs_f64() > remaining.as_secs_f64() * threshold
}

fn move_is_legal(board: &Board, candidate: ChessMove) -> bool {
    MoveGen::new_legal(board).any(|legal_move| legal_move == candidate)
}

fn in_check(board: &Board) -> bool {
    board.checkers().popcnt() > 0
}

fn gives_check(board: &Board, chess_move: ChessMove) -> bool {
    in_check(&board.make_move_new(chess_move))
}

fn is_castling(board: &Board, chess_move: ChessMove) -> bool {
    if board.piece_on(chess_move.get_source()) != Some(Piece::King) {
        return false;
    }
    let source_file = file_index(chess_move.get_source());
    let dest_file = file_index(chess_move.get_dest());
    (source_file - dest_file).abs() > 1
}

fn is_capture(board: &Board, chess_move: ChessMove) -> bool {
    victim_piece(board, chess_move).is_some()
}

fn is_en_passant_capture(board: &Board, chess_move: ChessMove) -> bool {
    board.piece_on(chess_move.get_source()) == Some(Piece::Pawn)
        && board.en_passant() == Some(chess_move.get_dest())
        && board.piece_on(chess_move.get_dest()).is_none()
        && file_index(chess_move.get_source()) != file_index(chess_move.get_dest())
}

fn victim_piece(board: &Board, chess_move: ChessMove) -> Option<Piece> {
    if let Some(piece) = board.piece_on(chess_move.get_dest()) {
        return Some(piece);
    }

    if board.piece_on(chess_move.get_source()) == Some(Piece::Pawn) {
        if let Some(ep_square) = board.en_passant() {
            let source_file = file_index(chess_move.get_source());
            let dest_file = file_index(chess_move.get_dest());
            if ep_square == chess_move.get_dest() && source_file != dest_file {
                return Some(Piece::Pawn);
            }
        }
    }

    None
}

fn piece_squares(board: &Board, color: Color, piece: Piece) -> Vec<Square> {
    (board.pieces(piece) & board.color_combined(color))
        .into_iter()
        .collect()
}

fn only_piece_color(board: &Board, color: Color) -> u64 {
    (*board.color_combined(color)).0
}

fn board_occupancy(board: &Board) -> u64 {
    (*board.combined()).0
}

fn file_index(square: Square) -> i32 {
    square.get_file().to_index() as i32
}

fn rank_index(square: Square) -> i32 {
    square.get_rank().to_index() as i32
}

fn square_from_coords(file: i32, rank: i32) -> Option<Square> {
    if !(0..=7).contains(&file) || !(0..=7).contains(&rank) {
        return None;
    }
    Some(Square::make_square(
        Rank::from_index(rank as usize),
        File::from_index(file as usize),
    ))
}

fn king_square(board: &Board, color: Color) -> Option<Square> {
    piece_squares(board, color, Piece::King).into_iter().next()
}

fn has_non_pawn_material(board: &Board, color: Color) -> bool {
    for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        if (*board.pieces(piece) & *board.color_combined(color)).popcnt() > 0 {
            return true;
        }
    }
    false
}

fn king_position_score(board: &Board, color: Color, endgame: bool) -> i32 {
    let Some(square) = king_square(board, color) else {
        return 0;
    };
    let table = if endgame {
        &KING_ENDGAME_TABLE
    } else {
        &KING_MIDGAME_TABLE
    };
    let index = square_index(square);
    if color == Color::White {
        table[index]
    } else {
        table[mirror_index(index)]
    }
}

fn game_phase(board: &Board) -> i32 {
    let knights = (*board.pieces(Piece::Knight)).popcnt() as i32;
    let bishops = (*board.pieces(Piece::Bishop)).popcnt() as i32;
    let rooks = (*board.pieces(Piece::Rook)).popcnt() as i32;
    let queens = (*board.pieces(Piece::Queen)).popcnt() as i32;
    (knights + bishops + rooks * 2 + queens * 4).min(PHASE_TOTAL)
}

fn tapered_score(midgame: i32, endgame: i32, phase: i32) -> i32 {
    (midgame * phase + endgame * (PHASE_TOTAL - phase)) / PHASE_TOTAL
}

fn analyze_pawns(board: &Board) -> PawnCacheEntry {
    let white_pawns = piece_squares(board, Color::White, Piece::Pawn);
    let black_pawns = piece_squares(board, Color::Black, Piece::Pawn);
    let mut white_ranks: [Vec<i32>; 8] = array::from_fn(|_| Vec::new());
    let mut black_ranks: [Vec<i32>; 8] = array::from_fn(|_| Vec::new());
    let mut white_files = [0_u8; 8];
    let mut black_files = [0_u8; 8];
    let mut white_center_mg = 0;
    let mut black_center_mg = 0;

    for square in white_pawns {
        let file = file_index(square) as usize;
        let rank = rank_index(square);
        white_ranks[file].push(rank);
        white_files[file] += 1;
        if file == 3 || file == 4 {
            if (3..=4).contains(&rank) {
                white_center_mg += CENTER_PAWN_BONUS;
            }
            if rank >= 4 {
                white_center_mg += 4;
            }
        }
    }

    for square in black_pawns {
        let file = file_index(square) as usize;
        let rank = rank_index(square);
        black_ranks[file].push(rank);
        black_files[file] += 1;
        if file == 3 || file == 4 {
            if (3..=4).contains(&rank) {
                black_center_mg += CENTER_PAWN_BONUS;
            }
            if rank <= 3 {
                black_center_mg += 4;
            }
        }
    }

    let (mut white_structure_mg, mut white_structure_eg) = pawn_structure_from_files(&white_ranks);
    let (mut black_structure_mg, mut black_structure_eg) = pawn_structure_from_files(&black_ranks);

    for square in piece_squares(board, Color::White, Piece::Pawn) {
        let file = file_index(square);
        let rank = rank_index(square);
        if is_passed_pawn(Color::White, file, rank, &black_ranks) {
            let progress = rank as usize;
            white_structure_mg += PASSED_PAWN_BONUS[progress];
            white_structure_eg += PASSED_PAWN_BONUS[progress] + ENDGAME_PASSED_PAWN_BONUS[progress];
            if supported_by_pawn(board, Color::White, square) {
                white_structure_mg += SUPPORTED_PASSED_PAWN_BONUS[progress] / 2;
                white_structure_eg += SUPPORTED_PASSED_PAWN_BONUS[progress];
            }
        }
    }
    for square in piece_squares(board, Color::Black, Piece::Pawn) {
        let file = file_index(square);
        let rank = rank_index(square);
        if is_passed_pawn(Color::Black, file, rank, &white_ranks) {
            let progress = (7 - rank) as usize;
            black_structure_mg += PASSED_PAWN_BONUS[progress];
            black_structure_eg += PASSED_PAWN_BONUS[progress] + ENDGAME_PASSED_PAWN_BONUS[progress];
            if supported_by_pawn(board, Color::Black, square) {
                black_structure_mg += SUPPORTED_PASSED_PAWN_BONUS[progress] / 2;
                black_structure_eg += SUPPORTED_PASSED_PAWN_BONUS[progress];
            }
        }
    }

    PawnCacheEntry {
        white_structure_mg,
        black_structure_mg,
        white_structure_eg,
        black_structure_eg,
        white_center_mg,
        black_center_mg,
        white_files,
        black_files,
    }
}

fn pawn_structure_from_files(files: &[Vec<i32>; 8]) -> (i32, i32) {
    let mut midgame = 0;
    let mut endgame = 0;
    for (file_idx, ranks) in files.iter().enumerate() {
        if ranks.len() > 1 {
            let penalty = DOUBLED_PAWN_PENALTY * (ranks.len() as i32 - 1);
            midgame -= penalty;
            endgame -= penalty * 3 / 4;
        }

        for _ in ranks {
            if is_isolated_pawn(files, file_idx as i32) {
                midgame -= ISOLATED_PAWN_PENALTY;
                endgame -= ISOLATED_PAWN_PENALTY * 3 / 4;
            }
        }
    }
    (midgame, endgame)
}

fn is_isolated_pawn(files: &[Vec<i32>], file_idx: i32) -> bool {
    let left_has = file_idx > 0 && !files[(file_idx - 1) as usize].is_empty();
    let right_has = file_idx < 7 && !files[(file_idx + 1) as usize].is_empty();
    !left_has && !right_has
}

fn is_passed_pawn(color: Color, file_idx: i32, rank: i32, enemy_files: &[Vec<i32>]) -> bool {
    for delta in -1..=1 {
        let enemy_file = file_idx + delta;
        if !(0..=7).contains(&enemy_file) {
            continue;
        }
        for enemy_rank in &enemy_files[enemy_file as usize] {
            if color == Color::White && *enemy_rank > rank {
                return false;
            }
            if color == Color::Black && *enemy_rank < rank {
                return false;
            }
        }
    }
    true
}

fn mobility_score(board: &Board, color: Color) -> i32 {
    let own = only_piece_color(board, color);
    let occupied = board_occupancy(board);
    let mut score = 0;

    for square in piece_squares(board, color, Piece::Knight) {
        score += attack_count_knight(square, own) * MOBILITY_KNIGHT;
    }
    for square in piece_squares(board, color, Piece::Bishop) {
        score += attack_count_slider(square, own, occupied, &[(1, 1), (1, -1), (-1, 1), (-1, -1)])
            * MOBILITY_BISHOP;
    }
    for square in piece_squares(board, color, Piece::Rook) {
        score += attack_count_slider(square, own, occupied, &[(1, 0), (-1, 0), (0, 1), (0, -1)])
            * MOBILITY_ROOK;
    }
    for square in piece_squares(board, color, Piece::Queen) {
        score += attack_count_slider(
            square,
            own,
            occupied,
            &[
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
            ],
        ) * MOBILITY_QUEEN;
    }

    score
}

fn rook_activity_score(
    board: &Board,
    color: Color,
    own_files: &[u8; 8],
    enemy_files: &[u8; 8],
) -> i32 {
    let mut score = 0;
    for rook in piece_squares(board, color, Piece::Rook) {
        let file = file_index(rook) as usize;
        if own_files[file] == 0 && enemy_files[file] == 0 {
            score += ROOK_OPEN_FILE_BONUS;
        } else if own_files[file] == 0 {
            score += ROOK_SEMI_OPEN_FILE_BONUS;
        }

        let rank = rank_index(rook);
        if (color == Color::White && rank == 6) || (color == Color::Black && rank == 1) {
            score += ROOK_SEVENTH_RANK_BONUS;
        }
    }

    score
}

fn knight_outpost_score(board: &Board, color: Color) -> i32 {
    let mut score = 0;
    for square in piece_squares(board, color, Piece::Knight) {
        let rank = rank_index(square);
        let file = file_index(square);
        let advanced = if color == Color::White {
            rank >= 3
        } else {
            rank <= 4
        };
        if !advanced {
            continue;
        }
        if !supported_by_pawn(board, color, square) {
            continue;
        }
        if attacked_by_enemy_pawn(board, color, square) {
            continue;
        }

        score += KNIGHT_OUTPOST_BONUS;
        if (2..=5).contains(&file) {
            score += 4;
        }
    }
    score
}

fn development_score(board: &Board, color: Color, endgame: bool) -> i32 {
    if endgame {
        return 0;
    }

    let home_minors = match color {
        Color::White => [Square::B1, Square::G1, Square::C1, Square::F1],
        Color::Black => [Square::B8, Square::G8, Square::C8, Square::F8],
    };
    let home_pieces = [Piece::Knight, Piece::Knight, Piece::Bishop, Piece::Bishop];
    let mut score = 0;
    for (square, piece) in home_minors.into_iter().zip(home_pieces) {
        if board.piece_on(square) == Some(piece) && board.color_on(square) == Some(color) {
            score -= UNDEVELOPED_MINOR_PENALTY;
        }
    }
    score
}

fn supported_by_pawn(board: &Board, color: Color, square: Square) -> bool {
    let file = file_index(square);
    let rank = rank_index(square);
    let support_rank = if color == Color::White {
        rank - 1
    } else {
        rank + 1
    };

    for delta in [-1, 1] {
        let Some(candidate) = square_from_coords(file + delta, support_rank) else {
            continue;
        };
        if board.piece_on(candidate) == Some(Piece::Pawn)
            && board.color_on(candidate) == Some(color)
        {
            return true;
        }
    }

    false
}

fn attacked_by_enemy_pawn(board: &Board, color: Color, square: Square) -> bool {
    let file = file_index(square);
    let rank = rank_index(square);
    let attack_rank = if color == Color::White {
        rank + 1
    } else {
        rank - 1
    };

    for delta in [-1, 1] {
        let Some(candidate) = square_from_coords(file + delta, attack_rank) else {
            continue;
        };
        if board.piece_on(candidate) == Some(Piece::Pawn)
            && board.color_on(candidate) == Some(!color)
        {
            return true;
        }
    }

    false
}

fn attack_count_knight(square: Square, own: u64) -> i32 {
    let file = file_index(square);
    let rank = rank_index(square);
    let deltas = [
        (-2, -1),
        (-2, 1),
        (-1, -2),
        (-1, 2),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
    ];
    let mut count = 0;
    for (df, dr) in deltas {
        if let Some(target) = square_from_coords(file + df, rank + dr) {
            if own & (1_u64 << square_index(target)) == 0 {
                count += 1;
            }
        }
    }
    count
}

fn attack_count_slider(square: Square, own: u64, occupied: u64, deltas: &[(i32, i32)]) -> i32 {
    let file = file_index(square);
    let rank = rank_index(square);
    let mut count = 0;

    for (df, dr) in deltas {
        let mut step_file = file + df;
        let mut step_rank = rank + dr;
        while let Some(target) = square_from_coords(step_file, step_rank) {
            let bit = 1_u64 << square_index(target);
            if own & bit != 0 {
                break;
            }
            count += 1;
            if occupied & bit != 0 {
                break;
            }
            step_file += df;
            step_rank += dr;
        }
    }

    count
}

fn king_safety_score(
    board: &Board,
    color: Color,
    phase: i32,
    own_files: &[u8; 8],
    enemy_files: &[u8; 8],
) -> i32 {
    if phase <= 4 {
        return 0;
    }

    let Some(king_sq) = king_square(board, color) else {
        return 0;
    };

    let mut score = 0;
    let rank = rank_index(king_sq);
    let file = file_index(king_sq);

    if (color == Color::White && (king_sq == Square::G1 || king_sq == Square::C1))
        || (color == Color::Black && (king_sq == Square::G8 || king_sq == Square::C8))
    {
        score += CASTLED_KING_BONUS;
    }

    let shield_rank = if color == Color::White {
        rank + 1
    } else {
        rank - 1
    };
    if (0..=7).contains(&shield_rank) {
        for delta in -1..=1 {
            let shield_file = file + delta;
            let Some(shield_square) = square_from_coords(shield_file, shield_rank) else {
                continue;
            };
            let piece = board.piece_on(shield_square);
            let piece_color = board.color_on(shield_square);
            if piece == Some(Piece::Pawn) && piece_color == Some(color) {
                score += 8;
            } else {
                score -= 6;
            }
        }
    }

    for delta in -1..=1 {
        let king_file = file + delta;
        if !(0..=7).contains(&king_file) {
            continue;
        }

        let idx = king_file as usize;
        let mut penalty = if delta == 0 { 12 } else { 8 };
        if enemy_files[idx] == 0 {
            penalty -= 3;
        }
        if own_files[idx] == 0 {
            score -= penalty.max(4);
        }
    }

    let pressure = king_ring_attack_pressure(board, color);
    if pressure > 0 {
        let enemy_has_queen = !piece_squares(board, !color, Piece::Queen).is_empty();
        let multiplier = if enemy_has_queen {
            phase + 6
        } else {
            phase + 1
        };
        score -= pressure * multiplier / 12;
    }

    score
}

fn king_ring_attack_pressure(board: &Board, color: Color) -> i32 {
    let Some(king_sq) = king_square(board, color) else {
        return 0;
    };

    let enemy = !color;
    let occupied = board_occupancy(board);
    let mut pressure = 0;

    for pawn in piece_squares(board, enemy, Piece::Pawn) {
        pressure += piece_ring_attack_count(pawn, Piece::Pawn, enemy, king_sq, occupied) * 2;
    }
    for knight in piece_squares(board, enemy, Piece::Knight) {
        pressure += piece_ring_attack_count(knight, Piece::Knight, enemy, king_sq, occupied) * 2;
    }
    for bishop in piece_squares(board, enemy, Piece::Bishop) {
        pressure += piece_ring_attack_count(bishop, Piece::Bishop, enemy, king_sq, occupied) * 2;
    }
    for rook in piece_squares(board, enemy, Piece::Rook) {
        pressure += piece_ring_attack_count(rook, Piece::Rook, enemy, king_sq, occupied) * 3;
    }
    for queen in piece_squares(board, enemy, Piece::Queen) {
        pressure += piece_ring_attack_count(queen, Piece::Queen, enemy, king_sq, occupied) * 4;
    }

    pressure
}

fn piece_ring_attack_count(
    source: Square,
    piece: Piece,
    color: Color,
    king_sq: Square,
    occupied: u64,
) -> i32 {
    let king_file = file_index(king_sq);
    let king_rank = rank_index(king_sq);
    let mut count = 0;
    for delta_file in -1..=1 {
        for delta_rank in -1..=1 {
            let Some(target) = square_from_coords(king_file + delta_file, king_rank + delta_rank)
            else {
                continue;
            };
            if piece_attacks_square(source, piece, color, target, occupied) {
                count += 1;
            }
        }
    }
    count
}

fn piece_attacks_square(
    source: Square,
    piece: Piece,
    color: Color,
    target: Square,
    occupied: u64,
) -> bool {
    match piece {
        Piece::Pawn => pawn_attacks_square(source, color, target),
        Piece::Knight => knight_attacks_square(source, target),
        Piece::Bishop => slider_attacks_square(
            source,
            target,
            occupied,
            &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
        ),
        Piece::Rook => slider_attacks_square(
            source,
            target,
            occupied,
            &[(1, 0), (-1, 0), (0, 1), (0, -1)],
        ),
        Piece::Queen => slider_attacks_square(
            source,
            target,
            occupied,
            &[
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
            ],
        ),
        Piece::King => king_attacks_square(source, target),
    }
}

fn pawn_attacks_square(source: Square, color: Color, target: Square) -> bool {
    let rank_step = if color == Color::White { 1 } else { -1 };
    let source_file = file_index(source);
    let source_rank = rank_index(source);
    for file_step in [-1, 1] {
        let Some(candidate) = square_from_coords(source_file + file_step, source_rank + rank_step)
        else {
            continue;
        };
        if candidate == target {
            return true;
        }
    }
    false
}

fn knight_attacks_square(source: Square, target: Square) -> bool {
    let file_delta = (file_index(source) - file_index(target)).abs();
    let rank_delta = (rank_index(source) - rank_index(target)).abs();
    (file_delta == 1 && rank_delta == 2) || (file_delta == 2 && rank_delta == 1)
}

fn king_attacks_square(source: Square, target: Square) -> bool {
    let file_delta = (file_index(source) - file_index(target)).abs();
    let rank_delta = (rank_index(source) - rank_index(target)).abs();
    file_delta <= 1 && rank_delta <= 1
}

fn slider_attacks_square(
    source: Square,
    target: Square,
    occupied: u64,
    deltas: &[(i32, i32)],
) -> bool {
    let source_file = file_index(source);
    let source_rank = rank_index(source);

    for (file_step, rank_step) in deltas {
        let mut next_file = source_file + file_step;
        let mut next_rank = source_rank + rank_step;
        while let Some(square) = square_from_coords(next_file, next_rank) {
            if square == target {
                return true;
            }
            if occupied & (1_u64 << square_index(square)) != 0 {
                break;
            }
            next_file += file_step;
            next_rank += rank_step;
        }
    }

    false
}

