use rs_store::{DispatchOp, FnReducer, StoreImpl};

fn main() {
    // new store with reducer
    let store = StoreImpl::new_with_reducer(
        0,
        Box::new(FnReducer::from(|state: &i32, action: &i32| {
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })),
    );

    let mut iter = store.iter();

    // dispatch actions
    let _ = store.dispatch(41);
    let _ = store.dispatch(1);

    assert_eq!(iter.next(), Some((41, 41)));
    assert_eq!(iter.next(), Some((42, 1)));

    // stop the store
    store.stop();

    assert_eq!(iter.next(), None);
    eprintln!("exit");
}
