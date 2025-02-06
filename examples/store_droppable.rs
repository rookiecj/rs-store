use rs_store::store::Store;
use rs_store::{DispatchOp, DroppableStore, FnReducer, StoreBuilder};
use std::sync::Arc;

fn main() {
    let store = StoreBuilder::new(0)
        .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })))
        .build()
        .unwrap();
    let droppable_store = DroppableStore::new(store);
    do_with_store(droppable_store.clone());

    // no need to stop or drop, because DroppableStore
    //droppable_store.stop();
    //drop(droppable_store);
}

fn do_with_store(store: Arc<dyn Store<i32, i32>>) {
    let _ = store.dispatch(41);
    let _ = store.dispatch(1);
}
