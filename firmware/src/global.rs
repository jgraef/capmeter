//static GLOBAL_MUX: Mutex<Cell<GlobalMux>> = ;

use core::cell::Cell;

use avr_device::interrupt::Mutex;

pub struct Global<T> {
    inner: Mutex<Cell<State<T>>>,
}

impl<T> Global<T> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Cell::new(State::Uninitialized)),
        }
    }

    pub fn install(&self, init: T) {
        avr_device::interrupt::free(move |cs| {
            let state = self.inner.borrow(cs);
            state.set(State::Released(init));
        });
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut value = avr_device::interrupt::free(move |cs| {
            let global_mux = self.inner.borrow(cs);
            match global_mux.replace(State::Acquired) {
                State::Released(value) => value,
                State::Uninitialized => panic!("Global not initialized"),
                State::Acquired => panic!("Global already acquired"),
            }
        });

        let output = f(&mut value);

        avr_device::interrupt::free(move |cs| {
            let global_mux = self.inner.borrow(cs);
            global_mux.replace(State::Released(value));
        });

        output
    }
}

enum State<T> {
    Uninitialized,
    Acquired,
    Released(T),
}
