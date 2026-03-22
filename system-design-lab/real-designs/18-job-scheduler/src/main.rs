//! # Job Scheduler - Mini Implementation
//!
//! Demonstrates:
//! - Priority queue scheduling
//! - Worker pool management
//! - Job dependencies (DAG)
//! - Retry with backoff
//! - Cron-like recurring jobs
//!
//! Run: cargo run -p job-scheduler

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone)]
struct Job {
    id: String,
    name: String,
    priority: u32,
    scheduled_at: Instant,
    payload: String,
    max_retries: u32,
    retry_count: u32,
    timeout: Duration,
    dependencies: Vec<String>, // Job IDs that must complete first
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum JobStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
struct JobResult {
    job_id: String,
    status: JobStatus,
    output: Option<String>,
    error: Option<String>,
    started_at: Instant,
    completed_at: Instant,
    worker_id: String,
}

// =============================================================================
// Priority Queue
// =============================================================================

#[derive(Debug, Eq, PartialEq)]
struct QueuedJob {
    job_id: String,
    priority: u32,
    scheduled_at: Instant,
}

impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then earlier scheduled time
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.scheduled_at.cmp(&self.scheduled_at))
    }
}

impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// =============================================================================
// Worker
// =============================================================================

struct Worker {
    id: String,
    status: RwLock<WorkerStatus>,
    current_job: RwLock<Option<String>>,
    jobs_completed: AtomicU64,
    jobs_failed: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WorkerStatus {
    Idle,
    Busy,
    Offline,
}

impl Worker {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            status: RwLock::new(WorkerStatus::Idle),
            current_job: RwLock::new(None),
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
        }
    }

    async fn execute(&self, job: &Job) -> Result<String, String> {
        *self.status.write() = WorkerStatus::Busy;
        *self.current_job.write() = Some(job.id.clone());

        // Simulate job execution
        let duration_ms = rand::thread_rng().gen_range(10..100);
        sleep(Duration::from_millis(duration_ms)).await;

        // Random failure (10% chance)
        let success = rand::thread_rng().gen::<f64>() > 0.1;

        *self.status.write() = WorkerStatus::Idle;
        *self.current_job.write() = None;

        if success {
            self.jobs_completed.fetch_add(1, Ordering::SeqCst);
            Ok(format!("Processed: {}", job.payload))
        } else {
            self.jobs_failed.fetch_add(1, Ordering::SeqCst);
            Err("Random failure".to_string())
        }
    }
}

// =============================================================================
// DAG Scheduler (for dependencies)
// =============================================================================

struct DagScheduler {
    // job_id -> set of dependent job_ids (jobs that depend on this one)
    dependents: DashMap<String, HashSet<String>>,
    // job_id -> remaining dependency count
    pending_deps: DashMap<String, AtomicU64>,
}

impl DagScheduler {
    fn new() -> Self {
        Self {
            dependents: DashMap::new(),
            pending_deps: DashMap::new(),
        }
    }

    fn register(&self, job: &Job) {
        // Track this job's dependencies
        self.pending_deps
            .insert(job.id.clone(), AtomicU64::new(job.dependencies.len() as u64));

        // Register as dependent of each dependency
        for dep_id in &job.dependencies {
            self.dependents
                .entry(dep_id.clone())
                .or_default()
                .insert(job.id.clone());
        }
    }

    fn is_ready(&self, job_id: &str) -> bool {
        self.pending_deps
            .get(job_id)
            .map(|c| c.load(Ordering::SeqCst) == 0)
            .unwrap_or(true)
    }

    fn complete(&self, job_id: &str) -> Vec<String> {
        // Return jobs that are now ready to run
        let mut newly_ready = Vec::new();

        if let Some(dependents) = self.dependents.get(job_id) {
            for dep_id in dependents.iter() {
                if let Some(count) = self.pending_deps.get(dep_id) {
                    let remaining = count.fetch_sub(1, Ordering::SeqCst) - 1;
                    if remaining == 0 {
                        newly_ready.push(dep_id.clone());
                    }
                }
            }
        }

        newly_ready
    }
}

// =============================================================================
// Cron Scheduler
// =============================================================================

struct CronJob {
    id: String,
    name: String,
    interval: Duration,
    last_run: RwLock<Option<Instant>>,
    next_run: RwLock<Instant>,
    job_template: Job,
}

impl CronJob {
    fn new(id: &str, name: &str, interval: Duration, template: Job) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            interval,
            last_run: RwLock::new(None),
            next_run: RwLock::new(Instant::now()),
            job_template: template,
        }
    }

    fn should_run(&self) -> bool {
        Instant::now() >= *self.next_run.read()
    }

    fn mark_run(&self) {
        let now = Instant::now();
        *self.last_run.write() = Some(now);
        *self.next_run.write() = now + self.interval;
    }
}

// =============================================================================
// Job Scheduler
// =============================================================================

struct JobScheduler {
    jobs: DashMap<String, Job>,
    results: DashMap<String, JobResult>,
    status: DashMap<String, JobStatus>,
    queue: Mutex<BinaryHeap<QueuedJob>>,
    workers: Vec<Arc<Worker>>,
    dag: DagScheduler,
    cron_jobs: DashMap<String, CronJob>,
    job_counter: AtomicU64,
}

impl JobScheduler {
    fn new(num_workers: usize) -> Self {
        let workers = (0..num_workers)
            .map(|i| Arc::new(Worker::new(&format!("worker_{}", i))))
            .collect();

        Self {
            jobs: DashMap::new(),
            results: DashMap::new(),
            status: DashMap::new(),
            queue: Mutex::new(BinaryHeap::new()),
            workers,
            dag: DagScheduler::new(),
            cron_jobs: DashMap::new(),
            job_counter: AtomicU64::new(0),
        }
    }

    fn submit(&self, job: Job) -> String {
        let job_id = job.id.clone();

        // Register with DAG scheduler
        self.dag.register(&job);

        // Store job
        self.jobs.insert(job_id.clone(), job.clone());
        self.status.insert(job_id.clone(), JobStatus::Pending);

        // Add to queue if ready
        if self.dag.is_ready(&job_id) {
            self.enqueue(&job);
        }

        job_id
    }

    fn enqueue(&self, job: &Job) {
        self.status.insert(job.id.clone(), JobStatus::Queued);

        let queued = QueuedJob {
            job_id: job.id.clone(),
            priority: job.priority,
            scheduled_at: job.scheduled_at,
        };

        self.queue.lock().push(queued);
    }

    fn get_next_job(&self) -> Option<Job> {
        let mut queue = self.queue.lock();

        while let Some(queued) = queue.pop() {
            if let Some(job) = self.jobs.get(&queued.job_id) {
                let status = self.status.get(&queued.job_id);
                if status.map(|s| *s == JobStatus::Queued).unwrap_or(false) {
                    return Some(job.clone());
                }
            }
        }

        None
    }

    fn find_idle_worker(&self) -> Option<Arc<Worker>> {
        self.workers
            .iter()
            .find(|w| *w.status.read() == WorkerStatus::Idle)
            .cloned()
    }

    async fn run_job(&self, job: Job, worker: Arc<Worker>) {
        self.status.insert(job.id.clone(), JobStatus::Running);

        let started_at = Instant::now();
        let result = worker.execute(&job).await;
        let completed_at = Instant::now();

        match result {
            Ok(output) => {
                self.status.insert(job.id.clone(), JobStatus::Completed);

                let job_result = JobResult {
                    job_id: job.id.clone(),
                    status: JobStatus::Completed,
                    output: Some(output),
                    error: None,
                    started_at,
                    completed_at,
                    worker_id: worker.id.clone(),
                };
                self.results.insert(job.id.clone(), job_result);

                // Unblock dependent jobs
                let newly_ready = self.dag.complete(&job.id);
                for ready_id in newly_ready {
                    if let Some(ready_job) = self.jobs.get(&ready_id) {
                        self.enqueue(&ready_job);
                    }
                }
            }
            Err(error) => {
                // Check for retry
                if let Some(mut job) = self.jobs.get_mut(&job.id) {
                    job.retry_count += 1;

                    if job.retry_count <= job.max_retries {
                        // Retry with backoff
                        let backoff = Duration::from_millis(100 * (1 << job.retry_count));
                        job.scheduled_at = Instant::now() + backoff;
                        self.enqueue(&job);
                        return;
                    }
                }

                self.status.insert(job.id.clone(), JobStatus::Failed);

                let job_result = JobResult {
                    job_id: job.id.clone(),
                    status: JobStatus::Failed,
                    output: None,
                    error: Some(error),
                    started_at,
                    completed_at,
                    worker_id: worker.id.clone(),
                };
                self.results.insert(job.id.clone(), job_result);
            }
        }
    }

    fn register_cron(&self, cron: CronJob) {
        self.cron_jobs.insert(cron.id.clone(), cron);
    }

    fn check_cron_jobs(&self) {
        for entry in self.cron_jobs.iter() {
            let cron = entry.value();
            if cron.should_run() {
                let mut job = cron.job_template.clone();
                job.id = format!(
                    "{}_{}", 
                    cron.id, 
                    self.job_counter.fetch_add(1, Ordering::SeqCst)
                );
                job.scheduled_at = Instant::now();

                self.submit(job);
                cron.mark_run();
            }
        }
    }

    fn get_stats(&self) -> (usize, usize, usize, usize) {
        let pending = self
            .status
            .iter()
            .filter(|e| *e.value() == JobStatus::Pending || *e.value() == JobStatus::Queued)
            .count();
        let running = self
            .status
            .iter()
            .filter(|e| *e.value() == JobStatus::Running)
            .count();
        let completed = self
            .status
            .iter()
            .filter(|e| *e.value() == JobStatus::Completed)
            .count();
        let failed = self
            .status
            .iter()
            .filter(|e| *e.value() == JobStatus::Failed)
            .count();

        (pending, running, completed, failed)
    }
}

// =============================================================================
// Main Demo
// =============================================================================

#[tokio::main]
async fn main() {
    println!("=== Job Scheduler Demo ===\n");

    let scheduler = Arc::new(JobScheduler::new(4));

    // Submit jobs with different priorities
    println!("--- Submitting Jobs ---");

    let jobs = vec![
        Job {
            id: "job_1".to_string(),
            name: "High Priority Task".to_string(),
            priority: 10,
            scheduled_at: Instant::now(),
            payload: "Important data".to_string(),
            max_retries: 3,
            retry_count: 0,
            timeout: Duration::from_secs(30),
            dependencies: vec![],
        },
        Job {
            id: "job_2".to_string(),
            name: "Medium Priority Task".to_string(),
            priority: 5,
            scheduled_at: Instant::now(),
            payload: "Regular data".to_string(),
            max_retries: 2,
            retry_count: 0,
            timeout: Duration::from_secs(30),
            dependencies: vec![],
        },
        Job {
            id: "job_3".to_string(),
            name: "Low Priority Task".to_string(),
            priority: 1,
            scheduled_at: Instant::now(),
            payload: "Background data".to_string(),
            max_retries: 1,
            retry_count: 0,
            timeout: Duration::from_secs(30),
            dependencies: vec![],
        },
    ];

    for job in jobs {
        let id = scheduler.submit(job.clone());
        println!("  Submitted: {} (priority {})", id, job.priority);
    }
    println!();

    // Submit DAG with dependencies
    println!("--- Submitting DAG (A -> B, A -> C, B+C -> D) ---");

    let job_a = Job {
        id: "dag_a".to_string(),
        name: "DAG Job A".to_string(),
        priority: 5,
        scheduled_at: Instant::now(),
        payload: "First".to_string(),
        max_retries: 2,
        retry_count: 0,
        timeout: Duration::from_secs(30),
        dependencies: vec![],
    };

    let job_b = Job {
        id: "dag_b".to_string(),
        name: "DAG Job B (depends on A)".to_string(),
        priority: 5,
        scheduled_at: Instant::now(),
        payload: "Second".to_string(),
        max_retries: 2,
        retry_count: 0,
        timeout: Duration::from_secs(30),
        dependencies: vec!["dag_a".to_string()],
    };

    let job_c = Job {
        id: "dag_c".to_string(),
        name: "DAG Job C (depends on A)".to_string(),
        priority: 5,
        scheduled_at: Instant::now(),
        payload: "Third".to_string(),
        max_retries: 2,
        retry_count: 0,
        timeout: Duration::from_secs(30),
        dependencies: vec!["dag_a".to_string()],
    };

    let job_d = Job {
        id: "dag_d".to_string(),
        name: "DAG Job D (depends on B and C)".to_string(),
        priority: 5,
        scheduled_at: Instant::now(),
        payload: "Final".to_string(),
        max_retries: 2,
        retry_count: 0,
        timeout: Duration::from_secs(30),
        dependencies: vec!["dag_b".to_string(), "dag_c".to_string()],
    };

    scheduler.submit(job_a);
    scheduler.submit(job_b);
    scheduler.submit(job_c);
    scheduler.submit(job_d);

    println!("  dag_a ready: {}", scheduler.dag.is_ready("dag_a"));
    println!("  dag_b ready: {}", scheduler.dag.is_ready("dag_b"));
    println!("  dag_d ready: {}", scheduler.dag.is_ready("dag_d"));
    println!();

    // Process jobs
    println!("--- Processing Jobs ---");

    for _ in 0..20 {
        if let Some(job) = scheduler.get_next_job() {
            if let Some(worker) = scheduler.find_idle_worker() {
                let s = Arc::clone(&scheduler);
                tokio::spawn(async move {
                    s.run_job(job, worker).await;
                });
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    // Wait for completion
    sleep(Duration::from_millis(500)).await;

    // Show results
    println!("\n--- Results ---");
    let (pending, running, completed, failed) = scheduler.get_stats();
    println!("Pending: {}, Running: {}, Completed: {}, Failed: {}", pending, running, completed, failed);

    println!("\nJob results:");
    for entry in scheduler.results.iter() {
        let r = entry.value();
        let duration = r.completed_at.duration_since(r.started_at);
        println!(
            "  {} - {:?} ({}ms, worker: {})",
            r.job_id,
            r.status,
            duration.as_millis(),
            r.worker_id
        );
    }

    // Worker stats
    println!("\n--- Worker Stats ---");
    for worker in &scheduler.workers {
        println!(
            "  {}: completed={}, failed={}",
            worker.id,
            worker.jobs_completed.load(Ordering::SeqCst),
            worker.jobs_failed.load(Ordering::SeqCst)
        );
    }

    println!("\n=== Key Concepts ===");
    println!("1. Priority Queue: Higher priority jobs execute first");
    println!("2. Worker Pool: Multiple workers for parallel execution");
    println!("3. DAG: Jobs with dependencies execute in order");
    println!("4. Retry + Backoff: Failed jobs retry with exponential delay");
    println!("5. Status Tracking: Pending -> Queued -> Running -> Completed/Failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_queue_order() {
        let mut queue = BinaryHeap::new();

        queue.push(QueuedJob {
            job_id: "low".to_string(),
            priority: 1,
            scheduled_at: Instant::now(),
        });
        queue.push(QueuedJob {
            job_id: "high".to_string(),
            priority: 10,
            scheduled_at: Instant::now(),
        });
        queue.push(QueuedJob {
            job_id: "medium".to_string(),
            priority: 5,
            scheduled_at: Instant::now(),
        });

        assert_eq!(queue.pop().unwrap().job_id, "high");
        assert_eq!(queue.pop().unwrap().job_id, "medium");
        assert_eq!(queue.pop().unwrap().job_id, "low");
    }

    #[test]
    fn test_dag_dependencies() {
        let dag = DagScheduler::new();

        let job_a = Job {
            id: "a".to_string(),
            name: "A".to_string(),
            priority: 5,
            scheduled_at: Instant::now(),
            payload: "".to_string(),
            max_retries: 0,
            retry_count: 0,
            timeout: Duration::from_secs(30),
            dependencies: vec![],
        };

        let job_b = Job {
            id: "b".to_string(),
            name: "B".to_string(),
            priority: 5,
            scheduled_at: Instant::now(),
            payload: "".to_string(),
            max_retries: 0,
            retry_count: 0,
            timeout: Duration::from_secs(30),
            dependencies: vec!["a".to_string()],
        };

        dag.register(&job_a);
        dag.register(&job_b);

        assert!(dag.is_ready("a"));
        assert!(!dag.is_ready("b"));

        let ready = dag.complete("a");
        assert!(ready.contains(&"b".to_string()));
        assert!(dag.is_ready("b"));
    }
}
