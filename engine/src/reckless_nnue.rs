use chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    Board, ChessMove, Color, File, Piece, Square,
};
use std::sync::OnceLock;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub const L1_SIZE: usize = 768;
pub const L2_SIZE: usize = 16;
pub const L3_SIZE: usize = 32;
pub const INPUT_BUCKETS: usize = 10;
pub const OUTPUT_BUCKETS: usize = 8;
pub const FT_QUANT: i16 = 255;
pub const FT_SHIFT: i32 = 9;
pub const L1_QUANT: f32 = 64.0;
pub const NETWORK_SCALE: f32 = 110.0;
pub const DEQUANT_MULTIPLIER: f32 =
    (1 << FT_SHIFT) as f32 / (FT_QUANT as f32 * FT_QUANT as f32 * L1_QUANT);

#[repr(C, align(64))]
pub struct Parameters {
    pub ft_threat_weights: [[i8; L1_SIZE]; 66864],
    pub ft_piece_weights: [[i16; L1_SIZE]; INPUT_BUCKETS * 768],
    pub ft_biases: [i16; L1_SIZE],
    pub l1_weights: [[i8; L2_SIZE * L1_SIZE]; OUTPUT_BUCKETS],
    pub l1_biases: [[f32; L2_SIZE]; OUTPUT_BUCKETS],
    pub l2_weights: [[[f32; L3_SIZE]; L2_SIZE]; OUTPUT_BUCKETS],
    pub l2_biases: [[f32; L3_SIZE]; OUTPUT_BUCKETS],
    pub l3_weights: [[f32; L3_SIZE]; OUTPUT_BUCKETS],
    pub l3_biases: [f32; OUTPUT_BUCKETS],
}

pub static PARAMETERS: OnceLock<&'static Parameters> = OnceLock::new();

#[repr(C, align(64))]
struct NetworkData([u8; 63266880]);

static V58_BYTES: NetworkData = NetworkData(*include_bytes!("v58.nnue"));

pub fn load_parameters() {
    let params = unsafe { std::mem::transmute::<&NetworkData, &Parameters>(&V58_BYTES) };
    PARAMETERS.set(params).ok();
    initialize_threat_lookups();
}

#[derive(Copy, Clone)]
struct PiecePair {
    inner: u32,
}

impl PiecePair {
    const fn new(excluded: bool, semi_excluded: bool, base: i32) -> Self {
        Self {
            inner: (((semi_excluded && !excluded) as u32) << 30)
                | ((excluded as u32) << 31)
                | ((base & 0x3FFFFFFF) as u32),
        }
    }

    const fn base(self, attacking: u8, attacked: u8) -> isize {
        let below = (attacking < attacked) as u32;
        ((self.inner.wrapping_add(below << 30)) & 0x80FFFFFF) as i32 as isize
    }
}

static mut PIECE_PAIR_LOOKUP: [[PiecePair; 12]; 12] = [[PiecePair { inner: 0 }; 12]; 12];
static mut PIECE_OFFSET_LOOKUP: [[i32; 64]; 12] = [[0; 64]; 12];
static mut ATTACK_INDEX_LOOKUP: [[[u8; 64]; 64]; 12] = [[[0; 64]; 64]; 12];

fn initialize_threat_lookups() {
    const PIECE_INTERACTION_MAP: [[i32; 6]; 6] = [
        [0, 1, -1, 2, -1, -1],
        [0, 1, 2, 3, 4, -1],
        [0, 1, 2, 3, -1, -1],
        [0, 1, 2, 3, -1, -1],
        [0, 1, 2, 3, 4, -1],
        [0, 1, 2, 3, -1, -1],
    ];

    const PIECE_TARGET_COUNT: [i32; 6] = [6, 10, 8, 8, 10, 8];

    let mut offset = 0;
    let mut piece_offsets = [0; 12];
    let mut offset_tables = [0; 12];

    for pc in 0..2 {
        for pt in 0..6 {
            let piece = (pt << 1) | pc;
            let mut count = 0;
            for square in 0..64 {
                unsafe { PIECE_OFFSET_LOOKUP[piece][square] = count };
                if pt != 0 || (square >= 8 && square < 56) {
                    count += reckless_attacks(pt as u8, pc as u8, square as u8, 0).popcnt() as i32;
                }
            }
            piece_offsets[piece] = count;
            offset_tables[piece] = offset;
            offset += PIECE_TARGET_COUNT[pt] * count;
        }
    }

    for attacking in 0..12 {
        for attacked in 0..12 {
            let attacking_type = attacking >> 1;
            let attacking_color = attacking & 1;
            let attacked_type = attacked >> 1;
            let attacked_color = attacked & 1;

            let map = PIECE_INTERACTION_MAP[attacking_type][attacked_type];
            let base = offset_tables[attacking]
                + (attacked_color as i32 * (PIECE_TARGET_COUNT[attacking_type] / 2) + map)
                    * piece_offsets[attacking];

            let enemy = attacking_color != attacked_color;
            let semi_excluded = attacking_type == attacked_type && (enemy || attacking_type != 0);
            let excluded = map < 0;

            unsafe {
                PIECE_PAIR_LOOKUP[attacking][attacked] =
                    PiecePair::new(excluded, semi_excluded, base)
            };
        }
    }

    for piece in 0..12 {
        let pt = piece >> 1;
        let pc = piece & 1;
        for from in 0..64 {
            let attacks = reckless_attacks(pt as u8, pc as u8, from as u8, 0);
            for to in 0..64 {
                let mask = if to == 0 { 0 } else { (1u64 << to) - 1 };
                unsafe {
                    ATTACK_INDEX_LOOKUP[piece][from][to] = (BitBoard(mask) & attacks).popcnt() as u8
                };
            }
        }
    }
}

fn reckless_attacks(pt: u8, pc: u8, sq: u8, occ: u64) -> BitBoard {
    let square = unsafe { std::mem::transmute::<u8, Square>(sq) };
    let occupancy = BitBoard(occ);
    match pt {
        0 => get_pawn_attacks(
            square,
            if pc == 0 { Color::White } else { Color::Black },
            BitBoard(0xFFFFFFFFFFFFFFFF),
        ),
        1 => get_knight_moves(square),
        2 => get_bishop_moves(square, occupancy),
        3 => get_rook_moves(square, occupancy),
        4 => get_rook_moves(square, occupancy) | get_bishop_moves(square, occupancy),
        5 => get_king_moves(square),
        _ => BitBoard(0),
    }
}

const INPUT_BUCKETS_LAYOUT: [u8; 64] = [
    0, 1, 2, 3, 3, 2, 1, 0, 4, 5, 6, 7, 7, 6, 5, 4, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
];

const OUTPUT_BUCKETS_LAYOUT: [usize; 33] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 7,
    7,
];

#[derive(Clone, Copy)]
pub struct Accumulator {
    pub psq: [[i16; L1_SIZE]; 2],
    pub threats: [[i16; L1_SIZE]; 2],
}

impl Accumulator {
    pub fn new() -> Self {
        let params = PARAMETERS.get().expect("Parameters not loaded");
        Self {
            psq: [params.ft_biases; 2],
            threats: [[0; L1_SIZE]; 2],
        }
    }
}

pub fn evaluate(board: &Board, acc: &Accumulator) -> i32 {
    let params = PARAMETERS.get().expect("Parameters not loaded");
    let occ = board.combined();
    let bucket = OUTPUT_BUCKETS_LAYOUT[occ.popcnt().min(32) as usize];
    let stm = board.side_to_move();

    let mut ft_out = [0u8; L1_SIZE];

    // Scalar implementation for stability
    let stm_idx = if stm == Color::White { 0 } else { 1 };
    for flip in 0..2 {
        let idx = stm_idx ^ flip;
        let psq_vals = &acc.psq[idx];
        let threat_vals = &acc.threats[idx];
        for i in 0..L1_SIZE / 2 {
            let left = (psq_vals[i] + threat_vals[i]).clamp(0, FT_QUANT);
            let right =
                (psq_vals[i + L1_SIZE / 2] + threat_vals[i + L1_SIZE / 2]).clamp(0, FT_QUANT);
            ft_out[i + flip * L1_SIZE / 2] = ((left as i32 * right as i32) >> FT_SHIFT) as u8;
        }
    }

    let mut l2_out = [0.0f32; L2_SIZE];
    let mut pre_activations = [0i32; L2_SIZE];
    for i in 0..L1_SIZE {
        if ft_out[i] == 0 {
            continue;
        }
        let index = i / 4;
        let k = i % 4;
        let chunk_weights = &params.l1_weights[bucket][index * L2_SIZE * 4..];
        for j in 0..L2_SIZE {
            pre_activations[j] += ft_out[i] as i32 * chunk_weights[j * 4 + k] as i32;
        }
    }
    for i in 0..L2_SIZE {
        l2_out[i] = (pre_activations[i] as f32 * DEQUANT_MULTIPLIER + params.l1_biases[bucket][i])
            .max(0.0)
            .min(1.0);
    }

    let mut l3_in = [0.0f32; L3_SIZE];
    for i in 0..L2_SIZE {
        for j in 0..L3_SIZE {
            l3_in[j] += l2_out[i] * params.l2_weights[bucket][i][j];
        }
    }
    for j in 0..L3_SIZE {
        l3_in[j] = (l3_in[j] + params.l2_biases[bucket][j]).max(0.0).min(1.0);
    }

    let mut output = 0.0f32;
    for i in 0..L3_SIZE {
        output += l3_in[i] * params.l3_weights[bucket][i];
    }
    output += params.l3_biases[bucket];

    (output * NETWORK_SCALE) as i32
}

fn pst_index(
    color: Color,
    piece_type: Piece,
    square: Square,
    king_sq: Square,
    pov: Color,
) -> usize {
    let king_f = king_sq.to_index() ^ (pov.to_index() * 56);
    let mirrored = (king_f % 8) >= 4;
    let flip = if mirrored { 7 } else { 0 } ^ (pov.to_index() * 56);
    let k_idx = INPUT_BUCKETS_LAYOUT[king_sq.to_index() ^ flip] as usize;
    let p_idx = if color == pov { 0 } else { 1 };
    let pt_idx = piece_type as usize;
    let s_idx = square.to_index() ^ flip;
    k_idx * 768 + p_idx * 384 + pt_idx * 64 + s_idx
}

pub fn update(board: &Board, m: ChessMove, acc: &Accumulator) -> Accumulator {
    let params = PARAMETERS.get().expect("Parameters not loaded");
    let mut next_acc = *acc;
    let side = board.side_to_move();
    let source = m.get_source();
    let dest = m.get_dest();
    let piece = board.piece_on(source).unwrap();
    let capture = victim_piece(board, m);
    if piece == Piece::King {
        let next_board = board.make_move_new(m);
        return full_refresh_psq(&next_board);
    }
    for pov in [Color::White, Color::Black] {
        let pov_idx = if pov == Color::White { 0 } else { 1 };
        let king_sq = board.king_square(pov);
        let idx_src = pst_index(side, piece, source, king_sq, pov);
        let weights_src = &params.ft_piece_weights[idx_src];
        for i in 0..L1_SIZE {
            next_acc.psq[pov_idx][i] -= weights_src[i];
        }
        let final_piece = m.get_promotion().unwrap_or(piece);
        let idx_dest = pst_index(side, final_piece, dest, king_sq, pov);
        let weights_dest = &params.ft_piece_weights[idx_dest];
        for i in 0..L1_SIZE {
            next_acc.psq[pov_idx][i] += weights_dest[i];
        }
        if let Some(cp) = capture {
            let cp_sq = if is_en_passant_capture(board, m) {
                Square::make_square(source.get_rank(), dest.get_file())
            } else {
                dest
            };
            let idx_cp = pst_index(!side, cp, cp_sq, king_sq, pov);
            let weights_cp = &params.ft_piece_weights[idx_cp];
            for i in 0..L1_SIZE {
                next_acc.psq[pov_idx][i] -= weights_cp[i];
            }
        }
        if is_castling(board, m) {
            let rank = source.get_rank();
            let (r_src, r_dst) = if dest.get_file() == File::G {
                (
                    Square::make_square(rank, File::H),
                    Square::make_square(rank, File::F),
                )
            } else {
                (
                    Square::make_square(rank, File::A),
                    Square::make_square(rank, File::D),
                )
            };
            let idx_r_src = pst_index(side, Piece::Rook, r_src, king_sq, pov);
            let idx_r_dst = pst_index(side, Piece::Rook, r_dst, king_sq, pov);
            for i in 0..L1_SIZE {
                next_acc.psq[pov_idx][i] -= params.ft_piece_weights[idx_r_src][i];
                next_acc.psq[pov_idx][i] += params.ft_piece_weights[idx_r_dst][i];
            }
        }
    }
    next_acc
}

pub fn full_refresh_psq(board: &Board) -> Accumulator {
    let params = PARAMETERS.get().expect("Parameters not loaded");
    let mut acc = Accumulator::new();
    for pov in [Color::White, Color::Black] {
        let pov_idx = if pov == Color::White { 0 } else { 1 };
        let king_sq = board.king_square(pov);
        for color in [Color::White, Color::Black] {
            for piece in [
                Piece::Pawn,
                Piece::Knight,
                Piece::Bishop,
                Piece::Rook,
                Piece::Queen,
                Piece::King,
            ] {
                let bb = board.pieces(piece) & board.color_combined(color);
                for sq in bb {
                    let idx = pst_index(color, piece, sq, king_sq, pov);
                    let weights = &params.ft_piece_weights[idx];
                    for i in 0..L1_SIZE {
                        acc.psq[pov_idx][i] += weights[i];
                    }
                }
            }
        }
    }
    acc
}

pub fn refresh_threats(board: &Board, acc: &mut Accumulator) {
    let params = PARAMETERS.get().expect("Parameters not loaded");
    acc.threats = [[0; L1_SIZE]; 2];
    let occupancies = *board.combined();
    for pov in [Color::White, Color::Black] {
        let pov_idx = if pov == Color::White { 0 } else { 1 };
        let king_sq = board.king_square(pov);
        let mirrored = (king_sq.to_index() ^ (pov_idx * 56)) % 8 >= 4;
        for sq in occupancies {
            let piece = board.piece_on(sq).unwrap();
            let p_type = piece as u8;
            let p_color = if (board.color_combined(Color::White).0 & (1 << sq.to_index())) != 0 {
                0
            } else {
                1
            };
            let p_idx = (p_type << 1) | p_color;
            let attacks = reckless_attacks(p_type, p_color, sq.to_index() as u8, occupancies.0);
            for target in attacks & occupancies {
                let t_type = board.piece_on(target).unwrap() as u8;
                let t_color =
                    if (board.color_combined(Color::White).0 & (1 << target.to_index())) != 0 {
                        0
                    } else {
                        1
                    };
                let t_idx = (t_type << 1) | t_color;
                let idx = threat_index(
                    p_idx,
                    sq.to_index() as u8,
                    t_idx,
                    target.to_index() as u8,
                    mirrored,
                    pov_idx as u8,
                );
                if idx >= 0 {
                    let weights = &params.ft_threat_weights[idx as usize];
                    for i in 0..L1_SIZE {
                        acc.threats[pov_idx][i] += weights[i] as i16;
                    }
                }
            }
        }
    }
}

pub fn threat_index(piece: u8, from: u8, attacked: u8, to: u8, mirrored: bool, pov: u8) -> isize {
    let flip = if mirrored { 7 } else { 0 } ^ (pov * 56);
    let from_f = from ^ flip;
    let to_f = to ^ flip;
    let attacking = (piece as usize) ^ (pov as usize);
    let attacked = (attacked as usize) ^ (pov as usize);
    unsafe {
        let pair = PIECE_PAIR_LOOKUP[attacking][attacked];
        pair.base(from_f, to_f)
            + PIECE_OFFSET_LOOKUP[attacking][from_f as usize] as isize
            + ATTACK_INDEX_LOOKUP[attacking][from_f as usize][to_f as usize] as isize
    }
}

fn victim_piece(board: &Board, m: ChessMove) -> Option<Piece> {
    if is_en_passant_capture(board, m) {
        Some(Piece::Pawn)
    } else {
        board.piece_on(m.get_dest())
    }
}

fn is_en_passant_capture(board: &Board, m: ChessMove) -> bool {
    board.piece_on(m.get_source()) == Some(Piece::Pawn)
        && board.piece_on(m.get_dest()).is_none()
        && m.get_source().get_file() != m.get_dest().get_file()
}

fn is_castling(board: &Board, m: ChessMove) -> bool {
    board.piece_on(m.get_source()) == Some(Piece::King)
        && (m.get_source().get_file() as i32 - m.get_dest().get_file() as i32).abs() > 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess::Board;
    use std::str::FromStr;
    #[test]
    fn test_startpos() {
        load_parameters();
        let board = Board::default();
        let mut acc = full_refresh_psq(&board);
        refresh_threats(&board, &mut acc);
        let score = evaluate(&board, &acc);
        println!("Startpos score: {}", score);
    }
    #[test]
    fn test_update() {
        load_parameters();
        let board = Board::default();
        let acc_full = full_refresh_psq(&board);
        let m = ChessMove::from_str("e2e4").unwrap();
        let next_board = board.make_move_new(m);
        let acc_updated = update(&board, m, &acc_full);
        let acc_refreshed = full_refresh_psq(&next_board);
        for i in 0..L1_SIZE {
            assert_eq!(acc_updated.psq[0][i], acc_refreshed.psq[0][i]);
            assert_eq!(acc_updated.psq[1][i], acc_refreshed.psq[1][i]);
        }
    }
}
