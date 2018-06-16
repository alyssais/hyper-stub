# hyper-stub

hyper-stub is a Rust library that provides functions to create [hyper][] clients
that convert requests to responses using predefined functions, without doing any
actual networking. This means the entire request/response lifecycle happens in a
single process, and should have performance and stability improvements over,
for example, binding to a port. One potential use case for this is stubbing
HTTP interactions in tests to avoid slow/flakey/absent internet connections.

For API reference and usage examples, see the [documentation][].

[hyper]: https://hyper.rs
[documentation]: https://docs.rs/hyper-stub
