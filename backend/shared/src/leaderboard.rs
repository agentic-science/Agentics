pub fn should_replace_leaderboard_entry(current: Option<f64>, candidate: f64) -> bool {
    match current {
        None => true,
        Some(current_score) => candidate > current_score,
    }
}
