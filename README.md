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
- 🧪 Comprehensive test coverage
- 📚 Selector support
- 📊 Metrics support
- 🔄 Decoupling state updates from notification delivery

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rs-store = "0.18.2"
```

with the `notify-channel` feature:

```toml
[dependencies]
rs-store = { version = "0.18.2", features = ["notify-channel"] }
```

## Quick Start

```rust
use rs_store::{Store, CalcState, CalcAction, CalcReducer, CalcSubscriber};
use std::{thread, sync::Arc};

impl Reducer<CalcState> for CalcReducer {
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState, CalcAction> {
        match action {
            CalcAction::Add(value) => {
                println!("Adding value: {}", value);
                DispatchOp::Dispatch(
                    CalcState {
                        value: state.value + value,
                    },
                    None,
                )
            }
            CalcAction::Subtract(value) => {
                println!("Subtracting value: {}", value);
                DispatchOp::Dispatch(
                    CalcState {
                        value: state.value - value,
                    },
                    None,
                )
            }
        }
    }
}

fn main() {
    // Initialize store with default reducer
    let store = Store::<CalcState, CalcAction>::new(Box::new(CalcReducer::default()));

    // Add subscriber
    store.add_subscriber(Arc::new(CalcSubscriber::default()));

    // Dispatch action
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    // add more subscriber
    store.add_subscriber(Arc::new(CalcSubscriber::default()));

    // Dispatch action
    store.dispatch(CalcAction::Subtract(1));

    // Clean up
    store.stop();
}
```

## Side Effects in Reducers

Unlike traditional Redux implementations, rs-store allows reducers to produce side effects directly. This means reducers can produce asynchronous operations.

This design choice provides more flexibility while maintaining predictable state management.

## Notification Channel feature

The notification channel feature provides a dedicated channel for state notifications to subscribers, separating the concerns of state updates and notification delivery. This can help improve performance and reliability by:

- Decoupling state updates from notification delivery
- Preventing slow subscribers from blocking state updates
- Allowing independent backpressure handling for notifications
- Supporting different threading models for notification delivery


## Documentation

For detailed documentation, visit:

- [API Documentation (docs.rs)](https://docs.rs/rs-store/0.18.2/rs_store/)
- [Crate Page (crates.io)](https://crates.io/crates/rs-store)

## Implementation Status

### In Progress 🚧
- Latest state notification for new subscribers
- [x] Notification scheduler (CurrentThread, ThreadPool)
- Stream-based pull model

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
