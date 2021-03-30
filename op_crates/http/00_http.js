// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
"use strict";

((window) => {
  function createServer(address) {
    return Deno.core.jsonOpAsync(
      "op_http_create_server",
      address,
    );
  }

  function accept(serverRid) {
    return Deno.core.jsonOpAsync("op_http_accept", serverRid);
  }

  function nextRequest(connRid) {
    return Deno.core.jsonOpAsync("op_http_next_request", connRid);
  }

  function readRequest(requestRid) {
    return Deno.core.jsonOpAsync("op_http_read_request", requestRid);
  }

  function respond(responseSenderRid, resp, ...zeroCopyBufs) {
    return Deno.core.jsonOpSync("op_http_respond", [
      responseSenderRid,
      resp.status ?? 200,
      Object.entries(resp.headers ?? {}),
    ], ...zeroCopyBufs);
  }

  function writeResponse(responseBodyRid, zeroCopyBuf) {
    return Deno.core.jsonOpAsync(
      "op_http_write_response",
      responseBodyRid,
      zeroCopyBuf,
    );
  }

  window.__bootstrap.http = {
    createServer,
    accept,
    readRequest,
    nextRequest,
    respond,
    writeResponse,
  };
})(this);
