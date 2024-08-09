# Design

This document is a description, as far as I understand it, of the inner design
of the solver and PostFlopGame. This is a working document for me to get my
bearings.

## PostFlopGame

### Build/Allocate/Initialize

To set up a `PostFlopGame` we need to **create a  `PostFlopGame` instance**, 
**allocate global storage and `PostFlopNode`s**, and **initialize the
`PostFlopNode` child/parent relationship**. This is done in several steps.


We begin by creating a `PostFlopGame`. A `PostFlopGame` requires an `ActionTree`
and a `CardConfig`. The `ActionTree` represents the full game tree modded out by
different runouts (so an `ActionTree` might have an abstract _line_ **Bet 10;
Call; Bet 30; Call** while the game tree would have concrete _nodes_ like
**Bet 10; Call; Th; Bet 30; Call**, etc).

1. **Create Configurations**:
   + We need a `tree_config: TreeConfig`
   + We need an `action_tree: ActionTree::new(tree_config)`
   + We need a `card_config: CardConfig`

2. **Create PostFlopGame**: We build a `PostFlopGame` from `action_tree` and `card_config`:

   ```rust
   let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
   ```

Once the game is created we need to allocate the following memory and initialize
its values:

+ `game.node_arena`
+ `game.storage1`
+ `game.storage2`
+ `game.storage_ip`
+ `game.storage_chance`

These fields are not allocated/initialized at the same time;
+ `game.node_arena` is allocated and initialized via `with_config`,
+ other storage is allocated via `game.allocate_memory()`.

#### Allocating and Initializing `node_arena`

We construct a `PostFlopGame` by calling
`PostFlopGame::with_config(card_config, action_tree)`, which calls
`update_config`. `PostFlopGame::update_config` sets up configuration data,
sanity checks things are correct, and then calls `self.init_root()`.

`init_root` is responsible for:

1. Counting number of `PostFlopNode`s to be allocated (`self.num_nodes`), broken
   up by flop, turn, and river
2. Allocating `self.num_nodes` `PostFlopNode`s in `node_arena` field
3. Clearing storage: `self.clear_storage()` sets each storage item to a new
   `Vec`
4. Invoking `build_tree_recursive` which initializes each node's child/parent
   relationship via `child_offset` (through calls to `push_actions` and
   `push_chances`).

Each `PostFlopNode` points to node-specific data (eg., strategies and cfregrets)
that is located inside of `PostFlopGame.storage*` fields (which is currently
unallocated) via similarly named fields `PostFlopNode.storage*`.

Additionally, each node points to the children offset with `children_offset`,
which records where in `node_arena` relative to the current node that node's
children begin. We allocate this memory via:

```rust
game.allocate_memory(false);  // pass `true` to use compressed memory
```

This allocates the following memory:

+ `self.storage1`
+ `self.storage2`
+ `self.storage3`
+ `self.storage_chance`

Next, `allocate_memory()` calls `allocate_memory_nodes(&mut self)`, which
iterates through each node in `node_arena` and sets storage pointers.

After `allocate_memory` returns we still need to set `child_offset`s.

### Storage

There are several fields marked as `// global storage` in `game::mod::PostFlopGame`:

```rust
    // global storage
    // `storage*` are used as a global storage and are referenced by `PostFlopNode::storage*`.
    // Methods like `PostFlopNode::strategy` define how the storage is used.
    node_arena: Vec<MutexLike<PostFlopNode>>,
    storage1: Vec<u8>,
    storage2: Vec<u8>,
    storage_ip: Vec<u8>,
    storage_chance: Vec<u8>,
    locking_strategy: BTreeMap<usize, Vec<f32>>,
```

These are referenced from `PostFlopNode`:

```rust
    storage1: *mut u8, // strategy
    storage2: *mut u8, // regrets or cfvalues
    storage3: *mut u8, // IP cfvalues
```

- `storage1` seems to store the strategy
- `storage2` seems to store regrets/cfvalues, and
- `storage3` stores IP's cf values (does that make `storage2` store OOP's cfvalues?)

Storage is a byte vector `Vec<u8>`, and these store floating point values.

> [!IMPORTANT]
> Why are these stored as `Vec<u8>`s? Is this for swapping between
> `f16` and `f32`s?

Some storage is allocated in `game::base::allocate_memory`:

```rust
    let storage_bytes = (num_bytes * self.num_storage) as usize;
    let storage_ip_bytes = (num_bytes * self.num_storage_ip) as usize;
    let storage_chance_bytes = (num_bytes * self.num_storage_chance) as usize;

    self.storage1 = vec![0; storage_bytes];
    self.storage2 = vec![0; storage_bytes];
    self.storage_ip = vec![0; storage_ip_bytes];
    self.storage_chance = vec![0; storage_chance_bytes];
```

`node_arena` is allocated in `game::base::init_root()`:

```rust
    let num_nodes = self.count_nodes_per_street();
    let total_num_nodes = num_nodes[0] + num_nodes[1] + num_nodes[2];

    if total_num_nodes > u32::MAX as u64
        || mem::size_of::<PostFlopNode>() as u64 * total_num_nodes > isize::MAX as u64
    {
        return Err("Too many nodes".to_string());
    }

    self.num_nodes = num_nodes;
    self.node_arena = (0..total_num_nodes)
        .map(|_| MutexLike::new(PostFlopNode::default()))
        .collect::<Vec<_>>();
    self.clear_storage();
```

`locking_strategy` maps node indexes (`PostFlopGame::node_index`) to a locked
strategy.  `locking_strategy` is initialized to an empty `BTreeMap<usize,
Vec<f32>>` by deriving Default. It is inserted into via
`PostFlopGame::lock_current_strategy`

### Serialization/Deserialization

Serialization relies on the `bincode` library's `Encode` and `Decode`. We can set
the `target_storage_mode` to allow for a non-full save. For instance,

```rust
game.set_target_storage_mode(BoardState::Turn);
```

will ensure that when `game` is encoded, it will only save Flop and Turn data.
When a serialized tree is deserialized, if it is a parital save (e.g., a Turn
save) you will not be able to navigate to unsaved streets.

Several things break when we deserialize a partial save:
- `node_arena` is only partially populated
- `node.children()` points to raw data when `node` points to an street that is
  not serialized (e.g., a chance node before the river for a Turn save).

### Allocating `node_arena`

We want to first allocate nodes for `node_arena`, and then run some form of
`build_tree_recursive`. This assumes that `node_arena` is already allocated, and
recursively visits children of nodes and modifies them to 


### Data Coupling/Relations/Invariants

- A node is locked iff it is contained in the game's locking_strategy
- `PostFlopGame.node_arena` is pointed to by `PostFlopNode.children_offset`. For
  instance, this is the basic definition of the `PostFlopNode.children()`
  function:

  ```rust
    slice::from_raw_parts(
        self_ptr.add(self.children_offset as usize),
        self.num_children as usize,
    )
  ```

  We get a pointer to `self` and add children offset.