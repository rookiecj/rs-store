use crate::metrics::Metrics;
use crate::ActionOp;
#[cfg(feature = "store-log")]
use crate::store_impl::describe_action_op;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};

/// the Backpressure policy
#[derive(Default)]
pub enum BackpressurePolicy<T>
where
    T: Send + Sync + Clone + 'static,
{
    /// Block the sender when the queue is full
    #[default]
    BlockOnFull,
    /// Drop an item from the oldest to the newest when the queue is full
    #[deprecated(note = "Use DropOldestIf(None) instead")]
    DropOldest,
    /// Drop an item from the newest to the oldest when the queue is full
    #[deprecated(note = "Use DropLatestIf(None) instead")]
    DropLatest,
    /// Drop items based on predicate when the queue is full, it drops from the newest to the oldest
    /// With this policy, [send] method can be Err when all items are not droppable
    /// Predicate is only applied to ActionOp::Action variants, other variants are never dropped
    DropLatestIf(Option<Box<dyn Fn(&T) -> bool + Send + Sync>>),
    /// Drop items based on predicate when the queue is full, it drops from the oldest to the newest
    /// With this policy, [send] method can be Err when all items are not droppable
    /// Predicate is only applied to ActionOp::Action variants, other variants are never dropped
    DropOldestIf(Option<Box<dyn Fn(&T) -> bool + Send + Sync>>),
}

#[derive(thiserror::Error)]
pub(crate) enum SenderError<T> {
    #[error("Failed to send item")]
    SendError(T),
    #[error("Channel is closed")]
    ChannelClosed,
}

impl<T> fmt::Debug for SenderError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SenderError::SendError(_) => f.write_str("SendError(..)"),
            SenderError::ChannelClosed => f.write_str("ChannelClosed"),
        }
    }
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

    /// send put an item to the queue with backpressure policy
    /// when it is full, the send will try to drop an item based on the policy:
    /// - with BlockOnFull policy, the send will block until the space is available
    /// - with DropOldest policy, the send will drop the oldest item and put the new item to the queue
    /// - with DropLatest policy, the send will drop the newest item and put the new item to the queue
    /// - with DropOldestIf policy, the send will drop an item if the predicate is true from the oldest to the newest
    /// - with DropLatestIf policy, the send will drop an item if the predicate is true from the latest to the oldest
    /// if nothing is dropped, the send will block until the space is available
    fn send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        // Check if channel is closed
        if *self.closed.lock().unwrap() {
            return Err(SenderError::ChannelClosed);
        }

        let mut queue: std::sync::MutexGuard<'_, VecDeque<ActionOp<T>>> =
            self.queue.lock().unwrap();

        // Loop until we can successfully add the item to the queue
        loop {
            if queue.len() < self.capacity {
                // Queue has space, add the item
                queue.push_back(item);
                break;
            }

            // Queue is full, try to handle based on policy
            #[allow(deprecated)]
            match &self.policy {
                BackpressurePolicy::BlockOnFull => {
                    // Wait until space is available
                    while queue.len() >= self.capacity {
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                    }
                    // Continue loop to add item
                }
                BackpressurePolicy::DropOldest | BackpressurePolicy::DropOldestIf(None) => {
                    // Drop the oldest item, but only if it's an Action variant
                    let mut found_action_to_drop = false;
                    let mut i = 0;
                    while i < queue.len() {
                        if matches!(queue[i], ActionOp::Action(_)) {
                            if let Some(dropped_item) = queue.remove(i) {
                                found_action_to_drop = true;
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

                    // If no Action variant was found to drop, block until space is available
                    if !found_action_to_drop {
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                        continue; // Continue the outer loop to try again
                    }
                    // Continue loop to add item
                }
                BackpressurePolicy::DropLatest | BackpressurePolicy::DropLatestIf(None) => {
                    // Drop the new item only if it's an Action variant
                    let mut found_action_to_drop = false;
                    let mut i = 0;
                    while i < queue.len() {
                        let idx = queue.len() - i - 1;
                        if matches!(queue[idx], ActionOp::Action(_)) {
                            if let Some(dropped_item) = queue.remove(idx) {
                                found_action_to_drop = true;
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

                    // If no Action variant was found to drop, block until space is available
                    if !found_action_to_drop {
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                        continue; // Continue the outer loop to try again
                    }
                    // Continue loop to add item
                }
                BackpressurePolicy::DropOldestIf(Some(predicate)) => {
                    // Find and drop items that match the predicate from oldest to newest
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: check droppable {}/{}: {}",
                            i,
                            queue.len(),
                            describe_action_op(&queue[i])
                        );
                        // Only apply predicate to Action variants
                        let should_drop = if let ActionOp::Action(action) = &queue[i] {
                            predicate(action)
                        } else {
                            false // Never drop non-Action variants
                        };
                        if should_drop {
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

                    if dropped_count == 0 {
                        // Nothing was dropped, block until space is available
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: no droppable items found, blocking until space available: queue len={}",
                            queue.len()
                        );
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                    }
                    // Continue loop to try again
                }
                BackpressurePolicy::DropLatestIf(Some(predicate)) => {
                    // Find and drop items that match the predicate from latest to oldest
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        let index = queue.len() - i - 1;
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: check droppable {}/{}: {}",
                            index,
                            queue.len(),
                            describe_action_op(&queue[index])
                        );
                        // Only apply predicate to Action variants
                        let should_drop = if let ActionOp::Action(action) = &queue[index] {
                            predicate(action)
                        } else {
                            false // Never drop non-Action variants
                        };
                        if should_drop {
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

                    if dropped_count == 0 {
                        // Nothing was dropped, block until space is available
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: no droppable items found, blocking until space available: queue len={}",
                            queue.len()
                        );
                        queue = self.condvar.wait(queue).unwrap();
                        if *self.closed.lock().unwrap() {
                            return Err(SenderError::ChannelClosed);
                        }
                    }
                    // Continue loop to try again
                }
            }
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

        if queue.len() < self.capacity {
            queue.push_back(item);
        } else {
            #[allow(deprecated)]
            match &self.policy {
                BackpressurePolicy::BlockOnFull => {
                    return Err(SenderError::SendError(item));
                }
                BackpressurePolicy::DropOldest | BackpressurePolicy::DropOldestIf(None) => {
                    // Drop the oldest item, but only if it's an Action variant
                    let mut found_action_to_drop = false;
                    let mut i = 0;
                    while i < queue.len() {
                        if matches!(queue[i], ActionOp::Action(_)) {
                            if let Some(dropped_item) = queue.remove(i) {
                                found_action_to_drop = true;
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

                    if found_action_to_drop {
                        queue.push_back(item);
                    } else {
                        // No Action variant found to drop, return error
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: failed to drop oldest action while trying to send: queue len={}",
                            queue.len()
                        );
                        return Err(SenderError::SendError(item));
                    }
                }
                BackpressurePolicy::DropLatest | BackpressurePolicy::DropLatestIf(None) => {
                    let mut found_action_to_drop = false;
                    let mut i = 0;
                    while i < queue.len() {
                        let index = queue.len() - i - 1;
                        if matches!(queue[index], ActionOp::Action(_)) {
                            if let Some(dropped_item) = queue.remove(index) {
                                found_action_to_drop = true;
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

                    if found_action_to_drop {
                        queue.push_back(item);
                    } else {
                        // No Action variant found to drop, return error
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: failed to drop latest action while trying to send: queue len={}",
                            queue.len()
                        );
                        return Err(SenderError::SendError(item));
                    }
                }
                BackpressurePolicy::DropOldestIf(Some(predicate)) => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: check droppable {}/{}: {}",
                            i,
                            queue.len(),
                            describe_action_op(&queue[i])
                        );
                        // Only apply predicate to Action variants
                        let should_drop = if let ActionOp::Action(action) = &queue[i] {
                            predicate(action)
                        } else {
                            false // Never drop non-Action variants
                        };
                        if should_drop {
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
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: failed to drop the oldestif while trying to send: queue len={}",
                            queue.len()
                        );
                        return Err(SenderError::SendError(item));
                    }
                }
                BackpressurePolicy::DropLatestIf(Some(predicate)) => {
                    // Find and drop items that match the predicate
                    let mut dropped_count = 0;
                    let mut i = 0;
                    while i < queue.len() {
                        let index = queue.len() - i - 1;
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: check droppable {}/{}: {}",
                            index,
                            queue.len(),
                            describe_action_op(&queue[index])
                        );
                        // Only apply predicate to Action variants
                        let should_drop = if let ActionOp::Action(action) = &queue[index] {
                            predicate(action)
                        } else {
                            false // Never drop non-Action variants
                        };
                        if should_drop {
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
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: failed to drop the latestif while trying to send: queue len={}",
                            queue.len()
                        );
                        return Err(SenderError::SendError(item));
                    }
                }
            }
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
    /// when it is full, the send will try to drop an item based on the policy
    /// if nothing is dropped, the send will block until the space is available
    pub fn send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        self.queue.send(item)
    }

    /// when it is full, it will return Err
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
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropOldestIf(None));

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
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf(None));

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap(); // should drop 2
        sender.send(ActionOp::Action(3)).unwrap();

        assert_eq!(receiver.recv(), Some(ActionOp::Action(1)));
        assert_eq!(receiver.recv(), Some(ActionOp::Action(3)));
        assert_eq!(receiver.try_recv(), None);
    }

    #[test]
    fn test_predicate_dropping() {
        // predicate: 5보다 작은 값들은 drop
        let predicate = Box::new(|value: &i32| *value < 5);

        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf(Some(predicate)));

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
        // predicate는 Action에만 적용되므로 AddSubscriber는 자동으로 보존됨
        let predicate = Box::new(|value: &i32| *value < 5);

        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf(Some(predicate)));

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
            BackpressurePolicy::DropOldestIf(Some(Box::new(|_| false))), // Predicate always returns false
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
            BackpressurePolicy::DropOldestIf(Some(Box::new(|value: &i32| *value < 5))), // Drop values less than 5
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

    #[test]
    fn test_drop_oldest_only_actions() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropOldestIf(None));

        // Fill the channel with non-Action items
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Exit(std::time::Instant::now())).unwrap();
        assert_eq!(receiver.len(), 2);

        // Try to send an Action - should block because no Actions to drop
        let result = sender.try_send(ActionOp::Action(1));
        assert!(
            result.is_err(),
            "Should fail because no Actions can be dropped"
        );

        // Channel should still contain the original non-Action items
        assert_eq!(receiver.len(), 2);
        assert_eq!(receiver.recv(), Some(ActionOp::AddSubscriber));
        assert!(matches!(receiver.recv(), Some(ActionOp::Exit(_))));
    }

    #[test]
    fn test_drop_oldest_with_mixed_types() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(3, BackpressurePolicy::DropOldestIf(None));

        // Fill the channel with mixed types
        sender.send(ActionOp::Action(1)).unwrap(); // This should be droppable
        sender.send(ActionOp::AddSubscriber).unwrap(); // This should NOT be droppable
        sender.send(ActionOp::Action(2)).unwrap(); // This should be droppable
        assert_eq!(receiver.len(), 3);

        // Send another Action - should drop the first Action(1)
        sender.send(ActionOp::Action(3)).unwrap();
        assert_eq!(receiver.len(), 3);

        // Verify contents: AddSubscriber should still be there, Action(1) should be dropped
        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 3);

        // Should contain AddSubscriber, Action(2), and Action(3)
        let has_add_subscriber =
            received_items.iter().any(|item| matches!(item, ActionOp::AddSubscriber));
        let has_action_2 = received_items.iter().any(|item| matches!(item, ActionOp::Action(2)));
        let has_action_3 = received_items.iter().any(|item| matches!(item, ActionOp::Action(3)));
        let has_action_1 = received_items.iter().any(|item| matches!(item, ActionOp::Action(1)));

        assert!(has_add_subscriber, "AddSubscriber should be preserved");
        assert!(has_action_2, "Action(2) should be preserved");
        assert!(has_action_3, "Action(3) should be added");
        assert!(!has_action_1, "Action(1) should be dropped");
    }

    #[test]
    fn test_drop_latest_only_actions() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(3, BackpressurePolicy::DropLatestIf(None));

        // Fill the channel with non-Action items
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Exit(std::time::Instant::now())).unwrap();
        assert_eq!(receiver.len(), 3);

        // Try to send an Action - should be dropped
        let result = sender.try_send(ActionOp::Action(2));
        assert!(result.is_ok(), "Action should be dropped successfully");

        // Try to send an Action - should be dropped
        let result = sender.try_send(ActionOp::StateFunction);
        assert!(result.is_ok(), "Action should be dropped successfully");

        // Try to send a non-Action - should fail because channel is full
        let result = sender.try_send(ActionOp::AddSubscriber);
        assert!(
            result.is_err(),
            "Should fail because channel is full and non-Actions can't be dropped"
        );

        // Channel should still contain the original non-Action items
        assert_eq!(receiver.len(), 3);
        assert_eq!(receiver.recv(), Some(ActionOp::AddSubscriber));
        assert!(matches!(receiver.recv(), Some(ActionOp::Exit(_))));
        assert_eq!(receiver.recv(), Some(ActionOp::StateFunction));
    }

    #[test]
    fn test_drop_policy_preserves_critical_operations() {
        // 이 테스트는 중요한 작업(AddSubscriber, Exit)이 drop되지 않는지 확인합니다
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(3, BackpressurePolicy::DropOldestIf(None));

        // 채널을 가득 채우기: 2개의 Action과 1개의 AddSubscriber
        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::AddSubscriber).unwrap();
        assert_eq!(receiver.len(), 3);

        // 새로운 Action을 보내면 기존 Action 중 하나가 drop되어야 함
        sender.send(ActionOp::Action(3)).unwrap();
        assert_eq!(receiver.len(), 3);

        // AddSubscriber는 여전히 존재해야 함
        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        let has_add_subscriber =
            received_items.iter().any(|item| matches!(item, ActionOp::AddSubscriber));
        assert!(
            has_add_subscriber,
            "AddSubscriber should never be dropped by drop policy"
        );

        // Action(1)이 drop되고 Action(2), Action(3)이 남아있어야 함
        let action_values: Vec<i32> = received_items
            .iter()
            .filter_map(|item| {
                if let ActionOp::Action(val) = item {
                    Some(*val)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(action_values.len(), 2, "Should have 2 Actions remaining");
        assert!(action_values.contains(&2), "Action(2) should be preserved");
        assert!(action_values.contains(&3), "Action(3) should be added");
        assert!(!action_values.contains(&1), "Action(1) should be dropped");
    }

    #[test]
    fn test_drop_policy_with_exit_operations() {
        // Exit 작업이 drop되지 않는지 확인하는 테스트
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropLatestIf(None));

        let exit_time = std::time::Instant::now();

        // 채널을 가득 채우기: 1개의 Action과 1개의 Exit
        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Exit(exit_time)).unwrap();
        assert_eq!(receiver.len(), 2);

        // 새로운 Action을 보내면 최근 아이템이 drop되어야 함 (Exit은 보존)
        let result = sender.send(ActionOp::Action(2));
        assert!(result.is_ok(), "Action should be dropped, not Exit");

        // Exit은 여전히 존재해야 함
        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 2);

        let has_action_1 = received_items.get(0).unwrap() == &ActionOp::Action(1);
        assert!(!has_action_1, "Action(1) should be dropped");

        let has_exit = received_items.get(0).unwrap() == &ActionOp::Exit(exit_time);
        assert!(has_exit, "Exit should never be dropped by drop policy");

        let has_action_2 = received_items.get(1).unwrap() == &ActionOp::Action(2);
        assert!(has_action_2, "Action(2) added");
    }

    #[test]
    fn test_drop_oldest_action_ordering() {
        // DropOldest가 Action들 중에서 가장 오래된 것을 drop하는지 확인
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(4, BackpressurePolicy::DropOldestIf(None));

        // 채널에 순서대로 추가: Action(1), AddSubscriber, Action(2), Action(3)
        sender.send(ActionOp::Action(1)).unwrap(); // 가장 오래된 Action
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::Action(3)).unwrap();
        assert_eq!(receiver.len(), 4);

        // 새로운 Action을 보내면 Action(1)이 drop되어야 함
        sender.send(ActionOp::Action(4)).unwrap();
        assert_eq!(receiver.len(), 4);

        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();

        // AddSubscriber는 보존되어야 함
        let has_add_subscriber =
            received_items.iter().any(|item| matches!(item, ActionOp::AddSubscriber));
        assert!(has_add_subscriber, "AddSubscriber should be preserved");

        // Action 값들 확인
        let action_values: Vec<i32> = received_items
            .iter()
            .filter_map(|item| {
                if let ActionOp::Action(val) = item {
                    Some(*val)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(action_values.len(), 3, "Should have 3 Actions remaining");
        assert!(
            !action_values.contains(&1),
            "Action(1) should be dropped (oldest)"
        );
        assert!(action_values.contains(&2), "Action(2) should be preserved");
        assert!(action_values.contains(&3), "Action(3) should be preserved");
        assert!(action_values.contains(&4), "Action(4) should be added");
    }

    #[test]
    fn test_drop_policy_blocking_behavior() {
        // Action이 없을 때 blocking 동작을 확인하는 테스트
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(2, BackpressurePolicy::DropOldestIf(None));

        // 채널을 non-Action items로 가득 채우기
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Exit(std::time::Instant::now())).unwrap();
        assert_eq!(receiver.len(), 2);

        // try_send로 새로운 Action을 보내려 하면 실패해야 함 (drop할 Action이 없음)
        let result = sender.try_send(ActionOp::Action(1));
        assert!(
            result.is_err(),
            "Should fail because no Actions available to drop"
        );

        // try_send로 새로운 non-Action을 보내려 해도 실패해야 함
        let result = sender.try_send(ActionOp::AddSubscriber);
        assert!(
            result.is_err(),
            "Should fail because channel is full and no Actions to drop"
        );

        // 채널 내용이 변경되지 않았는지 확인
        assert_eq!(receiver.len(), 2);
        assert_eq!(receiver.recv(), Some(ActionOp::AddSubscriber));
        assert!(matches!(receiver.recv(), Some(ActionOp::Exit(_))));
    }

    #[test]
    fn test_drop_oldest_if_predicate_always_true() {
        let (sender, receiver) = BackpressureChannel::<i32>::pair(
            3,
            BackpressurePolicy::DropOldestIf(Some(Box::new(|_| true))),
        );

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::Action(3)).unwrap();

        let result = sender.send(ActionOp::Action(4));
        assert!(result.is_ok(), "Action should be dropped successfully");

        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 3);

        let has_action_1 = received_items.get(0).unwrap() == &ActionOp::Action(1);
        assert!(!has_action_1, "Action(1) should be dropped");

        let has_action_2 = received_items.get(0).unwrap() == &ActionOp::Action(2);
        assert!(has_action_2, "Action(2) should be preserved");

        let has_action_3 = received_items.get(1).unwrap() == &ActionOp::Action(3);
        assert!(has_action_3, "Action(3) should be preserved");

        let has_action_4 = received_items.get(2).unwrap() == &ActionOp::Action(4);
        assert!(has_action_4, "Action(4) should be added");
    }

    #[test]
    fn test_drop_latest_if_predicate_always_true() {
        let (sender, receiver) = BackpressureChannel::<i32>::pair(
            3,
            BackpressurePolicy::DropLatestIf(Some(Box::new(|_| true))),
        );

        sender.send(ActionOp::Action(1)).unwrap();
        sender.send(ActionOp::Action(2)).unwrap();
        sender.send(ActionOp::Action(3)).unwrap(); // should be dropped

        let result = sender.send(ActionOp::Action(4));
        assert!(result.is_ok(), "Action should be dropped successfully");

        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();
        assert_eq!(received_items.len(), 3);

        let has_action_1 = received_items.get(0).unwrap() == &ActionOp::Action(1);
        assert!(has_action_1, "Action(1) should be preserved");

        let has_action_2 = received_items.get(1).unwrap() == &ActionOp::Action(2);
        assert!(has_action_2, "Action(2) should be preserved");

        let has_action_3 = received_items.get(2).unwrap() == &ActionOp::Action(3);
        assert!(!has_action_3, "Action(3) should be dropped");

        let has_action_4 = received_items.get(2).unwrap() == &ActionOp::Action(4);
        assert!(has_action_4, "Action(4) should be added");
    }

    #[test]
    fn test_drop_latest_vs_drop_oldest_action_selection() {
        // DropLatest와 DropOldest가 서로 같은 Action을 선택하는지 확인

        // DropOldest 테스트
        let (sender_oldest, receiver_oldest) =
            BackpressureChannel::<i32>::pair(3, BackpressurePolicy::DropOldestIf(None));

        sender_oldest.send(ActionOp::Action(10)).unwrap(); // 가장 오래된 Action
        sender_oldest.send(ActionOp::AddSubscriber).unwrap();
        sender_oldest.send(ActionOp::Action(20)).unwrap(); // 가장 새로운 Action

        sender_oldest.send(ActionOp::Action(30)).unwrap(); // Action(10)이 drop되어야 함

        let oldest_items: Vec<_> = std::iter::from_fn(|| receiver_oldest.try_recv()).collect();
        let oldest_actions: Vec<i32> = oldest_items
            .iter()
            .filter_map(|item| {
                if let ActionOp::Action(val) = item {
                    Some(*val)
                } else {
                    None
                }
            })
            .collect();

        assert!(
            oldest_actions.contains(&20),
            "DropOldest should preserve Action(20)"
        );
        assert!(
            oldest_actions.contains(&30),
            "DropOldest should preserve Action(30)"
        );

        // DropLatest 테스트
        let (sender_latest, receiver_latest) =
            BackpressureChannel::<i32>::pair(3, BackpressurePolicy::DropLatestIf(None));

        sender_latest.send(ActionOp::Action(100)).unwrap(); // 가장 오래된 Action
        sender_latest.send(ActionOp::AddSubscriber).unwrap();
        sender_latest.send(ActionOp::Action(200)).unwrap(); // should be dropped

        // 새로운 Action을 보내면 마지막 drop되어야 함
        let result = sender_latest.send(ActionOp::Action(300));
        assert!(
            result.is_ok(),
            "send Action should be success, should drop the latest Action(200)"
        );

        let latest_items: Vec<_> = std::iter::from_fn(|| receiver_latest.try_recv()).collect();
        assert_eq!(latest_items.len(), 3);

        let has_action_100 = latest_items.get(0).unwrap() == &ActionOp::Action(100);
        assert!(has_action_100, "DropLatest should preserve Action(100)");

        assert_eq!(
            latest_items.get(1).unwrap(),
            &ActionOp::AddSubscriber,
            "DropLatest should preserve AddSubscriber"
        );

        let has_action_200 = latest_items.get(2).unwrap() == &ActionOp::Action(200);
        assert!(!has_action_200, "DropLatest should drop Action(200)");

        let has_action_300 = latest_items.get(2).unwrap() == &ActionOp::Action(300);
        assert!(has_action_300, "DropLatest should add Action(300)");
    }

    #[test]
    fn test_comprehensive_drop_policy_verification() {
        // 종합적인 drop policy 검증 테스트
        let (sender, receiver) =
            BackpressureChannel::<String>::pair(5, BackpressurePolicy::DropOldestIf(None));

        // 다양한 타입의 ActionOp를 순서대로 추가
        sender.send(ActionOp::Action("action1".to_string())).unwrap();
        sender.send(ActionOp::AddSubscriber).unwrap();
        sender.send(ActionOp::Action("action2".to_string())).unwrap();
        sender.send(ActionOp::Exit(std::time::Instant::now())).unwrap();
        sender.send(ActionOp::Action("action3".to_string())).unwrap();
        assert_eq!(receiver.len(), 5);

        // 채널이 가득 찬 상태에서 새로운 Action 추가
        // action1이 drop되어야 함 (가장 오래된 Action)
        sender.send(ActionOp::Action("action4".to_string())).unwrap();
        assert_eq!(receiver.len(), 5);

        let received_items: Vec<_> = std::iter::from_fn(|| receiver.try_recv()).collect();

        // 모든 non-Action items는 보존되어야 함
        let has_add_subscriber =
            received_items.iter().any(|item| matches!(item, ActionOp::AddSubscriber));
        let has_exit = received_items.iter().any(|item| matches!(item, ActionOp::Exit(_)));
        assert!(has_add_subscriber, "AddSubscriber must be preserved");
        assert!(has_exit, "Exit must be preserved");

        // Action items 검증
        let action_values: Vec<String> = received_items
            .iter()
            .filter_map(|item| {
                if let ActionOp::Action(val) = item {
                    Some(val.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(action_values.len(), 3, "Should have 3 Actions remaining");
        assert!(
            !action_values.contains(&"action1".to_string()),
            "action1 should be dropped (oldest Action)"
        );
        assert!(
            action_values.contains(&"action2".to_string()),
            "action2 should be preserved"
        );
        assert!(
            action_values.contains(&"action3".to_string()),
            "action3 should be preserved"
        );
        assert!(
            action_values.contains(&"action4".to_string()),
            "action4 should be added"
        );

        // 전체 아이템 개수 확인
        assert_eq!(received_items.len(), 5, "Total items should remain 5");
    }
}
