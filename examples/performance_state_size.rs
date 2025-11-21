use rs_store::{DispatchOp, FnReducer, FnSubscriber, StoreBuilder};
use std::backtrace::Backtrace;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

/// Clone 횟수를 추적하는 공유 카운터
#[derive(Clone, Debug)]
struct CloneCounter {
    count: Arc<Mutex<u64>>,
    locations: Arc<Mutex<Vec<String>>>,
}

impl CloneCounter {
    fn new() -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
            locations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn increment(&self) {
        *self.count.lock().unwrap() += 1;
    }

    fn get_count(&self) -> u64 {
        *self.count.lock().unwrap()
    }

    fn add_location(&self, location: String) {
        self.locations.lock().unwrap().push(location);
    }

    fn get_locations(&self) -> Vec<String> {
        self.locations.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    fn reset(&self) {
        *self.count.lock().unwrap() = 0;
        self.locations.lock().unwrap().clear();
    }
}

/// 작은 State 구조체 (약 8 bytes) - Clone 추적 포함
#[derive(Debug)]
struct SmallState {
    counter: i32,
    clone_counter: CloneCounter,
}

impl SmallState {
    #[allow(dead_code)]
    fn new(counter: i32, clone_counter: CloneCounter) -> Self {
        SmallState {
            counter,
            clone_counter,
        }
    }

    fn counter(&self) -> i32 {
        self.counter
    }

    fn set_counter(&mut self, counter: i32) {
        self.counter = counter;
    }

    fn get_clone_count(&self) -> u64 {
        self.clone_counter.get_count()
    }

    fn get_clone_locations(&self) -> Vec<String> {
        self.clone_counter.get_locations()
    }
}

impl Clone for SmallState {
    #[inline(never)] // 인라인 방지로 backtrace가 더 정확하게 작동하도록
    fn clone(&self) -> Self {
        self.clone_counter.increment();
        // Backtrace를 사용하여 호출 스택 정보 수집
        let backtrace = Backtrace::force_capture();
        let location_str = if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            // Backtrace를 문자열로 변환하고 실제 호출 위치 찾기
            // clone() 함수 자체를 제외한 실제 호출자 위치 찾기
            let bt_str = backtrace.to_string();
            let lines: Vec<String> = bt_str
                .lines()
                .filter(|line| {
                    // clone 관련 라인과 backtrace 내부 라인 제외
                    !line.contains("Clone::clone")
                        && !line.contains("performance_state_size::")
                        && !line.contains("backtrace")
                        && !line.contains("libunwind")
                        && (line.contains("at") || line.contains("::"))
                })
                .map(|line| line.trim().to_string())
                .take(2) // 실제 호출자 위치 2개만
                .collect();
            if !lines.is_empty() {
                lines.join(" | ")
            } else {
                // Fallback to Location::caller()
                let location = std::panic::Location::caller();
                format!(
                    "{:?}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            }
        } else {
            // Fallback to Location::caller() if backtrace is not available
            let location = std::panic::Location::caller();
            format!(
                "{:?}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        };
        self.clone_counter.add_location(location_str);

        SmallState {
            counter: self.counter,
            clone_counter: self.clone_counter.clone(),
        }
    }
}

impl Default for SmallState {
    fn default() -> Self {
        SmallState {
            counter: 0,
            clone_counter: CloneCounter::new(),
        }
    }
}

/// 중간 크기 State 구조체 (약 1KB) - Clone 추적 포함
#[derive(Debug)]
struct MediumState {
    counter: i32,
    #[allow(dead_code)]
    data: [u8; 1000], // 1000 bytes
    clone_counter: CloneCounter,
}

impl MediumState {
    fn counter(&self) -> i32 {
        self.counter
    }

    fn set_counter(&mut self, counter: i32) {
        self.counter = counter;
    }

    fn get_clone_count(&self) -> u64 {
        self.clone_counter.get_count()
    }

    fn get_clone_locations(&self) -> Vec<String> {
        self.clone_counter.get_locations()
    }
}

impl Clone for MediumState {
    #[inline(never)] // 인라인 방지로 backtrace가 더 정확하게 작동하도록
    fn clone(&self) -> Self {
        self.clone_counter.increment();
        // Backtrace를 사용하여 호출 스택 정보 수집
        let backtrace = Backtrace::force_capture();
        let location_str = if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            // Backtrace를 문자열로 변환하고 실제 호출 위치 찾기
            // clone() 함수 자체를 제외한 실제 호출자 위치 찾기
            let bt_str = backtrace.to_string();
            let lines: Vec<String> = bt_str
                .lines()
                .filter(|line| {
                    // clone 관련 라인과 backtrace 내부 라인 제외
                    !line.contains("Clone::clone")
                        && !line.contains("performance_state_size::")
                        && !line.contains("backtrace")
                        && !line.contains("libunwind")
                        && (line.contains("at") || line.contains("::"))
                })
                .map(|line| line.trim().to_string())
                .take(2) // 실제 호출자 위치 2개만
                .collect();
            if !lines.is_empty() {
                lines.join(" | ")
            } else {
                // Fallback to Location::caller()
                let location = std::panic::Location::caller();
                format!(
                    "{:?}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            }
        } else {
            // Fallback to Location::caller() if backtrace is not available
            let location = std::panic::Location::caller();
            format!(
                "{:?}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        };
        self.clone_counter.add_location(location_str);

        MediumState {
            counter: self.counter,
            data: self.data,
            clone_counter: self.clone_counter.clone(),
        }
    }
}

impl Default for MediumState {
    fn default() -> Self {
        MediumState {
            counter: 0,
            data: [0; 1000],
            clone_counter: CloneCounter::new(),
        }
    }
}

/// 큰 State 구조체 (약 1MB) - Clone 추적 포함
#[derive(Debug)]
struct LargeState {
    counter: i32,
    #[allow(dead_code)]
    data: Vec<u8>, // 1MB
    clone_counter: CloneCounter,
}

impl LargeState {
    fn counter(&self) -> i32 {
        self.counter
    }

    fn set_counter(&mut self, counter: i32) {
        self.counter = counter;
    }

    fn get_clone_count(&self) -> u64 {
        self.clone_counter.get_count()
    }

    fn get_clone_locations(&self) -> Vec<String> {
        self.clone_counter.get_locations()
    }
}

impl Clone for LargeState {
    #[inline(never)] // 인라인 방지로 backtrace가 더 정확하게 작동하도록
    fn clone(&self) -> Self {
        self.clone_counter.increment();
        // Backtrace를 사용하여 호출 스택 정보 수집
        let backtrace = Backtrace::force_capture();
        let location_str = if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            // Backtrace를 문자열로 변환하고 실제 호출 위치 찾기
            // clone() 함수 자체를 제외한 실제 호출자 위치 찾기
            let bt_str = backtrace.to_string();
            let lines: Vec<String> = bt_str
                .lines()
                .filter(|line| {
                    // clone 관련 라인과 backtrace 내부 라인 제외
                    !line.contains("Clone::clone")
                        && !line.contains("performance_state_size::")
                        && !line.contains("backtrace")
                        && !line.contains("libunwind")
                        && (line.contains("at") || line.contains("::"))
                })
                .map(|line| line.trim().to_string())
                .take(2) // 실제 호출자 위치 2개만
                .collect();
            if !lines.is_empty() {
                lines.join(" | ")
            } else {
                // Fallback to Location::caller()
                let location = std::panic::Location::caller();
                format!(
                    "{:?}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            }
        } else {
            // Fallback to Location::caller() if backtrace is not available
            let location = std::panic::Location::caller();
            format!(
                "{:?}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        };
        self.clone_counter.add_location(location_str);

        LargeState {
            counter: self.counter,
            data: self.data.clone(),
            clone_counter: self.clone_counter.clone(),
        }
    }
}

impl Default for LargeState {
    fn default() -> Self {
        LargeState {
            counter: 0,
            data: vec![0; 1_000_000], // 1MB
            clone_counter: CloneCounter::new(),
        }
    }
}

/// 매우 큰 State 구조체 (약 10MB) - Clone 추적 포함
#[derive(Debug)]
struct VeryLargeState {
    counter: i32,
    #[allow(dead_code)]
    data: Vec<u8>, // 10MB
    clone_counter: CloneCounter,
}

impl VeryLargeState {
    fn counter(&self) -> i32 {
        self.counter
    }

    fn set_counter(&mut self, counter: i32) {
        self.counter = counter;
    }

    fn get_clone_count(&self) -> u64 {
        self.clone_counter.get_count()
    }

    fn get_clone_locations(&self) -> Vec<String> {
        self.clone_counter.get_locations()
    }
}

impl Clone for VeryLargeState {
    #[inline(never)] // 인라인 방지로 backtrace가 더 정확하게 작동하도록
    fn clone(&self) -> Self {
        self.clone_counter.increment();
        // Backtrace를 사용하여 호출 스택 정보 수집
        let backtrace = Backtrace::force_capture();
        let location_str = if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            // Backtrace를 문자열로 변환하고 실제 호출 위치 찾기
            // clone() 함수 자체를 제외한 실제 호출자 위치 찾기
            let bt_str = backtrace.to_string();
            let lines: Vec<String> = bt_str
                .lines()
                .filter(|line| {
                    // clone 관련 라인과 backtrace 내부 라인 제외
                    !line.contains("Clone::clone")
                        && !line.contains("performance_state_size::")
                        && !line.contains("backtrace")
                        && !line.contains("libunwind")
                        && (line.contains("at") || line.contains("::"))
                })
                .map(|line| line.trim().to_string())
                .take(3) // 실제 호출자 위치 2개만
                .collect();
            if !lines.is_empty() {
                lines.join(" | ")
            } else {
                // Fallback to Location::caller()
                let location = std::panic::Location::caller();
                format!(
                    "{:?}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            }
        } else {
            // Fallback to Location::caller() if backtrace is not available
            let location = std::panic::Location::caller();
            format!(
                "{:?}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        };
        self.clone_counter.add_location(location_str);

        VeryLargeState {
            counter: self.counter,
            data: self.data.clone(),
            clone_counter: self.clone_counter.clone(),
        }
    }
}

impl Default for VeryLargeState {
    fn default() -> Self {
        VeryLargeState {
            counter: 0,
            data: vec![0; 10_000_000], // 10MB
            clone_counter: CloneCounter::new(),
        }
    }
}

/// 공통 Action 타입
#[derive(Clone, Debug)]
enum TestAction {
    Increment,
    Decrement,
}

fn main() {
    println!("=== State 크기에 따른 성능 테스트 ===\n");

    // 작은 State 테스트
    test_small_state();

    // 중간 크기 State 테스트
    test_medium_state();

    // 큰 State 테스트
    test_large_state();

    // 매우 큰 State 테스트 (값으로 전달)
    test_very_large_state();

    // 매우 큰 State 테스트 (Arc로 전달)
    test_very_large_state_with_arc();
}

fn test_small_state() {
    println!("--- Small State 테스트 (8 bytes, 1000 actions) ---");
    let reducer_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));
    let subscriber_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));

    let reducer_times_clone = reducer_times.clone();
    let subscriber_times_clone = subscriber_times.clone();

    let reducer = FnReducer::from(move |state: SmallState, action: TestAction| {
        let start = Instant::now();
        let mut new_state = state.clone();
        match action {
            TestAction::Increment => new_state.set_counter(new_state.counter() + 1),
            TestAction::Decrement => new_state.set_counter(new_state.counter() - 1),
        }
        let elapsed = start.elapsed();
        reducer_times_clone.lock().unwrap().push(elapsed);
        DispatchOp::Dispatch(new_state, vec![])
    });

    let subscriber = FnSubscriber::from(move |state: SmallState, _action: TestAction| {
        let start = Instant::now();
        let _ = state.clone();
        let elapsed = start.elapsed();
        subscriber_times_clone.lock().unwrap().push(elapsed);
    });

    run_test(
        "Small (8 bytes)",
        SmallState::default(),
        1000,
        reducer,
        Arc::new(subscriber),
        reducer_times,
        subscriber_times,
    );
}

fn test_medium_state() {
    println!("--- Medium State 테스트 (1KB, 1000 actions) ---");
    let reducer_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));
    let subscriber_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));

    let reducer_times_clone = reducer_times.clone();
    let subscriber_times_clone = subscriber_times.clone();

    let reducer = FnReducer::from(move |state: MediumState, action: TestAction| {
        let start = Instant::now();
        let mut new_state = state.clone();
        match action {
            TestAction::Increment => new_state.set_counter(new_state.counter() + 1),
            TestAction::Decrement => new_state.set_counter(new_state.counter() - 1),
        }
        let elapsed = start.elapsed();
        reducer_times_clone.lock().unwrap().push(elapsed);
        DispatchOp::Dispatch(new_state, vec![])
    });

    let subscriber = FnSubscriber::from(move |state: MediumState, _action: TestAction| {
        let start = Instant::now();
        let _ = state.clone();
        let elapsed = start.elapsed();
        subscriber_times_clone.lock().unwrap().push(elapsed);
    });

    run_test(
        "Medium (1KB)",
        MediumState::default(),
        1000,
        reducer,
        Arc::new(subscriber),
        reducer_times,
        subscriber_times,
    );
}

fn test_large_state() {
    println!("--- Large State 테스트 (1MB, 1000 actions) ---");
    let reducer_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));
    let subscriber_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));

    let reducer_times_clone = reducer_times.clone();
    let subscriber_times_clone = subscriber_times.clone();

    let reducer = FnReducer::from(move |state: LargeState, action: TestAction| {
        let start = Instant::now();
        let mut new_state = state.clone();
        match action {
            TestAction::Increment => new_state.set_counter(new_state.counter() + 1),
            TestAction::Decrement => new_state.set_counter(new_state.counter() - 1),
        }
        let elapsed = start.elapsed();
        reducer_times_clone.lock().unwrap().push(elapsed);
        DispatchOp::Dispatch(new_state, vec![])
    });

    let subscriber = FnSubscriber::from(move |state: LargeState, _action: TestAction| {
        let start = Instant::now();
        let _ = state.clone();
        let elapsed = start.elapsed();
        subscriber_times_clone.lock().unwrap().push(elapsed);
    });

    run_test(
        "Large (1MB)",
        LargeState::default(),
        1000,
        reducer,
        Arc::new(subscriber),
        reducer_times,
        subscriber_times,
    );
}

fn test_very_large_state() {
    println!("--- Very Large State 테스트 (10MB, 값으로 전달, 1000 actions) ---");
    let reducer_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));
    let subscriber_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));

    let reducer_times_clone = reducer_times.clone();
    let subscriber_times_clone = subscriber_times.clone();

    let reducer = FnReducer::from(move |state: VeryLargeState, action: TestAction| {
        let start = Instant::now();
        let mut new_state = state.clone();
        match action {
            TestAction::Increment => new_state.set_counter(new_state.counter() + 1),
            TestAction::Decrement => new_state.set_counter(new_state.counter() - 1),
        }
        let elapsed = start.elapsed();
        reducer_times_clone.lock().unwrap().push(elapsed);
        DispatchOp::Dispatch(new_state, vec![])
    });

    let subscriber = FnSubscriber::from(move |state: VeryLargeState, _action: TestAction| {
        let start = Instant::now();
        let _ = state.clone();
        let elapsed = start.elapsed();
        subscriber_times_clone.lock().unwrap().push(elapsed);
    });

    run_test(
        "Very Large (10MB, 값으로 전달)",
        VeryLargeState::default(),
        1000,
        reducer,
        Arc::new(subscriber),
        reducer_times,
        subscriber_times,
    );
}

fn test_very_large_state_with_arc() {
    println!("--- Very Large State 테스트 (10MB, Arc로 전달, 1000 actions) ---");
    let reducer_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));
    let subscriber_times = Arc::new(Mutex::new(Vec::<std::time::Duration>::new()));

    let reducer_times_clone = reducer_times.clone();
    let subscriber_times_clone = subscriber_times.clone();

    // Arc<VeryLargeState>를 State로 사용
    type ArcVeryLargeState = Arc<VeryLargeState>;

    let reducer = FnReducer::from(move |state: ArcVeryLargeState, action: TestAction| {
        let start = Instant::now();
        // Arc::make_mut()을 사용하여 copy-on-write 방식으로 수정
        // Arc가 단일 소유권이면 in-place 수정, 여러 소유권이면 복사 후 수정
        let mut new_state = state.clone(); // Arc clone (참조 카운트만 증가)
        let inner = Arc::make_mut(&mut new_state); // 필요시에만 실제 데이터 복사
        match action {
            TestAction::Increment => {
                inner.set_counter(inner.counter() + 1);
            }
            TestAction::Decrement => {
                inner.set_counter(inner.counter() - 1);
            }
        }
        let elapsed = start.elapsed();
        reducer_times_clone.lock().unwrap().push(elapsed);
        DispatchOp::Dispatch(new_state, vec![])
    });

    let subscriber = FnSubscriber::from(move |state: ArcVeryLargeState, _action: TestAction| {
        let start = Instant::now();
        // Arc clone은 매우 빠름 (참조 카운트만 증가)
        let _ = state.clone();
        let elapsed = start.elapsed();
        subscriber_times_clone.lock().unwrap().push(elapsed);
    });

    run_test_with_arc(
        "Very Large (10MB, Arc로 전달)",
        Arc::new(VeryLargeState::default()),
        1000, // Arc를 사용하면 더 많은 액션을 빠르게 처리할 수 있음
        reducer,
        Arc::new(subscriber),
        reducer_times,
        subscriber_times,
    );
}

fn run_test<State>(
    _name: &str,
    initial_state: State,
    num_actions: usize,
    reducer: impl rs_store::Reducer<State, TestAction> + Send + Sync + 'static,
    subscriber: Arc<dyn rs_store::Subscriber<State, TestAction> + Send + Sync>,
    reducer_times: Arc<Mutex<Vec<std::time::Duration>>>,
    subscriber_times: Arc<Mutex<Vec<std::time::Duration>>>,
) where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    // Store 생성
    let store: Arc<dyn rs_store::Store<State, TestAction>> = StoreBuilder::new(initial_state)
        .with_reducer(
            // Box::new(reducer)만으로는 컴파일러가 목표 타입(Reducer<State, TestAction>)을 알 수 없습니다.
            // trait object로의 unsized coercion을 수행하여 목표 타입을 지정해줘야 합니다.
            // 목표 type 추론할 수 있도록 store에 타입을 선언했다. 이제 cast 제거 가능
            // Box::new(reducer) as Box<dyn rs_store::Reducer<State, TestAction> + Send + Sync>
            Box::new(reducer),
        )
        .build()
        .unwrap();

    // Subscriber 추가
    store.add_subscriber(subscriber).unwrap();

    // 전체 시간 측정 시작
    let total_start = Instant::now();

    // 여러 액션 dispatch
    for i in 0..num_actions {
        if i % 2 == 0 {
            store.dispatch(TestAction::Increment).unwrap();
        } else {
            store.dispatch(TestAction::Decrement).unwrap();
        }
    }

    // Store 정지 및 모든 액션 처리 완료 대기
    store.stop().unwrap();

    // 전체 시간 측정 종료
    let total_time = total_start.elapsed();

    // 결과 계산
    let reducer_times_vec = reducer_times.lock().unwrap();
    let subscriber_times_vec = subscriber_times.lock().unwrap();

    let reducer_total: std::time::Duration = reducer_times_vec.iter().sum::<std::time::Duration>();
    let subscriber_total: std::time::Duration =
        subscriber_times_vec.iter().sum::<std::time::Duration>();

    let avg_reducer_time = if !reducer_times_vec.is_empty() {
        reducer_total / reducer_times_vec.len() as u32
    } else {
        std::time::Duration::ZERO
    };

    let avg_subscriber_time = if !subscriber_times_vec.is_empty() {
        subscriber_total / subscriber_times_vec.len() as u32
    } else {
        std::time::Duration::ZERO
    };

    let avg_time_per_action = total_time / num_actions as u32;

    // Clone 횟수 추출 (State 타입에 따라 다름)
    let clone_count = get_clone_count(&store.get_state());
    let clone_locations = get_clone_locations(&store.get_state());

    // 결과 출력
    println!("  전체 시간: {:?}", total_time);
    println!("  액션당 평균 시간: {:?}", avg_time_per_action);
    println!("  Reducer 총 시간: {:?}", reducer_total);
    println!("  Reducer 평균 시간: {:?}", avg_reducer_time);
    println!("  Subscriber 총 시간: {:?}", subscriber_total);
    println!("  Subscriber 평균 시간: {:?}", avg_subscriber_time);
    println!("  Reducer 호출 횟수: {}", reducer_times_vec.len());
    println!("  Subscriber 호출 횟수: {}", subscriber_times_vec.len());
    println!("  전체 Clone 횟수: {}", clone_count);
    println!(
        "  평균 Clone 횟수: {:0.2}",
        clone_count as f64 / num_actions as f64
    );
    if !clone_locations.is_empty() {
        println!("  Clone 발생 위치 (처음 5개):");
        for (i, location) in clone_locations.iter().take(5).enumerate() {
            println!("    {}: {}", i + 1, location);
        }
        if clone_locations.len() > 5 {
            println!("    ... (총 {}개 위치)", clone_locations.len());
        }
    }
    println!();
}

// Helper function to get clone count from state
fn get_clone_count(state: &dyn std::any::Any) -> u64 {
    if let Some(small) = state.downcast_ref::<SmallState>() {
        small.get_clone_count()
    } else if let Some(medium) = state.downcast_ref::<MediumState>() {
        medium.get_clone_count()
    } else if let Some(large) = state.downcast_ref::<LargeState>() {
        large.get_clone_count()
    } else if let Some(very_large) = state.downcast_ref::<VeryLargeState>() {
        very_large.get_clone_count()
    } else if let Some(arc_very_large) = state.downcast_ref::<Arc<VeryLargeState>>() {
        arc_very_large.get_clone_count()
    } else {
        0
    }
}

// Helper function to get clone locations from state
fn get_clone_locations(state: &dyn std::any::Any) -> Vec<String> {
    if let Some(small) = state.downcast_ref::<SmallState>() {
        small.get_clone_locations()
    } else if let Some(medium) = state.downcast_ref::<MediumState>() {
        medium.get_clone_locations()
    } else if let Some(large) = state.downcast_ref::<LargeState>() {
        large.get_clone_locations()
    } else if let Some(very_large) = state.downcast_ref::<VeryLargeState>() {
        very_large.get_clone_locations()
    } else if let Some(arc_very_large) = state.downcast_ref::<Arc<VeryLargeState>>() {
        arc_very_large.get_clone_locations()
    } else {
        Vec::new()
    }
}

// Arc<VeryLargeState>를 사용하는 경우의 테스트 함수
fn run_test_with_arc(
    _name: &str,
    initial_state: Arc<VeryLargeState>,
    num_actions: usize,
    reducer: impl rs_store::Reducer<Arc<VeryLargeState>, TestAction> + Send + Sync + 'static,
    subscriber: Arc<dyn rs_store::Subscriber<Arc<VeryLargeState>, TestAction> + Send + Sync>,
    reducer_times: Arc<Mutex<Vec<std::time::Duration>>>,
    subscriber_times: Arc<Mutex<Vec<std::time::Duration>>>,
) {
    // Store 생성
    let store = StoreBuilder::new(initial_state)
        .with_reducer(Box::new(reducer)
            as Box<
                dyn rs_store::Reducer<Arc<VeryLargeState>, TestAction> + Send + Sync,
            >)
        .build()
        .unwrap();

    // Subscriber 추가
    store.add_subscriber(subscriber).unwrap();

    // 전체 시간 측정 시작
    let total_start = Instant::now();

    // 여러 액션 dispatch
    for i in 0..num_actions {
        if i % 2 == 0 {
            store.dispatch(TestAction::Increment).unwrap();
        } else {
            store.dispatch(TestAction::Decrement).unwrap();
        }
    }

    // Store 정지 및 모든 액션 처리 완료 대기
    store.stop().unwrap();

    // 전체 시간 측정 종료
    let total_time = total_start.elapsed();

    // 결과 계산
    let reducer_times_vec = reducer_times.lock().unwrap();
    let subscriber_times_vec = subscriber_times.lock().unwrap();

    let reducer_total: std::time::Duration = reducer_times_vec.iter().sum::<std::time::Duration>();
    let subscriber_total: std::time::Duration =
        subscriber_times_vec.iter().sum::<std::time::Duration>();

    let avg_reducer_time = if !reducer_times_vec.is_empty() {
        reducer_total / reducer_times_vec.len() as u32
    } else {
        std::time::Duration::ZERO
    };

    let avg_subscriber_time = if !subscriber_times_vec.is_empty() {
        subscriber_total / subscriber_times_vec.len() as u32
    } else {
        std::time::Duration::ZERO
    };

    let avg_time_per_action = total_time / num_actions as u32;

    // Clone 횟수 추출 (Arc<VeryLargeState>인 경우)
    let final_state = store.get_state();
    let clone_count = final_state.get_clone_count();
    let clone_locations = final_state.get_clone_locations();

    // 결과 출력
    println!("  전체 시간: {:?}", total_time);
    println!("  액션당 평균 시간: {:?}", avg_time_per_action);
    println!("  Reducer 총 시간: {:?}", reducer_total);
    println!("  Reducer 평균 시간: {:?}", avg_reducer_time);
    println!("  Subscriber 총 시간: {:?}", subscriber_total);
    println!("  Subscriber 평균 시간: {:?}", avg_subscriber_time);
    println!("  Reducer 호출 횟수: {}", reducer_times_vec.len());
    println!("  Subscriber 호출 횟수: {}", subscriber_times_vec.len());
    println!("  전체 Clone 횟수: {}", clone_count);
    println!(
        "  평균 Clone 횟수: {:0.2}",
        clone_count as f64 / num_actions as f64
    );
    println!("  ⚠️  참고: Arc를 사용하면 Clone은 참조 카운트만 증가하므로 실제 데이터 복사가 발생하지 않습니다.");
    if !clone_locations.is_empty() {
        println!("  Clone 발생 위치 (처음 5개):");
        for (i, location) in clone_locations.iter().take(5).enumerate() {
            println!("    {}: {}", i + 1, location);
        }
        if clone_locations.len() > 5 {
            println!("    ... (총 {}개 위치)", clone_locations.len());
        }
    }
    println!();
}
