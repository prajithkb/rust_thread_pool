use std::time::Instant;

#[macro_export]
macro_rules! timed {
    ($function_name:expr) => {
        let _log_time = log_time($function_name);
    };
}

pub fn log_time<'a>(function: &'static str) -> LogTime {
    // println!("--start--> {}", function);
    LogTime {
        function,
        start: Instant::now(),
    }
}

pub struct LogTime<'a> {
    function: &'a str,
    start: Instant,
}

impl<'a> Drop for LogTime<'a> {
    fn drop(&mut self) {
        let ns = self.start.elapsed().as_micros();
        let printable_value =format!("{} ms, {} ns", ns / 1000, ns);
        println!("Duration: [{}] ({})", self.function, printable_value);
    }
}

#[cfg(test)]
mod tests {
    use crate::timed_execution::log_time;
    #[test]
    fn measures_duration() {
        timed!("test");
        // No assertions
    }
}
