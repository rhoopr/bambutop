# Rust Best Practices & Coding Standards

An opinionated guide for writing maintainable, performant, and idiomatic Rust code.

---

## Table of Contents

1. [Error Handling](#error-handling)
2. [String Handling](#string-handling)
3. [Memory & Allocations](#memory--allocations)
4. [Constants & Magic Numbers](#constants--magic-numbers)
5. [Function Design](#function-design)
6. [Module Organization](#module-organization)
7. [Async Patterns](#async-patterns)
8. [Documentation](#documentation)
9. [Testing](#testing)

---

## Error Handling

### Use `thiserror` for Library Errors, `anyhow` for Applications

```rust
// GOOD: Custom error type with thiserror for library code
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    ReadFailed(#[from] std::io::Error),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

// GOOD: anyhow for application-level error propagation
fn main() -> anyhow::Result<()> {
    let config = load_config().context("failed to load configuration")?;
    Ok(())
}
```

### Always Add Context to Errors

```rust
// BAD: Raw error propagation loses context
let file = File::open(path)?;

// GOOD: Add context explaining what operation failed
let file = File::open(path)
    .with_context(|| format!("failed to open config file: {}", path.display()))?;
```

### Never Use Bare `unwrap()` in Production Code

```rust
// BAD: Panics without explanation
let value = map.get("key").unwrap();

// ACCEPTABLE: expect() with clear explanation
let value = map.get("key").expect("key should always exist after initialization");

// BEST: Proper error handling
let value = map.get("key").ok_or_else(|| anyhow!("missing required key"))?;
```

### Match on Specific Error Variants When Recovery is Possible

```rust
// GOOD: Handle specific cases, propagate others
match fs::read_to_string(path) {
    Ok(content) => Ok(content),
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
    Err(e) => Err(e).context("failed to read file")?,
}
```

### Use `Option` for Absence, `Result` for Failure

```rust
// GOOD: Option for "might not exist"
fn find_user(&self, id: UserId) -> Option<&User>

// GOOD: Result for "operation that can fail"
fn load_user(&self, id: UserId) -> Result<User, LoadError>
```

---

## String Handling

### Use `&'static str` for Constant String Returns

```rust
// BAD: Unnecessary allocation
fn status_text(code: u8) -> String {
    match code {
        0 => "idle".to_string(),
        1 => "running".to_string(),
        _ => "unknown".to_string(),
    }
}

// GOOD: Zero-cost static strings
fn status_text(code: u8) -> &'static str {
    match code {
        0 => "idle",
        1 => "running",
        _ => "unknown",
    }
}
```

### Use `Cow<'static, str>` When Most Returns Are Static

```rust
use std::borrow::Cow;

// GOOD: Static for known values, owned only when needed
fn error_message(code: u32) -> Cow<'static, str> {
    match code {
        404 => Cow::Borrowed("not found"),
        500 => Cow::Borrowed("internal error"),
        code => Cow::Owned(format!("error code: {}", code)),
    }
}
```

### Prefer `&str` Over `String` in Function Parameters

```rust
// BAD: Forces caller to allocate
fn greet(name: String) -> String

// GOOD: Accepts both &str and String
fn greet(name: &str) -> String

// ALSO GOOD: Generic for maximum flexibility
fn greet(name: impl AsRef<str>) -> String
```

### Use `as_deref()` to Avoid Cloning Options

```rust
// BAD: Clones the inner String
let name = optional_name.clone().unwrap_or_default();

// GOOD: Borrows the inner &str
let name = optional_name.as_deref().unwrap_or_default();
```

### Use `write!` for Complex String Building

```rust
use std::fmt::Write;

// BAD: Multiple allocations with format!
let mut result = String::new();
result.push_str(&format!("Name: {}\n", name));
result.push_str(&format!("Age: {}\n", age));

// GOOD: Single buffer, no intermediate allocations
let mut result = String::new();
write!(&mut result, "Name: {}\nAge: {}\n", name, age).unwrap();
```

---

## Memory & Allocations

### Pre-allocate Collections When Size is Known

```rust
// BAD: Multiple reallocations as vec grows
let mut items = Vec::new();
for i in 0..1000 {
    items.push(i);
}

// GOOD: Single allocation
let mut items = Vec::with_capacity(1000);
for i in 0..1000 {
    items.push(i);
}

// ALSO GOOD: Use collect with size hint
let items: Vec<_> = (0..1000).collect();
```

### Avoid Allocations in Hot Paths

```rust
// BAD: Allocates on every call
fn render_status(&self) -> String {
    format!("Status: {}", self.status)
}

// GOOD: Write directly to buffer
fn render_status(&self, buf: &mut String) {
    write!(buf, "Status: {}", self.status).unwrap();
}

// ALSO GOOD: Return reference when possible
fn status(&self) -> &str {
    &self.status
}
```

### Cache Expensive Computations

```rust
// BAD: Parses color on every render call
fn render(&self) {
    let color = parse_hex_color(&self.color_hex);
    // use color...
}

// GOOD: Parse once, store the result
struct Widget {
    color: Color, // Pre-parsed at construction
}

impl Widget {
    fn new(color_hex: &str) -> Self {
        Self {
            color: parse_hex_color(color_hex),
        }
    }
}
```

### Use `Box` for Large Stack Values or Recursive Types

```rust
// GOOD: Large data on heap
struct Config {
    data: Box<[u8; 1_000_000]>,
}

// GOOD: Recursive type requires indirection
enum Tree {
    Leaf(i32),
    Node(Box<Tree>, Box<Tree>),
}
```

### Prefer `&[T]` Over `&Vec<T>` in Function Parameters

```rust
// BAD: Requires Vec specifically
fn sum(numbers: &Vec<i32>) -> i32

// GOOD: Accepts any contiguous slice
fn sum(numbers: &[i32]) -> i32
```

---

## Constants & Magic Numbers

### Extract All Magic Numbers as Named Constants

```rust
// BAD: Magic numbers scattered in code
if temperature > 250.0 {
    warn!("High temperature!");
}
sleep(Duration::from_secs(30));

// GOOD: Named constants with clear meaning
const MAX_SAFE_TEMPERATURE: f64 = 250.0;
const RECONNECT_INTERVAL: Duration = Duration::from_secs(30);

if temperature > MAX_SAFE_TEMPERATURE {
    warn!("High temperature!");
}
sleep(RECONNECT_INTERVAL);
```

### Group Related Constants in Modules or Impl Blocks

```rust
// GOOD: Related constants grouped together
mod mqtt {
    pub const DEFAULT_PORT: u16 = 8883;
    pub const KEEPALIVE_SECS: u64 = 30;
    pub const MAX_RECONNECT_ATTEMPTS: u32 = 5;
}

// ALSO GOOD: Constants in impl block for associated values
impl Connection {
    const TIMEOUT: Duration = Duration::from_secs(10);
    const MAX_RETRIES: u32 = 3;
}
```

### Use `const fn` Where Possible

```rust
// GOOD: Computed at compile time
const fn calculate_buffer_size(count: usize) -> usize {
    count * std::mem::size_of::<u64>() + HEADER_SIZE
}

const BUFFER_SIZE: usize = calculate_buffer_size(100);
```

### Prefer `const` Over `static` Unless Mutability Required

```rust
// GOOD: Compile-time constant, inlined
const MAX_CONNECTIONS: usize = 100;

// USE ONLY WHEN NEEDED: Runtime initialization or interior mutability
static CONFIG: OnceLock<Config> = OnceLock::new();
```

---

## Function Design

### Keep Functions Short and Focused

A function should do one thing. If you need to describe it with "and", split it.

```rust
// BAD: Does too many things
fn process_and_save_and_notify(data: Data) -> Result<()>

// GOOD: Single responsibility
fn process(data: &Data) -> ProcessedData
fn save(data: &ProcessedData) -> Result<()>
fn notify(result: &SaveResult) -> Result<()>
```

### Use Builder Pattern for Complex Construction

```rust
// GOOD: Readable, flexible construction
let config = ConfigBuilder::new()
    .host("localhost")
    .port(8883)
    .timeout(Duration::from_secs(30))
    .build()?;
```

### Prefer Returning Values Over Output Parameters

```rust
// BAD: Output parameter
fn parse(input: &str, output: &mut Vec<Token>)

// GOOD: Return the result
fn parse(input: &str) -> Vec<Token>
```

### Use `impl Trait` in Return Position for Iterators

```rust
// GOOD: Hides concrete iterator type
fn active_items(&self) -> impl Iterator<Item = &Item> {
    self.items.iter().filter(|i| i.is_active())
}
```

### Take Ownership Only When Necessary

```rust
// BAD: Takes ownership unnecessarily
fn validate(data: String) -> bool

// GOOD: Borrows when not consuming
fn validate(data: &str) -> bool

// GOOD: Takes ownership when storing or transforming
fn store(data: String) { self.data = data; }
```

---

## Module Organization

### One Public Type Per File (for significant types)

```rust
// src/printer.rs - Contains Printer struct and its impl
// src/config.rs - Contains Config struct and its impl
// src/error.rs - Contains error types (exception: errors can be grouped)
```

### Use `mod.rs` Only for Re-exports

```rust
// src/ui/mod.rs
mod header;
mod progress;
mod status;

pub use header::Header;
pub use progress::ProgressBar;
pub use status::StatusPanel;
```

### Keep Module Depth Shallow

Prefer flat structure over deep nesting. Three levels max is a good rule of thumb:

```
src/
├── main.rs
├── lib.rs
├── config.rs
├── error.rs
└── handlers/
    ├── mod.rs
    ├── mqtt.rs
    └── http.rs
```

### Order Items Consistently Within Modules

1. `use` statements (std, external crates, internal modules)
2. `const` and `static` items
3. Type definitions (`struct`, `enum`, `type`)
4. `impl` blocks (inherent, then trait impls alphabetically)
5. Free functions
6. `#[cfg(test)]` module at the end

---

## Async Patterns

### Prefer `tokio` for I/O-bound Async, `rayon` for CPU-bound Parallelism

```rust
// GOOD: I/O-bound work with tokio
async fn fetch_data(url: &str) -> Result<Data> {
    let response = reqwest::get(url).await?;
    Ok(response.json().await?)
}

// GOOD: CPU-bound work with rayon
fn process_items(items: &[Item]) -> Vec<Processed> {
    items.par_iter().map(process_item).collect()
}
```

### Use `select!` for Racing Futures

```rust
// GOOD: Handle whichever completes first
tokio::select! {
    result = operation() => handle_result(result),
    _ = tokio::time::sleep(TIMEOUT) => handle_timeout(),
    _ = shutdown.recv() => return,
}
```

### Always Use Timeouts for Network Operations

```rust
// BAD: Can hang forever
let response = client.get(url).send().await?;

// GOOD: Bounded wait time
let response = tokio::time::timeout(
    Duration::from_secs(30),
    client.get(url).send()
).await??;
```

### Don't Hold Locks Across Await Points

```rust
// BAD: Lock held across await - can cause deadlock
let guard = mutex.lock().await;
do_async_work().await;  // Still holding lock!
drop(guard);

// GOOD: Release lock before awaiting
let data = {
    let guard = mutex.lock().await;
    guard.clone()
};
do_async_work().await;
```

### Use Channels for Async Communication

```rust
// GOOD: Bounded channel for backpressure
let (tx, rx) = tokio::sync::mpsc::channel(100);

// GOOD: Watch channel for shared state
let (tx, rx) = tokio::sync::watch::channel(initial_state);
```

---

## Documentation

### Document Public API with Examples

```rust
/// Connects to a Bambu printer via MQTT.
///
/// # Arguments
///
/// * `config` - Printer connection configuration
///
/// # Examples
///
/// ```
/// let printer = Printer::connect(&config).await?;
/// println!("Connected to: {}", printer.name());
/// ```
///
/// # Errors
///
/// Returns an error if the connection fails or authentication is rejected.
pub async fn connect(config: &Config) -> Result<Self>
```

### Use `//!` for Module-Level Documentation

```rust
//! MQTT client for Bambu Lab printer communication.
//!
//! This module handles the MQTT connection, message parsing, and
//! state synchronization with Bambu Lab 3D printers.

use ...
```

### Document Non-Obvious Behavior

```rust
/// Updates printer state from an MQTT message.
///
/// Note: Bambu printers send partial updates, so this merges
/// the new values with existing state rather than replacing it.
pub fn update_from_message(&mut self, msg: &Message)
```

### Use `# Safety` for Unsafe Code

```rust
/// Reinterprets the byte buffer as a struct.
///
/// # Safety
///
/// The caller must ensure:
/// - `bytes` is properly aligned for `T`
/// - `bytes.len() >= size_of::<T>()`
/// - The bytes represent a valid `T`
pub unsafe fn cast<T>(bytes: &[u8]) -> &T
```

---

## Testing

### Test Behavior, Not Implementation

```rust
// BAD: Tests internal structure
#[test]
fn test_internal_vec_has_three_items() {
    let widget = Widget::new();
    assert_eq!(widget.internal_vec.len(), 3);
}

// GOOD: Tests observable behavior
#[test]
fn test_widget_processes_all_inputs() {
    let widget = Widget::new();
    let results = widget.process(&[1, 2, 3]);
    assert_eq!(results, vec![2, 4, 6]);
}
```

### Use Descriptive Test Names

```rust
// BAD: Vague name
#[test]
fn test_parse()

// GOOD: Describes scenario and expectation
#[test]
fn parse_returns_none_for_empty_input()

#[test]
fn parse_extracts_temperature_from_valid_json()
```

### Group Related Tests with Nested Modules

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod parsing {
        use super::*;

        #[test]
        fn handles_valid_input() { ... }

        #[test]
        fn returns_error_for_malformed_input() { ... }
    }

    mod validation {
        use super::*;

        #[test]
        fn accepts_values_in_range() { ... }

        #[test]
        fn rejects_negative_values() { ... }
    }
}
```

### Use `#[should_panic]` Sparingly

```rust
// Prefer Result-based testing
#[test]
fn parse_error_returns_err() {
    let result = parse("invalid");
    assert!(result.is_err());
}

// Use should_panic only for actual panic conditions
#[test]
#[should_panic(expected = "index out of bounds")]
fn get_panics_on_invalid_index() {
    let items = Items::new();
    items.get(999);
}
```

### Use `proptest` or `quickcheck` for Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_roundtrip(value: i32) {
        let serialized = serialize(value);
        let parsed = parse(&serialized)?;
        prop_assert_eq!(parsed, value);
    }
}
```

---

## Quick Reference Checklist

- [ ] No bare `unwrap()` in production code
- [ ] All magic numbers are named constants
- [ ] Functions take `&str` not `String` when not storing
- [ ] Collections pre-allocated when size known
- [ ] Lookup functions return `&'static str` or `Cow`
- [ ] Public API has doc comments with examples
- [ ] Tests have descriptive names
- [ ] No allocations in hot paths
- [ ] Errors have context added
- [ ] Async operations have timeouts
