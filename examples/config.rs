use postflop_solver::*;

fn main() {
    // see `basic.rs` for the explanation of the following code

    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("60%, e, a", "2.5x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: 200,
        effective_stack: 900,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    let config1_json = game.configs_as_json().unwrap();

    // Write config
    std::fs::write(
        "config.json",
        serde_json::to_string_pretty(&config1_json).unwrap(),
    )
    .unwrap();

    let config2 = std::fs::read_to_string("config.json").unwrap();
    let config2_json = serde_json::from_str(&config2).unwrap();
    if config1_json != config2_json {
        println!("Unequal!");
    }
    let mut game2 = PostFlopGame::game_from_configs_json(&config2_json).unwrap();
    game2.allocate_memory(false);
    solve(&mut game2, 1000, 0.1, true);
}
