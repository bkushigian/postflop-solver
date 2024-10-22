use std::{
    fmt::Display,
    fs::{create_dir, File},
    io::Write,
};

use crate::{compute_average, game::utils::flop_helper::flop_to_string, Action, PostFlopGame};

/// Returns the player's equity, EV, and EQR (in that order)
/// Equity and EQR are float values from 0-100 (percentages)
fn get_player_stats(game: &PostFlopGame, player: usize) -> (f32, f32, f32) {
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let weights = game.normalized_weights(player);
    let average_equity = compute_average(&equity, weights);
    let average_ev = compute_average(&ev, weights);
    (
        100.0 * average_equity,
        average_ev,
        100.0 * average_ev / (average_equity * game.pot() as f32),
    )
}

fn get_action_percentages(game: &PostFlopGame) -> Vec<f32> {
    let player = game.current_player();
    let cards = game.private_cards(player);
    let strategy = game.strategy();
    let actions = game.available_actions();
    let weights = game.normalized_weights(player);

    (0..actions.len())
        .map(|i| compute_average(&strategy[i * cards.len()..(i + 1) * cards.len()], weights))
        .collect()
}

// TODO is there any better way to do this?
// I would rather not have to replay histories here b/c it is complicated and possibly slow
// NOTE: Since this mutates the game, it un-caches weights
fn get_action_evs(game: &mut PostFlopGame) -> Vec<f32> {
    let actions = game.available_actions();
    (0..actions.len())
        .map(|action_index| {
            let player = game.current_player();
            let history = game.history().to_owned();
            game.play(action_index);
            game.cache_normalized_weights();

            let evs = game.expected_values(player);
            let average_ev = evs.iter().sum::<f32>() / evs.len() as f32;

            game.back_to_root();
            game.apply_history(history.as_slice());

            average_ev
        })
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

fn fmt_floats(floats: &[f32], formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
        formatter,
        "{}",
        floats
            .iter()
            .map(|x| format!("{:.2}", x))
            .collect::<Vec<String>>()
            .join(",")
    )
}

fn report_header(actions: &Vec<Action>) -> String {
    format!(
        "Flop,IP Eq,IP EV,IP EQR,OOP Eq,OOP EV,OOP EQR,{}",
        // Title for likelihood & EV per action
        actions
            .iter()
            .map(|a| format!("{:?},{:?} EV", a, a))
            .collect::<Vec<String>>()
            .join(","),
    )
}

/// Single row in an aggregate report
/// flop -- describes the 3 cards of the flop
/// ip_equity -- equity (0-100 value) of the in-position player
/// ip_ev -- expected value (in chips) of the in-position player
/// ip_eqr -- equity realization (0-100 value) of the in-position player
/// oop_equity -- equity (0-100 value) of the out-of-position player
/// oop_ev -- expected value (in chips) of the out-of-position player
/// oop_eqr -- equity realization (0-100 value) of the out-of-position player
/// actions -- list of action likelihoods (0-100), corresponding to the list of action from the owning AggActionTree
/// action_evs -- expected value (in chips) resulting from each action
pub struct AggRow {
    flop: [u8; 3],
    ip_equity: f32,
    ip_ev: f32,
    ip_eqr: f32,
    oop_equity: f32,
    oop_ev: f32,
    oop_eqr: f32,
    actions: Vec<f32>,
    action_evs: Vec<f32>,
}

impl Display for AggRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut all_stats: Vec<f32> = vec![
            self.ip_equity,
            self.ip_ev,
            self.ip_eqr,
            self.oop_equity,
            self.oop_ev,
            self.oop_eqr,
        ];
        all_stats.append(&mut self.actions.clone());
        all_stats.append(&mut self.action_evs.clone());

        write!(formatter, "{},", flop_to_string(&self.flop))?;
        fmt_floats(&all_stats, formatter)
    }
}

/// Tree structure for computing aggregate reports
/// The strucutre mirrors the action tree of the pertinent game
/// Example use
/// TODO
pub struct AggActionTree {
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
    pub fn init_root() -> AggActionTree {
        Self::init(Vec::new(), Vec::new())
    }

    fn init(prev_actions: Vec<Action>, avail_actions: Vec<Action>) -> AggActionTree {
        AggActionTree {
            prev_actions,
            avail_actions,
            child_trees: Vec::new(),
            data: Vec::new(),
        }
    }

    // current_dir = dir that report should be written to
    pub fn write(&self, current_dir: &str, report_file_name: &str) -> std::io::Result<()> {
        // Write report
        let mut f = File::create_new(format!("{}/{}", current_dir, report_file_name))?;
        writeln!(f, "{}", report_header(&self.avail_actions))?;
        for row in &self.data {
            writeln!(f, "{}", row)?;
        }

        Ok(())
    }

    // TODO currently using `write!`, which will not overwrite file
    // Should allow to overwrite file with some force option
    pub fn write_self_and_children(
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

    pub fn update(&mut self, game: &mut PostFlopGame, &flop: &[u8; 3]) {
        game.cache_normalized_weights();

        self.avail_actions = game.available_actions();

        // Compute statistics
        let (oop_equity, oop_ev, oop_eqr) = get_player_stats(&game, 0);
        let (ip_equity, ip_ev, ip_eqr) = get_player_stats(&game, 1);
        self.data.push(AggRow {
            flop,
            ip_equity,
            ip_ev,
            ip_eqr,
            oop_equity,
            oop_ev,
            oop_eqr,
            actions: get_action_percentages(&game),
            action_evs: get_action_evs(game),
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

    pub fn print(&self) {
        println!("{}", report_header(&self.avail_actions));
        for row in &self.data {
            println!("{}", row)
        }
    }

    pub fn print_self_and_children(&self) {
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

    pub fn non_terminating_avail_actions(&self) -> Vec<Action> {
        self.avail_actions
            .iter()
            .filter(|&&action| !is_terminating_action(action, self.current_player()))
            .copied()
            .collect()
    }

    pub fn avail_actions(&self) -> Vec<Action> {
        self.avail_actions.clone()
    }
}
