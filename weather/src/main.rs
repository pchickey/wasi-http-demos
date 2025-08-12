use anyhow::{anyhow, Context, Result};
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

async fn handle(req: Request<IncomingBody>) -> Result<String> {
    let headers = req.headers();
    if req.method() != Method::GET {
        Err(anyhow!("unsupported method {}", req.method()).context(StatusCode::METHOD_NOT_ALLOWED))?
    }
    Ok("".to_string())
}

// this is a demo of using the http client with a json API.
// TODO:
// 1. take a city name param (unwrap or default="Berlin")
// 2. get a list of suggested locations from
//    https://geocoding-api.open-meteo.com/v1/search?name=Berlin&count=10&language=en&format=json.
// 3. for each location, get current temp and rain from
//    https://api.open-meteo.com/v1/forecast?latitude=52.52&longitude=13.41&current=temperature_2m,rain
// 4. report each location and its temp and rain
