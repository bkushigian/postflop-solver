# List of breaking changes

## v0.1.1
+ **Resolving and Reloading** v0.1.1 introduces capabilities to reload and
  resolve. Partial saves and reloading were already possible, but there were not
  good mechanisms in place to rebuild and resolve forgotten streets. Updates
  include:

  - **Updated Invariants for `PostFlopGame`**:

    Previously `PostFlopGame::state` stored if a game had been allocated or solved.
    This update expands `State` to account for partial solve information loaded
    from disk. In particular, `State::Solved` has been expanded to

    + `State::SolvedFlop`
    + `State::SolvedTurn`
    + `State::Solved`
    
    While `PostFlopGame::state` tracks the current solve status,
    `PostFlopGame::storage_mode` tracks how much memory is allocated. After a
    reload, `storage_mode` might be less than `BoardState::River`, meaning that
    some memory has not been allocated.

    For instance, if we run a flop solve (so a full tree startingat the flop)
    and save it as a flop save (save flop data, discarding turn and river data),
    only the flop data will be written to disk.  After reloading from disk, say
    into variable `game`, the following will be true:

    1. `game.storage_mode == BoardState::Flop`: This represents that only flop
       memory is allocated (though not necessarily solved).

    
    2. `game.state == State::SolvedFlop`: This represents that flop data is
      solved (but not turn/river).
      
    Allocating memory to store turn and river will update `storage_mode` to be
    `BoardState::River`. Thus `storage_mode == BoardState::Flop` together with
    `state == State::SolvedFlop` can be interpreted as "we've allocated the full
    game tree but only flop nodes have real data.

  - **Removed requirements for game to not be solved**: There were a lot of
    places that panicked if the game was already solved (e.g., trying to solve
    again, or node locking, etc). This felt like an unrealistic burden: we might
    want to nodelock a game after solving it, for instance, to compute some
    other results.

  - **Added `reload_and_resolve_copy()`**: This function does the following:
    1. Takes an input `g: PostFlopGame` that may be paritally loaded.
    2. Creates a new game `ng` from `g`'s configuration
    3. Initializes nodes and allocates memory for `ng`
    4. Copies loaded data from `g` (i.e., if `g.state == State::SolvedTurn`,
       then copy all flop and turn data)
    5. Locks copied nodes
    6. Solves `ng`
    7. Unlocks (restoring previous locking to whatever was passed in from `g`)
  
  - **Added `reload_and_resolve()`**: Similar to `reload_and_resolve_copy`, this
    modifies the supplied game in place. This is currently implemented using
    `reload_and_resolve_copy()`, and required memory for both the input game and
    the rebuilt game. This process overwrites the input game, so that memory
    will be released.

+ **Replacing panics with Result<(), String>**: we should be able to
    handle many instances of errors gracefully, so we've begun replacing
    `panic!()`s with `Result<>`s

+ **Helper Functions**: We've added several helper functions, including

  - `PostFlopNode::action_index(action: Action) -> Option<usize>`: return the index into
    this node of the specified action if it exists, and `None` otherwise.

  - `PostFlopNode::compute_history_recursive(&self, &PostFlopGame) -> Option<Vec<usize>>`:
    Recursively compute the history of the given node as a path of action indices. 

  - `PostFlopNode::actions() -> Vec<Action>`: compute the available actions of a given node


## 2023-10-01

- `BetSizeCandidates` and `DonkSizeCandidates` are renamed to `BetSizeOptions` and `DonkSizeOptions`, respectively.

## 2023-02-23

- `available_actions()` method of `PostFlopGame` now returns `Vec<Action>` instead of `&[Action]`.

## 2022-12-13

- revert: `compute_exploitability` function is back, and `compute_mes_ev_average` function is removed.

## 2022-12-11

- `TreeConfig`: new fields `rake_rate` and `rake_cap` are added.
- real numbers in `BetSize` enum  and `TreeConfig` struct are now represented as `f64` instead of `f32`.
- `compute_exploitability` function is renamed to `compute_mes_ev_average`.

## 2022-12-07

- `PostFlopGame`:
  - `play`: now terminal actions can be played.
  - `is_terminal_action` method is removed and `is_terminal_node` method is added.
  - `expected_values` and `expected_values_detail` methods now take a `player` argument.

## 2022-12-02

- `ActionTree`: `new` constructor now takes a `TreeConfig` argument.
- `ActionTree`: `with_config` and `update_config` methods are removed.

## 2022-11-30

- `TreeConfig`: `merging_threshold` field is added.
- `PostFlopGame`: `private_hand_cards` method is renamed to `private_cards`.

## 2022-11-29

- struct `GameConfig` is split into `CardConfig` and `TreeConfig`.
- new struct `ActionTree` is added: takes `TreeConfig` for instantiation.
- now `PostFlopGame` takes `CardConfig` and `ActionTree` for instantiation.
- `add_all_in_threshold` and `force_all_in_threshold` are renamed to `add_allin_threshold` and `force_allin_threshold`, respectively (`all_in` -> `allin`).
- `adjust_bet_size_before_all_in` (renamed from `adjust_last_two_bet_sizes`) is removed.

## 2022-11-27

- enum `BetSize` has new variants: `Additive(i32)`, `Geometric(i32, f32)`, and `AllIn`.
- `BetSize::LastBetRelative` is renamed to `BetSize::PrevBetRelative`.
- `BetSizeCandidates::try_from()` method is refactored. See the documentation for details. Now a pot-relative size must be specified with the '%' character, and the `try_from()` method rejects a single floating number.
- `adjust_last_two_bet_sizes` field of `GameConfig` struct is renamed to `adjust_bet_size_before_all_in`.

## 2022-11-14

- struct `GameConfig` has new fields: `turn_donk_sizes` and `river_donk_sizes`. Their types are `Option<DonkSizeCandidates>`. Specify these as `None` to maintain the previous behavior.
