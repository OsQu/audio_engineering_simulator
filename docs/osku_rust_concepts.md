# Rust concepts reference

A running reference of the Rust syntax and idioms we've covered while building this
project, organized by topic (not by when we hit them). Examples are drawn from the
codebase (mostly `crates/engine/src/`). Use it to skip re-explaining things you've
already seen.

> Status: covers through Story 1.3's compile/schedule — trait objects (`Box<dyn Node>`),
> `Box<T>`, `#[expect]`/`#[allow]`, enums + `match`, custom error types, disjoint-borrow
> destructuring, `Vec`-as-map / index handles, and unsafe / statics / atomics — on top of
> Story 1.2's electrical types, divider, cable filter, and test helpers. Plus the harness
> visualization demo (a detour after 1.3): implementing a *foreign* trait for a *local* type
> in a consumer crate, and `std::slice::from_mut`.

## Contents
1. [Modules & project layout](#1-modules--project-layout)
2. [Types & data](#2-types--data)
3. [Functions & methods](#3-functions--methods)
4. [Ownership, borrowing, lifetimes](#4-ownership-borrowing-lifetimes)
5. [Traits](#5-traits)
6. [Generics & the turbofish](#6-generics--the-turbofish)
7. [Numbers & conversions](#7-numbers--conversions)
8. [Collections, iterators, closures, ranges](#8-collections-iterators-closures-ranges)
9. [Errors & panics](#9-errors--panics)
10. [Testing](#10-testing)
11. [Tooling & ecosystem](#11-tooling--ecosystem)
12. [Unsafe, statics & atomics](#12-unsafe-statics--atomics)

---

## 1. Modules & project layout

**Crate, package, workspace.** A **crate** is Rust's unit of compilation — what `rustc`
compiles as a whole into one artifact. Two kinds: a **library** crate (root `src/lib.rs`, no
`main` — our `engine`, `wasm-bindings`) and a **binary** crate (root `src/main.rs` — our
`harness`). One crate = one **module tree** rooted at that file (`crate::` = its root). Above
crates sit Cargo concepts: a **package** is a bundle with one `Cargo.toml` (≤1 lib crate + any
binaries), and a **workspace** is a set of packages sharing one lockfile & `target/` (our
top-level `[workspace] members = […]`). So this repo is *one workspace → three packages →
three crates → many modules*. The crate is also the **privacy/distribution boundary**:
`pub(crate)` stops at the crate edge; only `pub` items on a public path are visible to other
crates (and `engine`'s internals stay hidden from `harness`/`wasm-bindings`). `cargo` drives
packages; `rustc` compiles crates.

**Modules are explicit.** A file isn't part of the build until a parent declares it:
```rust
mod signal;                                  // load signal.rs (or signal/mod.rs)
pub use signal::{AnalogRate, Volts};         // re-export: flattens the public API
```
- **Visibility is per *module* and graduated** (not per-type like C++ `private`). The default
  (no modifier) is "private" = visible in the **defining module and its descendants** — a child
  module sees its ancestors' private items, a **sibling** does not:

  | form | visible to |
  |---|---|
  | *(none)* | defining module **and its descendants** |
  | `pub(super)` | the parent module |
  | `pub(crate)` | anywhere in **this crate**, not other crates — "crate-internal API" |
  | `pub(in path)` | a named module subtree |
  | `pub` | as far as the **path is public** (incl. other crates) — *not* automatically world-visible |

- **Item and field visibility are separate.** A `pub` struct can have private (or `pub(crate)`)
  fields. `pub struct Graph { pub(crate) nodes: … }` exports the type but keeps fields
  crate-internal — outsiders use methods, while a *sibling* module (`compile` in `schedule.rs`)
  reads the fields directly. `pub struct NodeId(pub(crate) usize)` is opaque outside the crate
  but transparent within it. A plain (private) field would be reachable only from its own module.
- `pub use` **re-exports** so callers write `engine::Volts`, not `engine::signal::volts::Volts`.
  The modules (`mod graph;`) stay private; only re-exported names escape — which is why `pub`
  alone isn't enough to be world-visible (the path must be public too). Curate the surface here.
- Path keywords: `super::` (parent module), `crate::` (crate root), `self::` (current).

**Module file style.** A module *with children* needs a file holding its declarations/docs:
either `signal/mod.rs` **or** `signal.rs` beside a `signal/` folder. We use the latter
(`<name>.rs` + folder) — meaningful tab names, more modern. Leaf modules are just `<name>.rs`.

**Doc comments** become rendered docs and are compiled as tests:
- `///` documents the **item that follows**.
- `//!` documents the **enclosing module/file** (top of each file).
- `` [`Volts`] `` is an intra-doc link. `# Panics` is a convention heading for documenting panics.

## 2. Types & data

**Structs** — named-field or tuple:
```rust
pub struct VoltageBuffer { values: Vec<f32>, rate: AnalogRate }   // named
pub struct Volts(f32);                                            // tuple struct (.0)
```

**Newtype pattern** — wrap one value to make a *distinct* type the compiler enforces:
```rust
pub struct Volts(f32);   // can't be confused with a bare f32 or a dBFS sample
```
Also used to wrap an **external** type and **delegate**, controlling the public surface:
```rust
pub struct Rng(Pcg64Mcg);          // methods call self.0.…; impl can be swapped freely
```

**`#[derive(...)]`** auto-generates trait impls:
```rust
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
```
- `Debug` → `{:?}` formatting (and assert messages). `Clone`/`Copy` → see §4.
- `PartialEq`/`PartialOrd` → `==` / `< >` ("partial" because `NaN` breaks total ordering).
- `Default` → `T::default()`.

**`#[repr(transparent)]`** — guarantees a one-field struct has the exact memory layout of
its field (same size/ABI). Documents intent; keeps zero-cost reinterpretation possible.

**Constants:**
```rust
const V_REF_DBU: f64 = 0.774_596_669_241_483_4;   // module-level const
impl Volts { pub const ZERO: Volts = Volts(0.0); } // associated const (Volts::ZERO)
```
Underscores in numbers are just readability (`384_000.0`).

**`const fn`** — a function usable at compile time (so it can initialize other constants):
```rust
pub const fn new(volts: f32) -> Self { Volts(volts) }
```

**Enums carry data, and `match` destructures them.** Unlike C enums (named integers), a Rust
enum variant can hold fields — a tagged union / sum type:
```rust
enum Step {
    Node { node: usize, in_start: usize, /* … */ },        // struct-like variant
    Edge { src: usize, dst: usize, transform: EdgeTransform },
}
match step {
    Step::Node { node, in_start, .. } => { /* … */ }       // bind fields; `..` ignores the rest
    Step::Edge { src, dst, transform } => { /* … */ }
}
```
`match` is **exhaustive** — every variant must be handled (or an explicit `_`), so adding a
variant later turns every incomplete `match` into a compile error pointing at what to update.
`Result`/`Option` (§9) are just enums with this treatment.

## 3. Functions & methods

```rust
impl Volts {
    pub const fn new(v: f32) -> Self { Volts(v) }   // associated fn (constructor): Volts::new(..)
    pub fn abs(self) -> Self { Volts(self.0.abs()) } // method (has a receiver)
}
pub fn dbu_to_volts(dbu: f32) -> Volts { ... }       // free function: engine::dbu_to_volts(..)
```
- `Self` (capital) = the type; `self` (lowercase) = the instance.
- **Receiver forms:** `self` (by value — see §4), `&self` (read-only borrow), `&mut self` (mutable borrow).
- A function with **no receiver** is an *associated function* — the idiom for constructors (`new`, `from_*`, `zeros`).
- **No `return` needed:** the last expression without a `;` is the return value.
- **`&mut` parameters** mutate the caller's value in place: `fn f(x: &mut u64) { *x = ...; }`.

**Useful attributes:** `#[must_use]` (warn if the return value is ignored — good on
constructors/pure fns), `#[inline]` (hint to inline a hot-path call).

## 4. Ownership, borrowing, lifetimes

**Ownership:** every value has one owner. Passing it "by value" transfers ownership:
- **`Copy` types** (`Volts`, `AnalogRate`, `f32`, `usize`): the bits are **duplicated**; the
  original stays valid. Cheap value types derive `Copy`.
- **Non-`Copy` types** (`VoltageBuffer` — owns a heap `Vec`): "by value" is a **move**. The
  small header is shifted, the heap data is **not** copied, and the **original is
  invalidated** (using it after is a compile error). This prevents double-frees.

```rust
let b = a.abs();      // a is Copy → still usable
let buf2 = buf;       // VoltageBuffer move → `buf` no longer usable
```

**References / borrowing** — use a value without owning it:
- `&T` shared/immutable borrow — **many** allowed at once.
- `&mut T` exclusive/mutable borrow — **exactly one**, and no shared borrows may coexist.

The **borrow checker** enforces this **at compile time, zero runtime cost**. It guarantees
(1) no mutation while aliased (→ no data races) and (2) no reference outlives its target
(→ no dangling pointers, tracked via *lifetimes*).

**Slices** are borrowed views into contiguous data (pointer + length), no ownership/copy:
```rust
pub fn as_slice(&self) -> &[f32] { &self.values }       // read view (hot path)
pub fn as_mut_slice(&mut self) -> &mut [f32] { ... }    // write view
```
The `&` in the return ties the slice's lifetime to `&self`, so it can't dangle.

**`Box<T>`** — a unique, owned pointer to a **heap** allocation. The value lives on the heap;
the `Box` is a stack-sized handle that frees it on drop. Two main uses: (1) give a **fixed
size** to something unsized (a trait object — §5; the element type of `Vec<Box<dyn Node>>`),
and (2) hand off ownership cheaply by moving just the pointer (the schedule swap, later).
`Box::new(x)` moves `x` onto the heap.

**`std::mem::replace(&mut place, new) -> old`** moves `new` into a `&mut` location and returns
the previous value — the way to *take ownership out of a `&mut`* (you can't just move out; that
would leave the place uninitialized). `ScheduleSlot::install` uses it to swap in a new
`Box<Schedule>` and hand the old one back for off-path drop — an O(1) pointer move, no alloc/free.
Cousins: `mem::take` (replace with `Default::default()`), `mem::swap(&mut a, &mut b)`.

**Destructuring `self` for disjoint borrows.** The borrow checker tracks borrows **per field**,
but it can't see through a method call (`self.a(&self.b)` may look like one borrow of `self`).
Destructuring `&mut self` up front splits it into independent locals it reasons about separately:
```rust
let Self { nodes, input_pool, output_pool, .. } = self;   // independent &mut/& locals
let ins  = &input_pool[in_range];          // shared borrow of one Vec
let outs = &mut output_pool[out_range];    // exclusive borrow of a *different* Vec
nodes[node].process(ins, outs);            // + a third — all provably disjoint, no unsafe
```
This is what lets the schedule's hot loop hand a node `&input_pool[..]` and `&mut output_pool[..]`
at once: different `Vec`s ⇒ the borrows can't alias. (One shared buffer arena would instead need
`split_at_mut` or `unsafe`.)

## 5. Traits

Traits are shared behavior. You **implement** them for your types:
```rust
impl Add for Volts {
    type Output = Volts;                                  // associated type
    fn add(self, rhs: Volts) -> Volts { Volts(self.0 + rhs.0) }
}
```
- Operators are traits: `+` ⇒ `Add`, `+=` ⇒ `AddAssign`, etc. `a + b` desugars to `Add::add(a, b)`.
- **Associated type** `type Output` is the trait's declared result type.
- Traits are generic over the right-hand side (`Mul<f32>`), so you can impl `Volts * f32`
  *and* `f32 * Volts` separately. Allowed by the **orphan rule**: you may impl a trait if you
  own the trait *or* the type.
- **The "or the type" half, across crates.** The harness's `SineSource` is a *local* type that
  implements `engine::Node`, a trait it does **not** own — legal because the type is local. This
  is how a *consumer* crate plugs into an engine abstraction: define your type, `impl Node for`
  it, and the engine's `compile`/schedule treats it like any built-in node. To then *call* a
  trait method on it (`src.process(…)` in `main`), the trait must be in scope — `use engine::Node`.

**Traits must be in scope to call their methods.** A method lives on the trait; importing
it makes it callable:
```rust
use rand::Rng as _;                  // brings gen_range / sample into scope...
use rand::{RngCore, SeedableRng};    // ...next_u32 / next_u64 ; seed_from_u64
```
**`use ... as _`** imports a trait **anonymously** — you get its methods without binding its
name. We used it because our struct is also named `Rng`, which would clash with `rand::Rng`.

### Trait objects (`dyn Trait`) & dynamic dispatch

A graph stores nodes of *different concrete types* (`TestSource`, `GainStage`, …) in one
`Vec` — but a `Vec<T>` needs a single `T`. Traits give two kinds of polymorphism:

- **Static dispatch (generics):** `fn add<N: Node>(n: N)` — the compiler stamps out a copy
  per concrete `N` (monomorphization); calls resolved at compile time, zero overhead. But each
  `N` is a *distinct* type, so you **can't mix** different nodes in one `Vec`. (This is what
  closures / `measure_gain` use, §6/§8.)
- **Dynamic dispatch (trait objects):** `dyn Node` **erases** the concrete type. The value is
  reached through a **fat pointer** = (data pointer, **vtable** pointer); a method call looks up
  the impl in the vtable at runtime (one indirection). One type, `dyn Node`, stands for *any*
  node → heterogeneous storage works.

`dyn Node` is **unsized** (`?Sized`) — nodes differ in size — so it can't sit directly in a
`Vec` slot. **`Box<dyn Node>`** (§4) is a fixed-size handle whose data lives on the heap, so
`Vec<Box<dyn Node>>` has uniform-size elements:
```rust
nodes: Vec<Box<dyn Node>>,
pub fn add<N: Node + 'static>(&mut self, node: N) -> NodeId {
    self.nodes.push(Box::new(node));   // Box<N> → Box<dyn Node> (unsizing coercion)
    /* ... */
}
```
- The API stays ergonomic: callers pass a plain `GainStage` (generic `N`, monomorphized), and
  `Box::new` moves it to the heap and **coerces** `Box<N>` → `Box<dyn Node>` for uniform storage.
- **`+ 'static`**: a trait object defaults to requiring `'static` — the node borrows nothing
  with a shorter lifetime; it **owns all its data**. Our nodes hold owned fields (`Vec`,
  arrays, `f32`), so they qualify, and a boxed node can live as long as the schedule with no
  dangling. (A struct holding a `&'a T` would *not* be `'static`.)
- **Cost:** one pointer indirection + a non-inlinable call **per `process` call** — i.e. per
  *block*, not per sample → negligible, bought for the freedom of arbitrary graphs.
- **Object safety:** only traits whose methods are dispatchable on a `dyn` value can be trait
  objects (methods take `&self`/`&mut self`, no generic methods, don't return `Self`). `Node`
  qualifies.

**Rule of thumb:** generics when the type is known at each call and you want zero-cost
specialization; `dyn` when you must store/handle *mixed* types behind one interface.

## 6. Generics & the turbofish

Generic functions have **type parameters** (`<T>`) the compiler usually **infers**. When it
can't, you specify them with the **turbofish** `::<...>` (named because `::<>` looks like a fish):
```rust
self.0.sample::<f32, _>(StandardNormal)   // T = f32; `_` = infer the rest
"42".parse::<i32>()                        // T only appears in the return → needs help
(0..8).collect::<Vec<_>>()                 // which collection? Vec.
```
Two equivalent ways to pin a type — turbofish, or annotate the binding:
```rust
let n = "42".parse::<i32>().unwrap();
let n: i32 = "42".parse().unwrap();
```
Or nudge inference with a typed literal: `gen_range(0.0_f32..1.0)` returns `f32`. The `::` in
the turbofish disambiguates from the `<` `>` comparison operators. You'll mostly need it for
`parse`, `collect`, `.into()`, and generic constructors.

**`where` clauses** move a type parameter's bounds out of the angle brackets to after the
signature — purely for readability when bounds get long. These two are identical:
```rust
fn measure_gain<F: FnMut(&mut VoltageBuffer)>(/* ... */) -> f32 { ... }   // inline bound
fn measure_gain<F>(/* ... */) -> f32 where F: FnMut(&mut VoltageBuffer) { ... }  // where clause
```
(The `FnMut(...)` bound itself — taking a closure as a parameter — is in §8.)

## 7. Numbers & conversions

**Two conversion mechanisms — the choice signals intent:**
- **`From`/`Into`** — *lossless* only; exists when no data can be lost. `f64::from(x)` widens
  `f32 → f64`; `u64::from(x)` widens `u32 → u64`.
- **`as`** — explicit primitive **cast** that *may* lose data: `f64 → f32` (precision),
  `u64 → u32` (truncation), `usize → f32`. There's deliberately no `From` for these, so `as`
  is your opt-in "I accept the loss."

Rule of thumb: widen with `From`/`.into()`, narrow with `as`.

**Methods on primitives:** `u1.ln()`, `x.sqrt()`, `x.cos()`, `f.to_bits()` (raw bit pattern,
handy for exact float comparison).

**Integer overflow:** in **debug builds, overflow panics** by default. When you want modular
wraparound (hashing/PRNGs), opt in explicitly: `wrapping_mul`, `wrapping_add` (also
`checked_*` → `Option`, `saturating_*` → clamp).

**Floats:** never `==` on computed floats. In tests use `approx::assert_relative_eq!(a, b,
epsilon = …)`, or compare `.to_bits()` for exact equality. (Our scalar policy: `f32` storage,
`f64` only in accumulators.)

## 8. Collections, iterators, closures, ranges

```rust
let mut v: Vec<f32> = vec![0.0; len];   // owned growable heap array; vec! macro
&[f32]    &mut [f32]                     // borrowed slices (views), see §4
```

**Fixed arrays vs `Vec`, both viewed as slices:** `[T; N]` is a stack array of compile-time
length `N` (`outputs: [OutputZ; 1]`); `Vec<T>` is heap, runtime length (`inputs: Vec<InputZ>`
for a sum's N ports). Both **coerce to a slice**: `&self.outputs` turns `&[OutputZ; 1]` into
`&[OutputZ]`, and `&[]` is an empty slice (a source with no inputs). So a trait method returning
`&[InputZ]` works regardless of how each device stores its ports.

**Iterators** (lazy, composable):
```rust
buf.as_slice().iter().all(|&v| v.abs() < EPS)        // iter() → &f32
buf.as_mut_slice().iter_mut().enumerate()            // iter_mut() → &mut f32 ; (index, item)
(0..8).map(|_| rng.next_u32()).collect::<Vec<_>>()   // build a collection
items.iter().map(|x| ...).reduce(Ohms::parallel)     // fold w/o seed → Option; method as the op
```
**`reduce` vs function items:** `Iterator::reduce` folds with **no initial value** — it seeds
from the first element and returns `Option<T>` (`None` if empty). And a **function/method path**
(`Ohms::parallel`) can be passed anywhere a closure is expected, as long as its signature matches
— `parallel(self, other) -> Ohms` is the `(T, T) -> T` shape `reduce` wants, so no `|a, b| …`
wrapper is needed.

**Closures** are anonymous functions; they can capture surrounding variables and be bound to
a variable:
```rust
let run = |seed| { /* ... */ };   // closure stored in a variable, called as run(7)
|&v| v.abs()                       // `&v` pattern destructures the &f32 the iterator yields
*s = i as f32;                     // `*s` writes through a &mut from iter_mut()
```

**Closures as parameters (higher-order functions).** A function takes a closure via a generic
type bounded by one of the three closure traits:
```rust
pub fn measure_gain<F>(freq: f64, rate: AnalogRate, mut process: F) -> f32
where
    F: FnMut(&mut VoltageBuffer),         // accepts any closure of this shape
{ /* ... */ process(&mut output); /* ... */ }

measure_gain(10_000.0, r, |buf| for s in buf.as_mut_slice() { *s *= 0.5; }); // pass a closure
measure_gain(10_000.0, r, |_buf| {});                          // no-op; `_buf` = unused param
```
- **`Fn` / `FnMut` / `FnOnce`** differ by how the closure uses its captures: `Fn` only borrows
  (callable repeatedly), **`FnMut`** mutably borrows captured state (so the param is `mut
  process`), `FnOnce` consumes its captures (callable once). Pick the *loosest* the body needs;
  we use `FnMut` because a stateful filter mutates itself on each call.
- Each closure has its own unnameable type, so the parameter must be **generic** (`<F>`). This
  is **monomorphized** — zero-cost, statically dispatched, like any generic. (The dynamic
  alternative would be `&mut dyn FnMut(...)`.)
- `FnMut(&mut VoltageBuffer)` is the trait written in **function-call sugar**: takes
  `&mut VoltageBuffer`, returns `()`.

**`while let` & `bool::then_some`** (from the topo sort):
```rust
while let Some(node) = ready.pop() { ... }     // loop while pop() matches Some — drains the stack
(order.len() == n).then_some(order)            // Some(order) if the bool is true, else None
```
`while let PATTERN = expr { … }` runs the body as long as `expr` matches — the idiom for
draining a stack/queue (ends when `pop()` returns `None`). `bool::then_some(v)` turns a
condition into an `Option<T>`; `bool::then(|| v)` is the lazy version (closure, for an
expensive `v`).

**`Vec`-as-map, and index handles for graphs.** `std::collections::HashMap<K, V>` exists (and
`BTreeMap` for sorted keys) — but when keys are **dense integers** (`0..n`), a `Vec` indexed by
the id *is* the map: O(1), no hashing, denser. `out_offset[node.0]` is a "node → offset" map.
Reach for `HashMap` only when keys are sparse/arbitrary (strings, UUIDs). Deeper: Rust models
**graphs by storing nodes in one `Vec` (an "arena") and referencing them by integer index**
(`NodeId(usize)`), not by pointer/reference — indices are `Copy`, so this sidesteps the borrow
checker's pain with reference-linked structures while staying O(1). (Crates: `slab`, `slotmap`;
rustc's `IndexVec`.)

**A single value as a one-element slice.** `std::slice::from_mut(&mut x)` borrows `x` as a
`&mut [T]` of length 1 (and `slice::from_ref` for `&[T]`). Handy for calling a slice-based API
with one item without allocating a `Vec` or `[x]` array — e.g. `node.process(&[], slice::from_mut(&mut out))`
feeds the `Node`'s `&mut [VoltageBuffer]` port list a lone output buffer.

**Ranges are first-class values**, not just loop syntax:
```rust
for _ in 0..16 { ... }
(0.0..1.0).contains(&x)            // Range has methods; .contains takes a reference
gen_range(0.0_f32..1.0)            // a range passed as an argument
```

## 9. Errors & panics

- **`assert!(cond, "msg {var}")`** panics if false; the format string interpolates locals
  inline. Used for *construction-time* validation (programmer error).
- **Panic vs `Result`:** panic for bugs/violated preconditions; `Result<T, E>` for expected,
  recoverable failures. Tied to our hot-path rule — the `process` path must never panic.
- **`# Panics`** doc section documents when a function can panic.

**A custom error type** = an enum + `Display` + `Error`:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError { NoOutput, Cycle, InputAlreadyConnected { node: usize, port: usize }, /* … */ }

impl std::fmt::Display for CompileError { /* match self → human message */ }
impl std::error::Error for CompileError {}     // empty body: Error just requires Debug + Display
```
- `std::error::Error` requires `Debug + Display`; the impl body is usually empty — the two
  supertraits *are* the contract. Pair it with `Result<T, CompileError>` and `?`.
- **`Option::ok_or(err)`** turns `None` into `Err(err)` so `?` can bubble it:
  `output.ok_or(CompileError::NoOutput)?`.
- **`Result::err()`** drops the `Ok` and yields `Option<E>` — assert an error without requiring
  the `Ok` type to be `Debug` (which `.unwrap_err()` would):
  `assert_eq!(compile(…).err(), Some(CompileError::NoOutput))`.
- Other handy combinators: `Option::take()` (move the value out, leaving `None`),
  `Option::map_or(default, f)` (map, or a default when `None`).

**The `?` operator** desugars to "unwrap `Ok`, or `return Err(From::from(e))`" — so it both
short-circuits *and* converts the error via `From`. Define `impl From<SubError> for MyError` once
and every `sub()?` auto-converts; a leaf error like `CompileError` (wraps nothing) needs no `From`
impls. `?` on `Option` needs a function returning `Option` — convert a missing value to an error
first with `ok_or` (eager) / `ok_or_else` (lazy).

**Concrete enum vs `Box<dyn Error>`.** A library returns a **concrete error enum** so callers can
`match` specific cases (our `CompileError`). Application/glue code often returns
`Result<T, Box<dyn Error>>` (or uses `anyhow`): any `E: Error` coerces into `Box<dyn Error>` via
`?`, mixing error types freely — but the caller loses per-case matching. (`thiserror` derives the
`Display`/`Error`/`From` boilerplate; we hand-wrote it to keep deps minimal.)

## 10. Testing

```rust
#[cfg(test)]                      // compiled only for tests
mod tests {
    use super::*;                 // see the parent module's items (incl. private)

    #[test]
    fn it_adds() { assert_eq!(Volts::new(1.0) + Volts::new(0.5), Volts::new(1.5)); }

    #[test]
    #[should_panic(expected = "finite and > 0")]   // asserts it panics with that message
    fn rejects_zero() { let _ = AnalogRate::new(0.0); }   // let _ = discards a #[must_use]
}
```
- Unit tests live **in the same file** as the code, in a `#[cfg(test)] mod tests`.
- `assert_eq!` / `assert_ne!` / `assert!`; floats via `approx::assert_relative_eq!`.
- `let _ = expr;` explicitly discards a value (e.g. a `#[must_use]` result).

**Shared test helpers** — a *whole module* can be test-only, not just an inline `mod tests`:
```rust
// lib.rs
#[cfg(test)]
mod test_util;                              // entire module compiled only for tests

// electrical/cable.rs — a *different* module's tests
use crate::test_util::{sine, measure_gain}; // reached from the crate root
```
- `#[cfg(test)] mod test_util;` gates the whole file: its `pub fn`s are visible crate-wide
  *during the test build* and absent otherwise (so no dead-code warnings in release).
- Contrast the `tests/` integration directory: that's a separate crate seeing only the
  **public** API. An in-crate `#[cfg(test)] mod` can touch **private** items and never ships.

## 11. Tooling & ecosystem

- **Cargo dependencies** use semver: `"0.8"` = "≥0.8.0, <0.9.0". Exact picks are pinned in
  `Cargo.lock`.
- **Features** are optional, additive compile-time flags. `default-features = false` strips a
  crate's defaults. (We use this on `rand`/`rand_distr` so `getrandom` stays out and the engine
  compiles for `wasm32`.)
- **clippy** is the linter; we deny `clippy::all` (so its lints are hard errors). It also
  teaches — e.g. `excessive_precision` rejected an `f32` literal with more digits than `f32`
  can hold and gave the exact fix.
- **`#[allow(lint)]` vs `#[expect(lint, reason = "…")]`** — both suppress a lint locally, but
  `expect` *asserts the lint would fire*: if it later **stops** firing, `expect` itself warns
  ("unfulfilled expectation"), so it self-removes the day the code catches up and stale
  suppressions can't rot. **Crucial caveat:** the expectation must hold in *every* build
  configuration. clippy `--all-targets` compiles the crate as both the **lib** (`cfg(not test)`)
  and the **test** crate (`cfg(test)`):
  - `Edge`'s fields are read by *neither* yet → `dead_code` fires in both → `#[expect]` ✅.
  - `topo_sort` is unused by the lib but **called by its own `#[cfg(test)]` tests** → in the
    test build `dead_code` doesn't fire → `expect` is "unfulfilled" there → denied. Use
    **`#[allow(dead_code)]`** when an item is used in one cfg but not another (it tolerates both).
- **rustfmt** owns layout (`cargo fmt`); it auto-wraps long lines and chains.

## 12. Unsafe, statics & atomics

**`unsafe` marks a contract the compiler can't verify — in two directions:**
- **`unsafe fn` / `unsafe { }`** — *calling* requires upholding preconditions; the **caller** is
  responsible. (Edition 2024: an `unsafe fn` body is **not** implicitly unsafe — you still wrap
  unsafe calls in `unsafe { }`, so each unsafe op stays visible.)
- **`unsafe trait` / `unsafe impl`** — *implementing* requires upholding invariants the trait
  relies on (`GlobalAlloc` must return valid, aligned, non-overlapping blocks). `unsafe impl` is
  your promise you did.

Convention: every `unsafe { }` gets a `// SAFETY: …` comment. The workspace denies `unsafe_code`;
a file that genuinely needs it opts back in with `#![allow(unsafe_code, reason = "…")]` (a local
`allow` overrides the `deny`).

**`static` vs `const`:** a `const` is inlined at each use (no address); a **`static`** is one
value at a fixed address living for the whole program. Shared *mutable* global state must be a
`static` of a thread-safe type (an atomic) — plain `static mut` is unsafe.

**Atomics** are lock-free shared-mutable primitives (`AtomicUsize`, `AtomicBool`, …):
```rust
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
ALLOCS.fetch_add(1, Ordering::Relaxed);   // atomic increment (returns the old value)
ALLOCS.load(Ordering::Relaxed);
```
`Ordering` controls how an atomic synchronizes with *other* memory. **`Relaxed`** is the weakest
— right for a standalone counter, where only the count matters. (`Acquire`/`Release`/`SeqCst` are
for when an atomic guards access to other data.) Atomics are how the engine does its lock-free
cross-thread lanes (params, schedule swap) with **no `Mutex`** on the audio path.

**`#[global_allocator]`** swaps the program's allocator: a `static` of a `GlobalAlloc` type tagged
with the attribute. The no-alloc test (`tests/no_alloc.rs`) installs one that counts allocations
and forwards to `System`, then asserts `process` adds zero across many blocks — a *separate
integration crate* so its allocator is isolated from the lib's unit tests (§10).

---

*Add to this file as new concepts come up, so it stays a complete personal reference.*
