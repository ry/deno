// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

pub fn init(rt: &mut deno_core::JsRuntime) {
  super::reg_json_async(
    rt,
    "op_http_create_server",
    deno_http::op_create_server,
  );
  super::reg_json_async(rt, "op_http_accept", deno_http::op_accept_http);
  super::reg_json_async(rt, "op_http_next_request", deno_http::op_next_request);
  super::reg_json_async(rt, "op_http_read_request", deno_http::op_read_request);
  super::reg_json_sync(rt, "op_http_respond", deno_http::op_respond);
  super::reg_json_async(
    rt,
    "op_http_write_response",
    deno_http::op_write_response,
  );
}
