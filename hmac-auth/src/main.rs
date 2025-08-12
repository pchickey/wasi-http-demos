use anyhow::{anyhow, Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use wstd::http::{
    body::IncomingBody,
    server::{Finished, Responder},
    IntoBody, Method, Request, Response, StatusCode,
};

#[wstd::http_server]
async fn main(req: Request<IncomingBody>, responder: Responder) -> Finished {
    let resp = match handle(req).await {
        Ok(body) => Response::builder()
            .status(200)
            .body(body.into_body())
            .unwrap(),
        Err(e) => Response::builder()
            .status(
                e.downcast_ref::<StatusCode>()
                    .unwrap_or(&StatusCode::INTERNAL_SERVER_ERROR),
            )
            .body(format!("{e:?}").into_body())
            .unwrap(),
    };
    responder.respond(resp).await
}

const SECRET_KEY: Option<&str> = option_env!("SECRET_KEY");

async fn handle(req: Request<IncomingBody>) -> Result<String> {
    let headers = req.headers();
    let signature = headers.get("signature");
    if signature.is_none() {
        Err(anyhow!("missing Signature header").context(StatusCode::BAD_REQUEST))?
    }
    let signature = hex::decode(signature.expect("validated signature is some"))
        .context(StatusCode::BAD_REQUEST)?;

    if req.method() != Method::GET {
        Err(anyhow!("unsupported method {}", req.method()).context(StatusCode::METHOD_NOT_ALLOWED))?
    }

    let secret_key = SECRET_KEY.unwrap_or("12345678");
    let secret_key = hex::decode(secret_key).context("decoding secret key")?;

    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_key).context("constucting hmac")?;
    let uri = req.uri().to_string();
    mac.update(uri.as_bytes());

    mac.verify_slice(&signature)
        .context(StatusCode::UNAUTHORIZED)?;

    Ok("authorized".to_string())
}
