// ./tools/build.py deno_core_test && ./target/debug/deno_core_test  --nocapture isolate::tests::test_execute
#![allow(dead_code)]
use crate::js_errors::JSError;
use crate::libdeno;
use crate::libdeno::deno_buf;
use libc::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::{Once, ONCE_INIT};

type RecvCb = Box<dyn FnMut(deno_buf)>;

pub struct CoreIsolate {
  libdeno_isolate: *const libdeno::isolate,
  recv_cb: RecvCb,
}

static DENO_INIT: Once = ONCE_INIT;

impl CoreIsolate {
  pub fn new<R: 'static + FnMut(deno_buf)>(
    recv_cb: R,
    shared: Option<deno_buf>,
    load_snapshot: Option<deno_buf>,
  ) -> Self {
    DENO_INIT.call_once(|| {
      unsafe { libdeno::deno_init() };
    });

    let config = libdeno::deno_config {
      will_snapshot: 0,
      load_snapshot: match load_snapshot {
        Some(s) => s,
        None => libdeno::deno_buf::empty(),
      },
      shared: match shared {
        Some(s) => s,
        None => libdeno::deno_buf::empty(),
      },
      recv_cb: pre_dispatch,
    };
    let libdeno_isolate = unsafe { libdeno::deno_new(config) };

    Self {
      libdeno_isolate,
      recv_cb: Box::new(recv_cb),
    }
  }

  fn zero_copy_release(&self, zero_copy_id: usize) {
    unsafe {
      libdeno::deno_zero_copy_release(self.libdeno_isolate, zero_copy_id)
    }
  }

  #[inline]
  pub unsafe fn from_raw_ptr<'a>(ptr: *const c_void) -> &'a mut Self {
    let ptr = ptr as *mut _;
    &mut *ptr
  }

  #[inline]
  pub fn as_raw_ptr(&self) -> *const c_void {
    self as *const _ as *const c_void
  }

  pub fn execute(
    &self,
    js_filename: &str,
    js_source: &str,
  ) -> Result<(), JSError> {
    let filename = CString::new(js_filename).unwrap();
    let source = CString::new(js_source).unwrap();
    unsafe {
      libdeno::deno_execute(
        self.libdeno_isolate,
        self.as_raw_ptr(),
        filename.as_ptr(),
        source.as_ptr(),
      )
    };
    if let Some(err) = self.last_exception() {
      return Err(err);
    }
    Ok(())
  }

  fn last_exception(&self) -> Option<JSError> {
    let ptr = unsafe { libdeno::deno_last_exception(self.libdeno_isolate) };
    if ptr.is_null() {
      None
    } else {
      let cstr = unsafe { CStr::from_ptr(ptr) };
      let v8_exception = cstr.to_str().unwrap();
      debug!("v8_exception\n{}\n", v8_exception);
      let js_error = JSError::from_v8_exception(v8_exception).unwrap();
      Some(js_error)
    }
  }

  fn check_promise_errors(&self) {
    unsafe {
      libdeno::deno_check_promise_errors(self.libdeno_isolate);
    }
  }

  fn respond(&mut self) -> Result<(), JSError> {
    let buf = deno_buf::empty();
    unsafe {
      libdeno::deno_respond(self.libdeno_isolate, self.as_raw_ptr(), buf)
    }
    if let Some(err) = self.last_exception() {
      Err(err)
    } else {
      Ok(())
    }
  }
}

extern "C" fn pre_dispatch(
  user_data: *mut c_void,
  control_buf: deno_buf,
  zero_copy_buf: deno_buf,
) {
  let isolate = unsafe { CoreIsolate::from_raw_ptr(user_data) };
  assert_eq!(control_buf.len(), 0);
  (isolate.recv_cb)(zero_copy_buf);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn js_check(r: Result<(), JSError>) {
    if let Err(e) = r {
      panic!(e.to_string());
    }
  }

  #[test]
  fn test_execute() {
    let mut counter = Arc::new(0);
    let counter_ = counter.clone();
    let recv_cb = move |_zero_copy_buf| {
      *Arc::make_mut(&mut counter) += 1;
      println!("Hello world recv_cb {}", counter);
    };
    let isolate = CoreIsolate::new(recv_cb, None, None);
    js_check(isolate.execute(
      "filename.js",
      r#"
        libdeno.send();
        async function main() {
          libdeno.send();
        }
        main();
        "#,
    ));
    assert_eq!(*counter_, 2);
  }

}
