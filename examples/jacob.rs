use postflop_solver::*;

// Uncomment if reloading from previous game saves
use utils::flop_helper::flop_to_string;

fn main() {
    let oop_range ="JJ:0.973,TT-99,88:0.989,77:0.964,66:0.959,55:0.981,44-22,AQs:0.633,AJs,ATs:0.947,A9s:0.978,A8s:0.983,A7s:0.777,A6s:0.946,A5s:0.485,A4s:0.904,A3s:0.775,A2s:0.805,AKo:0.076,AQo:0.987,AJo:0.993,ATo:0.996,A9o:0.947,A8o:0.474,A5o:0.398,A4o:0.115,KQs:0.469,KJs:0.89,KTs:0.56,K9s:0.846,K8s:0.869,K7s:0.806,K6s:0.686,K5s:0.585,K4s:0.506,K3s:0.924,K2s:0.824,KQo:0.94,KJo:0.923,KTo:0.994,K9o:0.095,QJs:0.36,QTs:0.562,Q9s:0.872,Q8s:0.649,Q7s:0.99,Q6s:0.78,Q5s,Q4s:0.979,Q3s:0.932,Q2s:0.233,QJo:0.991,QTo:0.997,JTs:0.756,J9s:0.899,J8s:0.934,J7s:0.995,J6s:0.443,J5s:0.171,JTo:0.974,J9o:0.005,T9s:0.713,T8s,T7s:0.882,T6s:0.959,T9o:0.56,98s:0.866,97s:0.943,96s:0.997,95s:0.988,98o:0.447,87s:0.678,86s:0.988,85s:0.94,84s:0.935,87o:0.468,86o:0.026,76s:0.67,75s-74s,73s:0.187,76o:0.423,65s:0.465,64s:0.999,63s,65o:0.638,54s:0.465,53s,52s:0.973,54o:0.36,43s:0.859,42s:0.979,32s:0.778";
    let ip_range = "66+,55:0.729,44:0.386,33:0.134,22:0.109,A2s+,ATo+,A9o:0.38,A8o:0.001,K8s+,K7s:0.985,K6s:0.733,K5s:0.287,KJo+,KTo:0.709,Q9s+,Q8s:0.211,QJo:0.895,QTo:0.231,JTs,J9s:0.439,T9s:0.506,T8s:0.001,98s:0.261,87s:0.23,76s:0.276,65s:0.412,54s:0.339";

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 45,
        effective_stack: 980,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [
            // oop
            BetSizeOptions::try_from(("", "45%")).unwrap(),
            // ip
            BetSizeOptions::try_from(("30%, 66%", "45%")).unwrap(),
            // BetSizeOptions::try_from(("66%", "45%")).unwrap(),
        ],
        turn_bet_sizes: [
            BetSizeOptions::try_from(("30%, 70%, a", "45%, a")).unwrap(),
            BetSizeOptions::try_from(("70%, 150%", "45%, a")).unwrap(),
            // BetSizeOptions::try_from(("150%", "45%, a")).unwrap(),
        ],
        river_bet_sizes: [
            BetSizeOptions::try_from(("30%, 100%, a", "45%, a")).unwrap(),
            BetSizeOptions::try_from(("65%, 100%, 150%, a", "45%, a")).unwrap(),
        ],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };

    let flop = flop_from_str("As9d5d").unwrap();

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop,
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    // Removed lines
    // let removed_lines = [vec![Action::Check, Action::Check]];
    // game.remove_lines(&removed_lines).unwrap();

    game.allocate_memory(false);

    let max_num_iterations = 1000;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
    solve(&mut game, max_num_iterations, target_exploitability, true);

    save_data_to_file(
        &game,
        "memo string",
        format!("flops/{}.pfs", flop_to_string(&flop).unwrap()),
        None,
    )
    .unwrap();
}
