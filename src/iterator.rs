use spmc::Receiver;

/// Iterator for the state.
/// which blocks on recv() until a new state is available.
pub struct Iter<State: Send> {
    // single producer, multiple consumer
    rx: spmc::Receiver<State>,
}

impl<State: Send> Iter<State> {
    pub fn new(rx: Receiver<State>) -> Self {
        Self { rx }
    }
}

impl<State> Iterator for Iter<State>
where
    State: Send,
{
    type Item = State;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

/// TryIter is an iterator for the state.
/// which does not block on recv() and returns None if no new state is available.
pub struct TryIter<State: Send> {
    rx: spmc::Receiver<State>,
}

impl<State: Send> TryIter<State> {
    pub fn new(rx: Receiver<State>) -> Self {
        Self { rx }
    }
}

impl<State> Iterator for TryIter<State>
where
    State: Send,
{
    type Item = State;

    /// try_recv() is non-blocking
    fn next(&mut self) -> Option<Self::Item> {
        self.rx.try_recv().ok()
    }
}
