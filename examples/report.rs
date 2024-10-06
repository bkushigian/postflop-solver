use std::{
    fmt::Display,
    fs::{create_dir, File},
    io::Write,
};

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

fn is_terminating_action(action: Action, player: usize) -> bool {
    match action {
        Action::AllIn(_) => false,
        Action::Bet(_) => false,
        Action::Raise(_) => false,
        // Check is terminating iff current player is IP
        Action::Check => player == 1,
        _ => true,
    }
}

fn folder_name_from_action(action: Action) -> String {
    match action {
        Action::AllIn(x) => format!("allin{}", x),
        Action::Bet(x) => format!("bet{}", x),
        Action::Raise(x) => format!("raise{}", x),
        Action::Check => "check".to_string(),
        _ => unimplemented!("Cannot currently make folder for terminating action"),
    }
}

struct AggRow {
    flop: [u8; 3],
    ip_equity: f32,
    ip_ev: f32,
    oop_equity: f32,
    oop_ev: f32,
    actions: Vec<f32>,
}

impl Display for AggRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{:.2},{:.2},{:.2},{:.2},{}",
            flop_to_string(&self.flop),
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

impl AggRow {
    fn write(&self, file: &mut File) -> std::io::Result<()> {
        writeln!(file, "{}", self)
    }
}

struct AggActionTree {
    prev_actions: Vec<Action>,
    avail_actions: Vec<Action>,
    // NOTE: |child_trees| <= |avail_actions|
    // Because no child is created for terminating nodes
    // Use non_terminating_avail_actions to get corresponding list
    // of actions taken to reach child trees
    child_trees: Vec<AggActionTree>,
    data: Vec<AggRow>,
}

impl AggActionTree {
    fn init(prev_actions: Vec<Action>, avail_actions: Vec<Action>) -> AggActionTree {
        AggActionTree {
            prev_actions,
            avail_actions,
            child_trees: Vec::new(),
            data: Vec::new(),
        }
    }

    // current_dir = dir that report should be written to
    fn write(&self, current_dir: &str, report_file_name: &str) -> std::io::Result<()> {
        // Write report
        let mut f = File::create_new(format!("{}/{}", current_dir, report_file_name))?;
        writeln!(
            f,
            "Flop,IP Eq,IP EV,OOP Eq,OOP EV,{}",
            self.avail_actions
                .iter()
                .map(|a| format!("{:?}", a))
                .collect::<Vec<String>>()
                .join(",")
        )?;
        for row in &self.data {
            writeln!(f, "{}", row)?;
        }

        Ok(())
    }

    fn write_self_and_children(
        &self,
        current_dir: &str,
        report_file_name: &str,
    ) -> std::io::Result<()> {
        self.write(current_dir, report_file_name)?;

        for (child, action) in self
            .child_trees
            .iter()
            .zip(self.non_terminating_avail_actions().iter())
        {
            create_dir(format!(
                "{}/{}",
                current_dir,
                folder_name_from_action(*action)
            ))?;
            child.write_self_and_children(
                format!("{}/{}", current_dir, folder_name_from_action(*action)).as_str(),
                report_file_name,
            )?;
        }

        Ok(())
    }

    fn update(&mut self, game: &mut PostFlopGame, &flop: &[u8; 3]) {
        game.cache_normalized_weights();

        self.avail_actions = game.available_actions();

        // Compute statistics
        let (oop_equity, oop_ev) = get_equity_and_ev(&game, 0);
        let (ip_equity, ip_ev) = get_equity_and_ev(&game, 1);
        self.data.push(AggRow {
            flop,
            ip_equity,
            ip_ev,
            oop_equity,
            oop_ev,
            actions: get_strategy_percentages(&game),
        });

        let history = game.history().to_owned();

        let mut child_tree_index = 0;
        for (i, action) in self.avail_actions.iter().enumerate() {
            // Skip terminating actions (e.g. Call)
            if is_terminating_action(*action, self.current_player()) {
                continue;
            }

            let mut new_prev_actions = self.prev_actions.clone();
            new_prev_actions.push(*action);

            game.play(i);

            // Initialize child tree if it doesn't exist
            if child_tree_index >= self.child_trees.len() {
                self.child_trees.push(AggActionTree::init(
                    new_prev_actions,
                    game.available_actions(),
                ));
            }

            let child_tree = &mut self.child_trees[child_tree_index];
            child_tree.update(game, &flop);

            game.back_to_root();
            game.apply_history(history.as_slice());

            child_tree_index += 1;
        }
    }

    fn print(&self) {
        println!(
            "Flop,IP Eq,IP EV,OOP Eq,OOP EV,{}",
            self.avail_actions
                .iter()
                .map(|a| format!("{:?}", a))
                .collect::<Vec<String>>()
                .join(",")
        );
        for row in &self.data {
            println!("{}", row)
        }
    }

    fn print_self_and_children(&self) {
        println!("Line: {:?}", self.prev_actions);
        self.print();
        println!("");
        for tree in &self.child_trees {
            tree.print_self_and_children();
        }
    }

    fn current_player(&self) -> usize {
        self.prev_actions.len() % 2
    }

    fn non_terminating_avail_actions(&self) -> Vec<Action> {
        self.avail_actions
            .iter()
            .filter(|&&action| !is_terminating_action(action, self.current_player()))
            .copied()
            .collect()
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

    let mut report_tree = AggActionTree::init(Vec::new(), Vec::new());

    for (i, &flop) in flops.iter().enumerate() {
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
        let (mut game, _): (PostFlopGame, _) =
            load_data_from_file(format!("flops/{}.bin", flop_to_string(&flop)), None).unwrap();

        if report_tree.avail_actions.is_empty() {
            report_tree.avail_actions = game.available_actions();
        }

        report_tree.update(&mut game, &flop);

        println!("Done with {} flops", i + 1);
        // Uncomment to save game
        // save_data_to_file(
        //     &game,
        //     "memo string",
        //     format!("flops/{}.bin", flop_to_string(&flop)),
        //     None,
        // )
        // .unwrap();
    }

    report_tree
        .write_self_and_children("reports/root", "report.csv")
        .expect("Problem writing to files");
}
