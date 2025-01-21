use rs_store::{DispatchOp, Dispatcher, FnReducer, FnSubscriber, StoreBuilder};
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
    store.add_subscriber(Arc::new(FnSubscriber::from(
        |state: &i32, _action: &i32| {
            println!("subscriber: state: {}", state);
        },
    )));

    // dispatch actions
    store.dispatch(41);
    store.dispatch(1);

    // stop the store
    store.stop();

    assert_eq!(store.get_state(), 42);
}
