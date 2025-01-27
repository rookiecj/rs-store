use crate::channel::{
    ReceiverChannel, SenderChannel,
};
use crate::{Subscriber, Subscription};
use crate::store::ActionOp;

pub(crate) struct StateSubscriber<State>
where
    State: Send + Sync + Clone + 'static,
{
    iter_tx: Option<SenderChannel<State>>,
}

impl<State, Action> Subscriber<State, Action> for StateSubscriber<State>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn on_notify(&self, state: &State, _action: &Action) {
        if let Some(iter_tx) = self.iter_tx.as_ref() {
            match iter_tx.send(ActionOp::Action(state.clone())) {
                Ok(_) => {}
                Err(_e) => {
                    #[cfg(any(dev))]
                    eprintln!("store: Error while sending state to iterator");
                }
            }
        }
    }

    fn on_unsubscribe(&self) {
        // when the subscriber is unsubscribed, send an exit message to the iterator not to wait forever
        if let Some(iter_tx) = self.iter_tx.as_ref() {
            let _ = iter_tx.send(ActionOp::Exit);
        }
    }
}

impl<State> Drop for StateSubscriber<State>
where
    State: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        if let Some(iter_tx) = self.iter_tx.take() {
            drop(iter_tx);
        }
        #[cfg(any(dev))]
        eprintln!("store: StateSubscriber done");
    }
}

impl<State> StateSubscriber<State>
where
    State: Send + Sync + Clone + 'static,
{
    pub fn new(tx: SenderChannel<State>) -> Self {
        StateSubscriber { iter_tx: Some(tx) }
    }
}

pub(crate) struct StateIterator<State>
where
    State: Send + Sync + Clone + 'static,
{
    iter_rx: Option<ReceiverChannel<State>>,
    subscription: Option<Box<dyn Subscription>>,
}

impl<State> Iterator for StateIterator<State>
where
    State: Send + Sync + Clone + 'static,
{
    type Item = State;

    fn next(&mut self) -> Option<Self::Item> {
        #[cfg(any(dev))]
        eprintln!("store: StateIterator next");

        if let Some(iter_rx) = self.iter_rx.as_ref() {
            match iter_rx.recv() {
                Some(ActionOp::Action(state)) => return Some(state),
                Some(ActionOp::Exit) => {
                    #[cfg(any(dev))]
                    eprintln!("store: StateIterator exit");
                }
                None => {
                    #[cfg(any(dev))]
                    eprintln!("store: StateIterator error");
                }
            };
        };

        if let Some(subscription) = self.subscription.take() {
            subscription.unsubscribe()
        }
        if let Some(rx) = self.iter_rx.take() {
            drop(rx);
        }

        #[cfg(any(dev))]
        eprintln!("store: StateIterator done");

        None
    }
}

impl<State> Drop for StateIterator<State>
where
    State: Send + Sync + Clone,
{
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            subscription.unsubscribe();
        }
        #[cfg(any(dev))]
        eprintln!("store: StateIterator drop");
    }
}

impl<State> StateIterator<State>
where
    State: Send + Sync + Clone + 'static,
{
    pub(crate) fn new(
        iter_rx: ReceiverChannel<State>,
        subscription: Box<dyn Subscription>,
    ) -> Self {
        Self {
            iter_rx: Some(iter_rx),
            subscription: Some(subscription),
        }
    }
}
