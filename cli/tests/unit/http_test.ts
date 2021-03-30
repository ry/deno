// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
import {
  assert,
  assertEquals,
  assertNotEquals,
  assertThrows,
  assertThrowsAsync,
  deferred,
  unitTest,
} from "./test_util.ts";

async function handleConn(connRid) {
  while (true) {
    const req = await Deno.http.nextRequest(connRid);

    echoRequest(req);

    if (req.connectionClosed) {
      break;
    }
  }
}

async function echoRequest(req) {
  const { requestBodyRid, responseSenderRid } = req;

  // Don't care about request body, discard it outright.
  if (requestBodyRid) {
    Deno.core.close(requestBodyRid);
  }

  // FIXME(bartlomieju): echo the request body
  const resp = {
    status: 200,
    headers: { "content-type": "text/plain" },
    body: Deno.core.encode("ok"),
  };

  const zeroCopyBufs = resp.body instanceof Uint8Array ? [resp.body] : [];

  const { responseBodyRid } = Deno.http.respond(
    responseSenderRid,
    resp,
    ...zeroCopyBufs,
  );

  assert(responseBodyRid == null);
}

async function runServer(serverRid: number) {
  while (true) {
    const { rid: connRid } = await Deno.http.accept(serverRid);
    handleConn(connRid);
  }
}

unitTest({ perms: { net: true } }, async function httpServer() {
  const server = Deno.http.createServer("127.0.0.1:3500");
  const _serverPromise = runServer(server.rid);
  const resp = await fetch("127.0.0.1:3500");
  const text = await resp.text();
  assertEquals(text, "ok");
});
