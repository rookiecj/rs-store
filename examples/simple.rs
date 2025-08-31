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
