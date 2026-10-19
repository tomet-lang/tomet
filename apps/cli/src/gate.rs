//! The jobserver gate governing parallel document processing.
//!
//! # The Jobserver Protocol
//!
//! To eliminate CPU over-subscription across cooperating processes (a parent process
//! such as a build tool), concurrency is bounded by the GNU jobserver
//! protocol. Rather than every process spawning an unbounded thread pool on all cores,
//! processes share a token budget coordinated via inherited pipes or semaphores.
//!
//! # The Implicit Token Rule
//!
//! A process owns one *implicit* token representing its own execution right.
//! The gate manages token acquisition for parallel rayon closures:
//! - One caller at a time claims the implicit token via an atomic flag (`implicit_token_held`).
//! - Every other concurrent caller acquires a token from the jobserver pipe (`Client::acquire()`).
//! - When the implicit token is dropped after completing a file, it is donated into the
//!   jobserver pipe (`Client::release_raw()`) so that waiting rayon threads wake up
//!   immediately without busy polling.
//! - Once a parallel phase finishes (e.g. `par_iter().map(..).collect()`), the donated
//!   implicit token is reclaimed via [`reclaim_implicit_token`] (`Client::acquire()` followed
//!   by `drop_without_releasing()`), restoring the pipe's original token count.
//!
//! # The Fallback (No Jobserver)
//!
//! If no jobserver environment variable (`CARGO_MAKEFLAGS`, `MAKEFLAGS`) is present,
//! or if the inherited descriptor cannot be connected to, `tomet` falls back silently
//! (or with exactly one warning line if a variable was set but invalid) to an in-process
//! jobserver sized to `std::thread::available_parallelism() - 1` tokens plus the implicit token.
//! If even that cannot be created (for example the pipe cannot be opened), one warning line is
//! printed and the commands run without a gate: [`acquire`] then returns `None` and the
//! parallelism is rayon's own.

use std::io;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use jobserver::{Acquired, Client};

/// Gate managing tokens for parallel file processing.
pub struct JobServerGate {
    client: Client,
    implicit_token_held: AtomicBool,
}

impl JobServerGate {
    /// Create a new gate wrapping `client`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            implicit_token_held: AtomicBool::new(false),
        }
    }

    /// Access the underlying [`Client`].
    #[cfg(test)]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Acquire a token. Blocks until one is available.
    pub fn acquire(&self) -> io::Result<JobToken<'_>> {
        if self
            .implicit_token_held
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Ok(JobToken::Implicit(self))
        } else {
            let acquired = self.client.acquire()?;
            Ok(JobToken::Acquired(acquired))
        }
    }

    /// Reclaim the donated implicit token after a parallel phase has completed.
    ///
    /// When the implicit token is dropped, it is donated into the jobserver
    /// pipe so that blocked rayon threads wake up. Once the parallel loop has
    /// joined, this reclaims that single token without releasing it back,
    /// restoring the pipe to its original token count and resetting `implicit_token_held`.
    ///
    /// Note: with an inherited jobserver whose parent is saturated, a sibling
    /// process may acquire the donated token before this call, causing this to
    /// block until some process returns a token to the pipe.
    pub fn reclaim_implicit_token(&self) {
        if self.implicit_token_held.load(Ordering::Acquire) {
            if let Ok(acquired) = self.client.acquire() {
                acquired.drop_without_releasing();
            }
            self.implicit_token_held.store(false, Ordering::Release);
        }
    }
}

/// An RAII guard representing a held job token.
///
/// Dropping this token releases it back to the gate or the jobserver.
pub enum JobToken<'a> {
    Implicit(&'a JobServerGate),
    Acquired(#[allow(dead_code)] Acquired),
}

impl Drop for JobToken<'_> {
    fn drop(&mut self) {
        if let JobToken::Implicit(gate) = self {
            // Forward the implicit token to the jobserver pipe so that
            // any threads blocked in Client::acquire() wake up and make progress.
            let _ = gate.client.release_raw();
        }
    }
}

static GATE: OnceLock<JobServerGate> = OnceLock::new();

/// Initialize the process-wide jobserver gate from the environment or fallback.
pub fn init_from_env() {
    // SAFETY: `from_env_ext` reads and takes ownership of file descriptors specified in
    // environment variables (such as `CARGO_MAKEFLAGS`). This function is called once,
    // early in `main` before any other file descriptors are opened or threads are spawned,
    // ensuring those descriptors are valid and not concurrently reused.
    let from_env = unsafe { Client::from_env_ext(true) };
    let client = match from_env.client {
        Ok(client) => client,
        Err(err) => {
            if let Some((var_name, var_val)) = from_env.var
                && !matches!(
                    err.kind(),
                    jobserver::FromEnvErrorKind::NoJobserver
                        | jobserver::FromEnvErrorKind::NoEnvVar
                )
            {
                eprintln!(
                    "warning: failed to connect to jobserver from environment variable `{}=\"{}\"`: {}",
                    var_name,
                    var_val.to_string_lossy(),
                    err
                );
            }
            let limit = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let pipe_tokens = limit.saturating_sub(1);
            match Client::new(pipe_tokens) {
                Ok(client) => client,
                Err(err) => {
                    eprintln!("warning: failed to create in-process jobserver: {err}");
                    return;
                }
            }
        }
    };
    let _ = GATE.set(JobServerGate::new(client));
}

/// Acquire a token from the global gate, blocking until one is available.
///
/// If token acquisition fails or no gate is initialized, this returns `None`
/// and the caller processes the file without a token.
pub fn acquire() -> Option<JobToken<'static>> {
    GATE.get().and_then(|g| g.acquire().ok())
}

/// Reclaim the donated implicit token from the global gate.
pub fn reclaim_implicit_token() {
    if let Some(gate) = GATE.get() {
        gate.reclaim_implicit_token();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn gate_concurrency_ceiling() {
        for pipe_tokens in [0, 1, 3] {
            let total_capacity = pipe_tokens + 1; // pipe tokens + implicit token
            let client = Client::new(pipe_tokens).unwrap();
            let gate = Arc::new(JobServerGate::new(client));

            let current_active = Arc::new(AtomicUsize::new(0));
            let max_observed = Arc::new(AtomicUsize::new(0));

            thread::scope(|s| {
                for _ in 0..10 {
                    let gate = Arc::clone(&gate);
                    let current = Arc::clone(&current_active);
                    let max_active = Arc::clone(&max_observed);

                    s.spawn(move || {
                        let _token = gate.acquire().unwrap();
                        let active = current.fetch_add(1, Ordering::SeqCst) + 1;
                        max_active.fetch_max(active, Ordering::SeqCst);

                        // Hold long enough for other threads to overlap
                        thread::sleep(Duration::from_millis(30));

                        current.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            });

            gate.reclaim_implicit_token();

            let peak = max_observed.load(Ordering::SeqCst);
            assert_eq!(
                peak, total_capacity,
                "for capacity {total_capacity}, peak observed was {peak}"
            );

            // Verify tokens in pipe are preserved exactly
            assert_eq!(gate.client().available().unwrap_or(0), pipe_tokens);
        }
    }
}
