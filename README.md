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
rs-store = "2.7"
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

#### Backpressure Policies

rs-store supports multiple backpressure policies:

- **BlockOnFull**: Blocks the sender when the queue is full (default)
- **DropOldest**: Drops the oldest item when the queue is full
- **DropLatest**: Drops new item when the queue is full
- **DropOldestIf**: Drops items from the oldest based on a custom predicate when the queue is full
- **DropLatestIf**: Drops items from the latest based on a custom predicate when the queue is full

#### Predicate-based Backpressure

The `DropLatestIf` policy allows you to implement intelligent message dropping based on custom criteria:

```rust
use rs_store::{BackpressurePolicy, StoreBuilder};
use std::sync::Arc;

// Create a predicate that drops low-priority messages
let predicate = Arc::new(|action_op: &rs_store::ActionOp<i32>| {
    match action_op {
        rs_store::ActionOp::Action(value) => *value < 5, // Drop values less than 5
        rs_store::ActionOp::Exit(_) => false, // Never drop exit messages
    }
});

let policy = BackpressurePolicy::DropLatestIf { predicate };

let store = StoreBuilder::new(0)
    .with_capacity(3) // Small capacity to trigger backpressure
    .with_policy(policy)
    .build()
    .unwrap();
```

This allows you to prioritize important messages and drop less critical ones when the system is under load.

### Side Effects in Reducers

Unlike traditional Redux implementations, rs-store allows reducers to produce side effects directly. This means reducers can produce asynchronous operations.

### Middleware

Middleware is a powerful feature that allows you to intercept and modify actions before they reach the reducer, or to handle side effects, logging, metrics, etc.

### Channeled Subscription

The channeled subscription feature provides a way to subscribe to a store in new context with a channel.

### Latest State Notification for New Subscribers

When a new subscriber is added to the store, it automatically receives the current state through the `on_subscribe` method. This ensures that new subscribers don't miss the current state and can start with the latest information.

```rust
use rs_store::{Subscriber, StoreBuilder};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct MyState {
    counter: i32,
}

#[derive(Clone)]
enum MyAction {
    Increment,
    Decrement,
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
let store = StoreBuilder::new_with_reducer(MyState { counter: 0 }, reducer)
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

- [API Documentation (docs.rs)](https://docs.rs/rs-store/2.7.0/rs_store/)
- [Crate Page (crates.io)](https://crates.io/crates/rs-store)

## Implementation Status

### In Progress 🚧
- [x] Latest state notification for new subscribers
- [x] Notification scheduler (CurrentThread, ThreadPool)
- [ ] Stop store after all effects are scheduled
- [X] drop store after all references are dropped
- [x] dispatcher has weak reference to the store
- [ ] effects system

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
