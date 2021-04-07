// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
"use strict";

((window) => {
  const { Request, dontValidateUrl, fastBody, Response } =
    window.__bootstrap.fetch;
  const { Headers } = window.__bootstrap.headers;
  const errors = window.__bootstrap.errors.errors;
  const core = window.Deno.core;
  const { ReadableStream } = window.__bootstrap.streams;

  function flatEntries(obj) {
    const entries = [];
    for (const key in obj) {
      entries.push(key);
      entries.push(obj[key]);
    }
    return entries;
  }

  function startHttp(conn) {
    const rid = Deno.core.jsonOpSync("op_http_serve_connection", conn.rid);
    return new HttpConnection(rid);
  }

  class HttpConnection {
    #rid = 0;

    constructor(rid) {
      this.#rid = rid;
    }

    get rid() {
      return this.#rid;
    }

    async next() {
      try {
        const [
          connectionClosed,
          requestBodyRid,
          responseSenderRid,
          method,
          headersList,
          url,
        ] = await Deno.core.jsonOpAsync("op_http_next_request", this.#rid);

        if (connectionClosed) {
          if (responseSenderRid === 0) {
            core.close(this.#rid);
            return { done: true };
          } else {
            throw Error("unhandled");
          }
        }

        /** @type {ReadableStream<Uint8Array> | undefined} */
        let body = undefined;
        if (typeof requestBodyRid === "number") {
          body = createRequestBodyStream(requestBodyRid);
        }

        const request = new Request(url, {
          body,
          method,
          headers: new Headers(headersList),
          [dontValidateUrl]: true,
        });

        const respondWith = createRespondWith(responseSenderRid, this.#rid);
        const value = { request, respondWith };
        return { value, done: false };
      } catch (error) {
        if (error instanceof errors.BadResource) {
          return { value: undefined, done: true };
        } else if (error instanceof errors.Interrupted) {
          return { value: undefined, done: true };
        }
        throw error;
      }
    }

    return(value) {
      core.close(this.#rid);
      return Promise.resolve({ value, done: true });
    }

    [Symbol.asyncIterator]() {
      return this;
    }
  }

  function readRequest(requestRid) {
    return Deno.core.binOpAsync(
      "op_http_read_request",
      requestRid,
    );
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

  function createRespondWith(responseSenderRid, connRid) {
    return async function (resp) {
      if (resp instanceof Promise) {
        resp = await resp;
      }

      if (!(resp instanceof Response)) {
        throw new TypeError(
          "First argument to respondWith must be a Response or a promise resolving to a Response.",
        );
      }
      // If response body is Uint8Array it will be sent synchronously
      // in a single op, in other case a "response body" resource will be
      // created and we'll be streaming it.
      const body = resp[fastBody]();
      let zeroCopyBuf;
      if (body instanceof ArrayBuffer) {
        zeroCopyBuf = new Uint8Array(body);
      } else if (!body) {
        zeroCopyBuf = new Uint8Array(0);
      } else {
        zeroCopyBuf = null;
      }

      const responseBodyRid = respond(
        responseSenderRid,
        resp,
        zeroCopyBuf,
      );

      // If `respond` returns a responseBodyRid, we should stream the body
      // to that resource.
      if (typeof responseBodyRid === "number") {
        if (!body || !(body instanceof ReadableStream)) {
          throw new Error(
            "internal error: recieved responseBodyRid, but response has no body or is not a stream",
          );
        }
        for await (const chunk of body) {
          const data = new Uint8Array(
            chunk.buffer,
            chunk.byteOffset,
            chunk.byteLength,
          );
          await writeResponse(
            responseBodyRid,
            data,
          );
        }

        // Once all chunks are sent, and the request body is closed, we can close
        // the response body.
        core.close(responseBodyRid);
        await Deno.core.jsonOpAsync("op_http_poll", connRid);
      }
    };
  }

  function createRequestBodyStream(requestBodyRid) {
    return new ReadableStream({
      type: "bytes",
      async pull(controller) {
        try {
          // This is the largest possible size for a single packet on a TLS
          // stream.
          console.log("readRequest before");
          const chunk = await readRequest(requestBodyRid);
          console.log(`readRequest after ${chunk}`);
          if (chunk && chunk.length > 0) {
            controller.enqueue(chunk);
          } else {
            // We have reached the end of the body, so we close the stream.
            controller.close();
            core.close(requestBodyRid);
          }
        } catch (err) {
          // There was an error while reading a chunk of the body, so we
          // error.
          controller.error(err);
          controller.close();
          core.close(requestBodyRid);
        }
      },
      cancel() {
        core.close(requestBodyRid);
      },
    });
  }

  window.__bootstrap.http = {
    startHttp,
  };
})(this);
