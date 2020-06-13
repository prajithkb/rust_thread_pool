use std::time::Instant;


pub fn log_time<'a>(function: &'static str) -> LogTime {
    // println!("--start--> {}", function);
    LogTime { function, start: Instant::now() }
}

pub struct LogTime<'a> {
    function: &'a str,
    start: Instant,
}

impl<'a> Drop for LogTime<'a> {
    fn drop(&mut self) {
        println!("Duration: [{}] ({:?} us)", self.function, self.start.elapsed().as_micros());
    }
}