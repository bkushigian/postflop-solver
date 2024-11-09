use std::{
    collections::HashMap,
    fmt::Display,
    fs::{create_dir, File},
    io::Write,
};

use crate::{
    card_to_string, compute_average, game::utils::flop_helper::flop_to_string, Action, ActionTree,
    PostFlopGame, TreeConfig,
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

// TODO is there any better way to do this?
// I would rather not have to replay histories here b/c it is complicated and possibly slow
// NOTE: Since this mutates the game, it un-caches weights
fn get_action_evs(game: &mut PostFlopGame) -> Vec<f32> {
    let actions = game.available_actions();
    let history = game.history().to_owned();

    (0..actions.len())
        .map(|action_index| {
            let player = game.current_player();

            // TODO: Do we want the likelihood of the player having the hand _before_ playing the action?
            // This would effectively ignore the strategy w.r.t. the action being played

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

fn is_street_terminating_action(action: Action, player: usize) -> bool {
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
    flop: [u8; 3], // could also have turn/river here
    turn: Option<u8>,
    river: Option<u8>,
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
        for (&action, &ev) in self.actions.iter().zip(self.action_evs.iter()) {
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

/// Tree structure for computing aggregate reports
/// The strucutre mirrors the action tree of the pertinent game
/// Example use
/// TODO
pub struct AggActionTree {
    // Bet(20), Raise(50), Call
    // Bet(20), Call
    // Check, Check, Chance(3), Check
    prev_actions: Vec<Action>,
    available_actions: Vec<Action>,
    // NOTE: |child_trees| <= |available_actions|
    // Because no child is created for terminating nodes
    // Use non_terminating_available_actions to get corresponding list
    // of actions taken to reach child trees
    // NOTE/TODO: Chance nodes should NOT exist in here, b/c they're essentially encoded in AggRow
    child_trees: HashMap<Action, AggActionTree>,
    data: Vec<AggRow>, // Can make this an option (for street-terminating actions)
}

impl AggActionTree {
    // TODO: Unclear if this should take slices or vecs. Slices of slices are weird and hard to convert to from vec of vec
    // TODO: for turns/rivers, lines should not include chance nodes (or at least they're stripped out)
    pub fn init_root(lines: Vec<Vec<Action>>, config: TreeConfig) -> Result<Self, String> {
        let mut action_tree = ActionTree::new(config)
            .map_err(|e| format!("Error constructing ActionTree with input TreeConfig: {e}"))?;

        let mut root = Self::init(Vec::new(), Vec::new());

        for line in lines {
            let mut current_node = &mut root;

            action_tree.back_to_root();

            for (i, &action) in line.iter().enumerate() {
                action_tree
                    .play(action)
                    .map_err(|e| format!("Invalid action sequence: {line:?}. Cannot perform action {action:?} at index {i}.\nCaused by: {e}"))?;

                let available_actions = action_tree.available_actions().to_vec();

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

    pub fn update_report_for_game(&mut self, game: &mut PostFlopGame, board: &Vec<u8>) {
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

                let mut new_board = board.clone();
                new_board.push(card);
                self.update_report_for_game(game, &new_board);

                game.back_to_root();
                game.apply_history(&history);
            }
        } else {
            // Otherwise, update the data, then recursively update all child nodes
            game.cache_normalized_weights();

            // Compute statistics
            let (oop_equity, oop_ev, oop_eqr) = get_player_stats(game, 0);
            let (ip_equity, ip_ev, ip_eqr) = get_player_stats(game, 1);
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
                actions: get_action_percentages(game),
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

                child_tree.update_report_for_game(game, board);

                game.back_to_root();
                game.apply_history(&history);
            }
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
            .filter(|&&action| !is_street_terminating_action(action, self.current_player()))
            .copied()
            .collect()
    }

    pub fn available_actions(&self) -> Vec<Action> {
        self.available_actions.clone()
    }
}
