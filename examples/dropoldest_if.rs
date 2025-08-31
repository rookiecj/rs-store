use rs_store::{BackpressurePolicy, DispatchOp, FnReducer, FnSubscriber, StoreBuilder};
use std::sync::Arc;

fn main() {
    // predicate 기반 drop 정책 사용 예시
    // 작은 값들을 우선적으로 drop하는 predicate
    // predicate는 이제 Action의 내용(T)에만 적용됩니다
    let predicate = Arc::new(|value: &i32| {
        println!("droppable {} ? {}", *value, *value < 5);
        *value < 5 // 5보다 작은 값들은 drop
    });

    let policy = BackpressurePolicy::DropOldestIf { predicate };

    // 매우 작은 capacity로 store 생성하여 backpressure 상황 시뮬레이션
    let store = StoreBuilder::new(0)
        .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
            // reducer에서 지연을 추가하여 backpressure 상황 생성
            std::thread::sleep(std::time::Duration::from_millis(200));
            println!("reducer: {} + {}", state, action);
            DispatchOp::Dispatch(state + action, None)
        })))
        .with_capacity(2) // 매우 작은 capacity로 설정
        .with_policy(policy)
        .build()
        .unwrap();

    // subscriber 추가
    store
        .add_subscriber(Arc::new(FnSubscriber::from(|state: &i32, action: &i32| {
            println!("subscriber: state: {}, action: {}", state, action);
        })))
        .unwrap();

    println!("=== Predicate 기반 Backpressure 테스트 ===");
    println!("채널 capacity: 2");
    println!("predicate: 5보다 작은 값들은 drop");
    println!("reducer 지연: 200ms");
    println!();

    // 다양한 값들을 빠르게 dispatch
    let actions = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    for action in actions {
        println!("dispatch: {}", action);
        match store.dispatch(action) {
            Ok(_) => println!("  -> 성공"),
            Err(e) => println!("  -> 실패: {:?}", e),
        }

        // 빠른 dispatch로 backpressure 상황 생성
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // 처리 완료 대기
    // std::thread::sleep(std::time::Duration::from_millis(3000));

    // stop can be failed when the queue is full , the send will be blocked
    match store.stop() {
        Ok(_) => println!("store stopped"),
        Err(e) => {
            println!("store stop failed  : {:?}", e);
            panic!("store stop failed  : {:?}", e);
        }
    }

    // 최종 상태 확인
    let final_state = store.get_state();
    println!();
    println!("최종 상태: {}", final_state);

    // // 메트릭 확인
    // let metrics = store.get_metrics();
    // println!("메트릭:");
    // println!("  - 받은 액션: {}", metrics.action_received);
    // println!("  - 처리된 액션: {}", metrics.action_reduced);
    // println!("  - drop된 액션: {}", metrics.action_dropped);
}
