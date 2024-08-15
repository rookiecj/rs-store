# rs-store

rs-store is a Redux Store written in Rust.

### Key Features

Manage and receive notifications of state changes through the Reducer and Subscriber interfaces.
Store registers Reducer and Subscriber, dispatches actions to change the state, and sends notifications.
Store is designed to be thread-safe, enabling parallel processing.

### How to use

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
- [ ] add backpressure policy(drop oldest)
- [ ] bounding channel size(sync_channel), dispatch can be failed
- [ ] add scheduler for notification(CurrentThread, ThreadPool)
- [ ] stream  (pull model) instead of subscription(push model)
- [ ] stats middleware which can be used to log the state changes
- [ ] Add tests