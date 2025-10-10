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
- 📚 Channeled subscription support
- 📊 Metrics support
- 🧪 Comprehensive test coverage

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rs-store = "2.9.0"
```

## Quick Start

```rust
use rs_store::{DispatchOp, FnReducer, FnSubscriber, StoreBuilder};
use std::sync::Arc;

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
    store
        .add_subscriber(Arc::new(FnSubscriber::from(
            |state: &i32, _action: &i32| {
                println!("subscriber: state: {}", state);
            },
        )))
        .unwrap();

    // dispatch actions
    store.dispatch(41).expect("no error");
    store.dispatch(1).expect("no error");

    // stop the store
    match store.stop() {
        Ok(_) => println!("store stopped"),
        Err(e) => {
            panic!("store stop failed  : {:?}", e);
        }
    }

    assert_eq!(store.get_state(), 42);
}
```

## Feature Details

### Backpressure feature

Backpressure is a feature that allows you to control the rate of state updates.
and it also can be used to prevent slow subscribers from blocking state updates.

#### Backpressure Policies

rs-store supports multiple backpressure policies:

- **BlockOnFull**: Blocks the sender when the queue is full (default)
- **DropOldestIf(None)**: Drops the oldest item when the queue is full (replaces deprecated `DropOldest`)
- **DropLatestIf(None)**: Drops the newest item when the queue is full (replaces deprecated `DropLatest`)
- **DropOldestIf(Some(predicate))**: Drops items from the oldest based on a custom predicate when the queue is full
- **DropLatestIf(Some(predicate))**: Drops items from the latest based on a custom predicate when the queue is full

> **Note**: `DropOldest` and `DropLatest` are deprecated. Use `DropOldestIf(None)` and `DropLatestIf(None)` instead for unconditional dropping, or provide a predicate for conditional dropping.

##### Predicate-based Backpressure

The `DropLatestIf` and `DropOldestIf` policies allow you to implement intelligent message dropping based on custom criteria.

**Example: Unconditional dropping**
```rust
use rs_store::{BackpressurePolicy, StoreBuilder};

// Drop the oldest item unconditionally when queue is full
let policy = BackpressurePolicy::DropOldestIf(None);

let store = StoreBuilder::new(0)
    .with_capacity(3)
    .with_policy(policy)
    .build()
    .unwrap();
```

**Example: Conditional dropping with predicate**
```rust
use rs_store::{BackpressurePolicy, StoreBuilder};

// Drop low-priority messages based on a predicate
let predicate = Box::new(|value: &i32| {
    *value < 5 // Drop values less than 5
});

let policy = BackpressurePolicy::DropLatestIf(Some(predicate));

let store = StoreBuilder::new(0)
    .with_capacity(3) // Small capacity to trigger backpressure
    .with_policy(policy)
    .build()
    .unwrap();
```

This allows you to prioritize important messages and drop less critical ones when the system is under load.

### Side Effects in Reducers

Unlike traditional Redux implementations, rs-store allows reducers to produce side effects directly.
This means reducers can produce asynchronous operations.

```rust
impl Reducer<CalcState, CalcAction> for CalcReducer {
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState, CalcAction> {
        match action {
            CalcAction::AddWillProduceThunk(i) => {
                println!("CalcReducer::reduce: + {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count + i,
                    },
                    Some(Effect::Thunk(subtract_effect_thunk(*i))),
                )
            }
        }
    }
}

```

### Middleware

Middleware is a powerful feature that allows you to intercept and modify actions before they reach the reducer,
or to handle side effects, logging, metrics, etc.

### Channeled Subscription

The channeled subscription feature provides a way to subscribe to a store in new context with a channel.

### Latest State Notification for New Subscribers

When a new subscriber is added to the store, it automatically receives the current state through the `on_subscribe`
method.
This ensures that new subscribers don't miss the current state and can start with the latest information.

```rust
use rs_store::{DispatchOp, Reducer, Subscriber, StoreBuilder};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct MyState {
    counter: i32,
}

#[derive(Clone, Debug)]
enum MyAction {
    Increment,
    Decrement,
}

struct MyReducer;

impl Reducer<MyState, MyAction> for MyReducer {
    fn reduce(&self, state: &MyState, action: &MyAction) -> DispatchOp<MyState, MyAction> {
        match action {
            MyAction::Increment => {
                DispatchOp::Dispatch(MyState { counter: state.counter + 1 }, None)
            }
            MyAction::Decrement => {
                DispatchOp::Dispatch(MyState { counter: state.counter - 1 }, None)
            }
        }
    }
}

struct MySubscriber {
    received_states: Arc<Mutex<Vec<MyState>>>,
}

impl Subscriber<MyState, MyAction> for MySubscriber {
    fn on_subscribe(&self, state: &MyState) {
        // Called when the subscriber is first added to the store
        // Receives the current state immediately
        println!("New subscriber received initial state: {:?}", state);
        self.received_states.lock().unwrap().push(state.clone());
    }

    fn on_notify(&self, state: &MyState, action: &MyAction) {
        // Called when the state changes due to an action
        println!("State updated: {:?}", state);
        self.received_states.lock().unwrap().push(state.clone());
    }
}

// Usage
let store = StoreBuilder::new_with_reducer(MyState { counter: 0 }, Box::new(MyReducer))
    .build()
    .unwrap();

// Dispatch some actions to change the state
store.dispatch(MyAction::Increment).unwrap();
store.dispatch(MyAction::Increment).unwrap();

// Add a new subscriber - it will receive the current state (counter: 2)
let subscriber = Arc::new(MySubscriber {
    received_states: Arc::new(Mutex::new(Vec::new())),
});
store.add_subscriber(subscriber);
```

This feature ensures that new subscribers are immediately synchronized with the current state of the store.

### Metrics

The metrics feature provides a way to collect metrics.

## Documentation

For detailed documentation, visit:

- [API Documentation (docs.rs)](https://docs.rs/rs-store/2.9.0/rs_store/)
- [Crate Page (crates.io)](https://crates.io/crates/rs-store)

## Implementation Status

### In Progress 🚧
- [x] Latest state notification for new subscribers
- [x] Notification scheduler (CurrentThread, ThreadPool)
- [-] ~~Stop store after all effects are scheduled~~ (removed)
- [X] drop store after all references are dropped
- [x] dispatcher has weak reference to the store
- [ ] effects system

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
