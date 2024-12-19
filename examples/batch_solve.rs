use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    process::exit,
};

use clap::Parser;
use postflop_solver::{
    cards_from_str, deserialize_configs_from_file, save_data_to_file, serialize_configs_to_json,
    solve, Action, ActionTree, BoardState, CardConfig, PostFlopGame, Range, TreeConfig,
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

/*
#[derive(Debug, Serialize, Deserialize)]
enum SolveConfig {
    File(PathBuf),
    Object(SolveConfigData),
} */

#[derive(Debug, Serialize, Deserialize)]
struct SolveConfig {
    // name: Option<String>,
    card_config: CardConfig,
    // NOTE: For now, just manually ignore the flop in the config
    tree_config: TreeConfig,
    added_lines: Vec<Vec<Action>>,
    removed_lines: Vec<Vec<Action>>,
}

/*
impl SolveConfig {
    fn to_config_data(self) -> Result<SolveConfigData, Box<dyn Error>> {
        match self {
            SolveConfig::Object(d) => Ok(d),
            SolveConfig::File(path_buf) => {
                let file = File::open(path_buf)?;
                let reader = BufReader::new(file);
                Ok(serde_json::from_reader(reader)?)
            }
        }
    }
} */

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    config: Option<String>,

    #[clap(flatten)]
    boards: Option<Boards>,

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

fn main() -> Result<(), String> {
    let args = Args::parse();
    let dir = PathBuf::from(args.dir);

    /* ASSUMTIONS */
    /*
     * The output dir exists, and is empty if it does not contain the SDB.
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
    let config_read_path = args.config.unwrap(); // TODO make config path non-optional
    let config: SolveConfig = {
        let file = File::open(&config_read_path)
            .map_err(|e| format!("Error when opening config file: {e:?}"))?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)
            .map_err(|e| format!("Error when deserializing config file: {e:?}"))?
    };

    // TODO could take in config from cmdline somehow here...

    // Copy config to SDB (if not already contained in SDB)
    if Path::new(&config_read_path)
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        != dir.canonicalize().unwrap()
    {
        std::fs::write(
            dir.join(&solve_name).join(CONFIG_FILE_EXTENSION),
            serde_json::to_string_pretty(&config).expect("Could not serialize config"),
        )
        .expect("Could not write config file")
    }

    // Load boards
    // TODO
    let boards: Vec<String> = vec![];

    // Check that the requested solves don't already exist
    // TODO currently this check could be slow with a large number of existing & requested solves
    // NOTE/TODO: For now, only checking that to see if the config files are the same
    if args.halt_on_existing {}

    // Do the solving
    // TODO

    // Update the SDB metadata
    for board in boards {
        let board_metadata = SolveMetadata {
            // TODO make this a function
            path: format!("{solve_name:?}{board:?}{SOLVE_FILE_EXTENSION:?}"),
            config: format!("{solve_name:?}{CONFIG_FILE_EXTENSION:?}"),
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

    /*
     *
     *
     *
     *
     *
     *
     *
     *
     */

    // Set up output paths for both configs and boards. These will be stored in
    // the solved database directory. We want to check to see if there will be a
    // conflict:
    let config_output_path = dir.join("config.json");
    let boards_output_path = dir.join("boards.txt");

    let config_path = if let Some(config) = args.config {
        if config_output_path.exists() && !args.overwrite {
            println!(
                "Error: `--config {}` was specified but `{}` already exists!",
                &config,
                config_output_path.display()
            );
            exit(1);
        }
        PathBuf::from(config)
    } else if config_output_path.exists() {
        config_output_path.clone()
    } else {
        println!(
            "No config specified, and `{}` doesn't exist!",
            config_output_path.display()
        );
        exit(1);
    };

    // Boards was specified from command line (either --boards or --boards-file)
    let boards = if let Some(boards) = args.boards {
        if let Some(boards) = boards.boards {
            if boards_output_path.exists() && !args.overwrite {
                println!(
                    "Error: `--boards {}` was specified but `{}` already exists!",
                    boards.join(" "),
                    boards_output_path.display()
                );
                exit(1);
            }
            boards
        } else if let Some(boards_path) = boards.boards_file {
            if boards_output_path.exists() {
                println!(
                    "Error: `--boards-file {}` was specified but `{}` already exists!",
                    &boards_path,
                    boards_output_path.display()
                );
                exit(1);
            }
            std::fs::read_to_string(boards_path)
                .expect("Unable to read boards_file")
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
        } else {
            panic!("Unreachable!")
        }
    } else
    // Otherwise, nothing specified on command line, so check if `boards.txt` exists
    {
        let boards_path = dir.join("boards.txt");
        if boards_path.exists() {
            std::fs::read_to_string(&boards_path)
                .expect("Unable to read boards_file")
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
        } else {
            println!(
                "No boards or boards-file was specified, and `{}` doesn't exist!",
                boards_path.display()
            );
            exit(1);
        }
    };

    // INVARIANT: At this point it is always safe to write "config.json" and
    // "boards.txt" to disk. This will either result in writing the file
    // contents to itself (basically a no-op) or overwriting old data.

    let (mut card_config, tree_config, added_lines, removed_lines) =
        deserialize_configs_from_file(&config_path).expect("Couldn't deserialize config");

    // Update card_config and tree_config with command-line specified data
    if let Some(range_string) = args.oop_range {
        let range_result = range_string.parse::<Range>();
        if let Ok(range) = range_result {
            card_config.range[0] = range;
        } else {
            println!("Couldn't parse OOP Range \"{}\"", range_string);
            println!("{}", range_result.unwrap_err());
            exit(1);
        }
    }

    if let Some(range_string) = args.ip_range {
        let range_result = range_string.parse::<Range>();
        if let Ok(range) = range_result {
            card_config.range[1] = range;
        } else {
            println!("Couldn't parse IP Range \"{}\"", range_string);
            println!("{}", range_result.unwrap_err());
            exit(1);
        }
    }

    let max_num_iterations = args.max_iterations;
    let target_exploitability = tree_config.starting_pot as f32 * args.exploitability;
    println!("Starting pot: {}", tree_config.starting_pot);
    println!("Effective stacks: {}", tree_config.effective_stack);
    println!(
        "Exploitable for {}% of staring pot ({} chips)",
        args.exploitability * 100.0,
        target_exploitability
    );

    // Save config to output directory

    let config_json =
        serialize_configs_to_json(&card_config, &tree_config, &added_lines, &removed_lines)?;

    let config_contents = serde_json::to_string_pretty(&config_json).map_err(|e| e.to_string())?;
    std::fs::write(&config_output_path, config_contents).map_err(|e| e.to_string())?;

    let existing_board_files = boards
        .iter()
        .map(|b| dir.join(format!("{}.pfs", b.replace(" ", ""))))
        .filter(|b| b.exists())
        .collect::<Vec<PathBuf>>();

    let boards_file_contents = boards.join("\n");
    std::fs::write(&boards_output_path, &boards_file_contents).map_err(|e| e.to_string())?;

    // Check if boards exist
    if args.halt_on_existing && !existing_board_files.is_empty() {
        println!("Halting. Board files already exist: ");
        existing_board_files
            .iter()
            .for_each(|b| println!("- {}", b.display()));
        exit(1);
    }

    let num_boards = boards.len();
    println!("\nBeginning Solves\n----------------\n");
    for (i, board) in boards.iter().enumerate() {
        println!("\nSolving board {}/{}: {}", i + 1, num_boards, board);
        let path = dir.join(format!("{}.pfs", board.replace(" ", "")));
        if !args.overwrite && path.exists() {
            println!("Sim {} already exists...continuing...", path.display());
            continue;
        }
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

        game.allocate_memory(false);
        solve(&mut game, max_num_iterations, target_exploitability, true);
        game.set_target_storage_mode(TARGET_STORAGE_MODE).unwrap();
        if path.exists() {
            println!("Overwriting save at {}", path.display());
        }
        match save_data_to_file(&game, "batch solve", &path, None) {
            Ok(_) => println!("Saved to {}", path.display()),
            Err(_) => panic!("Unable to save to {:?}", &path),
        }
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
        std::fs::create_dir_all(dir).map_err(|_| "Couldn't create dir".to_string())
    }
}
