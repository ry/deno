// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
async function main(addr) {
  const rid = await Deno.http.createServer(addr);
  console.log("Server listening on", addr);

  while (true) {
    const connRid = await Deno.http.accept(rid);
    handleConn(connRid);
  }
}

async function handleConn(connRid) {
  while (true) {
    const [connectionClosed, ...req] = await Deno.http.nextRequest(connRid);

    handle(req);

    if (connectionClosed) {
      break;
    }
  }
}

async function handle(req) {
  const [requestBodyRid, responseSenderRid, method, headers, url] = req;

  // Don't care about request body, discard it outright.
  if (requestBodyRid) {
    Deno.core.close(requestBodyRid);
  }

  const resp = await globalThis.handler({ method, headers, url });

  // If response body is Uint8Array it will be sent synchronously
  // in a single op, in other case a "response body" resource will be
  // created and we'll be streaming it.
  const body = resp.body ?? new Uint8Array();
  const zeroCopyBufs = body instanceof Uint8Array ? [body] : [];

  const responseBodyRid = Deno.http.respond(
    responseSenderRid,
    resp,
    ...zeroCopyBufs,
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

const body = Deno.core.encode("Hello World");

globalThis.handler = ({ url }) => {
  return {
    status: 200,
    headers: { "content-type": "text/plain" },
    body,
  };
};

const addr = Deno.args[0] || "127.0.0.1:4500";
await main(addr);
