use std::{
    path::{Path, PathBuf},
    process::exit,
};

use clap::Parser;
use postflop_solver::{
    cards_from_str, configs_to_json, deserialize_configs_from_file, save_data_to_file, solve,
    ActionTree, BoardState, PostFlopGame, Range,
};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    config: Option<String>,

    #[clap(flatten)]
    boards: Option<Boards>,

    /// Directory to output solves to
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

fn main() -> Result<(), String> {
    let args = Args::parse();

    // Create output directory if needed. Check if ".pfs" files exist, and if so abort
    let dir = PathBuf::from(args.dir);
    setup_output_directory(&dir)?;

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
                dir.display()
            );
            exit(1);
        }
    };

    // INVARIANT: At this point it is always safe to write "config.json" and
    // "boards.txt" to disk. This will either result in writing the file
    // contents to itself (basically a no-op) or overwriting old data.

    let (mut card_config, tree_config) =
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

    let config_json = configs_to_json(&card_config, &tree_config)?;
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
        game.set_target_storage_mode(BoardState::Turn).unwrap();
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
