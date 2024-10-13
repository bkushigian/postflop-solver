use std::path::{Path, PathBuf};

use clap::Parser;
use postflop_solver::{cards_from_str, solve, ActionTree, CardConfig, PostFlopGame, TreeConfig};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(required = true)]
    config: String,

    /// Boards to run on
    #[arg(short, long)]
    boards: Option<Vec<String>>,

    /// File with boards to run on
    #[arg(short, long)]
    boards_file: Option<String>,

    /// Directory to output solves to
    #[arg(short, long, default_value = ".")]
    dir: String,

    /// Max number of iterations to run
    #[arg(short = 'n', long, default_value = "1000")]
    max_iterations: u32,

    /// Default exploitability as ratio of pot. Defaults to 0.2 (20% of pot),
    /// but for accurate solves we recommend choosing a lower value.
    #[arg(short = 'e', long, default_value = "0.2")]
    exploitability: f32,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let config = std::fs::read_to_string(args.config).expect("Unable to read in config");

    let boards = if let Some(boards) = args.boards {
        boards
    } else {
        let boards_files = args
            .boards_file
            .expect("Must specify boards or boards_file");
        let boards_contents =
            std::fs::read_to_string(boards_files).expect("Unable to read boards_file");
        boards_contents
            .lines()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    };
    let configs_json: serde_json::Value =
        serde_json::from_str(&config).expect("Unable to parse config");
    let configs_map = configs_json.as_object().expect("Expected a json object");

    let card_config = configs_map.get("card_config").unwrap();
    let card_config: CardConfig = serde_json::from_value(card_config.clone()).unwrap();

    let tree_config = configs_map.get("tree_config").unwrap();
    let tree_config: TreeConfig = serde_json::from_value(tree_config.clone()).unwrap();

    // Create output directory if needed. Check if ".pfs" files exist, and if so abort
    let dir = PathBuf::from(args.dir);
    setup_output_directory(&dir)?;
    ensure_no_conflicts_in_output_dir(&dir, &boards)?;

    for board in &boards {
        let cards =
            cards_from_str(&board).expect(format!("Couldn't parse board {}", board).as_str());

        let mut game = PostFlopGame::with_config(
            card_config.with_cards(cards).unwrap(),
            ActionTree::new(tree_config.clone()).unwrap(),
        )
        .unwrap();

        game.allocate_memory(false);

        let max_num_iterations = args.max_iterations;
        let target_exploitability = game.tree_config().starting_pot as f32 * args.exploitability;
        solve(&mut game, max_num_iterations, target_exploitability, true);
    }
    Ok(())
}

fn setup_output_directory(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        if !dir.is_dir() {
            panic!(
                "output directory {} exists but is not a directory",
                dir.to_str().unwrap()
            );
        }
        Ok(())
    } else {
        std::fs::create_dir_all(&dir).map_err(|_| "Couldn't create dir".to_string())
    }
}

fn ensure_no_conflicts_in_output_dir(dir: &Path, boards: &[String]) -> Result<(), String> {
    for board in boards {
        // create board file name
        let board_file_name = board
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        let board_path = dir.join(board_file_name).with_extension("pfs");
        if board_path.exists() {
            return Err(format!(
                "board path {} already exists",
                board_path.to_string_lossy()
            ));
        }
    }
    Ok(())
}
