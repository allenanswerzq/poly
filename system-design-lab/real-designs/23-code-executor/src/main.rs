//! # Code Executor (LeetCode) - Mini Implementation
//!
//! Demonstrates:
//! - Sandboxed execution
//! - Resource limits (time, memory)
//! - Test case validation
//! - Execution queuing
//!
//! Run: cargo run -p code-executor

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Problem {
    id: String,
    title: String,
    description: String,
    test_cases: Vec<TestCase>,
    time_limit_ms: u64,
    memory_limit_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestCase {
    input: String,
    expected_output: String,
    is_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Submission {
    id: u64,
    problem_id: String,
    user_id: String,
    language: Language,
    code: String,
    submitted_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum Language {
    Python,
    Rust,
    JavaScript,
    Cpp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ExecutionResult {
    Accepted {
        runtime_ms: u64,
        memory_kb: u64,
    },
    WrongAnswer {
        test_case: usize,
        expected: String,
        actual: String,
    },
    TimeLimitExceeded {
        test_case: usize,
    },
    MemoryLimitExceeded {
        test_case: usize,
    },
    RuntimeError {
        test_case: usize,
        error: String,
    },
    CompilationError {
        error: String,
    },
}

#[derive(Debug, Clone)]
struct SubmissionResult {
    submission_id: u64,
    status: ExecutionResult,
    test_results: Vec<TestCaseResult>,
    total_runtime_ms: u64,
    total_memory_kb: u64,
}

#[derive(Debug, Clone)]
struct TestCaseResult {
    passed: bool,
    runtime_ms: u64,
    memory_kb: u64,
    output: Option<String>,
}

// =============================================================================
// Sandbox Executor
// =============================================================================

struct Sandbox {
    timeout: Duration,
    memory_limit_kb: u64,
}

impl Sandbox {
    fn new(timeout: Duration, memory_limit_kb: u64) -> Self {
        Self {
            timeout,
            memory_limit_kb,
        }
    }

    fn execute(
        &self,
        _code: &str,
        _language: Language,
        input: &str,
    ) -> Result<(String, u64, u64), String> {
        // Simulated execution - in real system, this would:
        // 1. Spawn isolated container/sandbox
        // 2. Compile code if needed
        // 3. Run with resource limits (cgroups, seccomp)
        // 4. Capture stdout/stderr

        let start = Instant::now();
        let mut rng = rand::thread_rng();

        // Simulate random execution time
        let runtime_ms = rng.gen_range(1..100);
        let memory_kb = rng.gen_range(1000..10000);

        // Simulate timeout check
        if runtime_ms as u64 > self.timeout.as_millis() as u64 {
            return Err("Time limit exceeded".to_string());
        }

        // Simulate memory check
        if memory_kb > self.memory_limit_kb {
            return Err("Memory limit exceeded".to_string());
        }

        // Simulate output (in real system, this comes from actual execution)
        // For demo, we'll echo input as output (most tests expect transformation)
        let output = format!("result_for_{}", input.trim());

        Ok((output, start.elapsed().as_millis() as u64, memory_kb))
    }
}

// =============================================================================
// Code Runner
// =============================================================================

struct CodeRunner {
    sandbox: Sandbox,
}

impl CodeRunner {
    fn new(timeout: Duration, memory_limit_kb: u64) -> Self {
        Self {
            sandbox: Sandbox::new(timeout, memory_limit_kb),
        }
    }

    fn compile(&self, code: &str, language: Language) -> Result<(), String> {
        // Simulated compilation
        match language {
            Language::Rust | Language::Cpp => {
                // Check for syntax errors (simplified)
                if code.contains("syntax_error") {
                    return Err("Compilation failed: syntax error".to_string());
                }
                Ok(())
            }
            Language::Python | Language::JavaScript => {
                // Interpreted - basic syntax check
                Ok(())
            }
        }
    }

    fn run_test(
        &self,
        code: &str,
        language: Language,
        test_case: &TestCase,
    ) -> TestCaseResult {
        match self.sandbox.execute(code, language, &test_case.input) {
            Ok((output, runtime_ms, memory_kb)) => {
                let passed = output.trim() == test_case.expected_output.trim();
                TestCaseResult {
                    passed,
                    runtime_ms,
                    memory_kb,
                    output: Some(output),
                }
            }
            Err(error) => TestCaseResult {
                passed: false,
                runtime_ms: 0,
                memory_kb: 0,
                output: Some(error),
            },
        }
    }

    fn run_all_tests(
        &self,
        code: &str,
        language: Language,
        problem: &Problem,
    ) -> SubmissionResult {
        // First compile
        if let Err(error) = self.compile(code, language) {
            return SubmissionResult {
                submission_id: 0,
                status: ExecutionResult::CompilationError { error },
                test_results: vec![],
                total_runtime_ms: 0,
                total_memory_kb: 0,
            };
        }

        let mut test_results = Vec::new();
        let mut total_runtime = 0u64;
        let mut max_memory = 0u64;

        for (i, test_case) in problem.test_cases.iter().enumerate() {
            let result = self.run_test(code, language, test_case);

            if !result.passed {
                // Determine failure type
                let status = if result.output.as_deref() == Some("Time limit exceeded") {
                    ExecutionResult::TimeLimitExceeded { test_case: i }
                } else if result.output.as_deref() == Some("Memory limit exceeded") {
                    ExecutionResult::MemoryLimitExceeded { test_case: i }
                } else if let Some(output) = &result.output {
                    if output.contains("error") {
                        ExecutionResult::RuntimeError {
                            test_case: i,
                            error: output.clone(),
                        }
                    } else {
                        ExecutionResult::WrongAnswer {
                            test_case: i,
                            expected: test_case.expected_output.clone(),
                            actual: output.clone(),
                        }
                    }
                } else {
                    ExecutionResult::WrongAnswer {
                        test_case: i,
                        expected: test_case.expected_output.clone(),
                        actual: String::new(),
                    }
                };

                test_results.push(result);
                return SubmissionResult {
                    submission_id: 0,
                    status,
                    test_results,
                    total_runtime_ms: total_runtime,
                    total_memory_kb: max_memory,
                };
            }

            total_runtime += result.runtime_ms;
            max_memory = max_memory.max(result.memory_kb);
            test_results.push(result);
        }

        SubmissionResult {
            submission_id: 0,
            status: ExecutionResult::Accepted {
                runtime_ms: total_runtime,
                memory_kb: max_memory,
            },
            test_results,
            total_runtime_ms: total_runtime,
            total_memory_kb: max_memory,
        }
    }
}

// =============================================================================
// Execution Queue
// =============================================================================

struct ExecutionQueue {
    queue: Mutex<VecDeque<Submission>>,
    processing: DashMap<u64, Submission>,
    results: DashMap<u64, SubmissionResult>,
    next_id: AtomicU64,
}

impl ExecutionQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            processing: DashMap::new(),
            results: DashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    fn enqueue(&self, mut submission: Submission) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        submission.id = id;
        self.queue.lock().push_back(submission);
        id
    }

    fn dequeue(&self) -> Option<Submission> {
        let submission = self.queue.lock().pop_front()?;
        self.processing.insert(submission.id, submission.clone());
        Some(submission)
    }

    fn complete(&self, submission_id: u64, result: SubmissionResult) {
        self.processing.remove(&submission_id);
        self.results.insert(submission_id, result);
    }

    fn get_result(&self, submission_id: u64) -> Option<SubmissionResult> {
        self.results.get(&submission_id).map(|r| r.clone())
    }

    fn queue_length(&self) -> usize {
        self.queue.lock().len()
    }
}

// =============================================================================
// Problem Store
// =============================================================================

struct ProblemStore {
    problems: DashMap<String, Problem>,
}

impl ProblemStore {
    fn new() -> Self {
        Self {
            problems: DashMap::new(),
        }
    }

    fn add(&self, problem: Problem) {
        self.problems.insert(problem.id.clone(), problem);
    }

    fn get(&self, id: &str) -> Option<Problem> {
        self.problems.get(id).map(|p| p.clone())
    }

    fn create_sample_problems(&self) {
        // Two Sum
        self.add(Problem {
            id: "two-sum".to_string(),
            title: "Two Sum".to_string(),
            description: "Find two numbers that add up to target".to_string(),
            test_cases: vec![
                TestCase {
                    input: "[2,7,11,15], 9".to_string(),
                    expected_output: "result_for_[2,7,11,15], 9".to_string(),
                    is_hidden: false,
                },
                TestCase {
                    input: "[3,2,4], 6".to_string(),
                    expected_output: "result_for_[3,2,4], 6".to_string(),
                    is_hidden: false,
                },
                TestCase {
                    input: "[3,3], 6".to_string(),
                    expected_output: "result_for_[3,3], 6".to_string(),
                    is_hidden: true,
                },
            ],
            time_limit_ms: 1000,
            memory_limit_kb: 65536,
        });

        // Reverse String
        self.add(Problem {
            id: "reverse-string".to_string(),
            title: "Reverse String".to_string(),
            description: "Reverse a string in place".to_string(),
            test_cases: vec![
                TestCase {
                    input: "hello".to_string(),
                    expected_output: "result_for_hello".to_string(),
                    is_hidden: false,
                },
            ],
            time_limit_ms: 500,
            memory_limit_kb: 32768,
        });
    }
}

// =============================================================================
// Leaderboard
// =============================================================================

#[derive(Debug, Clone)]
struct LeaderboardEntry {
    user_id: String,
    problem_id: String,
    runtime_ms: u64,
    memory_kb: u64,
    submitted_at: u64,
}

struct Leaderboard {
    entries: RwLock<Vec<LeaderboardEntry>>,
}

impl Leaderboard {
    fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }

    fn add(&self, entry: LeaderboardEntry) {
        let mut entries = self.entries.write();

        // Remove old entry from same user for same problem
        entries.retain(|e| !(e.user_id == entry.user_id && e.problem_id == entry.problem_id));

        entries.push(entry);

        // Sort by runtime
        entries.sort_by_key(|e| e.runtime_ms);
    }

    fn top(&self, problem_id: &str, n: usize) -> Vec<LeaderboardEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.problem_id == problem_id)
            .take(n)
            .cloned()
            .collect()
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Code Executor (LeetCode) Demo ===\n");

    // Initialize system
    let problems = ProblemStore::new();
    problems.create_sample_problems();

    let queue = ExecutionQueue::new();
    let runner = CodeRunner::new(Duration::from_secs(1), 65536);
    let leaderboard = Leaderboard::new();

    // Submit solution
    println!("\n  ═══ Submitting Solution ═══");
    let submission = Submission {
        id: 0,
        problem_id: "two-sum".to_string(),
        user_id: "alice".to_string(),
        language: Language::Python,
        code: r#"
def twoSum(nums, target):
    seen = {}
    for i, num in enumerate(nums):
        complement = target - num
        if complement in seen:
            return [seen[complement], i]
        seen[num] = i
"#
        .to_string(),
        submitted_at: 1234567890,
    };

    let submission_id = queue.enqueue(submission.clone());
    println!("Submitted! ID: {}", submission_id);
    println!("Queue length: {}", queue.queue_length());

    // Process submission
    println!("\n--- Processing ---");
    if let Some(sub) = queue.dequeue() {
        println!("Executing submission {} for '{}'", sub.id, sub.problem_id);

        if let Some(problem) = problems.get(&sub.problem_id) {
            let mut result = runner.run_all_tests(&sub.code, sub.language, &problem);
            result.submission_id = sub.id;

            match &result.status {
                ExecutionResult::Accepted { runtime_ms, memory_kb } => {
                    println!("✅ Accepted!");
                    println!("   Runtime: {} ms", runtime_ms);
                    println!("   Memory: {} KB", memory_kb);
                    println!("   Tests passed: {}/{}", result.test_results.len(), problem.test_cases.len());

                    // Add to leaderboard
                    leaderboard.add(LeaderboardEntry {
                        user_id: sub.user_id.clone(),
                        problem_id: sub.problem_id.clone(),
                        runtime_ms: *runtime_ms,
                        memory_kb: *memory_kb,
                        submitted_at: sub.submitted_at,
                    });
                }
                ExecutionResult::WrongAnswer { test_case, expected, actual } => {
                    println!("❌ Wrong Answer on test case {}", test_case);
                    println!("   Expected: {}", expected);
                    println!("   Actual: {}", actual);
                }
                ExecutionResult::TimeLimitExceeded { test_case } => {
                    println!("⏰ Time Limit Exceeded on test case {}", test_case);
                }
                ExecutionResult::MemoryLimitExceeded { test_case } => {
                    println!("💾 Memory Limit Exceeded on test case {}", test_case);
                }
                ExecutionResult::RuntimeError { test_case, error } => {
                    println!("💥 Runtime Error on test case {}: {}", test_case, error);
                }
                ExecutionResult::CompilationError { error } => {
                    println!("🔧 Compilation Error: {}", error);
                }
            }

            queue.complete(sub.id, result);
        }
    }

    // Test compilation error
    println!("\n--- Testing Compilation Error ---");
    let bad_submission = Submission {
        id: 0,
        problem_id: "two-sum".to_string(),
        user_id: "bob".to_string(),
        language: Language::Rust,
        code: "fn main() { syntax_error }".to_string(),
        submitted_at: 1234567891,
    };

    if let Some(problem) = problems.get(&bad_submission.problem_id) {
        let result = runner.run_all_tests(&bad_submission.code, bad_submission.language, &problem);
        if let ExecutionResult::CompilationError { error } = result.status {
            println!("🔧 Compilation Error: {}", error);
        }
    }

    // Leaderboard
    println!("\n--- Leaderboard ---");

    // Add more fake entries
    leaderboard.add(LeaderboardEntry {
        user_id: "charlie".to_string(),
        problem_id: "two-sum".to_string(),
        runtime_ms: 45,
        memory_kb: 15000,
        submitted_at: 1234567800,
    });
    leaderboard.add(LeaderboardEntry {
        user_id: "dave".to_string(),
        problem_id: "two-sum".to_string(),
        runtime_ms: 32,
        memory_kb: 18000,
        submitted_at: 1234567700,
    });

    println!("Top 5 for 'two-sum':");
    for (i, entry) in leaderboard.top("two-sum", 5).iter().enumerate() {
        println!(
            "  {}. {} - {} ms, {} KB",
            i + 1,
            entry.user_id,
            entry.runtime_ms,
            entry.memory_kb
        );
    }

    // Show problem info
    println!("\n--- Problem Info ---");
    if let Some(problem) = problems.get("two-sum") {
        println!("Title: {}", problem.title);
        println!("Description: {}", problem.description);
        println!("Time Limit: {} ms", problem.time_limit_ms);
        println!("Memory Limit: {} KB", problem.memory_limit_kb);
        println!(
            "Test Cases: {} ({} hidden)",
            problem.test_cases.len(),
            problem.test_cases.iter().filter(|t| t.is_hidden).count()
        );
    }

    println!("\n=== Key Concepts ===");
    println!("1. Sandboxing: Isolate code execution (containers, seccomp)");
    println!("2. Resource Limits: Enforce time & memory limits (cgroups)");
    println!("3. Test Validation: Compare output against expected");
    println!("4. Execution Queue: Handle concurrent submissions");
    println!("5. Compilation: Support multiple languages");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_queue() {
        let queue = ExecutionQueue::new();

        let sub = Submission {
            id: 0,
            problem_id: "test".to_string(),
            user_id: "user".to_string(),
            language: Language::Python,
            code: "print('hello')".to_string(),
            submitted_at: 0,
        };

        let id = queue.enqueue(sub);
        assert!(id > 0);
        assert_eq!(queue.queue_length(), 1);

        let dequeued = queue.dequeue();
        assert!(dequeued.is_some());
        assert_eq!(queue.queue_length(), 0);
    }

    #[test]
    fn test_compile_error() {
        let runner = CodeRunner::new(Duration::from_secs(1), 65536);

        let result = runner.compile("syntax_error", Language::Rust);
        assert!(result.is_err());
    }

    #[test]
    fn test_leaderboard() {
        let lb = Leaderboard::new();

        lb.add(LeaderboardEntry {
            user_id: "a".to_string(),
            problem_id: "p1".to_string(),
            runtime_ms: 100,
            memory_kb: 1000,
            submitted_at: 0,
        });

        lb.add(LeaderboardEntry {
            user_id: "b".to_string(),
            problem_id: "p1".to_string(),
            runtime_ms: 50,
            memory_kb: 1000,
            submitted_at: 0,
        });

        let top = lb.top("p1", 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].user_id, "b"); // Faster first
    }
}
