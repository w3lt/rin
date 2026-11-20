#[macro_export]
macro_rules! rinlogln {
    ($($arg:tt)*) => {
        println!("[Rin] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! rinerrln {
    ($($arg:tt)*) => {
        eprintln!("[Rin] {}", format_args!($($arg)*))
    };
}
