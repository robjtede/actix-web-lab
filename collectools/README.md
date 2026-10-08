# `collectools`

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/collectools?label=latest)](https://crates.io/crates/collectools)
[![Documentation](https://docs.rs/collectools/badge.svg)](https://docs.rs/collectools)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/collectools.svg)
![Version](https://img.shields.io/badge/rustc-1.88+-ab6000.svg)

<!-- prettier-ignore-end -->

Collection traits and implementations for common collection types.

Use `List` to read a collection's length and elements. Use `MutableList` to append elements and access them mutably.

These traits are implemented for:

- `Vec`
- `arrayvec::ArrayVec`
- `smallvec::SmallVec`
- `tinyvec::ArrayVec`

Appending to a full fixed-capacity collection panics.

## Usage

Add `collectools` to your dependencies:

```toml
[dependencies]
collectools = "0.1.1"
```

```rust
use collectools::{List, MutableList};

fn append_value(values: &mut impl MutableList<i32>) {
    values.append(42);
}

let mut values = Vec::new();

append_value(&mut values);

assert_eq!(List::len(&values), 1);
assert_eq!(List::get(&values, 0), Some(&42));
```
