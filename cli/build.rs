// Copyright 2018-2020 the Deno authors. All rights reserved. MIT license.
#![allow(warnings)]

use deno_core::include_crate_modules;
use deno_core::js_check;
use deno_core::CoreIsolate;
use deno_core::ErrBox;
use deno_core::ModuleLoader;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::StartupData;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::pin::Pin;

fn main() {
  // Don't build V8 if "cargo doc" is being run. This is to support docs.rs.
  if env::var_os("RUSTDOCFLAGS").is_some() {
    return;
  }

  // To debug snapshot issues uncomment:
  // deno_typescript::trace_serializer();

  println!(
    "cargo:rustc-env=TS_VERSION={}",
    deno_typescript::ts_version()
  );

  println!(
    "cargo:rustc-env=TARGET={}",
    std::env::var("TARGET").unwrap()
  );

  let extern_crate_modules = include_crate_modules![deno_core];

  let c = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
  let o = PathBuf::from(env::var_os("OUT_DIR").unwrap());

  // Main snapshot
  //let bundle_path = o.join("CLI_SNAPSHOT.js");
  let snapshot_path = o.join("CLI_SNAPSHOT.bin");

  let loader = std::rc::Rc::new(Loader {});
  let mut isolate = deno_core::EsIsolate::new(loader, StartupData::None, true);
  js_check(isolate.execute("anon", "window = this"));

  let code = std::fs::read_to_string("js2/main.js").unwrap();

  let module_specifier =
    ModuleSpecifier::resolve_url("http://asdf/main.js").unwrap();
  let result = futures::executor::block_on(
    isolate.load_module(&module_specifier, Some(code)),
  );
  let mod_id = js_check(result);
  js_check(isolate.mod_evaluate(mod_id));

  let snapshot = isolate.snapshot();
  let snapshot_slice: &[u8] = &*snapshot;
  println!("Snapshot size: {}", snapshot_slice.len());
  std::fs::write(&snapshot_path, snapshot_slice).unwrap();
  println!("Snapshot written to: {} ", snapshot_path.display());
}

struct Loader {}

impl ModuleLoader for Loader {
  fn resolve(
    &self,
    specifier: &str,
    referrer: &str,
    _is_main: bool,
  ) -> Result<ModuleSpecifier, ErrBox> {
    ModuleSpecifier::resolve_import(specifier, referrer).map_err(ErrBox::from)
  }

  fn load(
    &self,
    _module_specifier: &ModuleSpecifier,
    _maybe_referrer: Option<ModuleSpecifier>,
    _is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>> {
    unreachable!()
  }
}
