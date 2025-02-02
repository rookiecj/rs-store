use rs_store::store::Store;
use rs_store::{DispatchOp, FnReducer, StoreBuilder};
use std::sync::Arc;

fn main() {
    let store = StoreBuilder::new(0)
        .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })))
        .build()
        .unwrap();

    do_with_store(store.clone());

    // no need to stop or drop, because DroppableStore
    //store.stop();
    //drop(store);
}

fn do_with_store(store: Arc<dyn Store<i32, i32>>) {
    let _ = store.dispatch(41);
    let _ = store.dispatch(1);
}
