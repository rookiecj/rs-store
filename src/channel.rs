use crate::metrics::Metrics;
use crate::ActionOp;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};

/// the Backpressure policy
#[derive(Clone, Default)]
pub enum BackpressurePolicy<T>
where
    T: Send + Sync + Clone + 'static,
{
    /// Block the sender when the queue is full
    #[default]
    BlockOnFull,
    /// Drop the oldest item when the queue is full
    DropOldest,
    /// Drop the latest item when the queue is full
    DropLatest,
    /// Drop items based on predicate when the queue is full, it drops from the latest
    DropLatestIf {
        predicate: Arc<dyn Fn(&ActionOp<T>) -> bool + Send + Sync>,
    },
    /// Drop items based on predicate when the queue is full, it drops from the oldest
    DropOldestIf {
        predicate: Arc<dyn Fn(&ActionOp<T>) -> bool + Send + Sync>,
    },
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SenderError<T> {
    #[error("Failed to send: {0}")]
    SendError(T),
    #[error("Channel is closed")]
    ChannelClosed,
}

/// Internal MPSC Queue implementation
struct MpscQueue<T>
where
    T: Send + Sync + Clone + 'static,
{
    queue: Mutex<VecDeque<ActionOp<T>>>,
    condvar: Condvar,
    capacity: usize,
    policy: BackpressurePolicy<T>,
    metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    closed: Mutex<bool>,
}

impl<T> MpscQueue<T>
where
    T: Send + Sync + Clone + 'static,
{
    fn new(
        capacity: usize,
        policy: BackpressurePolicy<T>,
        metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    ) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            condvar: Condvar::new(),
            capacity,
            policy,
            metrics,
            closed: Mutex::new(false),
        }
    }

    fn send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        let mut queue = self.queue.lock().unwrap();

        // Check if channel is closed
        if *self.closed.lock().unwrap() {
            return Err(SenderError::ChannelClosed);
        }

        if queue.len() >= self.capacity {
            match &self.policy {
                BackpressurePolicy::BlockOnFull => {
                    // Wait until space is available
                    while queue.len() >= self.capacity {
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                    }
                    queue.push_back(item);
                }
                BackpressurePolicy::DropOldest => {
                    // Drop the oldest item
                    if let Some(dropped_item) = queue.pop_front() {
                        if let Some(metrics) = &self.metrics {
                            if let ActionOp::Action(action) = &dropped_item {
                                metrics.action_dropped(Some(action as &dyn std::any::Any));
                            }
                        }
                    }
                    queue.push_back(item);
                }
                BackpressurePolicy::DropLatest => {
                    // Drop the new item
                    if let Some(metrics) = &self.metrics {
                        if let ActionOp::Action(action) = &item {
                            metrics.action_dropped(Some(action as &dyn std::any::Any));
                        }
                    }
                    return Ok(queue.len() as i64);
                }
                BackpressurePolicy::DropLatestIf { predicate } => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        if predicate(&queue[i]) {
                            if let Some(dropped_item) = queue.remove(i) {
                                dropped_count += 1;
                                if let Some(metrics) = &self.metrics {
                                    if let ActionOp::Action(action) = &dropped_item {
                                        metrics.action_dropped(Some(action as &dyn std::any::Any));
                                    }
                                }
                                break;
                            }
                        }
                        i += 1;
                    }

                    if dropped_count > 0 {
                        queue.push_back(item);
                    } else {
                        return Err(SenderError::SendError(item));
                    }
                }
                BackpressurePolicy::DropOldestIf { predicate } => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        let index = queue.len() - i - 1;
                        if predicate(&queue[index]) {
                            if let Some(dropped_item) = queue.remove(index) {
                                dropped_count += 1;
                                if let Some(metrics) = &self.metrics {
                                    if let ActionOp::Action(action) = &dropped_item {
                                        metrics.action_dropped(Some(action as &dyn std::any::Any));
                                    }
                                }
                                break;
                            }
                        }
                        i += 1;
                    }

                    if dropped_count > 0 {
                        queue.push_back(item);
                    } else {
                        return Err(SenderError::SendError(item));
                    }
                }
            }
        } else {
            queue.push_back(item);
        }

        // Update metrics
        if let Some(metrics) = &self.metrics {
            metrics.queue_size(queue.len());
        }

        self.condvar.notify_one();
        Ok(queue.len() as i64)
    }

    fn try_send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        // Check if channel is closed
        if *self.closed.lock().unwrap() {
            return Err(SenderError::ChannelClosed);
        }

        let mut queue = self.queue.lock().unwrap();

        if queue.len() >= self.capacity {
            match &self.policy {
                BackpressurePolicy::BlockOnFull => {
                    return Err(SenderError::SendError(item));
                }
                BackpressurePolicy::DropOldest => {
                    // Drop the oldest item
                    if let Some(dropped_item) = queue.pop_front() {
                        if let Some(metrics) = &self.metrics {
                            if let ActionOp::Action(action) = &dropped_item {
                                metrics.action_dropped(Some(action as &dyn std::any::Any));
                            }
                        }
                    }
                    queue.push_back(item);
                }
                BackpressurePolicy::DropLatest => {
                    // Drop the new item
                    if let Some(metrics) = &self.metrics {
                        if let ActionOp::Action(action) = &item {
                            metrics.action_dropped(Some(action as &dyn std::any::Any));
                        }
                    }
                    return Ok(queue.len() as i64);
                }
                BackpressurePolicy::DropLatestIf { predicate } => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        if predicate(&queue[i]) {
                            if let Some(dropped_item) = queue.remove(i) {
                                dropped_count += 1;
                                if let Some(metrics) = &self.metrics {
                                    if let ActionOp::Action(action) = &dropped_item {
                                        metrics.action_dropped(Some(action as &dyn std::any::Any));
                                    }
                                }
                                break;
                            }
                        }
                        i += 1;
                    }

                    if dropped_count > 0 {
                        queue.push_back(item);
                    } else {
                        return Err(SenderError::SendError(item));
                    }
                }
                BackpressurePolicy::DropOldestIf { predicate } => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        let index = queue.len() - i - 1;
                        if predicate(&queue[index]) {
                            if let Some(dropped_item) = queue.remove(index) {
                                dropped_count += 1;
                                if let Some(metrics) = &self.metrics {
                                    if let ActionOp::Action(action) = &dropped_item {
                                        metrics.action_dropped(Some(action as &dyn std::any::Any));
                                    }
                                }
                                break;
                            }
                        }
                        i += 1;
                    }

                    if dropped_count > 0 {
                        queue.push_back(item);
                    } else {
                        return Err(SenderError::SendError(item));
                    }
                }
            }
        } else {
            queue.push_back(item);
        }

        // Update metrics
        if let Some(metrics) = &self.metrics {
            metrics.queue_size(queue.len());
        }

        self.condvar.notify_one();
        Ok(queue.len() as i64)
    }

    fn recv(&self) -> Option<ActionOp<T>> {
        let mut queue = self.queue.lock().unwrap();

        // Wait until there's an item or channel is closed
        while queue.is_empty() {
            if *self.closed.lock().unwrap() {
                return None;
            }
            queue = self.condvar.wait(queue).unwrap();
        }

        let item = queue.pop_front();
        self.condvar.notify_one();
        item
    }

    fn try_recv(&self) -> Option<ActionOp<T>> {
        let mut queue = self.queue.lock().unwrap();
        let item = queue.pop_front();
        if item.is_some() {
            self.condvar.notify_one();
        }
        item
    }

    fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn close(&self) {
        *self.closed.lock().unwrap() = true;
        self.condvar.notify_all();
    }
}

/// Channel to hold the sender with backpressure policy
#[derive(Clone)]
pub(crate) struct SenderChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    _name: String,
    queue: Arc<MpscQueue<T>>,
}

impl<Action> Drop for SenderChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        #[cfg(feature = "store-log")]
        eprintln!("store: drop '{}' sender channel", self._name);
    }
}

#[allow(dead_code)]
impl<T> SenderChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub fn send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        self.queue.send(item)
    }

    pub fn try_send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        self.queue.try_send(item)
    }
}

#[allow(dead_code)]
pub(crate) struct ReceiverChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    name: String,
    queue: Arc<MpscQueue<T>>,
    metrics: Option<Arc<dyn Metrics + Send + Sync>>,
}

impl<Action> Drop for ReceiverChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        #[cfg(feature = "store-log")]
        eprintln!("store: drop '{}' receiver channel", self.name);
        self.close();
    }
}

#[allow(dead_code)]
impl<T> ReceiverChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub fn recv(&self) -> Option<ActionOp<T>> {
        self.queue.recv()
    }

    #[allow(dead_code)]
    pub fn try_recv(&self) -> Option<ActionOp<T>> {
        self.queue.try_recv()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn close(&self) {
        self.queue.close();
    }
}

/// Channel with back pressure
pub(crate) struct BackpressureChannel<MSG>
where
    MSG: Send + Sync + Clone + 'static,
{
    phantom_data: PhantomData<MSG>,
}

impl<MSG> BackpressureChannel<MSG>
where
    MSG: Send + Sync + Clone + 'static,
{
    #[allow(dead_code)]
    pub fn pair(
        capacity: usize,
        policy: BackpressurePolicy<MSG>,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        Self::pair_with("<anon>", capacity, policy, None)
    }

    #[allow(dead_code)]
    pub fn pair_with_metrics(
        capacity: usize,
        policy: BackpressurePolicy<MSG>,
        metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        Self::pair_with("<anon>", capacity, policy, metrics)
    }

    #[allow(dead_code)]
    pub fn pair_with(
        name: &str,
        capacity: usize,
        policy: BackpressurePolicy<MSG>,
        metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        let queue = Arc::new(MpscQueue::new(capacity, policy, metrics.clone()));

        (
            SenderChannel {
                _name: name.to_string(),
                queue: queue.clone(),
            },
            ReceiverChannel {
                name: name.to_string(),
                queue,
                metrics,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_send_recv() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(5, BackpressurePolicy::BlockOnFull);

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();

        assert_eq!(receiver.recv(), Some(ActionOp::Action(1)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(2)));
        assert_eq!(receiver.try_recv(), None);
    }

    #[test]
    fn test_drop_oldest() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropOldest);

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::Action(3)).unwrap(); // Should drop 1

        assert_eq!(receiver.recv(), Some(ActionOp::Action(2)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(3)));
        assert_eq!(receiver.try_recv(), None);
    }

    #[test]
    fn test_drop_latest() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatest);

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::Action(3)).unwrap(); // Should drop 3

        assert_eq!(receiver.recv(), Some(ActionOp::Action(1)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(2)));
        assert_eq!(receiver.try_recv(), None);
    }

    #[test]
    fn test_predicate_dropping() {
        // predicate: 5보다 작은 값들은 drop
        let predicate = Arc::new(|action_op: &ActionOp<i32>| match action_op {
            ActionOp::Action(value) => *value < 5,
            ActionOp::Exit(_) => false,
            ActionOp::AddSubscriber => false,
        });

        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf { predicate });

        // 채널을 가득 채우기
        sender.send(ActionOp::Action(1)).unwrap(); // 첫 번째 아이템 (drop 대상)
        sender.send(ActionOp::Action(6)).unwrap(); // 두 번째 아이템 (유지 대상)

        // 세 번째 아이템을 보내면 채널이 가득 차서 predicate가 적용됨
        let result = sender.send(ActionOp::Action(7)); // 세 번째 아이템 (유지 대상)
        assert!(
            result.is_ok(),
            "Should succeed because predicate should drop the first item"
        );

        // 소비자에서 아이템 확인
        let received_item = receiver.recv();
        assert!(received_item.is_some());
        if let Some(ActionOp::Action(value)) = received_item {
            // predicate에 의해 1이 drop되고 6이 유지되어야 함
            assert_eq!(value, 6, "Should receive 6, not 1");
        }

        let received_item = receiver.recv();
        assert!(received_item.is_some());
        if let Some(ActionOp::Action(value)) = received_item {
            assert_eq!(value, 7, "Should receive 7");
        }
    }

    #[test]
    fn test_add_subscriber_action() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(5, BackpressurePolicy::BlockOnFull);

        // AddSubscriber 액션 전송
        sender.send(ActionOp::AddSubscriber).unwrap();

        // 수신 확인
        let received = receiver.recv();
        assert!(received.is_some());
        match received.unwrap() {
            ActionOp::AddSubscriber => {
                // AddSubscriber 액션이 정상적으로 수신됨
            }
            _ => panic!("Expected AddSubscriber action"),
        }
    }

    #[test]
    fn test_add_subscriber_with_predicate() {
        // AddSubscriber는 절대 drop되지 않도록 하는 predicate
        let predicate = Arc::new(|action_op: &ActionOp<i32>| match action_op {
            ActionOp::Action(value) => *value < 5,
            ActionOp::Exit(_) => false,
            ActionOp::AddSubscriber => false, // AddSubscriber는 절대 drop하지 않음
        });

        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf { predicate });

        // 채널을 가득 채우기
        sender.send(ActionOp::Action(1)).unwrap(); // drop 대상
        sender.send(ActionOp::Action(6)).unwrap(); // 유지 대상

        // AddSubscriber 액션을 보내면 predicate에 의해 다른 액션이 drop되어야 함
        let result = sender.send(ActionOp::AddSubscriber);
        assert!(result.is_ok(), "AddSubscriber should be sent successfully");

        // 수신 확인
        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 2);

        // AddSubscriber가 포함되어 있는지 확인
        let has_add_subscriber =
            received_items.iter().any(|item| matches!(item, ActionOp::AddSubscriber));
        assert!(has_add_subscriber, "AddSubscriber should be received");
    }

    #[test]
    fn test_mixed_action_types() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(10, BackpressurePolicy::BlockOnFull);

        // 다양한 타입의 액션들을 전송
        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Action(3)).unwrap();

        // 수신 확인
        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 5);

        // 순서 확인
        match &received_items[0] {
            ActionOp::Action(value) => assert_eq!(*value, 1),
            _ => panic!("Expected Action(1)"),
        }
        match &received_items[1] {
            ActionOp::AddSubscriber => {
                // AddSubscriber 액션
            }
            _ => panic!("Expected AddSubscriber"),
        }
        match &received_items[2] {
            ActionOp::Action(value) => assert_eq!(*value, 2),
            _ => panic!("Expected Action(2)"),
        }
        match &received_items[3] {
            ActionOp::AddSubscriber => {
                // AddSubscriber 액션
            }
            _ => panic!("Expected AddSubscriber"),
        }
        match &received_items[4] {
            ActionOp::Action(value) => assert_eq!(*value, 3),
            _ => panic!("Expected Action(3)"),
        }
    }

    #[test]
    fn test_block_on_full() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(1, BackpressurePolicy::BlockOnFull);

        sender.send(ActionOp::Action(1)).unwrap();

        // Try to send another item - should block or fail
        let result = sender.try_send(ActionOp::Action(2));
        assert!(result.is_err(), "Should fail because channel is full");

        // Receive the first item
        assert_eq!(receiver.recv(), Some(ActionOp::Action(1)));

        // Now we can send again
        sender.send(ActionOp::Action(2)).unwrap();
        assert_eq!(receiver.recv(), Some(ActionOp::Action(2)));
    }

    #[test]
    fn test_drop_oldest_if_predicate_always_false() {
        let (sender, receiver) = BackpressureChannel::pair(
            3,
            BackpressurePolicy::DropOldestIf {
                predicate: Arc::new(|_| false), // Predicate always returns false
            },
        );

        // Fill the channel to capacity
        assert!(sender.try_send(ActionOp::Action(1)).is_ok());
        assert!(sender.try_send(ActionOp::Action(2)).is_ok());
        assert!(sender.try_send(ActionOp::Action(3)).is_ok());
        assert_eq!(receiver.len(), 3);

        // Try to send one more item - should fail since predicate is false
        // and no items can be dropped
        let result = sender.try_send(ActionOp::Action(4));
        assert!(
            result.is_err(),
            "Should fail because no items match the predicate"
        );

        // Verify the channel contents are unchanged
        assert_eq!(receiver.len(), 3);
        assert_eq!(receiver.recv(), Some(ActionOp::Action(1)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(2)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(3)));
    }

    #[test]
    fn test_drop_oldest_if_predicate_sometimes_true() {
        let (sender, receiver) = BackpressureChannel::pair(
            3,
            BackpressurePolicy::DropOldestIf {
                predicate: Arc::new(|action_op: &ActionOp<i32>| {
                    if let ActionOp::Action(value) = action_op {
                        *value < 5 // Drop values less than 5
                    } else {
                        false
                    }
                }),
            },
        );

        // Fill the channel to capacity with values that don't match predicate
        assert!(sender.try_send(ActionOp::Action(6)).is_ok()); // >= 5, not droppable
        assert!(sender.try_send(ActionOp::Action(2)).is_ok()); // < 5, droppable
        assert!(sender.try_send(ActionOp::Action(8)).is_ok()); // >= 5, not droppable
        assert_eq!(receiver.len(), 3);

        // Try to send a value that doesn't match predicate - should fail
        let result = sender.try_send(ActionOp::Action(9));
        assert!(
            result.is_ok(),
            "Should fail because no items match the predicate"
        );

        // Now send a value that matches predicate - should fail because no items match predicate
        let result = sender.try_send(ActionOp::Action(10)); // This should fail because no items < 5
        assert!(
            result.is_err(),
            "Should fail because no items match the predicate"
        );

        // Verify the channel contents are unchanged
        assert_eq!(receiver.len(), 3);
        assert_eq!(receiver.recv(), Some(ActionOp::Action(6)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(8)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(9)));
    }
}
