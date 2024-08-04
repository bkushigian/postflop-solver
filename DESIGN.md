# Design

This document is a description, as far as I understand it, of the inner design
of the solver and PostFlopGame. This is a working document for me to get my
bearings.

## PostFlopGame

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

`node_arena` is initialized in `game::base::init_root()`:

```rust
    let num_nodes = self.count_num_nodes();
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

### Serialization

Serialization relies on the `bincode` library's `Encode` and `Decode`.