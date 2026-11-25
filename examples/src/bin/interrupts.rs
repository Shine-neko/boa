//! Interrupting the engine from another thread: a sampling profiler, which
//! records the JavaScript stack every millisecond and lets the script go on,
//! and a timeout, which stops a script running for too long.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use boa_engine::context::{ContextBuilder, HostHooks};
use boa_engine::{Context, JsNativeError, JsResult, Source};

/// What the interrupts are for.
#[derive(Default)]
struct Hooks {
    /// Samples by stack, outermost function first.
    samples: RefCell<HashMap<Vec<String>, u32>>,
    /// When set, the script is stopped past it.
    deadline: Cell<Option<Instant>>,
}

impl HostHooks for Hooks {
    fn interrupt(&self, context: &mut Context) -> JsResult<()> {
        if self.deadline.get().is_some_and(|d| Instant::now() >= d) {
            return Err(JsNativeError::error()
                .with_message("script timed out")
                .into());
        }
        let mut stack: Vec<String> = context
            .stack_trace()
            .map(|frame| frame.position().function_name.to_std_string_escaped())
            .collect();
        stack.reverse();
        *self.samples.borrow_mut().entry(stack).or_default() += 1;
        Ok(())
    }
}

/// Requests an interrupt every `interval` until `stop` is set.
fn ticker(context: &Context, interval: Duration, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    let handle = context.interrupt_handle();
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(interval);
            handle.request();
        }
    })
}

const PROGRAM: &str = r"
function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
function mix(x) { return (x * 31 + 7) % 1009; }
function work() { let s = 0; for (let i = 0; i < 2000000; i++) s += mix(i); return s; }
fib(25) + work();
";

fn main() -> JsResult<()> {
    let hooks = Rc::new(Hooks::default());
    let mut context = ContextBuilder::default()
        .host_hooks(hooks.clone())
        .build()?;

    // Sampling: every millisecond, the stack of the function running.
    let stop = Arc::new(AtomicBool::new(false));
    let thread = ticker(&context, Duration::from_millis(1), stop.clone());
    let result = context.eval(Source::from_bytes(PROGRAM))?;
    stop.store(true, Ordering::Relaxed);
    thread.join().expect("the ticker does not panic");
    println!("result: {}", result.display());

    // Time spent in each function itself, and with the functions it called.
    let mut own: HashMap<String, u32> = HashMap::new();
    let mut total: HashMap<String, u32> = HashMap::new();
    let mut samples = 0;
    for (stack, count) in hooks.samples.borrow().iter() {
        samples += count;
        if let Some(leaf) = stack.last() {
            *own.entry(leaf.clone()).or_default() += count;
        }
        let mut seen: Vec<&String> = stack.iter().collect();
        seen.sort();
        seen.dedup();
        for function in seen {
            *total.entry(function.clone()).or_default() += count;
        }
    }
    let mut functions: Vec<_> = own.into_iter().collect();
    functions.sort_by_key(|f| std::cmp::Reverse(f.1));
    println!("{samples} samples");
    println!("{:>6} {:>6}  function", "own", "total");
    for (function, count) in functions {
        println!("{count:>6} {:>6}  {function}", total[&function]);
    }

    // Timeout: an endless loop, stopped after 100 ms.
    hooks
        .deadline
        .set(Some(Instant::now() + Duration::from_millis(100)));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = ticker(&context, Duration::from_millis(10), stop.clone());
    let error = context
        .eval(Source::from_bytes("while (true) {}"))
        .expect_err("the loop never ends by itself");
    stop.store(true, Ordering::Relaxed);
    thread.join().expect("the ticker does not panic");
    println!("endless loop: {error}");
    Ok(())
}
