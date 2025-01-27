# rs-store

[![Crates.io](https://img.shields.io/crates/v/rs-store.svg)](https://crates.io/crates/rs-store)
[![Documentation](https://docs.rs/rs-store/badge.svg)](https://docs.rs/rs-store)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A thread-safe Redux-style state management library implemented in Rust.

## Overview

rs-store provides a predictable state container inspired by Redux, featuring thread-safe state management with support for reducers, subscribers, and async actions through Thunk. Unlike traditional Redux, rs-store's reducers can produce side effects, providing more flexibility in state management.

## Features

- 🔒 Thread-safe state management
- 📢 Publisher/Subscriber pattern for state changes
- 🔄 Support for asynchronous operations via Thunk actions
- 📊 Side effect handling in reducers
- 📊 Middleware handles actions and effects
- 📊 Backpressure handling with configurable policies
- 🎯 Bounded channel size with sync channels
- 🔄 Decoupling state updates from notification delivery
- 📚 State Iterator support
- 📚 Selector support
- 📊 Metrics support
- 🧪 Comprehensive test coverage

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rs-store = "0.23.1"
```

with the `notify-channel` feature:

```toml
[dependencies]
rs-store = { version = "0.23.1", features = ["notify-channel"] }
```

## Quick Start

```rust
use std::sync::Arc;
use rs_store::{DispatchOp, Dispatcher, FnReducer, FnSubscriber, StoreBuilder};

pub fn main() {
    // new store with reducer
    let store = StoreBuilder::new(0)
        .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })))
        .build()
        .unwrap();

    // add subscriber
    store.add_subscriber(Arc::new(FnSubscriber::from(|state: &i32, _action: &i32| {
        println!("subscriber: state: {}", state);
    })));

    // dispatch actions
    store.dispatch(41);
    store.dispatch(1);

    // stop the store
    store.stop();

    assert_eq!(store.get_state(), 42);
}
```

## Feature Details

### Backpressure feature

Backpressure is a feature that allows you to control the rate of state updates.
and it also can be used to prevent slow subscribers from blocking state updates.

### Side Effects in Reducers

Unlike traditional Redux implementations, rs-store allows reducers to produce side effects directly. This means reducers can produce asynchronous operations.

### Middleware feature

Middleware is a powerful feature that allows you to intercept and modify actions before they reach the reducer, or to handle side effects, logging, metrics, etc.

### Notification Channel feature

The notification channel feature provides a dedicated channel for state notifications to subscribers, separating the concerns of state updates and notification delivery. 

### State Iterator feature

You can subscribe to a Store to get the state history, But the state iterator feature provides a way to iterate over the state.

### Selector feature

The selector feature provides a way to select a part of the state.

### Metrics feature

The metrics feature provides a way to collect metrics.


## Documentation

For detailed documentation, visit:

- [API Documentation (docs.rs)](https://docs.rs/rs-store/0.23.1/rs_store/)
- [Crate Page (crates.io)](https://crates.io/crates/rs-store)

## Implementation Status

### In Progress 🚧
- [ ] Latest state notification for new subscribers
- [x] Notification scheduler (CurrentThread, ThreadPool)
- [X] Stream-based pull model
- [ ] Stop store after all effects are scheduled

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
