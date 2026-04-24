pub mod prize;
pub mod scoring;
pub mod stats;
pub mod strategies;

pub use prize::{build_prize_index, compute_prize_stats};
pub use scoring::{is_completely_random, score_pick};
pub use stats::{analyze, print_analysis};
pub use strategies::generate_picks;
