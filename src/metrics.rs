use crate::StoreError;
use std::fmt::Formatter;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{fmt, sync::atomic::AtomicUsize, time::Duration};

/// StoreMetrics is a trait for metrics that can be used to track the state of the store.
#[allow(dead_code)]
#[allow(unused_variables)]
pub trait StoreMetrics<T>: Send + Sync
where
    T: Send + Sync + 'static,
{
    /// action_received is called when an action is received.
    fn action_received(&self, data: Option<&T>) {}
    /// action_dropped is called when an action is dropped.
    fn action_dropped(&self, data: Option<&T>) {}

    /// middleware_execution_time is called when a middleware is executed,
    /// it includes the time spent of reducing the action and the time spent in the middleware
    fn middleware_executed(&self, data: Option<&T>, middleware_name: &str, duration: Duration) {}

    /// action_reduced is called when an action is reduced.
    fn action_reduced(&self, data: Option<&T>, duration: Duration) {}

    /// effect_issued is called when the number of effects issued.
    fn effect_issued(&self, count: usize) {}

    /// effect_executed is called when an effect is executed.
    fn effect_executed(&self, count: usize, duration: Duration) {}

    /// subscriber_notified is called when a subscriber is notified.
    fn subscriber_notified(&self, data: Option<&T>, count: usize, duration: Duration) {}

    /// queue_size is called when the remaining queue is changed.
    fn queue_size(&self, current_size: usize) {}

    /// error_occurred is called when an error occurs.
    fn error_occurred(&self, error: &StoreError) {}
}

/// NoOpMetrics is a no-op implementation of StoreMetrics.
pub struct NoOpMetrics;

impl<T> StoreMetrics<T> for NoOpMetrics where T: Send + Sync + Clone + 'static {}

pub struct CountMetrics {
    /// total number of actions received
    pub action_received: AtomicUsize,
    /// total number of actions dropped
    pub action_dropped: AtomicUsize,
    /// total number of actions reduced
    pub action_reduced: AtomicUsize,
    /// total number of effects issued
    pub effect_issued: AtomicUsize,
    // total number of effects executed
    //pub effect_executed: AtomicUsize,
    /// max time spent in reducers
    pub reducer_time_max: AtomicUsize,
    /// min time spent in reducers
    pub reducer_time_min: AtomicUsize,
    /// total time spent in reducers
    pub reducer_execution_time: AtomicUsize,
    /// max time spent in middleware
    pub middleware_time_max: AtomicUsize,
    /// min time spent in middleware
    pub middleware_time_min: AtomicUsize,
    /// total time spent in middleware
    pub middleware_execution_time: AtomicUsize,
    /// total number of subscribers notified
    pub subscriber_notified: AtomicUsize,
    /// max time spent in subscribers
    pub subscriber_time_max: AtomicUsize,
    /// min time spent in subscribers
    pub subscriber_time_min: AtomicUsize,
    /// total time spent in subscribers
    pub subscriber_execution_time: AtomicUsize,

    /// remaining number of actions in the queue
    pub remaining_queue: AtomicUsize,
    /// max number of remaining actions in the queue
    pub remaining_queue_max: AtomicUsize,
    //pub remaining_queue_min: AtomicUsize,
    /// total number of errors occurred
    pub error_occurred: AtomicUsize,
}

impl Default for CountMetrics {
    fn default() -> Self {
        Self {
            action_received: AtomicUsize::new(0),
            action_dropped: AtomicUsize::new(0),
            action_reduced: AtomicUsize::new(0),
            effect_issued: AtomicUsize::new(0),
            //effect_executed: AtomicUsize::new(0),
            reducer_time_max: AtomicUsize::new(0),
            reducer_time_min: AtomicUsize::new(usize::MAX),
            reducer_execution_time: AtomicUsize::new(0),
            middleware_time_max: AtomicUsize::new(0),
            middleware_time_min: AtomicUsize::new(usize::MAX),
            middleware_execution_time: AtomicUsize::new(0),
            subscriber_notified: AtomicUsize::new(0),
            subscriber_time_max: AtomicUsize::new(0),
            subscriber_time_min: AtomicUsize::new(usize::MAX),
            subscriber_execution_time: AtomicUsize::new(0),
            remaining_queue: AtomicUsize::new(0),
            remaining_queue_max: AtomicUsize::new(0),
            //remaining_queue_min: AtomicUsize::new(0),
            error_occurred: AtomicUsize::new(0),
        }
    }
}

impl fmt::Display for CountMetrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "action_received: {:?}",
            self.action_received.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", action_dropped: {:?}",
            self.action_dropped.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", action_reduced: {:?}",
            self.action_reduced.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", reducer_time_max: {:?}",
            self.reducer_time_max.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", reducer_time_min: {:?}",
            self.reducer_time_min.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", reducer_execution_time: {:?}",
            self.reducer_execution_time.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", middleware_time_max: {:?}",
            self.middleware_time_max.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", middleware_time_min: {:?}",
            self.middleware_time_min.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", middleware_execution_time: {:?}",
            self.middleware_execution_time.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", subscriber_notified: {:?}",
            self.subscriber_notified.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", subscriber_time_max: {:?}",
            self.subscriber_time_max.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", subscriber_time_min: {:?}",
            self.subscriber_time_min.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", subscriber_execution_time: {:?}",
            self.subscriber_execution_time.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", remaining_queue: {:?}",
            self.remaining_queue.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", remaining_queue_max: {:?}",
            self.remaining_queue_max.load(Ordering::SeqCst)
        )?;
        write!(
            f,
            ", error_occurred: {:?}",
            self.error_occurred.load(Ordering::SeqCst)
        )?;
        Ok(())
    }
}

#[allow(dead_code)]
impl CountMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn reset(&self) {
        self.action_received.store(0, Ordering::SeqCst);
        self.action_dropped.store(0, Ordering::SeqCst);
        self.action_reduced.store(0, Ordering::SeqCst);
        self.effect_issued.store(0, Ordering::SeqCst);
        self.reducer_time_max.store(0, Ordering::SeqCst);
        self.reducer_time_min.store(0, Ordering::SeqCst);
        self.reducer_execution_time.store(0, Ordering::SeqCst);
        self.middleware_time_max.store(0, Ordering::SeqCst);
        self.middleware_time_min.store(0, Ordering::SeqCst);
        self.middleware_execution_time.store(0, Ordering::SeqCst);
        self.subscriber_notified.store(0, Ordering::SeqCst);
        self.subscriber_time_max.store(0, Ordering::SeqCst);
        self.subscriber_time_min.store(0, Ordering::SeqCst);
        self.subscriber_execution_time.store(0, Ordering::SeqCst);
        self.remaining_queue.store(0, Ordering::SeqCst);
        self.remaining_queue_max.store(0, Ordering::SeqCst);
        self.error_occurred.store(0, Ordering::SeqCst);
    }
}

#[allow(unused_variables)]
impl<T> StoreMetrics<T> for CountMetrics
where
    T: Send + Sync + Clone + 'static,
{
    fn action_received(&self, data: Option<&T>) {
        self.action_received.fetch_add(1, Ordering::SeqCst);
    }
    fn action_dropped(&self, data: Option<&T>) {
        self.action_dropped.fetch_add(1, Ordering::SeqCst);
    }

    fn middleware_executed(&self, data: Option<&T>, _middleware_name: &str, duration: Duration) {
        let duration_ms = duration.as_millis() as usize;
        if duration_ms > self.middleware_time_max.load(Ordering::SeqCst) {
            self.middleware_time_max.store(duration_ms, Ordering::SeqCst);
        }
        if duration_ms < self.middleware_time_min.load(Ordering::SeqCst) {
            self.middleware_time_min.store(duration_ms, Ordering::SeqCst);
        }
        self.middleware_execution_time.fetch_add(duration_ms, Ordering::SeqCst);
    }

    fn action_reduced(&self, data: Option<&T>, duration: Duration) {
        self.action_reduced.fetch_add(1, Ordering::SeqCst);
        let duration_ms = duration.as_millis() as usize;
        if duration_ms > self.reducer_time_max.load(Ordering::SeqCst) {
            self.reducer_time_max.store(duration_ms, Ordering::SeqCst);
        }
        if duration_ms < self.reducer_time_min.load(Ordering::SeqCst) {
            self.reducer_time_min.store(duration_ms, Ordering::SeqCst);
        }
        self.reducer_execution_time.fetch_add(duration_ms, Ordering::SeqCst);
    }

    fn effect_issued(&self, count: usize) {
        self.effect_issued.fetch_add(count, Ordering::SeqCst);
    }

    fn effect_executed(&self, _count: usize, _duration: Duration) {
        //self.effect_executed.fetch_add(1, Ordering::SeqCst);
    }

    fn subscriber_notified(&self, data: Option<&T>, _count: usize, duration: Duration) {
        self.subscriber_notified.fetch_add(1, Ordering::SeqCst);
        let duration_ms = duration.as_millis() as usize;
        if duration_ms > self.subscriber_time_max.load(Ordering::SeqCst) {
            self.subscriber_time_max.store(duration_ms, Ordering::SeqCst);
        }
        if duration_ms < self.subscriber_time_min.load(Ordering::SeqCst) {
            self.subscriber_time_min.store(duration_ms, Ordering::SeqCst);
        }
        self.subscriber_execution_time.fetch_add(duration_ms, Ordering::SeqCst);
    }

    fn queue_size(&self, current_size: usize) {
        self.remaining_queue.store(current_size, Ordering::SeqCst);
        if current_size > self.remaining_queue_max.load(Ordering::SeqCst) {
            self.remaining_queue_max.store(current_size, Ordering::SeqCst);
        }
        // if current_size < self.remaining_queue_min.load(Ordering::SeqCst) {
        //     self.remaining_queue_min.store(current_size, Ordering::SeqCst);
        // }
    }

    fn error_occurred(&self, error: &StoreError) {
        self.error_occurred.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BackpressurePolicy, DispatchOp, Dispatcher, Effect, Middleware, MiddlewareOp, Reducer,
        StoreBuilder,
    };
    use std::sync::{Arc, Mutex};
    use std::thread;

    struct TestReducer;
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = state + action;
            thread::sleep(Duration::from_millis(10)); // Add delay to test timing
            DispatchOp::Dispatch(new_state, None)
        }
    }

    struct TestMiddleware {
        #[allow(dead_code)]
        name: String,
    }

    impl TestMiddleware {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl<State, Action> Middleware<State, Action> for TestMiddleware
    where
        State: Send + Sync + 'static,
        Action: Send + Sync + Clone + 'static,
    {
        fn before_reduce(
            &mut self,
            _action: &Action,
            _state: &State,
            _dispatcher: Arc<dyn Dispatcher<Action>>,
        ) -> Result<MiddlewareOp, StoreError> {
            thread::sleep(Duration::from_millis(10)); // Add delay to test timing
            Ok(MiddlewareOp::ContinueAction)
        }

        fn after_reduce(
            &mut self,
            _action: &Action,
            _old_state: &State,
            _new_state: &State,
            _effects: &mut Vec<Effect<Action>>,
            _dispatcher: Arc<dyn Dispatcher<Action>>,
        ) -> Result<MiddlewareOp, StoreError> {
            thread::sleep(Duration::from_millis(10)); // Add delay to test timing
            Ok(MiddlewareOp::ContinueAction)
        }
    }

    #[test]
    fn test_count_metrics_basic() {
        // let metrics: Arc<Box<dyn StoreMetrics<i32, i32> + Send + Sync>> =
        //     Arc::new(Box::new(CountMetrics::default()));
        let metrics = CountMetrics::new();
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(5)
            .with_policy(BackpressurePolicy::DropOldest)
            .with_metrics(metrics.clone())
            .build()
            .unwrap();

        // when
        // Test multiple actions
        store.dispatch(1);
        store.dispatch(2);
        store.dispatch(3);

        // Action::Exit는 Store에서 처리하지 않음
        store.stop();

        println!("Metrics: {}", metrics);

        // then
        assert_eq!(metrics.action_received.load(Ordering::SeqCst), 3);
        assert_eq!(metrics.action_reduced.load(Ordering::SeqCst), 3);
        assert!(metrics.reducer_execution_time.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn test_count_metrics_with_dropped_actions() {
        // given
        let metrics = CountMetrics::new();
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(2)
            .with_policy(BackpressurePolicy::DropOldest)
            .with_metrics(metrics.clone())
            .build()
            .unwrap();

        // when
        // Dispatch more actions than capacity
        for i in 0..5 {
            store.dispatch(i);
        }
        store.stop();

        // then
        // actions should be dropped
        assert!(metrics.action_dropped.load(Ordering::SeqCst) > 0);
        // Remaining queue should be less than or equal to capacity
        assert!(metrics.remaining_queue_max.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn test_count_metrics_with_middleware() {
        // given
        let metrics = CountMetrics::new();
        let middleware = Arc::new(Mutex::new(TestMiddleware::new("test")));
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(5)
            .with_middleware(middleware)
            .with_metrics(metrics.clone())
            .build()
            .unwrap();

        // when
        store.dispatch(1);
        store.stop();

        assert_eq!(metrics.action_received.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.action_reduced.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.subscriber_notified.load(Ordering::SeqCst), 1);
        // Middleware should be executed
        assert!(metrics.middleware_execution_time.load(Ordering::SeqCst) > 0);
        assert!(metrics.reducer_execution_time.load(Ordering::SeqCst) > 0);
        // Middleware should take longer than reducer
        assert!(
            metrics.middleware_execution_time.load(Ordering::SeqCst)
                > metrics.reducer_execution_time.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn test_count_metrics_reset() {
        let metrics: CountMetrics = CountMetrics::default();

        // Add some counts
        metrics.action_received.fetch_add(5, Ordering::SeqCst);
        metrics.action_reduced.fetch_add(3, Ordering::SeqCst);
        metrics.subscriber_notified.fetch_add(2, Ordering::SeqCst);

        // Reset
        metrics.reset();

        // Verify all counters are zero
        assert_eq!(metrics.action_received.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.action_reduced.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.subscriber_notified.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.middleware_execution_time.load(Ordering::SeqCst), 0);
        assert_eq!(metrics.reducer_execution_time.load(Ordering::SeqCst), 0);
    }
}
