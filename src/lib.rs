// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! hyper-stub provides functions to create [hyper] clients that convert requests
//! to responses using predefined functions, without doing any actual
//! networking. This means the entire request/response lifecycle happens in a
//! single process, and should have performance and stability improvements over,
//! for example, binding to a port. One potential use case for this is stubbing
//! HTTP interactions in tests to avoid slow/flakey/absent internet connections.
//!
//! The simplest case uses [`proxy_client_fn_ok`] to create a client bound to a
//! simple function that directly maps from a request to a response:
//!
//! ```
//! # extern crate futures;
//! # extern crate hyper;
//! # extern crate hyper_stub;
//! # extern crate tokio;
//! #
//! use futures::{Future, Stream};
//! use hyper::{Request, Response, Uri};
//! use hyper_stub::proxy_client_fn_ok;
//! use tokio::runtime::current_thread::Runtime;
//!
//! let echo_client = proxy_client_fn_ok(|request| {
//!     let body = request.into_body();
//!     Response::new(body)
//! });
//!
//! let url: Uri = "http://example.com".parse().unwrap();
//! let mut builder = Request::post(url);
//! let request = builder.body("hello world".into()).unwrap();
//! let future = echo_client.request(request)
//!     .and_then(|res| res.into_body().concat2())
//!     .map(|bytes| {
//!         let body = String::from_utf8(bytes.to_vec()).unwrap();
//!         println!("{}", body);
//!     })
//!     .map_err(|error| panic!("ERROR: {:?}", error));
//!
//! Runtime::new().unwrap().block_on(future).unwrap();
//! ```
//!
//! If the function needs to return an error, or respond to the request
//! asynchronously, [`proxy_client_fn`] can be used.
//!
//! Finally, an advanced use case is using hyper [`services`] instead of simple
//! functions. This can be done with the [`proxy_client`] function.
//!
//! [hyper]: https://hyper.rs
//! [services]: https://docs.rs/hyper/0.12.1/hyper/service/index.html
//! [`proxy_client_fn_ok`]: fn.proxy_client_fn_ok.html
//! [`proxy_client_fn`]: fn.proxy_client_fn.html
//! [`proxy_client`]: fn.proxy_client.html

extern crate futures;
extern crate hyper;
extern crate memsocket;
extern crate tokio;

mod connector;
mod never;

use connector::Connector;
use futures::prelude::*;
use hyper::body::{Body, Payload};
use hyper::client::connect::Connect;
use hyper::service::{NewService, Service};
use hyper::{Client, Request, Response};
use never::Never;
use std::error::Error;

/// Creates a hyper client whose requests are converted to responses by being
/// passed through a hyper [`Service`] instantiated by and returned from the given
/// [`NewService`].
///
/// [`proxy_client_fn`] is much more simple and almost as powerful, so should
/// generally be preferred.
///
/// [`Service`]: https://docs.rs/hyper/0.12.1/hyper/service/index.html
/// [`NewService`]: https://docs.rs/hyper/0.12.1/hyper/service/trait.NewService.html
/// [`proxy_client_fn`]: fn.proxy_client_fn.html
pub fn proxy_client<ResBody, ResponseError, ServiceError, ResponseFuture, ServiceFuture, S, N>(
    new_service: N,
) -> Client<Connector<N>>
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
    Client::builder()
        .set_host(true)
        .build(Connector::new(new_service))
}

/// Creates a hyper client whose requests are converted to responses by being
/// passed through the given handler function, which returns a future.
pub fn proxy_client_fn<E, Fut, F>(handler: F) -> Client<impl Connect>
where
    E: Error + Send + Sync + 'static,
    Fut: Future<Item = Response<Body>, Error = E> + Send + 'static,
    F: Fn(Request<Body>) -> Fut + Send + Sync + Copy + 'static,
{
    use futures::future;
    use hyper::service::service_fn;

    proxy_client(move || future::ok::<_, Never>(service_fn(handler)))
}

/// Creates a hyper client whose requests are converted to responses by being
/// passed through the given handler function.
///
/// See [`proxy_client_fn`] if errors or asynchronous processing are required.
///
/// [`proxy_client_fn`]: fn.proxy_client_fn.html
pub fn proxy_client_fn_ok<F>(handler: F) -> Client<impl Connect>
where
    F: Fn(Request<Body>) -> Response<Body> + Send + Sync + Copy + 'static,
{
    use futures::future;

    proxy_client_fn(move |req| future::ok::<_, Never>(handler(req)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok() {
        use futures::prelude::*;
        use tokio::runtime::current_thread::Runtime;

        let client = proxy_client_fn_ok(|req| {
            let query = req.uri().query().unwrap().to_string();
            Response::new(query.into())
        });

        Runtime::new()
            .unwrap()
            .block_on({
                client
                    .get("https://example.com?foo=bar".parse().unwrap())
                    .and_then(|res| res.into_body().concat2())
                    .map(|bytes| {
                        let body = String::from_utf8(bytes.to_vec()).unwrap();
                        assert_eq!(body, "foo=bar");
                    })
                    .map_err(|err| panic!("{:?}", err))
            })
            .unwrap();
    }

    #[test]
    fn test_err() {
        use futures::future::{self, FutureResult};
        use futures::prelude::*;
        use std::fmt::{self, Display, Formatter};
        use tokio::runtime::current_thread::Runtime;

        #[derive(Debug)]
        struct NewServiceError;

        impl Display for NewServiceError {
            fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
                write!(fmt, "correct error for test")
            }
        }

        impl Error for NewServiceError {
            fn description(&self) -> &str {
                "It broke"
            }
        }

        impl Service for Never {
            type ReqBody = Body;
            type ResBody = Body;
            type Error = Self;
            type Future = FutureResult<Response<Self::ResBody>, Self>;

            fn call(&mut self, _: Request<Self::ReqBody>) -> Self::Future {
                unreachable!()
            }
        }

        let client = proxy_client(|| future::err::<Never, _>(NewServiceError));

        let _ = Runtime::new().unwrap().block_on({
            client
                .get("https://example.com".parse().unwrap())
                .map(|res| panic!("didn't error: {:?}", res))
                .map_err(|err| assert!(err.to_string().contains("correct error for test")))
        });
    }
}
