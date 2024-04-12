# rs-store

rs-store is a Redux Store written in Rust.

### Key Features

Manage and receive notifications of state changes through the Reducer and Subscriber interfaces.
Store registers Reducer and Subscriber, dispatches actions to change the state, and sends notifications.
Store is designed to be thread-safe, enabling parallel processing.

### How to use

```rust

#[derive(Debug)]
enum CalcAction {
    Add(i32),
    Subtract(i32),
}


#[derive(Debug, Clone)]
struct CalcState {
    count: i32,
}

impl Default for CalcState {
    fn default() -> CalcState {
        CalcState { count: 0 }
    }
}


fn calc_reducer(state: &CalcState, action: &CalcAction) -> CalcState
{
    match action {
        CalcAction::Add(i) => {
            println!("CalcReducer::reduce: + {}", i);
            CalcState {
                count: state.count + i,
            }
        }
        CalcAction::Subtract(i) => {
            println!("CalcReducer::reduce: - {}", i);
            CalcState {
                count: state.count - i,
            }
        }
    }
}


fn calc_subscriber(state: &CalcState, action: &CalcAction) {
    match action {
        CalcAction::Add(i) => {
            println!("CalcSubscriber::notify: state:{:?}, action:{}", state, i);
        }
        CalcAction::Subtract(i) => {
            println!("CalcSubscriber::notify: state:{:?}, action: -{}", state, i);
        }
    }
}

pub fn main() {
    println!("Hello, reduce function !");

    let mut store = Store::<CalcState, CalcAction>::new();
    store.lock().unwrap().add_reducer(Box::new(FnReducer::from(calc_reducer)));

    store.lock().unwrap().add_subscriber(Box::new(FnSubscriber::from(calc_subscriber)));
    store.lock().unwrap().dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.lock().unwrap().add_subscriber(Box::new(FnSubscriber::from(calc_subscriber)));
    store.lock().unwrap().dispatch(CalcAction::Subtract(1));

    // it should be called to close the store
    store.lock().unwrap().close();

    // if you want to wait for the store to close
    Store::wait_for(store.clone()).unwrap_or_else(|e| {
        println!("Error: {:?}", e);
    });
}

```

### To Do

- [ ] add Middleware
- [ ] add Thunk
- [ ] Add tests
