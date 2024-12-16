use std::{
    collections::HashMap,
    fmt::Display,
    fs::{self, File},
    io::Write,
    path::Path,
};

use crate::{
    card_to_string, compute_average, game::utils::flop_helper::flop_to_string, Action, ActionTree,
    PostFlopGame, TreeConfig,
};

// TODO do we ever realistically want the Skip option?
// TODO also, for `Error`, we may actually want to error if the directory exists at all
//      (as opposed to what's done now, which may not error until after some reports are already written)
/// Describes possible behaviors for writing reports to files that already exist.
///
/// `Skip` describes the behavior of skipping over existing report files, _not_ overwriting them when encountered.
/// `Overwrite` describes the behavior of overwriting existing report files without erroring.
/// `Error` describes the behavior of returning an error when an existing report is encountered.
#[derive(Clone, Copy)]
pub enum ExistingReportBehavior {
    Skip,
    Overwrite,
    Error,
}

/// Returns the player's equity, EV, and EQR (in that order).
/// *Requires game.cache_normalized_weights() to be called beforehand.*
/// Equity and EQR are float values in the range [0.0, 1.0].
/// Expected Value (EV) is a float representing the weighted average EV across all hands for the player at the current spot in game.
///
/// # Panics
///
/// A panic will occur if the input game is not solved.
/// Additionally, a panic will occur if game.cache_normalized_weights() is not called before this function.
fn get_player_stats(game: &PostFlopGame, player: usize) -> (f32, f32, f32) {
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let weights = game.normalized_weights(player);
    let average_equity = compute_average(&equity, weights);
    let average_ev = compute_average(&ev, weights);
    (
        average_equity,
        average_ev,
        // Compute EQR
        average_ev / (average_equity * game.pot() as f32),
    )
}

/// Return the action frequencies of the current player.
///
/// # Panics
///
/// A panic will occur if the input game is not solved.
/// Additionally, a panic will occur if game.cache_normalized_weights() is not called before this function.
fn get_action_frequencies(game: &PostFlopGame) -> Vec<f32> {
    let player = game.current_player();
    let cards = game.private_cards(player);
    let strategy = game.strategy();
    let actions = game.available_actions();
    let weights = game.normalized_weights(player);

    (0..actions.len())
        .map(|i| compute_average(&strategy[i * cards.len()..(i + 1) * cards.len()], weights))
        .collect()
}

// Need game to determine cost of Action::Call
fn get_action_cost(game: &PostFlopGame, action: Action) -> i32 {
    match action {
        Action::Bet(x) | Action::Raise(x) | Action::AllIn(x) => x,
        Action::Call => {
            let prev_action = game.prev_action();
            // Previous action should always be a bet, raise, or allin
            assert!(matches!(
                prev_action,
                Action::Bet(_) | Action::Raise(_) | Action::AllIn(_)
            ));
            get_action_cost(game, prev_action)
        }
        _ => 0,
    }
}

fn get_action_evs(game: &mut PostFlopGame) -> Vec<f32> {
    let actions = game.available_actions();
    let history = game.history().to_owned();

    (0..actions.len())
        .map(|action_index| {
            let player = game.current_player();

            game.play(action_index);
            game.cache_normalized_weights();

            let weights = game.normalized_weights(player).to_owned();
            let evs = game.expected_values(player);
            let average_ev_after_action = compute_average(&evs, &weights);

            game.back_to_root();
            game.apply_history(&history);

            // Actual EV of action
            average_ev_after_action - get_action_cost(game, actions[action_index]) as f32
        })
        .collect()
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
        "Flop,Turn,River,IP Eq,IP EV,IP EQR,OOP Eq,OOP EV,OOP EQR,{}",
        // Title for likelihood & EV per action
        actions
            .iter()
            .map(|a| format!("{:?},{:?} EV", a, a))
            .collect::<Vec<String>>()
            .join(","),
    )
}

/// Single row in an aggregate report
/// flop: describes the 3 cards of the flop
/// turn: optional value, describes the turn card (required if river is present)
/// river: optional value, describes the river card
/// ip_equity: equity (0-100 value) of the in-position player
/// ip_ev: expected value (in chips) of the in-position player
/// ip_eqr: equity realization (0-100 value) of the in-position player
/// oop_equity: equity (0-100 value) of the out-of-position player
/// oop_ev: expected value (in chips) of the out-of-position player
/// oop_eqr: equity realization (0-100 value) of the out-of-position player
/// actions: list of action likelihoods (0-100), corresponding to the list of action from the owning AggActionTree
/// action_evs: expected value (in chips) resulting from each action
pub struct AggRow {
    flop: [u8; 3],
    turn: Option<u8>,
    river: Option<u8>,
    ip_equity: f32,
    ip_ev: f32,
    ip_eqr: f32,
    oop_equity: f32,
    oop_ev: f32,
    oop_eqr: f32,
    action_frequencies: Vec<f32>,
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
    pub fn write(
        &self,
        current_dir: &str,
        report_file_name: &str,
        existing_file_behavior: ExistingReportBehavior,
    ) -> std::io::Result<()> {
        let file_path = format!("{}/{}", current_dir, report_file_name);
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

    pub fn write_self_and_children(
        &self,
        current_dir: &str,
        report_file_name: &str,
        existing_file_behavior: ExistingReportBehavior,
    ) -> std::io::Result<()> {
        self.write(current_dir, report_file_name, existing_file_behavior)?;

        for (&action, child) in &self.child_trees {
            let line_dir_path = format!("{}/{}", current_dir, folder_name_from_action(action));
            if !Path::new(&line_dir_path).exists() {
                fs::create_dir(&line_dir_path)?;
            }
            child.write_self_and_children(
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
    /// todo!()
    /// ```
    pub fn update_report_for_game(&mut self, game: &mut PostFlopGame) {
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

                self.update_report_for_game(game);

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
            self.data.push(AggRow {
                flop: board[0..3]
                    .try_into()
                    .expect("board must at least have flop"),
                turn: board.get(3).copied(),
                river: board.get(4).copied(),
                ip_equity,
                ip_ev,
                ip_eqr,
                oop_equity,
                oop_ev,
                oop_eqr,
                action_frequencies: get_action_frequencies(game),
                action_evs: get_action_evs(game),
            });

            // Update child nodes for each action
            // NOTE: actions should only appear in the tree if the line was explicitly requested
            for (&action, child_tree) in &mut self.child_trees {
                let action_index = game
                    .available_actions()
                    .iter()
                    .position(|&a| a == action)
                    .expect("Input PostFlopGame does not match TreeConfig from initialization.");

                game.play(action_index);

                child_tree.update_report_for_game(game);

                game.back_to_root();
                game.apply_history(&history);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{load_data_from_file, BetSizeOptions, BoardState, DonkSizeOptions};

    use super::*;

    fn load_game_and_config() -> (PostFlopGame, TreeConfig) {
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
        println!("OOP equity: {:?}", row.oop_equity);
        println!("IP equity: {:?}", row.ip_equity);
        // Skip check if both are NaN
        if !(row.ip_equity.is_nan() && row.oop_equity.is_nan()) {
            assert!((row.ip_equity + row.oop_equity - 1.0).abs() < 1e-3);
        }

        // Check that actions sum to ~1
        let action_freq_total: f32 = row.action_frequencies.iter().sum();
        println!("Action freq total: {:?}", action_freq_total);
        if !action_freq_total.is_nan() {
            assert!((action_freq_total - 1.0).abs() < 1e-3);
        }

        // Check evs sum to pot
        println!("OOP EV: {:?}", row.oop_ev);
        println!("IP EV: {:?}", row.ip_ev);
        println!("Pot: {:?}", pot);
        if !(row.ip_ev.is_nan() && row.oop_ev.is_nan()) {
            assert!((row.oop_ev + row.ip_ev - pot).abs() < 1e-3);
        }

        // Check action EVs weighted sum to player ev
        let action_ev_weighted_sum = compute_average(&row.action_evs, &row.action_frequencies);
        let player_ev = if player == 0 { row.oop_ev } else { row.ip_ev };
        println!("Action EV sum: {:?}", action_ev_weighted_sum);
        println!("Player EV: {:?}", player_ev);
        assert!((action_ev_weighted_sum - player_ev).abs() < 1e-3);
    }

    fn check_tree(tree: &AggActionTree, config: &TreeConfig) {
        let current_player = get_current_player(&tree.prev_actions, 0);
        println!("Prev actions: {:?}", tree.prev_actions);

        for row in &tree.data {
            if let Some(c) = row.river {
                println!("River: {c:?}");
            }
            let pot = config.starting_pot + get_total_bets(&tree.prev_actions);
            check_row(row, current_player, pot as f32);
        }

        for (_, child) in &tree.child_trees {
            check_tree(child, &config);
        }
    }

    #[test]
    fn test_update_report_basic_game() {
        let (mut game, config) = load_game_and_config();

        let all_lines = generate_all_lines(config.clone()).unwrap();
        let mut tree = AggActionTree::init_root(all_lines, config.clone()).unwrap();
        tree.update_report_for_game(&mut game);
        // Output the report for debugging
        // let report_dir = "reports/agg_test";
        // tree.write_self_and_children(&report_dir, "report.csv", ExistingReportBehavior::Overwrite)
        //     .expect("Problem writing to files");
        check_tree(&tree, &config);
    }
}
