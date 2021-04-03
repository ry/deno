// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
"use strict";

((window) => {
  function flatEntries(obj) {
    const entries = [];
    for (const key in obj) {
      entries.push(key);
      entries.push(obj[key]);
    }
    return entries;
  }

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

  function respond(responseSenderRid, resp, zeroCopyBuf) {
    return Deno.core.jsonOpSync("op_http_respond", [
      responseSenderRid,
      resp.status ?? 200,
      flatEntries(resp.headers ?? {}),
    ], zeroCopyBuf);
  }

  function writeResponse(responseBodyRid, zeroCopyBuf) {
    return Deno.core.jsonOpAsync(
      "op_http_write_response",
      responseBodyRid,
      zeroCopyBuf,
    );
  }

  async* function listen(addr) {
    const rid = await Deno.http.createServer(addr);
    while (true) {
      const connRid = await Deno.http.accept(rid);
      handleConn(connRid);
      (async function () {
        while (true) {
          const [isClosed, bodyRid, resRid, method, headers, url] = await Deno.http.nextRequest(connRid);

          const request = { method, headers, url };
          yield request;
          //handle(req[1], req[2], req[3], req[4], req[5]);

          if (isClosed) {
            break;
          }
        }

      })();
    }
    throw Error("todo");
  }

  async function handleConn(connRid) {
  }

  window.__bootstrap.http = {
    createServer,
    accept,
    readRequest,
    nextRequest,
    respond,
    writeResponse,
    listen,
  };
})(this);
/*
// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
async function main(addr) {
  console.log("Server listening on", addr);

}


async function handle(requestBodyRid, responseSenderRid, method, headers, url) {
  // Don't care about request body, discard it outright.
  if (requestBodyRid) {
    Deno.core.close(requestBodyRid);
  }

  const resp = await globalThis.handler({ method, headers, url });

  // If response body is Uint8Array it will be sent synchronously
  // in a single op, in other case a "response body" resource will be
  // created and we'll be streaming it.
  const body = resp.body ?? new Uint8Array();
  const zeroCopyBuf = body instanceof Uint8Array ? body : null;

  const responseBodyRid = Deno.http.respond(
    responseSenderRid,
    resp,
    zeroCopyBuf,
  );

  if (responseBodyRid) {
    for await (const chunk of body) {
      await Deno.http.writeResponse(
        responseBodyRid,
        chunk,
      );
    }
    Deno.core.close(responseBodyRid);
  }
}
*/
