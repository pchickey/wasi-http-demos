# wasi-http demos

These demos show some ways you can build Rust applications for the [wasi-http]
standard. Each application is implemented using the [`wstd`] crate.

[wasi-http]: https://github.com/WebAssembly/wasi-http
[`wstd`]: https://github.com/bytecodealliance/wstd

## Building

Use [`rustup`](https://rustup.rs) to install a rust toolchain, if you don't
have one.

Then, build the demos using:

```sh
cargo build --release --target wasm32-wasip2
```

You can run these demos in any wasi-http runtime. The Wasmtime cli is an
excellent choice. You can install it with

```sh
curl https://wasmtime.dev/install.sh -sSf | bash
```


## `hmac-auth`: HMAC request authorization

HMAC hash functions frequently used in authorization. This demo shows an
extremely simple authorization scheme which is not at all suitable for
production use. This demo checks the SHA-256 HMAC of the request URL against
the contents of the `Signature` header.

This demo is based loosely on an [`njs` example].

[`njs` example]: https://github.com/nginx/njs-examples?tab=readme-ov-file#authorizing-requests-using-auth-request-http-authorization-auth-request

The source code is found at [`hmac-auth/src/main.rs`].

[`hmac-auth/src/main.rs`]: https://github.com/pchickey/wasi-http-demos/blob/main/hmac-auth/src/main.rs

You can set the environment variable `SECRET_KEY` at build time to override
the default key, `12345678`, which is what I use on my luggage. You can use
any hexidecimal value for the secret key, as long as it is an even number of
bytes.

To run the `hmac-auth` demo in Wasmtime, use:

```sh
wasmtime serve -Scli ./target/wasm32-wasip2/release/hmac-auth.wasm
```

You can then send requests to the server (by default running on port `8080`).
For help calculating the correct signature, use `hmac-sign`:

```sh
curl -v -H "Signature: $(cargo run -p hmac-sign -- 'http://localhost:8080/foo?bar=baz')" \
    http://localhost:8080/foo?bar=baz
```

Note that the complete URL matching the request must be provided to `hmac-sign`.

## `weather`: A weather client

This demo shows how wasi-http permits making any number of http client
requests while serving a single request.

The source code is found at [`weather/src/main.rs`].

[`weather/src/main.rs`]: https://github.com/pchickey/wasi-http-demos/blob/main/hmac-auth/src/main.rs

To run the `weather` demo in Wasmtime, use:
```sh
wasmtime serve -Scli ./target/wasm32-wasip2/release/weather.wasm
```

You can then request the weather in your city. The weather demo will request a
geolocation API for cities with the given name, and then request the current
weather in each of those cities from a forecast API. Forecast requests are
made in parallel.

```sh
% curl -Ss localhost:8080/\?city=portland\&count=2 | jq
[
  {
    "location": {
      "name": "Portland",
      "qualified_name": "Multnomah, Oregon",
      "population": 652503,
      "lat": 45.52345,
      "lon": -122.67621
    },
    "weather": {
      "temp": 34.5,
      "temp_unit": "°C",
      "rain": 0.0,
      "rain_unit": "mm"
    }
  },
  {
    "location": {
      "name": "Portland",
      "qualified_name": "City of Portland, Cumberland, Maine",
      "population": 66881,
      "lat": 43.65737,
      "lon": -70.2589
    },
    "weather": {
      "temp": 25.8,
      "temp_unit": "°C",
      "rain": 0.0,
      "rain_unit": "mm"
    }
  }
]
```


## `jaq-http`: Transform json bodies using jaq

[jaq] is an alternative implementation of [jq] written in Rust. It is mostly
compatible with the jq expression languages, with [some differences].

[jaq]: https://github.com/01mf02/jaq
[jq]: https://jqlang.org/
[some differences]: https://github.com/01mf02/jaq?tab=readme-ov-file#differences-between-jq-and-jaq

The source code is found at [`jaq/src/main.rs`].

[`jaq/src/main.rs`]: https://github.com/pchickey/wasi-http-demos/blob/main/jaq/src/main.rs

The shell script [`generate-jaq.sh`] takes a jaq program as an argument, and
emits a `jaq-http.wasm` binary which runs that program on an HTTP request
body, returning the result in the response body.

[`generate-jaq.sh`]: https://github.com/pchickey/wasi-http-demos/blob/main/generate-jaq.sh

For example:
```sh
% ./generate-jaq.sh '.a.b'
% wasmtime serve -Scli jaq-http.wasm &
% curl -sS -X POST -d "{\"a\": { \"b\": [1, 2, 3] }, \"c\": { \"d\": 4 } }" http://localhost:8080
[1,2,3]
```

`generate-jaq.sh` will install the cli program [`component-init`] as part of
the build process. We use a shell script to build because, unlike the other
demos, the jaq demo has an optional additional step of running `component-init`
on the resulting component.

`component-init` takes a WebAssembly component which exports a special
function (named, of course, `component-init`), executes that function, and
then saves a new component using the state of the component after execution.
(Restrictions apply - for example, the component cannot call any import
functions during component-init.) The `jaq-http` demo parses the jaq program,
and generates its own internal bytecode for implementing the filter, during
`component-init`. Without `component-init`, this parsing and bytecode
generation is performed, with identical effect, at the start of serving
each http request, because Wasmtime makes a fresh instance of the component
as part of serving each request. Performing the initialization of `jaq-http`
ahead-of-time serves to increase the throughput of this demo by a factor of 5.

[`component-init`]: https://github.com/dicej/component-init
