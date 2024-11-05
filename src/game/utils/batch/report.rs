use std::{
    collections::HashMap,
    fmt::Display,
    fs::{create_dir, File},
    io::Write,
};

use crate::{
    compute_average, game::utils::flop_helper::flop_to_string, Action, ActionTree, PostFlopGame,
    TreeConfig,
};

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

// NOTE: We could save some time/space avoiding the enumeration of all lines,
//       but this time/space is dwarfed by the resouces actually needed to generate the report
pub fn generate_all_lines(config: TreeConfig) -> Result<Vec<Vec<Action>>, String> {
    let mut all_actions = ActionTree::new(config)
        .map_err(|e| format!("Error constructing ActionTree with input TreeConfig: {e}"))?;

    let mut all_lines = Vec::new();
    generate_all_lines_rec(&mut all_actions, &mut all_lines, Vec::new())?;

    Ok(all_lines)
}

fn generate_all_lines_rec(
    all_actions: &mut ActionTree,
    lines: &mut Vec<Vec<Action>>,
    current_line: Vec<Action>,
) -> Result<(), String> {
    if all_actions.is_terminal_node() {
        lines.push(current_line);
        return Ok(());
    }

    let history = all_actions.history().to_owned();

    for action in all_actions.available_actions().to_owned() {
        all_actions.play(action)?;

        // TODO This is assuming only flop reports
        // If next node is chance, skip this
        if all_actions.is_chance_node() {
            let mut new_line = current_line.clone();
            new_line.push(action);

            generate_all_lines_rec(all_actions, lines, new_line)?;
        }

        all_actions.back_to_root();
        all_actions.apply_history(&history)?;
    }

    Ok(())
}

/// Tree structure for computing aggregate reports
/// The strucutre mirrors the action tree of the pertinent game
/// Example use
/// TODO
pub struct AggActionTree {
    prev_actions: Vec<Action>,
    available_actions: Vec<Action>,
    // NOTE: |child_trees| <= |available_actions|
    // Because no child is created for terminating nodes
    // Use non_terminating_available_actions to get corresponding list
    // of actions taken to reach child trees
    child_trees: HashMap<Action, AggActionTree>,
    data: Vec<AggRow>,
}

impl AggActionTree {
    // NOTE: We take ownership of `all_actions` to ensure that
    // TODO: Unclear if this should take slices or vecs. Slices of slices are weird and hard to convert to from vec of vec
    pub fn init_root(lines: Vec<Vec<Action>>, config: TreeConfig) -> Result<Self, String> {
        let mut all_actions = ActionTree::new(config)
            .map_err(|e| format!("Error constructing ActionTree with input TreeConfig: {e}"))?;

        let mut root = Self::init(Vec::new(), Vec::new());

        for line in lines {
            let mut current_node = &mut root;

            all_actions.back_to_root();

            for (i, &action) in line.iter().enumerate() {
                all_actions
                    .play(action)
                    .map_err(|e| format!("Invalid action sequence: {line:?}. Cannot perform action {action:?} at index {i}.\nCaused by: {e}"))?;

                let available_actions = all_actions.available_actions().to_vec();

                // Add child node if it doesn't exist, and set current node to child node
                current_node = current_node.child_or_add(action, available_actions);
            }
        }

        Ok(root)
    }

    fn init(prev_actions: Vec<Action>, available_actions: Vec<Action>) -> Self {
        AggActionTree {
            prev_actions,
            available_actions: available_actions,
            child_trees: HashMap::new(),
            data: Vec::new(),
        }
    }

    // Create the child node if it doesn't exist
    // Then, return the child node
    fn child_or_add(
        &mut self,
        action: Action,
        available_actions: Vec<Action>,
    ) -> &mut AggActionTree {
        self.child_trees.entry(action).or_insert_with(|| {
            let mut new_prev_actions = self.prev_actions.clone();
            new_prev_actions.push(action);
            AggActionTree::init(new_prev_actions, available_actions)
        })
    }

    // current_dir = dir that report should be written to
    pub fn write(&self, current_dir: &str, report_file_name: &str) -> std::io::Result<()> {
        // Write report
        let mut f = File::create_new(format!("{}/{}", current_dir, report_file_name))?;
        writeln!(f, "{}", report_header(&self.available_actions))?;
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

        for (&action, child) in &self.child_trees {
            create_dir(format!(
                "{}/{}",
                current_dir,
                folder_name_from_action(action)
            ))?;
            child.write_self_and_children(
                format!("{}/{}", current_dir, folder_name_from_action(action)).as_str(),
                report_file_name,
            )?;
        }

        Ok(())
    }

    pub fn update_report_for_game(&mut self, game: &mut PostFlopGame, &flop: &[u8; 3]) {
        game.cache_normalized_weights();

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

        for (&action, child_tree) in &mut self.child_trees {
            let action_index = game
                .available_actions()
                .iter()
                .position(|&a| a == action)
                .expect("Input PostFlopGame does not match TreeConfig from initialization.");

            game.play(action_index);

            child_tree.update_report_for_game(game, &flop);

            game.back_to_root();
            game.apply_history(history.as_slice());
        }
    }

    pub fn print(&self) {
        println!("{}", report_header(&self.available_actions));
        for row in &self.data {
            println!("{}", row)
        }
    }

    pub fn print_self_and_children(&self) {
        println!("Line: {:?}", self.prev_actions);
        self.print();
        println!("");
        for (_, tree) in &self.child_trees {
            tree.print_self_and_children();
        }
    }

    fn current_player(&self) -> usize {
        self.prev_actions.len() % 2
    }

    pub fn non_terminating_available_actions(&self) -> Vec<Action> {
        self.available_actions
            .iter()
            .filter(|&&action| !is_terminating_action(action, self.current_player()))
            .copied()
            .collect()
    }

    pub fn available_actions(&self) -> Vec<Action> {
        self.available_actions.clone()
    }
}
