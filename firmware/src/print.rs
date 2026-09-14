#[macro_export]
macro_rules! eprint {
    ($mux:expr, $($t:tt)*) => {
        {
            let mut writer = $mux.writer(::protocol::DEBUG_PORT);
            let _ = ufmt::uwrite!(&mut writer, $($t)*);
        }
    };
}

#[macro_export]
macro_rules! eprintln {
    ($mux:expr, $($t:tt)*) => {
        {
            let mut writer = $mux.writer(::protocol::DEBUG_PORT);
            let _ = ufmt::uwriteln!(&mut writer, $($t)*);
        }
    };
}

#[macro_export]
macro_rules! print {
    ($($t:tt)*) => {
        $crate::mux::with(|mux| {
            eprint!(mux, $($t)*);
        });
    };
}

#[macro_export]
macro_rules! println {
    ($($t:tt)*) => {
        $crate::mux::with(|mux| {
            eprintln!(mux, $($t)*);
        });
    };
}
