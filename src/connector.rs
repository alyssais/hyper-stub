// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use futures::prelude::*;
use hyper::body::{Body, Payload};
use hyper::client::connect::{Connect, Connected, Destination};
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use hyper::Response;
use memsocket::{self, UnboundedSocket};
use std::error::Error;
use std::sync::Arc;
use tokio;

#[doc(hidden)]
pub struct Connector<N> {
    new_service: N,
    server: Arc<Http>,
}

impl<N> Connector<N> {
    pub fn new(new_service: N) -> Self {
        Connector {
            new_service,
            server: Arc::new(Http::new()),
        }
    }
}

// A custom future type is necessary because using Future::map returns a type
// that includes an anonymous type, and so can't be associated with a struct.
#[doc(hidden)]
pub struct ConnectorConnectFuture<ServiceFuture> {
    server: Arc<Http>,
    service_future: ServiceFuture,
}

impl<ResBody, ResponseError, ServiceError, ResponseFuture, ServiceFuture, S> Future
    for ConnectorConnectFuture<ServiceFuture>
where
    ResBody: Payload,
    ResponseError: Error + Send + Sync + 'static,
    ServiceError: Error + Send + Sync + 'static,
    ResponseFuture: Future<Item = Response<S::ResBody>, Error = ResponseError> + Send + 'static,
    ServiceFuture: Future<Item = S, Error = ServiceError> + Send + 'static,
    S: Service<ReqBody = Body, ResBody = ResBody, Error = ResponseError, Future = ResponseFuture>
        + Send
        + 'static,
{
    type Item = (UnboundedSocket, Connected);
    type Error = ServiceError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.service_future.poll().map(|async| {
            async.map(|service| {
                let (client_io, server_io) = memsocket::unbounded();
                tokio::spawn(
                    self.server
                        .serve_connection(server_io, service)
                        .map_err(|err| panic!("{:?}", err)),
                );

                (client_io, Connected::new().proxy(true))
            })
        })
    }
}

impl<ResBody, ResponseError, ServiceError, ResponseFuture, ServiceFuture, S, N> Connect
    for Connector<N>
where
    ResBody: Payload,
    ResponseError: Error + Send + Sync + 'static,
    ServiceError: Error + Send + Sync + 'static,
    ResponseFuture: Future<Item = Response<S::ResBody>, Error = ResponseError> + Send + 'static,
    ServiceFuture: Future<Item = S, Error = ServiceError> + Send + 'static,
    S: Service<ReqBody = Body, ResBody = ResBody, Error = ResponseError, Future = ResponseFuture>
        + Send
        + 'static,
    N: NewService<
            ReqBody = S::ReqBody,
            ResBody = S::ResBody,
            Future = ServiceFuture,
            Error = ResponseError,
            Service = S,
            InitError = ServiceError,
        >
        + Sync
        + Send,
{
    type Transport = UnboundedSocket;
    type Error = ServiceError;
    type Future = ConnectorConnectFuture<ServiceFuture>;

    fn connect(&self, _: Destination) -> Self::Future {
        let server = self.server.clone();
        ConnectorConnectFuture {
            server,
            service_future: self.new_service.new_service(),
        }
    }
}
