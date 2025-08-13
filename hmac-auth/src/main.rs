use anyhow::{Context, Result, anyhow};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use wstd::http::{
    IntoBody, Method, Request, Response, StatusCode,
    body::IncomingBody,
    server::{Finished, Responder},
};

/// Secret key to initialize SHA-256 HMAC. use the SECRET_KEY environment
/// variable at build time to provide a hex value (must be even number of
/// digits), or else it defaults to `12345678`, which is the combination on my
/// luggage.
fn secret_key() -> Result<Vec<u8>> {
    const SECRET_KEY: Option<&str> = option_env!("SECRET_KEY");
    let secret_key = SECRET_KEY.unwrap_or("12345678");
    hex::decode(secret_key).context("decoding secret key")
}

/// Handle an HTTP request, return the response body on success, or else an
/// error message. Errors containing a StatusCode will be used to set response
/// status.
fn handle(req: Request<IncomingBody>) -> Result<String> {
    // First extract signature, with error if not present
    let signature = request_signature(&req)?;

    // reject non-GET
    if req.method() != Method::GET {
        Err(anyhow!("unsupported method {}", req.method()).context(StatusCode::METHOD_NOT_ALLOWED))?
    }

    let secret_key = secret_key().context("calucating secret key")?;

    // Calculate HMAC of the request URI
    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_key).context("constucting hmac")?;
    let uri = req.uri().to_string();
    mac.update(uri.as_bytes());

    // Verify HMAC matches signature. verify_slice performs a constant-time
    // comparison.
    mac.verify_slice(&signature)
        .context(StatusCode::UNAUTHORIZED)?;

    // Success
    Ok("authorized".to_string())
}

/// Extract the request's Signature header, which should contain a hexadecimal
/// value.
fn request_signature(req: &Request<IncomingBody>) -> Result<Vec<u8>> {
    let headers = req.headers();
    let signature = headers.get("signature");
    if signature.is_none() {
        Err(anyhow!("missing Signature header").context(StatusCode::BAD_REQUEST))?
    }
    hex::decode(signature.expect("validated signature is some")).context(StatusCode::BAD_REQUEST)
}

/// The wstd http server runs `handle` and then packages the success or error into
/// the appropriate http response.
#[wstd::http_server]
async fn main(req: Request<IncomingBody>, responder: Responder) -> Finished {
    let resp = match handle(req) {
        Ok(body) => Response::builder()
            .status(200)
            .body(body.into_body())
            .unwrap(),
        Err(e) => Response::builder()
            .status(
                // If handle's Error contains a StatusCode in the context, we
                // use it here, or default to 503 internal server error.
                e.downcast_ref::<StatusCode>()
                    .unwrap_or(&StatusCode::INTERNAL_SERVER_ERROR),
            )
            // Since this is a demo, use the debug representation for the
            // error body. In prod you'd perhaps log this.
            .body(format!("{e:?}").into_body())
            .unwrap(),
    };
    responder.respond(resp).await
}
