mod reckless_nnue;
use chess::Board;

fn main() {
    reckless_nnue::load_parameters();
    let board = Board::default();
    let acc = reckless_nnue::full_refresh(&board);
    let score = reckless_nnue::evaluate(&board, &acc);
    println!("Startpos score: {}", score);
}
