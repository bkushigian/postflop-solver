use postflop_solver::*;

fn flop_to_string(flop: &[u8; 3]) -> String {
    flop.iter()
        .rev()
        .map(|card| card_to_string(*card).unwrap())
        .collect::<Vec<String>>()
        .join("")
}

fn get_equity_and_ev(game: &PostFlopGame, player: usize) -> (f32, f32) {
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let weights = game.normalized_weights(player);
    let average_equity = compute_average(&equity, weights);
    let average_ev = compute_average(&ev, weights);
    (100.0 * average_equity, average_ev)
}

fn get_strategy_percentages(game: &PostFlopGame) -> Vec<f32> {
    let player = game.current_player();
    let cards = game.private_cards(player);
    let strategy = game.strategy();
    let actions = game.available_actions();
    let weights = game.normalized_weights(player);

    (0..actions.len())
        .map(|i| compute_average(&strategy[i * cards.len()..(i + 1) * cards.len()], weights))
        .collect()
}

struct AggRow {
    ip_equity: f32,
    ip_ev: f32,
    oop_equity: f32,
    oop_ev: f32,
    actions: Vec<f32>,
}

impl AggRow {
    fn println(&self) {
        println!(
            "{:.2},{:.2},{:.2},{:.2},{}",
            self.ip_equity,
            self.ip_ev,
            self.oop_equity,
            self.oop_ev,
            self.actions
                .iter()
                .map(|a| format!("{:.2}", a * 100.0))
                .collect::<Vec<String>>()
                .join(",")
        )
    }
}

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

    let mut rows: Vec<AggRow> = Vec::new();
    let mut actions = Vec::new();

    for &flop in &flops {
        let card_config = CardConfig {
            range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
            flop,
            turn: NOT_DEALT,
            river: NOT_DEALT,
        };

        let action_tree = ActionTree::new(tree_config.clone()).unwrap();
        let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
        game.allocate_memory(false);

        let max_num_iterations = 1000;
        let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
        solve(&mut game, max_num_iterations, target_exploitability, true);

        // Uncomment to reload previously-saved game
        // (Should also comment out above solving code)
        // let (mut game, _): (PostFlopGame, _) =
        //     load_data_from_file(format!("flops/{}.bin", flop_to_string(&flop)), None).unwrap();

        game.cache_normalized_weights();

        // Store actions
        if actions.is_empty() {
            actions = game.available_actions();
        }

        // Compute statistics
        let (oop_equity, oop_ev) = get_equity_and_ev(&game, 0);
        let (ip_equity, ip_ev) = get_equity_and_ev(&game, 1);
        rows.push(AggRow {
            ip_equity,
            ip_ev,
            oop_equity,
            oop_ev,
            actions: get_strategy_percentages(&game),
        });

        // Uncomment to save game
        // save_data_to_file(
        //     &game,
        //     "memo string",
        //     format!("flops/{}.bin", flop_to_string(&flop)),
        //     None,
        // )
        // .unwrap();
    }

    println!(
        "Flop,IP Eq,IP EV,OOP Eq,OOP EV,{}",
        actions
            .iter()
            .map(|a| format!("{:?}", a))
            .collect::<Vec<String>>()
            .join(",")
    );
    for (row, flop) in rows.iter().zip(flops.iter()) {
        print!("{},", flop_to_string(flop));
        row.println();
    }
}
