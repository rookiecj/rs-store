use crate::StoreError;
use std::any::Any;
use std::fmt::Formatter;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{fmt, sync::atomic::AtomicUsize, time::Duration};

/// Metrics is a trait for metrics that can be used to track the state of the store.
#[allow(dead_code)]
#[allow(unused_variables)]
pub(crate) trait Metrics: Send + Sync {
    /// action_received is called when an action is received including Exit.
    /// data is the ActionOp that is received.
    fn action_received(&self, data: Option<&dyn Any>) {}

    /// action_dropped is called when an action is dropped.
    fn action_dropped(&self, data: Option<&dyn Any>) {}

    /// middleware_execution_time is called when a middleware is executed,
    /// it includes the time spent of reducing the action and the time spent in the middleware
    fn middleware_executed(
        &self,
        data: Option<&dyn Any>,
        middleware_name: &str,
        count: usize,
        duration: Duration,
    ) {
    }

    /// action_reduced is called when an action is reduced.
    fn action_reduced(&self, data: Option<&dyn Any>, duration: Duration) {}

    /// effect_issued is called when the number of effects issued.
    fn effect_issued(&self, count: usize) {}

    /// effect_executed is called when an effect is executed.
    fn effect_executed(&self, count: usize, duration: Duration) {}

    /// state_notified is called when the state is notified.
    fn state_notified(&self, data: Option<&dyn Any>) {}

    /// subscriber_notified is called when after all subscribers are notified even if there are no subscribers.
    fn subscriber_notified(&self, data: Option<&dyn Any>, count: usize, duration: Duration) {}

    /// queue_size is called when the remaining queue is changed.
    fn queue_size(&self, current_size: usize) {}

    /// error_occurred is called when an error occurs.
    fn error_occurred(&self, error: &StoreError) {}
}

pub(crate) struct CountMetrics {
    /// total number of actions received
    pub action_received: AtomicUsize,
    /// total number of actions dropped
    pub action_dropped: AtomicUsize,
    /// total number of actions reduced
    pub action_reduced: AtomicUsize,
    /// total number of effects issued
    pub effect_issued: AtomicUsize,
    // total number of effects executed
    pub effect_executed: AtomicUsize,
    /// max time spent in reducers
    pub reducer_time_max: AtomicUsize,
    /// min time spent in reducers
    pub reducer_time_min: AtomicUsize,
    /// total time spent in reducers
    pub reducer_execution_time: AtomicUsize,
    /// total number of middleware executed
    pub middleware_executed: AtomicUsize,
    /// max time spent in middleware
    pub middleware_time_max: AtomicUsize,
    /// min time spent in middleware
    pub middleware_time_min: AtomicUsize,
    /// total time spent in middleware
    pub middleware_execution_time: AtomicUsize,
    /// total number of states notified
    pub state_notified: AtomicUsize,
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
            effect_executed: AtomicUsize::new(0),
            reducer_time_max: AtomicUsize::new(0),
            reducer_time_min: AtomicUsize::new(usize::MAX),
            reducer_execution_time: AtomicUsize::new(0),
            middleware_executed: AtomicUsize::new(0),
            middleware_time_max: AtomicUsize::new(0),
            middleware_time_min: AtomicUsize::new(usize::MAX),
            middleware_execution_time: AtomicUsize::new(0),
            state_notified: Default::default(),
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
            ", middleware_executed: {:?}",
            self.middleware_executed.load(Ordering::SeqCst)
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
            ", state_notified: {:?}",
            self.state_notified.load(Ordering::SeqCst)
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

#[allow(unused_variables)]
impl Metrics for CountMetrics {
    fn action_received(&self, data: Option<&dyn Any>) {
        self.action_received.fetch_add(1, Ordering::SeqCst);
    }
    fn action_dropped(&self, data: Option<&dyn Any>) {
        self.action_dropped.fetch_add(1, Ordering::SeqCst);
    }

    fn middleware_executed(
        &self,
        data: Option<&dyn Any>,
        _middleware_name: &str,
        count: usize,
        duration: Duration,
    ) {
        self.middleware_executed.fetch_add(count, Ordering::SeqCst);
        let duration_ms = duration.as_millis() as usize;
        if duration_ms > self.middleware_time_max.load(Ordering::SeqCst) {
            self.middleware_time_max.store(duration_ms, Ordering::SeqCst);
        }
        if duration_ms < self.middleware_time_min.load(Ordering::SeqCst) {
            self.middleware_time_min.store(duration_ms, Ordering::SeqCst);
        }
        self.middleware_execution_time.fetch_add(duration_ms, Ordering::SeqCst);
    }

    fn action_reduced(&self, data: Option<&dyn Any>, duration: Duration) {
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
        self.effect_executed.fetch_add(1, Ordering::SeqCst);
    }

    fn state_notified(&self, data: Option<&dyn Any>) {
        self.state_notified.fetch_add(1, Ordering::SeqCst);
    }

    fn subscriber_notified(&self, data: Option<&dyn Any>, count: usize, duration: Duration) {
        self.subscriber_notified.fetch_add(count, Ordering::SeqCst);
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
        self.effect_executed.store(0, Ordering::SeqCst);
        self.reducer_time_max.store(0, Ordering::SeqCst);
        self.reducer_time_min.store(0, Ordering::SeqCst);
        self.reducer_execution_time.store(0, Ordering::SeqCst);
        self.middleware_executed.store(0, Ordering::SeqCst);
        self.middleware_time_max.store(0, Ordering::SeqCst);
        self.middleware_time_min.store(0, Ordering::SeqCst);
        self.middleware_execution_time.store(0, Ordering::SeqCst);
        self.state_notified.store(0, Ordering::SeqCst);
        self.subscriber_notified.store(0, Ordering::SeqCst);
        self.subscriber_time_max.store(0, Ordering::SeqCst);
        self.subscriber_time_min.store(0, Ordering::SeqCst);
        self.subscriber_execution_time.store(0, Ordering::SeqCst);
        self.remaining_queue.store(0, Ordering::SeqCst);
        self.remaining_queue_max.store(0, Ordering::SeqCst);
        self.error_occurred.store(0, Ordering::SeqCst);
    }
}

/// MetricsSnapshot is a snapshot of the metrics.
#[allow(dead_code)]
pub struct MetricsSnapshot {
    /// total number of actions received
    pub action_received: usize,
    /// total number of actions dropped
    pub action_dropped: usize,
    /// total number of actions reduced
    pub action_reduced: usize,
    /// total number of effects issued
    pub effect_issued: usize,
    // total number of effects executed
    pub(crate) effect_executed: usize,
    /// max time spent in reducers
    pub reducer_time_max: usize,
    /// min time spent in reducers
    pub reducer_time_min: usize,
    /// total time spent in reducers
    pub reducer_execution_time: usize,
    /// total number of middleware executed
    pub middleware_executed: usize,
    /// max time spent in middleware
    pub middleware_time_max: usize,
    /// min time spent in middleware
    pub middleware_time_min: usize,
    /// total time spent in middleware
    pub middleware_execution_time: usize,
    /// total number of states notified
    pub state_notified: usize,
    /// total number of subscribers notified
    pub subscriber_notified: usize,
    /// max time spent in subscribers
    pub subscriber_time_max: usize,
    /// min time spent in subscribers
    pub subscriber_time_min: usize,
    /// total time spent in subscribers
    pub subscriber_execution_time: usize,

    // remaining number of actions in the queue
    pub(crate) remaining_queue: usize,
    // max number of remaining actions in the queue
    pub(crate) remaining_queue_max: usize,
    //pub remaining_queue_min: usize,
    /// total number of errors occurred
    pub error_occurred: usize,
}

impl From<&CountMetrics> for MetricsSnapshot {
    fn from(value: &CountMetrics) -> Self {
        Self {
            action_received: value.action_received.load(Ordering::SeqCst),
            action_dropped: value.action_dropped.load(Ordering::SeqCst),
            action_reduced: value.action_reduced.load(Ordering::SeqCst),
            effect_issued: value.effect_issued.load(Ordering::SeqCst),
            effect_executed: value.effect_executed.load(Ordering::SeqCst),
            reducer_time_max: value.reducer_time_max.load(Ordering::SeqCst),
            reducer_time_min: value.reducer_time_min.load(Ordering::SeqCst),
            reducer_execution_time: value.reducer_execution_time.load(Ordering::SeqCst),
            middleware_executed: value.middleware_executed.load(Ordering::SeqCst),
            middleware_time_max: value.middleware_time_max.load(Ordering::SeqCst),
            middleware_time_min: value.middleware_time_min.load(Ordering::SeqCst),
            middleware_execution_time: value.middleware_execution_time.load(Ordering::SeqCst),
            state_notified: value.state_notified.load(Ordering::SeqCst),
            subscriber_notified: value.subscriber_notified.load(Ordering::SeqCst),
            subscriber_time_max: value.subscriber_time_max.load(Ordering::SeqCst),
            subscriber_time_min: value.subscriber_time_min.load(Ordering::SeqCst),
            subscriber_execution_time: value.subscriber_execution_time.load(Ordering::SeqCst),
            remaining_queue: value.remaining_queue.load(Ordering::SeqCst),
            remaining_queue_max: value.remaining_queue_max.load(Ordering::SeqCst),
            error_occurred: value.error_occurred.load(Ordering::SeqCst),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BackpressurePolicy, DispatchOp, Dispatcher, Effect, Middleware, MiddlewareOp, Reducer,
        StoreBuilder,
    };
    use std::sync::Arc;
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
            &self,
            _action: &Action,
            _state: &State,
            _dispatcher: Arc<dyn Dispatcher<Action>>,
        ) -> Result<MiddlewareOp, StoreError> {
            thread::sleep(Duration::from_millis(10)); // Add delay to test timing
            Ok(MiddlewareOp::ContinueAction)
        }

        fn before_effect(
            &self,
            _action: &Action,
            _state: &State,
            _effects: &mut Vec<Effect<Action>>,
            _dispatcher: Arc<dyn Dispatcher<Action>>,
        ) -> Result<MiddlewareOp, StoreError> {
            thread::sleep(Duration::from_millis(10)); // Add delay to test timing
            Ok(MiddlewareOp::ContinueAction)
        }
    }

    #[test]
    fn test_count_metrics_basic() {
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(5)
            .with_policy(BackpressurePolicy::DropOldest)
            .build()
            .unwrap();

        // when
        // Test multiple actions
        store.dispatch(1);
        store.dispatch(2);
        store.dispatch(3);

        // +1 : ActionExit
        store.stop();

        // then
        let metrics = store.get_metrics();
        // +1 for ActionExit
        assert_eq!(metrics.action_received, 3 + 1);
        assert_eq!(metrics.action_reduced, 3);
        assert!(metrics.reducer_execution_time > 0);
    }

    #[test]
    fn test_count_metrics_with_dropped_actions() {
        // given
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(2)
            .with_policy(BackpressurePolicy::DropOldest)
            .build()
            .unwrap();

        // when
        // Dispatch more actions than capacity
        for i in 0..5 {
            store.dispatch(i);
        }
        store.stop();

        // then
        let metrics = store.get_metrics();
        // actions should be dropped
        assert!(metrics.action_dropped > 0);
        // Remaining queue should be less than or equal to capacity
        assert!(metrics.remaining_queue_max <= 2);
    }

    #[test]
    fn test_count_metrics_with_middleware() {
        // given
        let middleware = Arc::new(TestMiddleware::new("test"));
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_capacity(5)
            .with_middleware(middleware)
            .build()
            .unwrap();

        // when
        store.dispatch(1);
        store.stop();

        // +1 for ActionExit
        let metrics = store.get_metrics();
        assert_eq!(metrics.action_received, 1 + 1);
        assert_eq!(metrics.action_reduced, 1);
        assert_eq!(metrics.state_notified, 1);
        // no subscribers
        assert_eq!(metrics.subscriber_notified, 0);
        // Middleware should be executed
        assert!(metrics.middleware_execution_time > 0);
        assert!(metrics.reducer_execution_time > 0);
        // Middleware should take longer than reducer
        assert!(metrics.middleware_execution_time > metrics.reducer_execution_time);
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
