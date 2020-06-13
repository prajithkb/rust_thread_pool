use crate::thread_pool::Runnable;
use std::fmt::Debug;

pub struct Task {
    pub runnable: Runnable,
    pub id: String,
}

impl Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Job")
            .field("runnable", &"Runnable")
            .field("id", &self.id)
            .finish()
    }
}

impl Task {
    pub fn new(runnable: Runnable, id: String) -> Task {
        Task {
            runnable,
            id,
        }
    }
}


