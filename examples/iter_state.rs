#[cfg(feature = "notify-channel")]
use rs_store::{DispatchOp, FnReducer, Store};


#[cfg(feature = "notify-channel")]
fn main() {
    // new store with reducer
    let store = Store::new_with_state(Box::new(FnReducer::from(|state: &i32, action: &i32| {
        println!("reducer: {} + {}", state, action);
        DispatchOp::Dispatch(state + action, None)
    })), 0);

    let mut iter = store.iter_state();

    // dispatch actions
    let _ = store.dispatch(41);
    let _ = store.dispatch(1);

    assert_eq!(iter.next(), Some(41));
    assert_eq!(iter.next(), Some(42));

    // stop the store
    store.stop();

    assert_eq!(iter.next(), None);
    eprintln!("exit");
}

#[cfg(not(feature = "notify-channel"))]
fn main() {
    println!("This example requires the 'notify-channel' feature");
}
