# rs-store

rs-store is a Redux Store written in Rust.

### Key Features

Manage and receive notifications of state changes through the Reducer and Subscriber interfaces.
Store registers Reducer and Subscriber, dispatches actions to change the state, and sends notifications to subscribers.
Store is designed to change states in thread-safe manner.

### How to use

rust documentation is available at [crates.io](https://crates.io/crates/rs-store)
and [docs.rs](https://docs.rs/rs-store/0.10.1/rs_store/).

```rust

pub fn main() {
    println!("Hello, Calc!");

    let store = Store::<CalcState, CalcAction>::new(Box::new(CalcReducer::default()));

    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Subtract(1));

    store.stop();
}

```

### Features

- [X] add thread name
- [X] subscription to unsubscribe
- [X] clear subscribers (clear_subscribers)
- [X] add Thunk action(dispatch_thunk)
- [ ] notify the latest state when a subscriber added
- [X] add backpressure policy(drop oldest)
- [x] bounding channel size(sync_channel), dispatch can be failed
- [ ] add scheduler for notification(CurrentThread, ThreadPool)
- [ ] stream  (pull model) instead of subscription(push model)
- [ ] stats middleware which can be used to log the state changes
- [x] add tests