// Copyright 2018-2019 the Deno authors. All rights reserved. MIT license.
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default)]
pub struct Progress(Arc<Mutex<Inner>>);

impl Progress {
  pub fn new() -> Self {
    Progress::default()
  }

  pub fn set_callback<F>(&self, f: F)
  where
    F: Fn(bool, usize, usize, &str) + Send + Sync + 'static,
  {
    let mut s = self.0.lock().unwrap();
    assert!(s.callback.is_none());
    s.callback = Some(Arc::new(f));
  }

  /// Returns job counts: (complete, total)
  pub fn progress(&self) -> (usize, usize) {
    let s = self.0.lock().unwrap();
    s.progress()
  }

  pub fn history(&self) -> Vec<String> {
    let s = self.0.lock().unwrap();
    s.job_names.clone()
  }

  pub fn add(&self, name: String) -> Job {
    let mut s = self.0.lock().unwrap();
    let id = s.job_names.len();
    let max = s.job_names.len() + 1;
    let cur = s.complete;
    s.maybe_call_callback(false, max, cur, &name);

    /*
    {
      let mut iter = name.split_whitespace();
      let name = iter.next().unwrap();
      let url = iter.next().unwrap();
      s.progress.print_now2(&name, &url).unwrap();
    }
    */
    s.tick();

    s.job_names.push(name);
    Job {
      id,
      inner: self.0.clone(),
    }
  }

  pub fn done(&self) {
    let mut s = self.0.lock().unwrap();
    s.progress.clear();
    // s.progress.print_now("Ready").unwrap();
    // s.progress.tick_now(cur, max, "done").unwrap();
    // s.progress.print_now("Foo").unwrap();
    s.maybe_call_callback(true, s.complete, s.job_names.len(), "");
  }
}

type Callback = dyn Fn(bool, usize, usize, &str) + Send + Sync;

use crate::cargo_progress::Progress as CargoProgress;
use crate::cargo_shell::Shell as CargoShell;

struct Inner {
  progress: CargoProgress,
  job_names: Vec<String>,
  complete: usize,
  callback: Option<Arc<Callback>>,
}

impl Default for Inner {
  fn default() -> Inner {
    let shell = CargoShell::new();
    // shell.status_header("hello world").unwrap();
    // shell.status("hello", "world").unwrap();
    Inner {
      progress: CargoProgress::new("Building", shell),
      job_names: Vec::new(),
      complete: 0,
      callback: None,
    }
  }
}

impl Inner {
  pub fn maybe_call_callback(
    &self,
    done: bool,
    complete: usize,
    total: usize,
    msg: &str,
  ) {
    if let Some(ref cb) = self.callback {
      cb(done, complete, total, msg);
    }
  }

  pub fn tick(&mut self) {
    let (complete, total) = self.progress();
    assert!(self.progress.is_enabled());
    // drop(self.progress.stderr("hi".to_string()));
    self.progress.tick_now(complete, total, "").unwrap();
  }

  /// Returns job counts: (complete, total)
  pub fn progress(&self) -> (usize, usize) {
    let total = self.job_names.len();
    (self.complete, total)
  }

  pub fn complete(&mut self, id: usize) {
    self.complete += 1;
    let name = self.job_names[id].clone();
    let (complete, total) = self.progress();
    {
      let mut iter = name.split_whitespace();
      let name = iter.next().unwrap();
      let url = iter.next().unwrap();
      self.progress.print_now2(&name, &url).unwrap();
    }
    self.maybe_call_callback(false, complete, total, &name);
  }
}

pub struct Job {
  inner: Arc<Mutex<Inner>>,
  id: usize,
}

impl Drop for Job {
  fn drop(&mut self) {
    let mut s = self.inner.lock().unwrap();
    s.complete(self.id)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn progress() {
    let p = Progress::new();
    assert_eq!(p.progress(), (0, 0));
    {
      let _j1 = p.add("hello world".to_string());
      assert_eq!(p.progress(), (0, 1));
    }
    assert_eq!(p.progress(), (1, 1));
    {
      let _j2 = p.add("foo bar".to_string());
      assert_eq!(p.progress(), (1, 2));
    }
    assert_eq!(p.progress(), (2, 2));
  }

  #[test]
  fn history() {
    let p = Progress::new();
    let _a = p.add("a".to_string());
    let _b = p.add("b".to_string());
    assert_eq!(p.history(), vec!["a", "b"]);
  }

  #[test]
  fn callback() {
    let callback_history: Arc<Mutex<Vec<(usize, usize, String)>>> =
      Arc::new(Mutex::new(Vec::new()));
    {
      let p = Progress::new();
      let callback_history_ = callback_history.clone();

      p.set_callback(move |_done, complete, total, msg| {
        // println!("callback: {}, {}, {}", complete, total, msg);
        let mut h = callback_history_.lock().unwrap();
        h.push((complete, total, String::from(msg)));
      });
      {
        let _a = p.add("a".to_string());
        let _b = p.add("b".to_string());
      }
      let _c = p.add("c".to_string());
    }

    let h = callback_history.lock().unwrap();
    assert_eq!(
      h.to_vec(),
      vec![
        (0, 1, "a".to_string()),
        (0, 2, "b".to_string()),
        (1, 2, "b".to_string()),
        (2, 2, "a".to_string()),
        (2, 3, "c".to_string()),
        (3, 3, "c".to_string()),
      ]
    );
  }

  #[test]
  fn thread_safe() {
    fn f<S: Send + Sync>(_: S) {}
    f(Progress::new());
  }
}
