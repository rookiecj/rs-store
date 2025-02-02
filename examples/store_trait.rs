use std::sync::Arc;
use rs_store::{DispatchOp, FnReducer, Store, StoreBuilder};

fn main() {
    let store = StoreBuilder::new(0)
        .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })))
        .build()
        .unwrap();

    do_with_store(store);
}

fn do_with_store(store: Arc<dyn Store<i32, i32>>) {
    let _ = store.dispatch(41);
    let _ = store.dispatch(1);
    store.stop();

    assert_eq!(store.get_state(), 42);
}
