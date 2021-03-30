// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

use std::borrow::Cow;
use std::cell::RefCell;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;

use deno_core::error::bad_resource_id;
use deno_core::error::null_opbuf;
use deno_core::error::type_error;
use deno_core::error::AnyError;
use deno_core::futures::future::poll_fn;
use deno_core::futures::FutureExt;
use deno_core::futures::Stream;
use deno_core::futures::StreamExt;
use deno_core::AsyncRefCell;
use deno_core::CancelFuture;
use deno_core::CancelHandle;
use deno_core::CancelTryFuture;
use deno_core::JsRuntime;
use deno_core::OpState;
use deno_core::RcRef;
use deno_core::Resource;
use deno_core::ResourceId;
use deno_core::ZeroCopyBuf;

use hyper::body::HttpBody;
use hyper::http;
use hyper::server::conn::Connection;
use hyper::server::conn::Http;
use hyper::service::Service;
use hyper::Body;
use hyper::Request;
use hyper::Response;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_util::io::StreamReader;

/// Load and execute the javascript code.
pub fn init(isolate: &mut JsRuntime) {
  let files =
    vec![("deno:op_crates/http/00_http.js", include_str!("00_http.js"))];
  for (url, source_code) in files {
    isolate.execute(url, source_code).unwrap();
  }
}

pub fn get_declaration() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib.deno_http.d.ts")
}

// Needed so hyper can use non Send futures
#[derive(Clone)]
struct LocalExecutor;

impl<Fut> hyper::rt::Executor<Fut> for LocalExecutor
where
  Fut: Future + 'static,
  Fut::Output: 'static,
{
  fn execute(&self, fut: Fut) {
    tokio::task::spawn_local(fut);
  }
}

struct DenoServiceInner {
  pub request: Request<Body>,
  pub response_tx: oneshot::Sender<Response<Body>>,
}

#[derive(Clone)]
struct DenoService {
  inner: Rc<RefCell<Option<DenoServiceInner>>>,
}

impl Default for DenoService {
  fn default() -> Self {
    Self {
      inner: Rc::new(RefCell::new(None)),
    }
  }
}

impl Service<Request<Body>> for DenoService {
  type Response = Response<Body>;
  type Error = http::Error;
  #[allow(clippy::type_complexity)]
  type Future =
    Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

  fn poll_ready(
    &mut self,
    _cx: &mut Context<'_>,
  ) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request<Body>) -> Self::Future {
    let (resp_tx, resp_rx) = oneshot::channel();
    self.inner.borrow_mut().replace(DenoServiceInner {
      request: req,
      response_tx: resp_tx,
    });

    async move { Ok(resp_rx.await.unwrap()) }.boxed_local()
  }
}

struct HttpServer {
  pub listener: TcpListener,
}

impl Resource for HttpServer {
  fn name(&self) -> Cow<str> {
    "httpServer".into()
  }
}

struct ConnResource {
  pub hyper_connection:
    Rc<RefCell<Connection<tokio::net::TcpStream, DenoService, LocalExecutor>>>,
  pub deno_service: DenoService,
}

impl Resource for ConnResource {
  fn name(&self) -> Cow<str> {
    "httpConnection".into()
  }
}

pub async fn op_create_server(
  state: Rc<RefCell<OpState>>,
  address: String,
  _data: Option<ZeroCopyBuf>,
) -> Result<ResourceId, AnyError> {
  let tcp_listener = TcpListener::bind(address).await?;
  let http_server = HttpServer {
    listener: tcp_listener,
  };

  let rid = state.borrow_mut().resource_table.add(http_server);

  Ok(rid)
}

pub async fn op_accept_http(
  state: Rc<RefCell<OpState>>,
  rid: ResourceId,
  _data: Option<ZeroCopyBuf>,
) -> Result<ResourceId, AnyError> {
  let http_server = state
    .borrow()
    .resource_table
    .get::<HttpServer>(rid)
    .ok_or_else(bad_resource_id)?;

  let (tcp_stream, _) = http_server.listener.accept().await?;
  let deno_service = DenoService::default();

  let hyper_connection = Http::new()
    .with_executor(LocalExecutor)
    .serve_connection(tcp_stream, deno_service.clone());

  let conn_resource = ConnResource {
    hyper_connection: Rc::new(RefCell::new(hyper_connection)),
    deno_service,
  };
  let rid = state.borrow_mut().resource_table.add(conn_resource);

  Ok(rid)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextRequestResponse(
  // connection_closed:
  bool,
  // request_body_rid:
  Option<ResourceId>,
  // response_sender_rid:
  ResourceId,
  // method:
  String,
  // headers:
  Vec<(String, String)>,
  // url:
  String,
);

pub async fn op_next_request(
  state: Rc<RefCell<OpState>>,
  rid: ResourceId,
  _data: Option<ZeroCopyBuf>,
) -> Result<NextRequestResponse, AnyError> {
  let conn_resource = state
    .borrow()
    .resource_table
    .get::<ConnResource>(rid)
    .ok_or_else(bad_resource_id)?;

  poll_fn(|cx| {
    let poll_result =
      conn_resource.hyper_connection.borrow_mut().poll_unpin(cx);

    if let Poll::Ready(Err(e)) = poll_result {
      return Poll::Ready(Err(e));
    }

    if let Some(request_resource) =
      conn_resource.deno_service.inner.borrow_mut().take()
    {
      let tx = request_resource.response_tx;
      let req = request_resource.request;
      let method = req.method().to_string();

      let headers = Vec::with_capacity(req.headers().len());

      // for (name, value) in req.headers().iter() {
      //   let name = name.to_string();
      //   let value = value.to_str().unwrap_or("").to_string();
      //   headers.push((name, value));
      // }

      let url = req.uri().to_string();

      let has_body = if let Some(exact_size) = req.size_hint().exact() {
        exact_size > 0
      } else {
        true
      };

      let maybe_request_body_rid = if has_body {
        let stream: BytesStream = Box::pin(req.into_body().map(|r| {
          r.map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
        }));
        let stream_reader = StreamReader::new(stream);
        let mut state = state.borrow_mut();
        let request_body_rid = state.resource_table.add(RequestBodyResource {
          reader: AsyncRefCell::new(stream_reader),
          cancel: CancelHandle::default(),
        });
        Some(request_body_rid)
      } else {
        None
      };

      let mut state = state.borrow_mut();
      let response_sender_rid =
        state.resource_table.add(ResponseSenderResource(tx));

      let req_json = NextRequestResponse(
        matches!(poll_result, Poll::Ready(Ok(()))),
        maybe_request_body_rid,
        response_sender_rid,
        method,
        headers,
        url,
      );

      return Poll::Ready(Ok(req_json));
    }
    Poll::Pending
  })
  .await
  .map_err(AnyError::from)
}

#[derive(Deserialize)]
pub struct RespondArgs(
  // rid:
  u32,
  // status:
  u16,
  // headers:
  Vec<(String, String)>,
);

pub fn op_respond(
  state: &mut OpState,
  args: RespondArgs,
  data: Option<ZeroCopyBuf>,
) -> Result<Option<ResourceId>, AnyError> {
  let rid = args.0;
  let status = args.1;
  let headers = args.2;

  let response_sender = state
    .resource_table
    .take::<ResponseSenderResource>(rid)
    .ok_or_else(bad_resource_id)?;
  let response_sender = Rc::try_unwrap(response_sender)
    .ok()
    .expect("multiple op_respond ongoing");

  let mut builder = Response::builder().status(status);

  builder.headers_mut().unwrap().reserve(headers.len());
  for (key, value) in headers {
    builder = builder.header(&key, &value);
  }

  let res;
  let maybe_response_body_rid = if let Some(d) = data {
    // If a body is passed, we use it, and don't return a body for streaming.
    res = builder.body(Vec::from(&*d).into())?;
    None
  } else {
    // If no body is passed, we return a writer for streaming the body.
    let (sender, body) = Body::channel();
    res = builder.body(body)?;

    let response_body_rid =
      state.resource_table.add(DyperResponseBodyResource {
        body: AsyncRefCell::new(sender),
        cancel: CancelHandle::default(),
      });

    Some(response_body_rid)
  };

  // oneshot::Sender::send(v) returns |v| on error, not an error object.
  // The only failure mode is the receiver already having dropped its end
  // of the channel.
  if response_sender.0.send(res).is_err() {
    eprintln!("op_respond: receiver dropped");
    return Err(type_error("internal communication error"));
  }

  Ok(maybe_response_body_rid)
}

pub async fn op_read_request(
  state: Rc<RefCell<OpState>>,
  rid: ResourceId,
  data: Option<ZeroCopyBuf>,
) -> Result<usize, AnyError> {
  let mut data = data.ok_or_else(null_opbuf)?;

  let resource = state
    .borrow()
    .resource_table
    .get::<RequestBodyResource>(rid as u32)
    .ok_or_else(bad_resource_id)?;
  let mut reader = RcRef::map(&resource, |r| &r.reader).borrow_mut().await;
  let cancel = RcRef::map(resource, |r| &r.cancel);
  let read = reader.read(&mut data).try_or_cancel(cancel).await?;
  Ok(read)
}

pub async fn op_write_response(
  state: Rc<RefCell<OpState>>,
  rid: ResourceId,
  data: Option<ZeroCopyBuf>,
) -> Result<(), AnyError> {
  let buf = data.ok_or_else(null_opbuf)?;

  let resource = state
    .borrow()
    .resource_table
    .get::<DyperResponseBodyResource>(rid as u32)
    .ok_or_else(bad_resource_id)?;
  let mut body = RcRef::map(&resource, |r| &r.body).borrow_mut().await;
  let cancel = RcRef::map(resource, |r| &r.cancel);
  todo!()
  // body.send_data((*buf).into()).or_cancel(cancel).await??;
  // Ok(())
}

type BytesStream =
  Pin<Box<dyn Stream<Item = std::io::Result<bytes::Bytes>> + Unpin>>;

struct RequestBodyResource {
  reader: AsyncRefCell<StreamReader<BytesStream, bytes::Bytes>>,
  cancel: CancelHandle,
}

impl Resource for RequestBodyResource {
  fn name(&self) -> Cow<str> {
    "requestBody".into()
  }
}

struct ResponseSenderResource(oneshot::Sender<Response<Body>>);

impl Resource for ResponseSenderResource {
  fn name(&self) -> Cow<str> {
    "responseSender".into()
  }
}

struct DyperResponseBodyResource {
  body: AsyncRefCell<hyper::body::Sender>,
  cancel: CancelHandle,
}

impl Resource for DyperResponseBodyResource {
  fn name(&self) -> Cow<str> {
    "requestBody".into()
  }
}
