# Why `ImtTreeZc` Exists

The source defines `ImtTree`, but a loaded `RootRegistry` account exposes its nested tree as `ImtTreeZc`. This is a consequence of Quasar's zero-copy account model.

The short version is:

- `ImtTree` is the native representation used when constructing a tree.
- `ImtTreeZc` is the storage representation used inside account data.
- `RootRegistryInner` is a temporary, convenient input to `set_inner`.
- `RootRegistryData` is the alignment-safe layout of the bytes stored in the account.
- `RootRegistry` is a validated handle that provides access to those bytes.

`ImtTreeZc` is not a separate account. It is a different Rust representation of the same logical IMT fields.

## The type written in source

The program declares the tree using convenient native Rust types:

```rust
#[derive(QuasarSerialize, Clone, Copy)]
pub struct ImtTree {
    pub root: [u8; 32],
    pub frontiers: [u8; 32 * MERKLE_TREE_DEPTH],
    pub zero_values: [u8; 32 * MERKLE_TREE_DEPTH],
    pub next_leaf_idx: u32,
}
```

This is the author-facing representation. In particular, `next_leaf_idx` is an ordinary `u32`, which is convenient to construct and use in ordinary Rust code.

The `QuasarSerialize` derive generates a zero-copy companion. Conceptually, the result looks like this:

```rust
#[repr(C)]
pub struct ImtTreeZc {
    pub root: [u8; 32],
    pub frontiers: [u8; 32 * MERKLE_TREE_DEPTH],
    pub zero_values: [u8; 32 * MERKLE_TREE_DEPTH],
    pub next_leaf_idx: PodU32,
}
```

This is a simplified view of the generated code, but it shows the important transformation:

```text
u32  -> PodU32
```

The byte arrays already have alignment 1, so their storage types do not need to change.

## Ownership is determined by the value

Calling `ImtTree` an "owned type" is imprecise. Ownership is a property of a particular value or reference, not an unchanging property of the type.

For example, this local variable owns an `ImtTree` value:

```rust
let imt: ImtTree = ImtTree::new()?;
```

All fields are contained in that local value. It is not a reference into account data.

When it is placed in `RootRegistryInner`, ownership moves into the new value:

```rust
let inner = RootRegistryInner {
    imt,
    roots_history,
    last_root_idx: 0,
    bump,
};
```

`set_inner` then converts the native fields into their zero-copy storage representations and writes them into the account.

An `ImtTreeZc` can also be owned. A unit test that returns one by value owns that value:

```rust
fn empty_tree() -> ImtTreeZc {
    ImtTree::new().unwrap().into()
}
```

The important distinction appears when processing a loaded account. In that situation, the program usually has a reference into the Solana account-data buffer:

```text
&mut RootRegistryData
    contains &mut ImtTreeZc
```

The runtime account holds the backing bytes. The instruction handler temporarily borrows those bytes through Quasar's account wrapper.

A more precise description is therefore:

```text
ImtTree
    Native representation used to construct a new tree before writing it
    into an account.

ImtTreeZc
    Storage representation used inside account data. A loaded account usually
    exposes it through &ImtTreeZc or &mut ImtTreeZc.
```

## Alignment

Every Rust type has both a size and an alignment requirement.

A native `u32` has:

```text
size      = 4 bytes
alignment = 4 bytes
```

Alignment 4 means a `u32` must normally begin at an address divisible by four:

```text
Allowed:     0x1000, 0x1004, 0x1008
Not allowed: 0x1001, 0x1002, 0x1003
```

A `PodU32` has:

```text
size      = 4 bytes
alignment = 1 byte
```

It still occupies four bytes, but those bytes may begin at any address:

```text
PodU32 beginning at 0x1001:

0x1001  byte 0
0x1002  byte 1
0x1003  byte 2
0x1004  byte 3
```

Alignment 1 is a placement rule. It does not mean that the value is one byte long, and it does not add an extra byte.

`PodU32` stores the integer as four alignment-safe bytes. Its methods convert between those bytes and a native integer:

```rust
let index: u32 = self.next_leaf_idx.get();
self.next_leaf_idx = PodU32::from(index);
```

## The account discriminator is not alignment padding

A Quasar account begins with a discriminator that identifies its account type. `RootRegistry` uses a one-byte discriminator with value `2`.

For a simplified account whose first payload field is a `PodU32`, the bytes can be placed as follows:

```text
0x1000  account discriminator
0x1001  PodU32 byte 0
0x1002  PodU32 byte 1
0x1003  PodU32 byte 2
0x1004  PodU32 byte 3
```

The discriminator is meaningful account metadata. It is not present to satisfy alignment and is not disposable padding.

If the payload used a native `u32` at that position, three padding bytes would be required before the integer could begin at the next four-byte boundary:

```text
0x1000  account discriminator
0x1001  padding
0x1002  padding
0x1003  padding
0x1004  native u32 begins
```

Quasar avoids that padding by representing native integers with alignment-1 `Pod*` types. Because every field in the generated zero-copy schema has alignment 1, the schema can safely begin immediately after the discriminator.

This is why the two tree representations have different alignment requirements:

```text
ImtTree
    alignment 4 because it contains a native u32

ImtTreeZc
    alignment 1 because it contains PodU32 and byte arrays
```

## What the account macro generates

The account is declared using the native field types:

```rust
#[account(discriminator = 2, set_inner)]
pub struct RootRegistry {
    pub imt: ImtTree,
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: u32,
    pub bump: u8,
}
```

Quasar uses this declaration as a schema and generates several types with different roles.

Conceptually, the generated types look like this:

```rust
// Validated handle to the Solana account.
pub struct RootRegistry {
    account_view: AccountView,
}

// Native input accepted by set_inner.
pub struct RootRegistryInner {
    pub imt: ImtTree,
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: u32,
    pub bump: u8,
}

// Actual zero-copy account-data layout.
#[repr(C)]
pub struct RootRegistryData {
    pub imt: ImtTreeZc,
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: PodU32,
    pub bump: u8,
}
```

`RootRegistryData` is the public alias for the generated account-data layout. Quasar also uses an internal `RootRegistryZc` name while building that layout.

The generated `RootRegistry` wrapper implements `Deref` and `DerefMut` to the zero-copy data. That is why methods written on `RootRegistry` can access account fields directly:

```rust
self.last_root_idx
self.roots_history
self.imt
```

Those expressions refer to fields in `RootRegistryData`. Consequently:

```text
self.last_root_idx has type PodU32
self.imt           has type ImtTreeZc
```

## Why `RootRegistryInner` and zero-copy data both exist

The zero-copy data representation is required for safe direct access to account bytes. `RootRegistryInner` is an optional convenience generated because the account requested `set_inner`.

Without `RootRegistryInner`, initialization would need to work with storage types directly:

```rust
root_registry.imt = ImtTree::new()?.into();
root_registry.last_root_idx = PodU32::from(0);
```

That is technically possible, but it exposes storage conversions throughout initialization code.

With `RootRegistryInner`, initialization can use native values:

```rust
root_registry.set_inner(RootRegistryInner {
    imt: ImtTree::new()?,
    roots_history,
    last_root_idx: 0,
    bump,
});
```

Conceptually, `set_inner` performs these conversions:

```text
ImtTree -> ImtTreeZc
u32     -> PodU32
u8      -> u8
```

`RootRegistryInner` is temporary and is not the account's persisted representation. The zero-copy layout is the representation of the persisted bytes.

One representation could technically handle both jobs if all initialization code used `Pod*` types directly. Quasar generates both so that:

- account access remains alignment-safe and zero-copy;
- initialization remains readable and uses ordinary native values.

## Why `insert` is implemented on `ImtTreeZc`

After initialization, the tree being modified is already inside account data. Access through `RootRegistry` reaches the generated zero-copy field:

```rust
pub fn insert(&mut self, leaf: [u8; 32]) -> Result<[u8; 32], ProgramError> {
    let root = self.imt.insert(leaf)?;
    // Record root in the history.
    Ok(root)
}
```

At that call site, `self.imt` has type `ImtTreeZc`. Rust therefore searches for an `insert` method implemented for `ImtTreeZc`.

Implementing `insert` on `ImtTreeZc` is not required for serialization. `QuasarSerialize` already handles the conversion and storage layout. The method is placed there so the algorithm can mutate the live account-backed representation directly.

If `insert` existed only on `ImtTree`, account mutation would require copying the full tree into a native value and then copying it back:

```rust
let mut imt: ImtTree = self.imt.into();
let root = imt.insert(leaf)?;
self.imt = imt.into();
```

The IMT contains more than a kilobyte of fixed arrays, so those conversions are meaningful copies. They also make it easier to forget to write the modified value back to the account.

The current separation avoids that problem:

```text
ImtTree::new()
    Constructs native initialization state.

ImtTreeZc::insert()
    Mutates the tree already stored in account data.

RootRegistry::insert()
    Coordinates the IMT mutation with the root-history ring buffer.
```

`RootRegistry::insert` still has an important role. An IMT insertion updates the tree itself, while the registry operation also advances `last_root_idx` and stores the resulting root in `roots_history`.

## Mental model

The complete initialization path is:

```text
ImtTree::new()
    -> ImtTree native value
    -> RootRegistryInner
    -> RootRegistry::set_inner
    -> conversion to ImtTreeZc and PodU32
    -> bytes stored in the RootRegistry account
```

The mutation path is:

```text
Solana account-data bytes
    -> RootRegistry account wrapper
    -> mutable RootRegistryData view
    -> mutable ImtTreeZc field
    -> ImtTreeZc::insert
    -> account bytes updated directly
```

The key distinction is not that one Rust type can be owned and the other cannot. The distinction is that `ImtTree` uses convenient native Rust fields, while `ImtTreeZc` uses an alignment-safe layout suitable for borrowing directly from account bytes.
