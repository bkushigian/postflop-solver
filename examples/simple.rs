use postflop_solver::*;

fn main() {
    // ranges of OOP and IP in string format
    // see the documentation of `Range` for more details about the format
    let oop_range = "66+";
    let ip_range = "66+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    // bet sizes -> 60% of the pot, geometric size, and all-in
    // raise sizes -> 2.5x of the previous bet
    // see the documentation of `BetSizeOptions` for more details
    let bet_sizes = BetSizeOptions::try_from(("100%", "100%")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop, // must match `card_config`
        starting_pot: 200,
        effective_stack: 200,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()], // [OOP, IP]
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None, // use default bet sizes
        river_donk_sizes: Some(DonkSizeOptions::try_from("100%").unwrap()),
        add_allin_threshold: 1.5, // add all-in if (maximum bet size) <= 1.5x pot
        force_allin_threshold: 0.15, // force all-in if (SPR after the opponent's call) <= 0.15
        merging_threshold: 0.1,
    };

    // build the game tree
    // `ActionTree` can be edited manually after construction
    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    // allocate memory without compression (use 32-bit float)
    game.allocate_memory(false);

    // solve the game
    let max_num_iterations = 20;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.100; // 10.0% of the pot
    let exploitability = solve(&mut game, max_num_iterations, target_exploitability, true);
    println!("Exploitability: {:.2}", exploitability);

    // get equity and EV of a specific hand
    game.cache_normalized_weights();
}
