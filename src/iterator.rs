use crate::channel::{ReceiverChannel, SenderChannel};
use crate::store_impl::ActionOp;
use crate::{Subscriber, Subscription};
use std::time::Instant;

/// StateIteratorSubscriber is a subscriber that sends state and action pairs to an iterator
#[allow(dead_code)]
pub(crate) struct StateIteratorSubscriber<T>
where
    T: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    iter_tx: Option<SenderChannel<T>>,
}

impl<State, Action> Subscriber<State, Action> for StateIteratorSubscriber<(State, Action)>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn on_notify(&self, state: &State, action: &Action) {
        if let Some(iter_tx) = self.iter_tx.as_ref() {
            match iter_tx.send(ActionOp::Action((state.clone(), action.clone()))) {
                Ok(_) => {}
                Err(_e) => {
                    #[cfg(feature = "store-log")]
                    eprintln!("store: Error while sending state to iterator");
                }
            }
        }
    }

    fn on_unsubscribe(&self) {
        // when the subscriber is unsubscribed, send an exit message to the iterator not to wait forever
        if let Some(iter_tx) = self.iter_tx.as_ref() {
            let _ = iter_tx.send(ActionOp::Exit(Instant::now()));
        }
    }
}

impl<State> Drop for StateIteratorSubscriber<State>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn drop(&mut self) {
        if let Some(iter_tx) = self.iter_tx.take() {
            drop(iter_tx);
        }
        #[cfg(feature = "store-log")]
        eprintln!("store: StateSubscriber done");
    }
}

#[allow(dead_code)]
impl<T> StateIteratorSubscriber<T>
where
    T: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    pub fn new(tx: SenderChannel<T>) -> Self {
        StateIteratorSubscriber { iter_tx: Some(tx) }
    }
}

pub(crate) struct StateIterator<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    iter_rx: Option<ReceiverChannel<(State, Action)>>,
    /// subscription for StateSubscriber
    subscription: Option<Box<dyn Subscription>>,
}

impl<State, Action> Iterator for StateIterator<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    type Item = (State, Action);

    fn next(&mut self) -> Option<Self::Item> {
        #[cfg(feature = "store-log")]
        eprintln!("store: StateIterator next");

        if let Some(iter_rx) = self.iter_rx.as_ref() {
            while let Some(action) = iter_rx.recv() {
                match action {
                    ActionOp::Action((state, action)) => return Some((state, action)),
                    ActionOp::Exit(_) => {
                        #[cfg(feature = "store-log")]
                        eprintln!("store: StateIterator exit");
                        break;
                    }
                    ActionOp::AddSubscriber => {
                        continue;
                    }
                    ActionOp::StateFunction => {
                        continue;
                    }
                }
            }
        };

        if let Some(subscription) = self.subscription.take() {
            subscription.unsubscribe()
        }

        // when the iterator is done, it will drop the receiver eventually
        // but drop it here to make sure it is dropped
        if let Some(rx) = self.iter_rx.take() {
            drop(rx);
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: StateIterator done");

        None
    }
}

impl<State, Action> Drop for StateIterator<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug,
    Action: Send + Sync + Clone + std::fmt::Debug,
{
    fn drop(&mut self) {
        if let Some(rx) = self.iter_rx.take() {
            drop(rx);
        }
        if let Some(subscription) = self.subscription.take() {
            subscription.unsubscribe();
        }
        #[cfg(feature = "store-log")]
        eprintln!("store: StateIterator drop");
    }
}

impl<State, Action> StateIterator<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    pub(crate) fn new(
        iter_rx: ReceiverChannel<(State, Action)>,
        subscription: Box<dyn Subscription>,
    ) -> Self {
        Self {
            iter_rx: Some(iter_rx),
            subscription: Some(subscription),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{BackpressureChannel, BackpressurePolicy};
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn test_state_subscriber() {
        // given
        // let (tx, rx) =
        //     BackpressureChannel::<i32>::pair_with("test", 5, BackpressurePolicy::DropOldest, None);
        // let subscriber = StateIteratorSubscriber::new(tx);
        // subscriber.iter_tx
        // // when
        // subscriber.on_notify(&42, &"action");
        //
        // // then
        // if let Some(ActionOp::Action(state)) = rx.recv() {
        //     assert_eq!(state, 42);
        // } else {
        //     panic!("Expected state not received");
        // }
    }

    #[test]
    fn test_state_iterator() {
        // given
        let (tx, rx) = BackpressureChannel::<(i32, i32)>::pair_with(
            "test",
            5,
            BackpressurePolicy::DropOldest,
            None,
        );

        let mock_subscription = MockSubscription::new();
        let mut iterator = StateIterator::new(rx, Box::new(mock_subscription));

        // when & then
        // Send some test states
        tx.send(ActionOp::Action((1, 1))).unwrap();
        assert_eq!(iterator.next(), Some((1, 1)));

        tx.send(ActionOp::Action((2, 1))).unwrap();
        assert_eq!(iterator.next(), Some((2, 1)));

        // Test exit message
        tx.send(ActionOp::Exit(Instant::now())).unwrap();
        assert_eq!(iterator.next(), None);
    }

    struct MockSubscription {
        unsubscribed: Arc<std::sync::atomic::AtomicBool>,
    }

    impl MockSubscription {
        fn new() -> Self {
            Self {
                unsubscribed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        #[allow(dead_code)]
        fn was_unsubscribed(&self) -> bool {
            self.unsubscribed.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl Subscription for MockSubscription {
        fn unsubscribe(&self) {
            self.unsubscribed.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
