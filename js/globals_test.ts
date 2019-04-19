// Copyright 2018-2019 the Deno authors. All rights reserved. MIT license.
import { test, assert } from "./test_util.ts";

test(function globalThisExists():void {
  assert(globalThis != null);
});

test(function windowExists():void {
  assert(window != null);
});

test(function windowWindowExists():void {
  assert(window.window === window);
});

test(function globalThisEqualsWindow():void {
  // @ts-ignore (TypeScript thinks globalThis and window don't match)
  assert(globalThis === window);
});

test(function DenoNamespaceExists():void {
  assert(Deno != null);
});

test(function DenoNamespaceEqualsWindowDeno():void {
  assert(Deno === window.Deno);
});

test(function DenoNamespaceIsFrozen():void {
  assert(Object.isFrozen(Deno));
});

test(function webAssemblyExists():void {
  assert(typeof WebAssembly.compile === "function");
});
