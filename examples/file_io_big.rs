use postflop_solver::*;
use std::time;

fn main() {
    // see `basic.rs` for the explanation of the following code

    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";
    let file_save_name = "test_save.pfs";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: NOT_DEALT, //card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

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

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);

    let max_num_iterations = 1000;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
    let full_solve_start = time::Instant::now();
    solve(&mut game, max_num_iterations, target_exploitability, true);

    println!(
        "Full solve: {:5.3} seconds",
        full_solve_start.elapsed().as_secs_f64()
    );

    // save the solved game tree to a file
    // 4th argument is zstd compression level (1-22); requires `zstd` feature to use
    save_data_to_file(&game, "memo string", file_save_name, None).unwrap();

    // load the solved game tree from a file
    // 2nd argument is the maximum memory usage in bytes
    let (mut game2, _memo_string): (PostFlopGame, _) =
        load_data_from_file(file_save_name, None).unwrap();

    // check if the loaded game tree is the same as the original one
    game.cache_normalized_weights();
    game2.cache_normalized_weights();
    assert_eq!(game.equity(0), game2.equity(0));

    println!("\n-----------------------------------------");
    println!("Saving [Turn Save] to {}", file_save_name);
    // discard information after the river deal when serializing
    // this operation does not lose any information of the game tree itself
    game2.set_target_storage_mode(BoardState::Turn).unwrap();

    // compare the memory usage for serialization
    println!(
        "Memory usage of the original game tree: {:.2}MB", // 11.50MB
        game.target_memory_usage() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Memory usage of the truncated game tree: {:.2}MB", // 0.79MB
        game2.target_memory_usage() as f64 / (1024.0 * 1024.0)
    );

    // Overwrite the file with the truncated game tree. The game tree
    // constructed from this file cannot access information after the turn; this
    // data will need to be recomputed via `PostFlopGame::reload_and_resolve`.
    save_data_to_file(&game2, "memo string", file_save_name, None).unwrap();

    println!("Reloading from Turn Save and Resolving...");
    let turn_solve_start = time::Instant::now();
    let game3 =
        PostFlopGame::copy_reload_and_resolve(&game2, 100, target_exploitability, true).unwrap();
    for (i, (a, b)) in game2.strategy().iter().zip(game3.strategy()).enumerate() {
        if (a - b).abs() > 0.001 {
            println!("{i}: Oh no");
        }
    }
    println!(
        "Turn solve: {:5.3} seconds",
        turn_solve_start.elapsed().as_secs_f64()
    );

    println!("\n-----------------------------------------");
    println!("Saving [Flop Save] to {}", file_save_name);
    // discard information after the flop deal when serializing
    // this operation does not lose any information of the game tree itself
    game2.set_target_storage_mode(BoardState::Flop).unwrap();

    // compare the memory usage for serialization
    println!(
        "Memory usage of the original game tree: {:.2}MB", // 11.50MB
        game.target_memory_usage() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Memory usage of the truncated game tree: {:.2}MB", // 0.79MB
        game2.target_memory_usage() as f64 / (1024.0 * 1024.0)
    );

    // overwrite the file with the truncated game tree
    // game tree constructed from this file cannot access information after the flop deal
    save_data_to_file(&game2, "This is a flop save", file_save_name, None).unwrap();

    println!("Reloading from Flop Save and Resolving...");
    let flop_solve_start = time::Instant::now();
    println!("Using copy_reload_and_resolve: this results in a new game");
    let game3 =
        PostFlopGame::copy_reload_and_resolve(&game2, 100, target_exploitability, true).unwrap();

    println!(
        "\nFlop solve: {:5.3} seconds",
        flop_solve_start.elapsed().as_secs_f64()
    );

    for (i, (a, b)) in game2.strategy().iter().zip(game3.strategy()).enumerate() {
        if (a - b).abs() > 0.001 {
            println!("{i}: Oh no");
        }
    }

    println!();
    println!("Using reload_and_resolve: this overwrites the existing game");
    let flop_solve_start = time::Instant::now();
    let _ = PostFlopGame::reload_and_resolve(&mut game2, 1000, target_exploitability, true);
    println!(
        "\nFlop solve: {:5.3} seconds",
        flop_solve_start.elapsed().as_secs_f64()
    );

    // delete the file
    std::fs::remove_file(file_save_name).unwrap();
}
