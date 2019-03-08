#![allow(unused_variables)]
#![allow(dead_code)]

use futures::future::lazy;

const MAX_RECORDS: usize = 100;

/// Total number of records added.
const INDEX_NUM_RECORDS: usize = 0;

/// Number of records that have been shifted off.
const INDEX_NUM_SHIFTED_OFF: usize = 1;

/// The head is the number of initialized bytes in SharedQueue.
/// It grows monotonically.
const INDEX_HEAD: usize = 2;

const INDEX_OFFSETS: usize = 3;
const INDEX_RECORDS: usize = 3 + MAX_RECORDS;

/// Byte offset of where the records begin. Also where the head starts.
const HEAD_INIT: usize = 4 * INDEX_RECORDS;

/// A rough guess at how big we should make the shared buffer in bytes.
const RECORDS_SIZE: usize = 128 * MAX_RECORDS;
const SIZE: usize = HEAD_INIT + RECORDS_SIZE;

type Buf = Box<[u8]>;

pub struct SharedQueue {
  bytes: Vec<u8>,
}

impl SharedQueue {
  fn new() -> Self {
    let mut bytes = Vec::new();
    bytes.resize(SIZE, 0);
    let mut q = Self { bytes };
    q.reset();
    q
  }

  /// Clears the shared buffer.
  fn reset(&mut self) {
    let s: &mut [u32] = self.as_u32_slice_mut();
    for i in 0..INDEX_RECORDS {
      s[i] = 0;
    }
    s[INDEX_HEAD] = HEAD_INIT as u32;
  }

  fn as_u32_slice<'a>(&'a self) -> &'a [u32] {
    let p = self.bytes.as_ptr() as *const u32;
    let s = unsafe { std::slice::from_raw_parts(p, self.bytes.len() / 4) };
    s
  }

  fn as_u32_slice_mut<'a>(&'a mut self) -> &'a mut [u32] {
    let p = self.bytes.as_mut_ptr() as *mut u32;
    unsafe { std::slice::from_raw_parts_mut(p, self.bytes.len() / 4) }
  }

  fn len(&self) -> usize {
    let s = self.as_u32_slice();
    (s[INDEX_NUM_RECORDS] - s[INDEX_NUM_SHIFTED_OFF]) as usize
  }

  fn num_records(&self) -> usize {
    let s = self.as_u32_slice();
    s[INDEX_NUM_RECORDS] as usize
  }

  fn head(&self) -> usize {
    let s = self.as_u32_slice();
    s[INDEX_HEAD] as usize
  }

  fn set_end(&mut self, index: usize, end: usize) {
    let s = self.as_u32_slice_mut();
    s[INDEX_OFFSETS + index] = end as u32;
  }

  fn get_end(&self, index: usize) -> Option<usize> {
    if index < self.num_records() {
      let s = self.as_u32_slice();
      Some(s[INDEX_OFFSETS + index] as usize)
    } else {
      None
    }
  }

  fn get_offset(&self, index: usize) -> Option<usize> {
    if index < self.num_records() {
      Some(if index == 0 {
        HEAD_INIT
      } else {
        let s = self.as_u32_slice();
        s[INDEX_OFFSETS + index - 1] as usize
      })
    } else {
      None
    }
  }

  /// Returns none if empty.
  pub fn shift<'a>(&'a mut self) -> Option<Buf> {
    let u32_slice = self.as_u32_slice();
    let i = u32_slice[INDEX_NUM_SHIFTED_OFF] as usize;
    if i >= self.num_records() {
      return None;
    }
    let off = self.get_offset(i).unwrap();
    let end = self.get_end(i).unwrap();
    println!("shift {} {}", off, end);

    let mut v = Vec::<u8>::new();
    v.resize(end - off, 0);
    v.copy_from_slice(&self.bytes[off..end]);

    let u32_slice = self.as_u32_slice_mut();
    u32_slice[INDEX_NUM_SHIFTED_OFF] += 1;

    Some(v.into_boxed_slice())
  }

  pub fn push(&mut self, record: Buf) -> bool {
    let off = self.head();
    let end = off + record.len();
    let index = self.num_records();
    if end > self.bytes.len() {
      return false;
    }
    self.set_end(index, end);
    assert_eq!(end - off, record.len());
    self.bytes[off..end].copy_from_slice(&record);
    let u32_slice = self.as_u32_slice_mut();
    u32_slice[INDEX_NUM_RECORDS] += 1;
    u32_slice[INDEX_HEAD] = end as u32;
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use deno_core::deno_buf;
  use deno_core::Behavior;
  use deno_core::Isolate;
  use deno_core::JSError;
  use deno_core::Op;
  use futures::future::Future;

  #[test]
  fn basic() {
    let mut q = SharedQueue::new();

    let h = q.head();
    assert!(h > 0);

    let r = vec![1u8, 2, 3, 4, 5].into_boxed_slice();
    let len = r.len() + h;
    assert!(q.push(r));
    assert_eq!(q.head(), len);

    let r = vec![6, 7].into_boxed_slice();
    assert!(q.push(r));

    let r = vec![8, 9, 10, 11].into_boxed_slice();
    assert!(q.push(r));
    assert_eq!(q.num_records(), 3);
    assert_eq!(q.len(), 3);

    let r = q.shift().unwrap();
    assert_eq!(r.as_ref(), vec![1, 2, 3, 4, 5].as_slice());
    assert_eq!(q.num_records(), 3);
    assert_eq!(q.len(), 2);

    let r = q.shift().unwrap();
    assert_eq!(r.as_ref(), vec![6, 7].as_slice());
    assert_eq!(q.num_records(), 3);
    assert_eq!(q.len(), 1);

    let r = q.shift().unwrap();
    assert_eq!(r.as_ref(), vec![8, 9, 10, 11].as_slice());
    assert_eq!(q.num_records(), 3);
    assert_eq!(q.len(), 0);

    assert!(q.shift().is_none());
    assert!(q.shift().is_none());

    assert_eq!(q.num_records(), 3);
    assert_eq!(q.len(), 0);

    q.reset();
    assert_eq!(q.num_records(), 0);
    assert_eq!(q.len(), 0);
  }

  fn alloc_buf(byte_length: usize) -> Buf {
    let mut v = Vec::new();
    v.resize(byte_length, 0);
    v.into_boxed_slice()
  }

  #[test]
  fn overflow() {
    let mut q = SharedQueue::new();
    assert!(q.push(alloc_buf(RECORDS_SIZE - 1)));
    assert_eq!(q.len(), 1);
    assert!(!q.push(alloc_buf(2)));
    assert_eq!(q.len(), 1);
    assert!(q.push(alloc_buf(1)));
    assert_eq!(q.len(), 2);

    assert_eq!(q.shift().unwrap().len(), RECORDS_SIZE - 1);
    assert_eq!(q.len(), 1);
    assert_eq!(q.shift().unwrap().len(), 1);
    assert_eq!(q.len(), 0);

    assert!(!q.push(alloc_buf(1)));
  }

  struct TestBehavior {
    recv_count: usize,
    push_count: usize,
    shift_count: usize,
    reset_count: usize,
    q: SharedQueue,
  }

  impl TestBehavior {
    fn new() -> Self {
      Self {
        recv_count: 0,
        push_count: 0,
        shift_count: 0,
        reset_count: 0,
        q: SharedQueue::new(),
      }
    }
  }

  impl Behavior<Buf> for TestBehavior {
    fn startup_snapshot(&mut self) -> Option<deno_buf> {
      None
    }

    fn startup_shared(&mut self) -> Option<deno_buf> {
      None
    }

    fn recv(
      &mut self,
      record: Buf,
      _zero_copy_buf: deno_buf,
    ) -> (bool, Box<Op<Buf>>) {
      self.recv_count += 1;
      // self.q.shift();
      // (false, Box::new(futures::future::ok(())))
      unimplemented!()
    }

    fn resolve(
      &mut self,
      specifier: &str,
      _referrer: deno_core::deno_mod,
    ) -> deno_core::deno_mod {
      unimplemented!()
    }

    fn records_reset(&mut self) {
      self.reset_count += 1;
      self.q.reset();
    }

    fn records_push(&mut self, record: Buf) -> bool {
      self.push_count += 1;
      self.q.push(record)
    }

    fn records_shift(&mut self) -> Option<Buf> {
      self.shift_count += 1;
      self.q.shift()
    }
  }

  fn js_check(r: Result<(), JSError>) {
    if let Err(e) = r {
      panic!(e.to_string());
    }
  }

  #[test]
  fn js() {
    let js_source = include_str!("shared_queue.js");

    let isolate = Isolate::new(TestBehavior::new());
    let future = lazy(move || {
      js_check(isolate.execute("shared_queue.js", js_source));
      isolate.then(|r| {
        js_check(r);
        Ok(())
      })
    });
    tokio::run(future);
  }
}
