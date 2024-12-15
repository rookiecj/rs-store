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
- 🔄 Support for async operations via Thunk actions
- 📊 Side effect handling in reducers
- 📊 Backpressure handling with configurable policies
- 🎯 Bounded channel size with sync channels
- 🧪 Comprehensive test coverage

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rs-store = "0.10.1"
```

## Quick Start

```rust
use rs_store::{Store, CalcState, CalcAction, CalcReducer, CalcSubscriber};
use std::{thread, sync::Arc};

impl Reducer<CalcState> for CalcReducer {
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> Option<CalcState> {
        match action {
            CalcAction::Add(value) => {
                println!("Adding value: {}", value);
                Some(CalcState { value: state.value + value })
            }
            CalcAction::Subtract(value) => {
                println!("Subtracting value: {}", value);
                Some(CalcState { value: state.value - value })
            }
        }
    }
}

fn main() {
    // Initialize store with default reducer
    let store = Store::<CalcState, CalcAction>::new(Box::new(CalcReducer::default()));

    // Add subscriber
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    
    // Dispatch actions
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Subtract(1));

    // Clean up
    store.stop();
}
```

## Side Effects in Reducers

Unlike traditional Redux implementations, rs-store allows reducers to produce side effects directly. This means reducers can produce asynchronous operations.

This design choice provides more flexibility while maintaining predictable state management.

## Documentation

For detailed documentation, visit:
- [API Documentation (docs.rs)](https://docs.rs/rs-store/0.10.1/rs_store/)
- [Crate Page (crates.io)](https://crates.io/crates/rs-store)

## Implementation Status

### Completed ✅
- Reducer with side effects support
- Thread naming support
- Subscription management (subscribe/unsubscribe)
- Subscriber clearing functionality
- Thunk action support
- Backpressure policy (drop oldest)
- Bounded channel size implementation
- Test coverage

### In Progress 🚧
- Latest state notification for new subscribers
- Notification scheduler (CurrentThread, ThreadPool)
- Stream-based pull model
- Stats middleware for state change logging

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
