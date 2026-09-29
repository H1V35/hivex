use super::Work;
use std::fmt;
use std::io::{self, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
  started: Option<Instant>,
}

impl Progress {
  pub fn new(enabled: bool) -> Self {
    Self {
      started: enabled.then(Instant::now),
    }
  }

  pub fn message(self, message: fmt::Arguments<'_>) {
    if let Some(started) = self.started {
      let _ = writeln!(
        io::stderr().lock(),
        "[hivex +{}s] {message}",
        started.elapsed().as_secs()
      );
    }
  }

  pub fn begin(self, mut work: Work, resumed: bool) -> Work {
    work.progress = self;
    self.checkpoint(
      if resumed {
        "work resumed"
      } else {
        "work started"
      },
      &work,
    );
    work
  }

  pub fn checkpoint(self, message: &'static str, work: &Work) {
    let remaining = work.remaining();
    let completed = work.value()["plannedUnits"].as_array().map(|planned| {
      planned
        .iter()
        .filter(|unit| {
          unit
            .as_str()
            .is_some_and(|id| !remaining.iter().any(|pending| pending == id))
        })
        .count()
    });
    let completed = completed.map_or_else(|| "unknown".to_owned(), |count| count.to_string());
    self.message(format_args!(
      "{message}; units completed={completed}, pending={}; calls={}/{}; state={}",
      remaining.len(),
      work.calls(),
      work.max_calls(),
      work.status().as_str()
    ));
  }

  pub fn waiting(self, stage: &'static str) -> Waiting {
    let (stop, receiver) = mpsc::channel();
    let thread = self.started.and_then(|_| {
      thread::Builder::new()
        .name("hivex-progress".into())
        .spawn(move || {
          let started = Instant::now();
          while matches!(
            receiver.recv_timeout(Duration::from_secs(15)),
            Err(RecvTimeoutError::Timeout)
          ) {
            self.message(format_args!(
              "{stage}: waiting for model; elapsed={}s; internal progress unknown",
              started.elapsed().as_secs()
            ));
          }
        })
        .ok()
    });
    Waiting { stop, thread }
  }
}

pub struct Waiting {
  stop: Sender<()>,
  thread: Option<JoinHandle<()>>,
}

impl Drop for Waiting {
  fn drop(&mut self) {
    let _ = self.stop.send(());
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}
