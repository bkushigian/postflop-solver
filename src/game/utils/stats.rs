use crate::{compute_average, Game, PostFlopGame};

/// Returns the player's equity, EV, and EQR (in that order).
/// *Requires game.cache_normalized_weights() to be called beforehand.*
/// Equity and EQR are float values in the range [0.0, 1.0].
/// Expected Value (EV) is a float representing the weighted average EV across all hands for the player at the current spot in game.
///
/// # Panics
///
/// A panic will occur if the input game is not solved.
/// Additionally, a panic will occur if game.cache_normalized_weights() is not called before this function.
pub fn get_player_stats(game: &PostFlopGame, player: usize) -> (f32, f32, f32) {
    assert!(
        game.is_normalized_weight_cached,
        "Normalized weights must be cached before calling get_player_stats"
    );

    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let weights = game.normalized_weights(player);
    let average_equity = compute_average(&equity, weights);
    let average_ev = compute_average(&ev, weights);
    (
        average_equity,
        average_ev,
        // Compute EQR
        average_ev / (average_equity * game.pot() as f32),
    )
}

/// Return the action frequencies of the current player.
///
/// # Panics
///
/// A panic will occur if the input game is not solved.
/// Additionally, a panic will occur if game.cache_normalized_weights() is not called before this function.
pub fn get_action_frequencies(game: &PostFlopGame) -> Vec<f32> {
    assert!(
        game.is_normalized_weight_cached,
        "Normalized weights must be cached before calling get_action_frequencies"
    );

    let player = game.current_player();
    let cards = game.private_cards(player);
    let strategy = game.strategy();
    let actions = game.available_actions();
    let weights = game.normalized_weights(player);

    (0..actions.len())
        .map(|i| compute_average(&strategy[i * cards.len()..(i + 1) * cards.len()], weights))
        .collect()
}

/// Returns the action EVs of the current player.
///
/// # Panics
/// A panic will occur if the input game is not solved.
/// Additionally, a panic will occur if game.cache_normalized_weights() is not called before this function.
pub fn get_action_evs(game: &mut PostFlopGame) -> Vec<f32> {
    assert!(
        game.is_normalized_weight_cached,
        "Normalized weights must be cached before calling get_action_evs"
    );

    let actions = game.available_actions();

    (0..actions.len())
        .map(|action_index| {
            let player = game.current_player();
            let num_private_hands = game.num_private_hands(player);
            let relevant_range =
                action_index * num_private_hands..(action_index + 1) * num_private_hands;

            let strategy = &game.strategy()[relevant_range.clone()];
            let evs_detail = &game.expected_values_detail(player)[relevant_range];
            let norm_weights = game.normalized_weights(player);
            let weights: Vec<f32> = strategy
                .iter()
                .zip(norm_weights)
                .map(|(s, w)| s * w)
                .collect();

            compute_average(evs_detail, &weights)
        })
        .collect()
}
