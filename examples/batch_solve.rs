use std::{
    collections::HashMap,
    fs::{remove_file, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use clap::Parser;
use postflop_solver::{
    cards_from_str, save_data_to_file, solve, Action, ActionTree, BoardState, CardConfig,
    PostFlopGame, Range, TreeConfig,
};
use serde::{Deserialize, Serialize};

const METADATA_FILENAME: &str = "meta.sdb";
const SOLVE_FILE_EXTENSION: &str = ".pfs";
const CONFIG_FILE_EXTENSION: &str = ".cfg";

// TODO make this an option
const TARGET_STORAGE_MODE: BoardState = BoardState::Turn;

fn get_fresh_name(dir: &PathBuf) -> Result<String, std::io::Error> {
    Ok(format!("solve{:?}", std::fs::read_dir(dir)?.count()))
}

#[derive(Debug, Serialize, Deserialize)]
struct SolveDBMetadata {
    // TODO is it ok to just map filename to SolveMetadata?
    // Also, could just do list of SolveMetadata
    /// Map from flop to solve file metadata
    solves: HashMap<String, Vec<SolveMetadata>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SolveMetadata {
    // name: Option<String>,
    path: String,
    config: String,
    save_state: BoardState,
}

#[derive(Debug, Serialize, Deserialize)]
struct SolveConfig {
    // name: Option<String>,
    // NOTE: For now, just manually ignore the flop in the config
    card_config: CardConfig,
    tree_config: TreeConfig,
    added_lines: Vec<Vec<Action>>,
    removed_lines: Vec<Vec<Action>>,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    config: String,

    #[clap(flatten)]
    boards: Boards,

    /// Directory representing the solve DB
    #[arg(short, long, default_value = ".")]
    dir: String,

    /// Max number of iterations to run
    #[arg(short = 'n', long, default_value = "1000")]
    max_iterations: u32,

    /// Default exploitability as ratio of pot. Defaults to 0.02 (2% of pot),
    /// but for accurate solves we recommend choosing a lower value.
    #[arg(short = 'e', long, default_value = "0.02")]
    exploitability: f32,

    /// Overwrite existing sims if a saved sim with the same name exists. By
    /// default these sims are skipped.
    #[arg(long, default_value = "false")]
    overwrite: bool,

    /// Halt the batch solve when encountering a sim with the same name. By
    /// default these sims are skipped.
    #[arg(long, default_value = "false")]
    halt_on_existing: bool,

    /// OOP's range (overwrite the range in the config)
    #[arg(long)]
    oop_range: Option<String>,

    /// IP's range (overwrites the range in the config)
    #[arg(long)]
    ip_range: Option<String>,
}

#[derive(Debug, clap::Args)]
#[group(multiple = false)]
struct Boards {
    /// Path to a file containing a list of boards
    #[clap(long)]
    boards_file: Option<String>,

    /// Specify the boards on command line
    #[clap(long, num_args=1..)]
    boards: Option<Vec<String>>,
}

impl Boards {
    fn as_list(self) -> Result<Vec<String>, std::io::Error> {
        if let Some(b) = self.boards {
            Ok(b)
        } else if let Some(bf) = self.boards_file {
            Ok(std::fs::read_to_string(bf)?
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<String>>())
        } else {
            panic!("Boards struct contains no boards!")
        }
    }
}

fn get_existing_solve_metadata<'a>(
    metadata: &'a SolveDBMetadata,
    board: &str,
    config_file_name: &str,
) -> Option<(&'a SolveMetadata, usize)> {
    for (i, data) in metadata.solves[board].iter().enumerate() {
        if data.config == config_file_name {
            return Some((data, i));
        }
    }
    None
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let dir = PathBuf::from(args.dir);

    /* ASSUMTIONS */
    /*
     * The output dir exists, and is empty if it does not contain the SDB.
     * There are no directories within dir (i.e. everything is a file in the SDB)
     * This binary is run atomically (obviously unrealistic, need to peel this back later).
     *** Need to be particularly careful about overwriting metadata before all solves/configs are written.
     */
    /**************/

    // Make a new name for this solve
    let solve_name = get_fresh_name(&dir).expect("Can't read from SDB directory");

    // Get the solve DB metadata, if it exists.
    // Otherwise, create the solve DB
    let metadata_path = dir.join(METADATA_FILENAME);
    let mut metadata = if metadata_path
        .try_exists()
        .map_err(|e| format!("Error checking SDB metadata file path existence: {e:?}"))?
    {
        let file = File::open(&metadata_path)
            .map_err(|e| format!("Error when opening SDB metadata file: {e:?}"))?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)
            .map_err(|e| format!("Error when deserializing SDB metadata file: {e:?}"))?
    } else {
        SolveDBMetadata {
            solves: HashMap::new(),
        }
    };

    // Load config
    let config_read_path = args.config;
    let mut config: SolveConfig = {
        let file = File::open(&config_read_path)
            .map_err(|e| format!("Error when opening config file: {e:?}"))?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)
            .map_err(|e| format!("Error when deserializing config file: {e:?}"))?
    };

    // TODO could take in config from cmdline somehow here...

    // Update config with command-line specified data
    // TODO can we geet rid of these clones?
    if let Some(range_string) = args.oop_range.clone() {
        config.card_config.range[0] = range_string
            .parse::<Range>()
            .map_err(|e| format!("Couldn't parse OOP Range {range_string:?}, got error:\n{e:?}"))?;
    }

    if let Some(range_string) = args.ip_range.clone() {
        config.card_config.range[1] = range_string
            .parse::<Range>()
            .map_err(|e| format!("Couldn't parse IP Range {range_string:?}, got error:\n{e:?}"))?;
    }

    // Save config to SDB
    // Unless it's already in the SDB AND no CLI args were provided
    // Need to keep track of the config path though, for storing metadata
    // TODO is there a better way to check if the given config is in the SDB?
    let config_file_name: String = if Path::new(&config_read_path)
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        == dir.canonicalize().unwrap()
        && args.oop_range.is_none()
        && args.ip_range.is_none()
    {
        // Case where no save is necessary
        // TODO better way of doing this?
        Path::new(&config_read_path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    } else {
        std::fs::write(
            dir.join(&solve_name).join(CONFIG_FILE_EXTENSION),
            serde_json::to_string_pretty(&config).expect("Could not serialize config"),
        )
        .expect("Could not write config file");
        format!("{solve_name:?}{CONFIG_FILE_EXTENSION:?}")
    };

    // Load boards
    // TODO need to canonicalize boards
    let boards: Vec<String> = args
        .boards
        .as_list()
        .map_err(|e| format!("Error getting boards: {e:?}"))?;

    // Define fn for determining of solve exists for a board (with the config at config_file_name)

    // Define fn for getting each solve path
    let solve_file_name = |board: &str| format!("{solve_name:?}{board:?}{SOLVE_FILE_EXTENSION:?}");

    // Check that the requested solves don't already exist
    // TODO currently this check could be slow with a large number of existing & requested solves
    // NOTE/TODO: For now, only checking that to see if the config files are the same
    if args.halt_on_existing {
        for board in &boards {
            if get_existing_solve_metadata(&metadata, board, &config_file_name).is_some() {
                return Err(format!(
                    "Solve already exists for board {board:?} with config {config_file_name}!"
                ));
            }
        }
    }

    // Print stats before solving
    let tree_config = config.tree_config;
    let card_config = config.card_config;
    let max_num_iterations = args.max_iterations;
    let target_exploitability = tree_config.starting_pot as f32 * args.exploitability;
    println!("Starting pot: {}", tree_config.starting_pot);
    println!("Effective stacks: {}", tree_config.effective_stack);
    println!(
        "Exploitable for {}% of staring pot ({} chips)",
        args.exploitability * 100.0,
        target_exploitability
    );

    // Begin the solving loop
    let num_boards = boards.len();
    println!("\nBeginning Solves\n----------------\n");
    for (i, board) in boards.iter().enumerate() {
        println!("\nSolving board {}/{}: {}", i + 1, num_boards, board);

        // Check for existence
        if !args.overwrite
            && get_existing_solve_metadata(&metadata, board, &config_file_name).is_some()
        {
            println!(
                "Sim for {:?} with config {:?} already exists...continuing...",
                board, config_file_name,
            );
            continue;
        }

        // Construct path, cards, game, etc.
        let path = dir.join(solve_file_name(board));
        let cards = cards_from_str(board)
            .unwrap_or_else(|e| panic!("Couldn't parse board {}: {}", board, e));
        let mut game = PostFlopGame::with_config(
            card_config.with_cards(cards).unwrap(),
            ActionTree::new(tree_config.clone()).unwrap(),
        )
        .unwrap();
        let mem_usage = game.memory_usage();
        let mem_usage_mb = (mem_usage.0 as f64) / (1024 * 1024) as f64;

        println!("Memory usage: {:5.2} MB", mem_usage_mb);

        // Solve the game
        game.allocate_memory(false);
        solve(&mut game, max_num_iterations, target_exploitability, true);
        game.set_target_storage_mode(TARGET_STORAGE_MODE).unwrap();

        // If args.overwrite is set and save already exists, remove the existing save
        if let Some((game_metadata, index)) =
            get_existing_solve_metadata(&metadata, board, &config_file_name)
        {
            println!("Overwriting save at {}", game_metadata.path);
            remove_file(dir.join(&game_metadata.path))
                .map_err(|e| format!("Error removing existing solve file: {e:?}"))?;
            metadata
                .solves
                .get_mut(board)
                .unwrap_or_else(|| panic!("Metadata unexpectedly did not contain board {board}."))
                .remove(index);
        }

        // Save the game
        match save_data_to_file(&game, "batch solve", &path, None) {
            Ok(_) => println!("Saved to {}", path.display()),
            Err(_) => panic!("Unable to save to {:?}", &path),
        }
    }

    // Update the SDB metadata
    for board in boards {
        let board_metadata = SolveMetadata {
            path: solve_file_name(&board),
            config: config_file_name.clone(),
            save_state: TARGET_STORAGE_MODE,
        };
        metadata
            .solves
            .entry(board)
            .or_insert(Vec::new())
            .push(board_metadata);
    }
    std::fs::write(
        metadata_path,
        serde_json::to_string_pretty(&metadata).expect("Could not serialize metadata"),
    )
    .expect("Couldn't write metadata file");

    Ok(())
}
