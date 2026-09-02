# Anchor v2 / Quasar zero-copy byte layout

Working notes from the Quasar-to-Anchor-v2 rewrite.

Zero-copy means: the bytes in the account *are* the struct. No serialize step. Rust will only overlay a struct on those bytes if every field starts at an address that type is allowed to use.

A `u8` can start on any byte.
A `u16` must start on a multiple of 2.
A `u32` must start on a multiple of 4.
A `u64` must start on a multiple of 8.

If the previous field ends in the wrong place, Rust inserts empty padding bytes so the next field lands on a legal start. Those empty bytes are holes. You did not declare them. Zero-copy accounts are not allowed to have them.

You do not have to write `#[repr(C)]` on the struct. The `#[account]` macro adds it for you when it expands.

`#[repr(C)]` keeps fields in the order you wrote them, and it is what inserts that padding.

## The original confusion

**What Quasar did**

You wrote `u32` on `ImtTree`. Quasar generated a second type, `ImtTreeZc`. On that second type the field was `PodU32`, not `u32`. You never wrote `PodU32` yourself. The loaded account used `ImtTreeZc`, so the onchain field was already `PodU32`.

**What `PodU32` is**

It is four bytes (`[u8; 4]`). You call `.get()` to read them as a number. Four bytes can start on any byte, so Rust does not insert padding in front of them.

A real `u32` cannot start on any byte. It must start on 0, 4, 8, 12, ... If a `u8` sits in front of it, Rust adds 3 empty bytes:

```
bump: u8            // byte 0
                    // bytes 1,2,3  <- padding you did not write
last_root_idx: u32  // bytes 4..7
```

With `PodU32` there is no padding:

```
bump: u8              // byte 0
last_root_idx: PodU32 // bytes 1..4
```

**What Anchor v2 does**

There is no second `*Zc` type. `ImtTree` is the type in the account. If a field needs to be `PodU32`, you write `PodU32` on `ImtTree` yourself.

Anchor v2 still has `PodU16`, `PodU32`, `PodU64`, `PodU128`, `PodBool`, and `PodVec`. They work like Quasar's: `.get()` reads the number, `From` builds one. `u8` stays `u8`. Do not use `bool` in an account (use `PodBool`). Do not use a plain enum in an account (use `#[pod_wrapper]`).

**When you can still write a normal `u64`**

The stub `Counter` is:

```
count: u64          // bytes 0..7
authority: Address  // bytes 8..39
```

`count` starts at byte 0, which is a multiple of 8, so a real `u64` is legal. `Address` is 32 bytes and can follow immediately. No padding. So this struct does not need `PodU64`.

If the first field were a `u8` bump and the next field were a `u32` or `u64`, a real integer would force padding. Use `PodU32` / `PodU64` there.

## `PodU32` is not a Vec

`[u8; 4]` is a fixed size. The length is part of the type. It is not written into the bytes.

`Vec<u8>` is a variable size. Borsh writes a length in front, then the bytes:

```
[len: 4][b0][b1][b2][b3]   // 8 bytes
```

`[u8; 4]` is only:

```
[b0][b1][b2][b3]           // 4 bytes
```

A real `u32` for the number `1` is also four bytes: `01 00 00 00`. `PodU32` is those same four bytes. `.get()` is `u32::from_le_bytes`. There is no length field.

For `Account<T>` nothing is serialized. Byte 1 through 4 of the account *are* the `PodU32`.

`bump: u8` plus `last_root_idx: PodU32` is 5 bytes.

## `repr(C)` vs `repr(Rust)`

Default Rust structs use `repr(Rust)`. The compiler may reorder fields to use less padding. Example:

```rust
struct S { a: u8, b: u64, c: u8 }
```

In written order, C layout needs a lot of padding (24 bytes). If the compiler puts `b` first, the same fields fit in 16 bytes. That is useful on the stack. It is useless for an account, because byte 0 would not be a stable meaning.

**`repr(Rust)`** (the default) - tells the compiler: you may reorder fields to waste less space. a then b might become b then a in the actual bytes. The compiler still bakes offsets into the code, but those offsets can change if you upgrade rustc. That is fine on the stack, because you only access by name and the same compiler compiled both the write and the read.

**`repr(C)`** - tells the compiler: pick offsets the C way. Field order as written. Insert padding so each field starts on its alignment. Same offsets on every compile.

### How it works with compilation and runtime

The `repr` note is used while compiling, then thrown away. What is left is machine code that already knows the offsets.
Offsets are baked into the binary. No type tag, no field names, no repr flag sits next to the value.

So:
- Compile time: repr chooses the layout (offsets, padding).
- Runtime: only those offsets remain, as numbers inside load/store instruction