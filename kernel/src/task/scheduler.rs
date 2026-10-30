//! Fixed-capacity round-robin scheduling policy.
//!
//! This module deliberately contains no architecture-specific register state. It selects the
//! next runnable task and tracks lifecycle/quantum state; saving and restoring CPU contexts is
//! a separate architecture backend milestone. The first implementation is allocation-free.

pub const MAX_TASKS: usize = 64;
pub const DEFAULT_QUANTUM_TICKS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskState {
    Runnable = 0,
    Running = 1,
    Blocked = 2,
    Exited = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub state: TaskState,
    pub quantum_remaining: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerError {
    Full,
    DuplicateTask,
    UnknownTask,
    NoCurrentTask,
    InvalidQuantum,
}

/// A deterministic, allocation-free round-robin run-queue model.
pub struct Scheduler {
    tasks: [Option<Task>; MAX_TASKS],
    current: Option<usize>,
    next_id: u64,
    quantum: u32,
}

impl Scheduler {
    pub const fn new(quantum: u32) -> Self {
        Self {
            tasks: [None; MAX_TASKS],
            current: None,
            next_id: 1,
            quantum,
        }
    }

    pub fn add_task(&mut self) -> Result<TaskId, SchedulerError> {
        if self.quantum == 0 {
            return Err(SchedulerError::InvalidQuantum);
        }
        let slot = self.tasks.iter().position(Option::is_none).ok_or(SchedulerError::Full)?;
        let id = TaskId(self.next_id);
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        if self.tasks.iter().flatten().any(|task| task.id == id) {
            return Err(SchedulerError::DuplicateTask);
        }
        self.tasks[slot] = Some(Task {
            id,
            state: TaskState::Runnable,
            quantum_remaining: self.quantum,
        });
        Ok(id)
    }

    pub fn current(&self) -> Option<TaskId> {
        self.current.and_then(|index| self.tasks[index].map(|task| task.id))
    }

    pub fn task(&self, id: TaskId) -> Option<Task> {
        self.tasks.iter().flatten().find(|task| task.id == id).copied()
    }

    /// Selects the next runnable task, starting after the previously running slot.
    pub fn schedule_next(&mut self) -> Option<TaskId> {
        let start = self.current.map_or(0, |index| (index + 1) % MAX_TASKS);
        if let Some(index) = self.current.take() {
            if let Some(task) = self.tasks[index].as_mut() {
                if task.state == TaskState::Running {
                    task.state = TaskState::Runnable;
                    task.quantum_remaining = self.quantum;
                }
            }
        }

        for offset in 0..MAX_TASKS {
            let index = (start + offset) % MAX_TASKS;
            if let Some(task) = self.tasks[index].as_mut() {
                if task.state == TaskState::Runnable {
                    task.state = TaskState::Running;
                    if task.quantum_remaining == 0 {
                        task.quantum_remaining = self.quantum;
                    }
                    self.current = Some(index);
                    return Some(task.id);
                }
            }
        }
        None
    }

    /// Accounts for one timer tick. Returns the next task only when a quantum expires.
    pub fn on_tick(&mut self) -> Option<TaskId> {
        let index = self.current?;
        let task = self.tasks[index].as_mut()?;
        if task.state != TaskState::Running {
            self.current = None;
            return self.schedule_next();
        }
        task.quantum_remaining = task.quantum_remaining.saturating_sub(1);
        if task.quantum_remaining == 0 {
            self.schedule_next()
        } else {
            None
        }
    }

    pub fn block_current(&mut self) -> Result<(), SchedulerError> {
        let index = self.current.take().ok_or(SchedulerError::NoCurrentTask)?;
        let task = self.tasks[index].as_mut().ok_or(SchedulerError::NoCurrentTask)?;
        task.state = TaskState::Blocked;
        task.quantum_remaining = self.quantum;
        Ok(())
    }

    pub fn wake(&mut self, id: TaskId) -> Result<(), SchedulerError> {
        let task = self.tasks.iter_mut().flatten().find(|task| task.id == id)
            .ok_or(SchedulerError::UnknownTask)?;
        if task.state == TaskState::Blocked {
            task.state = TaskState::Runnable;
            task.quantum_remaining = self.quantum;
        }
        Ok(())
    }

    pub fn exit_current(&mut self) -> Result<(), SchedulerError> {
        let index = self.current.take().ok_or(SchedulerError::NoCurrentTask)?;
        let task = self.tasks[index].as_mut().ok_or(SchedulerError::NoCurrentTask)?;
        task.state = TaskState::Exited;
        task.quantum_remaining = 0;
        Ok(())
    }

    pub fn runnable_count(&self) -> usize {
        self.tasks.iter().flatten().filter(|task| {
            matches!(task.state, TaskState::Runnable | TaskState::Running)
        }).count()
    }

    pub fn reap(&mut self, id: TaskId) -> Result<(), SchedulerError> {
        let index = self.tasks.iter().position(|entry| {
            entry.map(|task| task.id == id && task.state == TaskState::Exited).unwrap_or(false)
        }).ok_or(SchedulerError::UnknownTask)?;
        self.tasks[index] = None;
        Ok(())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(DEFAULT_QUANTUM_TICKS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_tasks_in_round_robin_order() {
        let mut scheduler = Scheduler::new(2);
        let first = scheduler.add_task().unwrap();
        let second = scheduler.add_task().unwrap();
        assert_eq!(scheduler.schedule_next(), Some(first));
        assert_eq!(scheduler.on_tick(), None);
        assert_eq!(scheduler.on_tick(), Some(second));
        assert_eq!(scheduler.current(), Some(second));
        assert_eq!(scheduler.on_tick(), None);
        assert_eq!(scheduler.on_tick(), Some(first));
    }

    #[test]
    fn blocked_task_is_skipped_and_can_be_woken() {
        let mut scheduler = Scheduler::default();
        let first = scheduler.add_task().unwrap();
        let second = scheduler.add_task().unwrap();
        assert_eq!(scheduler.schedule_next(), Some(first));
        scheduler.block_current().unwrap();
        assert_eq!(scheduler.schedule_next(), Some(second));
        scheduler.wake(first).unwrap();
        assert_eq!(scheduler.schedule_next(), Some(first));
    }

    #[test]
    fn exited_task_is_not_scheduled_and_can_be_reaped() {
        let mut scheduler = Scheduler::default();
        let first = scheduler.add_task().unwrap();
        let second = scheduler.add_task().unwrap();
        assert_eq!(scheduler.schedule_next(), Some(first));
        scheduler.exit_current().unwrap();
        assert_eq!(scheduler.schedule_next(), Some(second));
        assert_eq!(scheduler.task(first).unwrap().state, TaskState::Exited);
        scheduler.reap(first).unwrap();
        assert_eq!(scheduler.task(first), None);
    }

    #[test]
    fn rejects_zero_quantum_and_full_queue() {
        let mut invalid = Scheduler::new(0);
        assert_eq!(invalid.add_task(), Err(SchedulerError::InvalidQuantum));
        let mut scheduler = Scheduler::new(1);
        for _ in 0..MAX_TASKS { scheduler.add_task().unwrap(); }
        assert_eq!(scheduler.add_task(), Err(SchedulerError::Full));
    }

    #[test]
    fn wake_unknown_task_fails() {
        let mut scheduler = Scheduler::default();
        assert_eq!(scheduler.wake(TaskId(999)), Err(SchedulerError::UnknownTask));
    }
}
