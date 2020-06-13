//! #Threadpool

pub mod task;
pub mod thread_pool;
mod worker;
mod timed_execution;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
