use std::fs;

use aggregation::{generate_all_lines, AggActionTree, ExistingReportBehavior};
use postflop_solver::*;

// Uncomment if reloading from previous game saves
use utils::flop_helper::flop_to_string;

fn main() {
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    // Get all J-high unpaired rainbow flops
    let flops = textured_flops_from_list(
        Texture::Unpaired,
        textured_flops_from_list(Texture::Rainbow, high_flops(card_from_str("Jc").unwrap())),
    );

    let bet_sizes = BetSizeOptions::try_from(("60%, e, a", "2.5x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
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

    // let lines = generate_all_lines(tree_config.clone()).unwrap();
    let lines = vec![
        vec![Action::Check, Action::Check],
        vec![Action::Check, Action::Bet(120)],
    ];

    println!("Done generating lines");

    let mut report_tree = AggActionTree::init_root(lines, tree_config.clone()).unwrap();

    println!("Done initializing tree");

    for (i, &flop) in flops[0..1].iter().enumerate() {
        // let card_config = CardConfig {
        //     range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        //     flop,
        //     turn: NOT_DEALT,
        //     river: NOT_DEALT,
        // };

        // let action_tree = ActionTree::new(tree_config.clone()).unwrap();
        // let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
        // game.allocate_memory(false);

        // let max_num_iterations = 1000;
        // let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
        // solve(&mut game, max_num_iterations, target_exploitability, true);

        // Uncomment to reload previously-saved game
        // (Should also comment out above solving code)
        let (mut game, _): (PostFlopGame, _) = load_data_from_file(
            format!("flops/{}.bin", flop_to_string(&flop).unwrap()),
            None,
        )
        .unwrap();

        report_tree.update_report_for_game(&mut game);

        // Log progress
        //if (i + 1) % 10 == 0 {
        println!("Done with {} flops", i + 1);
        //}

        // Uncomment to save game
        // save_data_to_file(
        //     &game,
        //     "memo string",
        //     format!("flops/{}.bin", flop_to_string(&flop)),
        //     None,
        // )
        // .unwrap();
    }

    let report_dir = "reports/turn_root";
    report_tree
        .write(&report_dir, "report.csv", ExistingReportBehavior::Overwrite)
        .expect("Problem writing to files");
}
