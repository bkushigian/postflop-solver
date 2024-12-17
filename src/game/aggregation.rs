use std::{
    collections::HashMap,
    fmt::Display,
    fs::{self, File},
    io::Write,
    path::Path,
};

use crate::{
    card_to_string, flop_to_string,
    utils::stats::{get_action_evs, get_action_frequencies, get_player_stats},
    Action, ActionTree, PostFlopGame, TreeConfig,
};

// TODO do we ever realistically want the Skip option?
// TODO also, for `Error`, we may actually want to error if the directory exists at all
//      (as opposed to what's done now, which may not error until after some reports are already written)
/// Describes possible behaviors for writing reports to files that already exist.
#[derive(Clone, Copy)]
pub enum ExistingReportBehavior {
    /// Describes the behavior of skipping over existing report files, _not_ overwriting them when encountered.
    Skip,
    /// Describes the behavior of overwriting existing report files without erroring.
    Overwrite,
    /// Describes the behavior of returning an error when an existing report is encountered.
    Error,
}

fn folder_name_from_action(action: Action) -> String {
    match action {
        Action::AllIn(x) => format!("allin{}", x),
        Action::Bet(x) => format!("bet{}", x),
        Action::Raise(x) => format!("raise{}", x),
        Action::Check => "check".to_string(),
        Action::Call => "call".to_string(),
        _ => unimplemented!(
            "Cannot currently make folder for terminating action {:?}",
            action
        ),
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
        "Flop,Turn,River,Frequency,IP Eq,IP EV,IP EQR,OOP Eq,OOP EV,OOP EQR,{}",
        // Title for likelihood & EV per action
        actions
            .iter()
            .map(|a| format!("{:?},{:?} EV", a, a))
            .collect::<Vec<String>>()
            .join(","),
    )
}

/// Single row in an aggregate report
pub struct AggRow {
    /// Describes the 3 cards of the flop
    flop: [u8; 3],
    /// Describes the turn card (required if river is present)
    turn: Option<u8>,
    /// Describes the river card
    river: Option<u8>,
    /// Likelihood of arriving at the current spot
    global_frequency: f32,
    /// Equity of the in-position player
    ip_equity: f32,
    /// Expected value (in chips) of the in-position player
    ip_ev: f32,
    /// Equity realization of the in-position player
    ip_eqr: f32,
    /// Equity of the out-of-position player
    oop_equity: f32,
    /// Expected value (in chips) of the out-of-position player
    oop_ev: f32,
    /// Equity realization of the out-of-position player
    oop_eqr: f32,
    /// List of action likelihoods, corresponding to the list of action from the owning AggActionTree
    action_frequencies: Vec<f32>,
    /// Expected value (in chips) resulting from each action
    action_evs: Vec<f32>,
}

impl Display for AggRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut all_stats: Vec<f32> = vec![
            self.global_frequency,
            self.ip_equity,
            self.ip_ev,
            self.ip_eqr,
            self.oop_equity,
            self.oop_ev,
            self.oop_eqr,
        ];
        for (&action, &ev) in self.action_frequencies.iter().zip(self.action_evs.iter()) {
            all_stats.push(action);
            all_stats.push(ev);
        }

        // Write flop
        write!(
            formatter,
            "{},",
            flop_to_string(&self.flop)
                .expect(format!("Row contains invalid flop cards: {:?}", &self.flop).as_str()),
        )?;

        // Write turn and river
        let optional_card_to_string = |&opt| match opt {
            Some(card) => {
                card_to_string(card).expect(format!("Row contains invalid card: {card}").as_str())
            }
            None => String::from(""),
        };
        write!(formatter, "{},", optional_card_to_string(&self.turn))?;
        write!(formatter, "{},", optional_card_to_string(&self.river))?;

        fmt_floats(&all_stats, formatter)
    }
}

// NOTE: We could save some time/space avoiding the enumeration of all lines,
//       but this time/space is dwarfed by the resouces actually needed to generate the report
/// Returns a list of all possible _complete_ lines for the given TreeConfig.
/// It will not include partial lines (i.e. lines that do not end in a terminal action).
///
/// # Examples
///
/// ```
/// use postflop_solver::{load_data_from_file, Action, BetSizeOptions, BoardState, DonkSizeOptions, TreeConfig};
/// use postflop_solver::aggregation::generate_all_lines;
///
/// let bet_sizes = BetSizeOptions::try_from(("10%", "")).unwrap();
/// let tree_config = TreeConfig {
///     initial_state: BoardState::Turn,
///     starting_pot: 200,
///     effective_stack: 900,
///     rake_rate: 0.0,
///     rake_cap: 0.0,
///     flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
///     turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
///     river_bet_sizes: [bet_sizes.clone(), bet_sizes],
///     turn_donk_sizes: None,
///     river_donk_sizes: None,
///     add_allin_threshold: 1.5,
///     force_allin_threshold: 0.15,
///     merging_threshold: 0.1,
/// };
///
/// let all_lines = generate_all_lines(tree_config).unwrap();
/// assert!(!all_lines.contains(&vec![]));
/// assert!(!all_lines.contains(&vec![Action::Check]));
/// assert!(all_lines.contains(&vec![Action::Bet(20), Action::Fold]));
/// assert!(all_lines.contains(&vec![Action::Bet(20), Action::Call, Action::Check, Action::Check]));
/// ```
pub fn generate_all_lines(config: TreeConfig) -> Result<Vec<Vec<Action>>, String> {
    let mut action_tree = ActionTree::new(config)
        .map_err(|e| format!("Error constructing ActionTree with input TreeConfig: {e}"))?;

    let mut all_lines = Vec::new();
    generate_all_lines_rec(&mut action_tree, &mut all_lines)?;

    Ok(all_lines)
}

fn generate_all_lines_rec(
    action_tree: &mut ActionTree,
    lines: &mut Vec<Vec<Action>>,
) -> Result<(), String> {
    // NOTE: history does not include chance nodes
    let history = action_tree.history().to_owned();

    if action_tree.is_terminal_node() {
        lines.push(history);
        return Ok(());
    }

    for action in action_tree.available_actions().to_owned() {
        action_tree.play(action)?;

        generate_all_lines_rec(action_tree, lines)?;

        action_tree.back_to_root();
        action_tree.apply_history(&history)?;
    }

    Ok(())
}

/// Tree structure for computing aggregate reports.
/// The strucutre mirrors the action tree of the pertinent game.
pub struct AggActionTree {
    /// List of previous actions from this node.
    prev_actions: Vec<Action>,
    /// List of all available actions.
    available_actions: Vec<Action>,
    // NOTE: |child_trees| <= |available_actions|
    // Because no child is created for terminating nodes
    // Use non_terminating_available_actions to get corresponding list
    // of actions taken to reach child trees
    /// Map of non-terminating available actions to child `AggActionTree`s
    child_trees: HashMap<Action, AggActionTree>,
    /// Aggregate report rows associated with this node, one for each board at this state in the report.
    data: Vec<AggRow>,
}

impl AggActionTree {
    /// Initialize the AggActionTree with a set of lines and a specific TreeConfig.
    /// To generate a report for all possible lines, first call `generate_all_lines` with the config.
    ///
    /// Any call to [`update_report_for_game`] on the returned `AggActionTree` should use a game
    /// that was solved using the same `config` that was passed into `init_root`.
    ///
    /// [`update_report_for_game`]: #method.update_report_for_game
    pub fn init_root(lines: Vec<Vec<Action>>, config: TreeConfig) -> Result<Self, String> {
        let mut action_tree = ActionTree::new(config)
            .map_err(|e| format!("Error constructing ActionTree with input TreeConfig: {e}"))?;

        let mut root = Self::init(Vec::new(), Vec::new());

        // Update the available actions in the root node
        root.available_actions = action_tree.available_actions().to_vec();

        for line in lines {
            let mut current_node = &mut root;

            action_tree.back_to_root();

            for (i, &action) in line.iter().enumerate() {
                action_tree
                    .play(action)
                    .map_err(|e| format!("Invalid action sequence: {line:?}. Cannot perform action {action:?} at index {i}.\nCaused by: {e}"))?;

                let available_actions = action_tree.available_actions().to_vec();

                // When node isn't terminal, add child node if it doesn't exist and set current node to child node
                if !action_tree.is_terminal_node() {
                    current_node = current_node.child_or_add(action, available_actions);
                }
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

    // Create the child node if it doesn't exist.
    // Then, return the child node.
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

    fn write_self(
        &self,
        output_dir: &str,
        report_file_name: &str,
        existing_file_behavior: ExistingReportBehavior,
    ) -> std::io::Result<()> {
        let file_path = format!("{}/{}", output_dir, report_file_name);
        let mut f = match existing_file_behavior {
            ExistingReportBehavior::Skip => {
                // If the file already exists, skip overwriting it
                if Path::new(&file_path).exists() {
                    return Ok(());
                }
                // If not, create the file
                File::create(file_path)?
            }
            // Create the file, truncating if it exists
            ExistingReportBehavior::Overwrite => File::create(&file_path)?,
            // Create the file, returning an error if it already exists
            ExistingReportBehavior::Error => File::create_new(&file_path)?,
        };

        writeln!(f, "{}", report_header(&self.available_actions))?;
        for row in &self.data {
            writeln!(f, "{}", row)?;
        }

        Ok(())
    }

    /// Output the report as a series of CSV files.
    ///
    /// The report for the root node is output to `<output_dir>/<report_file_name>.csv`
    /// At each node, a new directory is created for each possible action, where the name of the directory corresponds to the action taken.
    /// (Specifically, `check`, `call`, `bet<X>`, `raise<X>`, or `allin<X>`, where `<X>` is the amount in chips used for the aggresive action.)
    ///
    /// # Arguments
    /// * `output_dir` - The name of the root directory for the aggregation reports.
    /// * `report_file_name` - The name of the CSV file report output for each node.
    /// * `existing_file_behavior` - Describes how `write` should behave when reports already exist at `output_dir`.
    pub fn write(
        &self,
        current_dir: &str,
        report_file_name: &str,
        existing_file_behavior: ExistingReportBehavior,
    ) -> std::io::Result<()> {
        self.write_self(current_dir, report_file_name, existing_file_behavior)?;

        for (&action, child) in &self.child_trees {
            let line_dir_path = format!("{}/{}", current_dir, folder_name_from_action(action));
            if !Path::new(&line_dir_path).exists() {
                fs::create_dir(&line_dir_path)?;
            }
            child.write(
                format!("{}/{}", current_dir, folder_name_from_action(action)).as_str(),
                report_file_name,
                existing_file_behavior,
            )?;
        }

        Ok(())
    }

    /// Based on the input game, updates the `AggActionTree` data at each node within each line specified at initialization.
    ///
    /// The input game _must_ be at the same node in the game tree as `self`.
    /// In other words, if `root` is the root `AggActionTree` and `self = root.child_trees[a_0].child_trees[a_1]...`,
    /// then `game.history()` must be action indexes corresponding the the action sequence `[a_0, a_1, ...]`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use postflop_solver::aggregation::load_test_game_and_config;
    /// # let (mut game1, config) = load_test_game_and_config();
    /// # let (mut game2, _) = load_test_game_and_config();
    /// #
    /// use postflop_solver::aggregation::{AggActionTree, generate_all_lines};
    ///
    /// // All solves within the aggregation report are expected to use the same config.
    /// let all_lines = generate_all_lines(config.clone()).unwrap();
    /// let mut tree = AggActionTree::init_root(all_lines, config.clone()).unwrap();
    ///
    /// tree.update_report_for_game(&mut game1);
    /// tree.update_report_for_game(&mut game2);
    /// // ...
    /// ```
    pub fn update_report_for_game(&mut self, game: &mut PostFlopGame) {
        self.update_report_for_game_with_frequency(game, 1.0);
    }

    fn update_report_for_game_with_frequency(
        &mut self,
        game: &mut PostFlopGame,
        global_frequency: f32,
    ) {
        // Generally, game needs to be reverted to this history after playing any action
        // This is necessary to process data for multiple lines over the game
        let history = game.history().to_owned();

        if game.is_chance_node() {
            // If we're at a chance node, call `update_report_for_game` on self for each possible chance card
            for card in 0..52 {
                // Don't process chance cards that are already on the board
                if game.current_board().contains(&card) {
                    continue;
                }

                game.play(card as usize);

                self.update_report_for_game_with_frequency(game, global_frequency);

                game.back_to_root();
                game.apply_history(&history);
            }
        } else {
            // Otherwise, update the data, then recursively update all child nodes
            game.cache_normalized_weights();

            // Compute statistics
            let (oop_equity, oop_ev, oop_eqr) = get_player_stats(game, 0);
            let (ip_equity, ip_ev, ip_eqr) = get_player_stats(game, 1);
            let board = game.current_board();
            let action_frequencies = get_action_frequencies(game);
            let row = AggRow {
                flop: board[0..3]
                    .try_into()
                    .expect("board must at least have flop"),
                turn: board.get(3).copied(),
                river: board.get(4).copied(),
                global_frequency,
                ip_equity,
                ip_ev,
                ip_eqr,
                oop_equity,
                oop_ev,
                oop_eqr,
                action_frequencies: action_frequencies.clone(),
                action_evs: get_action_evs(game),
            };
            self.data.push(row);

            // Update child nodes for each action
            // NOTE: actions should only appear in the tree if the line was explicitly requested
            for (&action, child_tree) in &mut self.child_trees {
                let action_index = game
                    .available_actions()
                    .iter()
                    .position(|&a| a == action)
                    .expect("Input PostFlopGame does not match TreeConfig from initialization.");

                game.play(action_index);

                let new_global_frequency = global_frequency * action_frequencies[action_index];
                child_tree.update_report_for_game_with_frequency(game, new_global_frequency);

                game.back_to_root();
                game.apply_history(&history);
            }
        }
    }
}

// NOTE: This is only used by tests and doctests
// But we can't use cfg(test), cfg(doctest) etc. because
// cfg(doctest) currently does not work as expected
/// ONLY USE FOR TESTING
pub fn load_test_game_and_config() -> (PostFlopGame, TreeConfig) {
    use crate::{load_data_from_file, BetSizeOptions, BoardState, DonkSizeOptions};
    let (game, _): (PostFlopGame, _) =
        load_data_from_file("test-artifacts/Td9d6hQc.pfs", None).unwrap();

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

    (game, tree_config)
}

#[cfg(test)]
mod tests {
    use crate::compute_average;

    use super::*;

    fn get_current_player(prev_actions: &[Action], starting_player: usize) -> usize {
        if prev_actions.is_empty() {
            return starting_player;
        }

        if prev_actions[0] == Action::Call {
            return get_current_player(&prev_actions[1..], 0);
        }

        get_current_player(&prev_actions[1..], (starting_player + 1) % 2)
    }

    // TODO this is jank
    fn get_total_bets(prev_actions: &[Action]) -> i32 {
        let mut total = 0;
        let mut prev_bet = 0;
        let mut street_starting_total = 0;
        for &action in prev_actions {
            match action {
                Action::AllIn(x) | Action::Bet(x) | Action::Raise(x) => {
                    total = street_starting_total + prev_bet + x;
                    prev_bet = x;
                }
                Action::Call => {
                    total = street_starting_total + prev_bet * 2;
                    prev_bet = 0;
                    street_starting_total = total;
                }
                _ => (),
            }
        }
        total
    }

    fn check_row(row: &AggRow, player: usize, pot: f32) {
        // Check that equities sum to ~1
        // Skip check if both are NaN
        if !(row.ip_equity.is_nan() && row.oop_equity.is_nan()) {
            assert!((row.ip_equity + row.oop_equity - 1.0).abs() < 1e-3);
        }

        // Check that actions sum to ~1
        // Skip if action frequency is NaN
        let action_freq_total: f32 = row.action_frequencies.iter().sum();
        if !action_freq_total.is_nan() {
            assert!((action_freq_total - 1.0).abs() < 1e-3);
        }

        // Check evs sum to pot
        // Skip check if both are NaN
        if !(row.ip_ev.is_nan() && row.oop_ev.is_nan()) {
            assert!((row.oop_ev + row.ip_ev - pot).abs() < 1e-3);
        }

        // Check action EVs weighted sum to player ev
        let action_ev_weighted_sum = compute_average(&row.action_evs, &row.action_frequencies);
        let player_ev = if player == 0 { row.oop_ev } else { row.ip_ev };
        if !(action_ev_weighted_sum.is_nan() && player_ev.is_nan()) {
            assert!((action_ev_weighted_sum - player_ev).abs() < 1e-3);
        }
    }

    fn check_tree(tree: &AggActionTree, config: &TreeConfig) {
        let current_player = get_current_player(&tree.prev_actions, 0);

        for row in &tree.data {
            let pot = config.starting_pot + get_total_bets(&tree.prev_actions);
            check_row(row, current_player, pot as f32);
        }

        for (_, child) in &tree.child_trees {
            check_tree(child, &config);
        }
    }

    #[test]
    fn test_update_report_basic_game() {
        let (mut game, config) = load_test_game_and_config();

        let all_lines = generate_all_lines(config.clone()).unwrap();
        let mut tree = AggActionTree::init_root(all_lines, config.clone()).unwrap();
        tree.update_report_for_game(&mut game);
        check_tree(&tree, &config);
    }
}
