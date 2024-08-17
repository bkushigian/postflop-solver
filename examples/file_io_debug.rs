use postflop_solver::*;

fn recursive_compare_strategies_helper(
    saved: &mut PostFlopGame,
    loaded: &mut PostFlopGame,
    storage_mode: BoardState,
) {
    let history = saved.history().to_vec();
    saved.cache_normalized_weights();
    loaded.cache_normalized_weights();

    // Check if OOP hands have the same evs
    let evs_oop_1 = saved.expected_values(0);
    let ws_oop_1 = saved.weights(0);
    let evs_oop_2 = loaded.expected_values(1);
    let ws_oop_2 = saved.weights(0);

    assert!(ws_oop_1.len() == ws_oop_2.len());
    for (w1, w2) in ws_oop_1.iter().zip(ws_oop_2) {
        assert!((w1 - w2).abs() < 0.001);
    }
    for (i, (e1, e2)) in evs_oop_1.iter().zip(&evs_oop_2).enumerate() {
        assert!((e1 - e2).abs() < 0.001, "ev diff({}): {}", i, e1 - e2);
    }

    let ev_oop_1 = compute_average(&evs_oop_1, &ws_oop_1);
    let ev_oop_2 = compute_average(&evs_oop_2, &ws_oop_2);

    let ev_diff = (ev_oop_1 - ev_oop_2).abs();
    println!("EV Diff: {:0.2}", ev_diff);
    assert!((ev_oop_1 - ev_oop_2).abs() < 0.01);
    for child_index in 0..saved.available_actions().len() {
        saved.play(child_index);
        loaded.play(child_index);

        recursive_compare_strategies_helper(saved, loaded, storage_mode);

        saved.apply_history(&history);
        loaded.apply_history(&history);
    }
}

fn compare_strategies(
    saved: &mut PostFlopGame,
    loaded: &mut PostFlopGame,
    storage_mode: BoardState,
) {
    saved.back_to_root();
    loaded.back_to_root();
    saved.cache_normalized_weights();
    loaded.cache_normalized_weights();
    for (i, ((e1, e2), cards)) in saved
        .expected_values(0)
        .iter()
        .zip(loaded.expected_values(0))
        .zip(saved.private_cards(0))
        .enumerate()
    {
        println!("ev {}: {}:{}", hole_to_string(*cards).unwrap(), e1, e2);
    }
    for (i, ((e1, e2), cards)) in saved
        .expected_values(1)
        .iter()
        .zip(loaded.expected_values(1))
        .zip(saved.private_cards(1))
        .enumerate()
    {
        println!("ev {}: {}:{}", hole_to_string(*cards).unwrap(), e1, e2);
    }
    recursive_compare_strategies_helper(saved, loaded, storage_mode);
}

fn print_strats_at_current_node(
    g1: &mut PostFlopGame,
    g2: &mut PostFlopGame,
    actions: &Vec<Action>,
) {
    let action_string = actions
        .iter()
        .map(|a| format!("{:?}", a))
        .collect::<Vec<String>>()
        .join(":");

    let player = g1.current_player();

    println!(
        "\x1B[32;1mActions To Reach Node\x1B[0m: [{}]",
        action_string
    );
    // Print high level node data
    if g1.is_chance_node() {
        println!("\x1B[32;1mPlayer\x1B[0m: Chance");
    } else if g1.is_terminal_node() {
        if player == 0 {
            println!("\x1B[32;1mPlayer\x1B[0m: OOP (Terminal)");
        } else {
            println!("\x1B[32;1mPlayer\x1B[0m: IP (Terminal)");
        }
    } else {
        if player == 0 {
            println!("\x1B[32;1mPlayer\x1B[0m: OOP");
        } else {
            println!("\x1B[32;1mPlayer\x1B[0m: IP");
        }
        let private_cards = g1.private_cards(player);
        let strat1 = g1.strategy_by_private_hand();
        let strat2 = g2.strategy_by_private_hand();
        let weights1 = g1.weights(player);
        let weights2 = g2.weights(player);
        let actions = g1.available_actions();

        // Print both games strategies
        for ((cards, (w1, s1)), (w2, s2)) in private_cards
            .iter()
            .zip(weights1.iter().zip(strat1))
            .zip(weights2.iter().zip(strat2))
        {
            let hole_cards = hole_to_string(*cards).unwrap();
            print!("\x1B[34;1m{hole_cards}\x1B[0m@({:.2} v {:.2})  ", w1, w2);
            let mut action_frequencies = vec![];
            for (a, (freq1, freq2)) in actions.iter().zip(s1.iter().zip(s2)) {
                action_frequencies.push(format!(
                    "\x1B[32;1m{:?}\x1B[0m: \x1B[31m{:0.3}\x1B[0m v \x1B[33m{:0>.3}\x1B[0m",
                    a, freq1, freq2
                ))
            }
            println!("{}", action_frequencies.join(" "));
        }
    }
}

fn main() {
    let oop_range = "AA,QQ";
    let ip_range = "KK";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("3h3s3d").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 100,
        effective_stack: 100,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [("e", "").try_into().unwrap(), ("e", "").try_into().unwrap()],
        turn_bet_sizes: [("e", "").try_into().unwrap(), ("e", "").try_into().unwrap()],
        river_bet_sizes: [("e", "").try_into().unwrap(), ("e", "").try_into().unwrap()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game1 = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game1.allocate_memory(false);

    solve(&mut game1, 100, 0.01, false);

    // save (turn)
    game1.set_target_storage_mode(BoardState::Turn).unwrap();
    save_data_to_file(&game1, "", "tmpfile.flop", None).unwrap();

    // load (turn)
    let mut game2: PostFlopGame = load_data_from_file("tmpfile.flop", None).unwrap().0;
    // compare_strategies(&mut game, &mut game2, BoardState::Turn);
    assert!(game2.rebuild_and_resolve_forgotten_streets().is_ok());

    let mut actions_so_far = vec![];

    // Print Root Node
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // OOP: Check
    actions_so_far.push(game1.available_actions()[0]);
    game1.play(0);
    game2.play(0);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // IP: Check
    actions_so_far.push(game1.available_actions()[0]);
    game1.play(0);
    game2.play(0);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // Chance: 2c
    actions_so_far.push(game1.available_actions()[0]);
    game1.play(0);
    game2.play(0);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // OOP: CHECK
    actions_so_far.push(game1.available_actions()[0]);
    game1.play(0);
    game2.play(0);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // IP: CHECK
    actions_so_far.push(game1.available_actions()[0]);
    game1.play(0);
    game2.play(0);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // CHANCE: 0
    actions_so_far.push(game1.available_actions()[1]);
    game1.play(1);
    game2.play(1);
    print_strats_at_current_node(&mut game1, &mut game2, &actions_so_far);

    // compare_strategies(&mut game, &mut game2, BoardState::Turn);
}
