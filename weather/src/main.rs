use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use wstd::http::{
    body::IncomingBody,
    server::{Finished, Responder},
    Client, IntoBody, Method, Request, Response, StatusCode, Uri,
};

// Be polite: informative user-agent
const USER_AGENT: &str =
    "Weather wasi-http demo (https://github.com/pchickey/wasi-http-demos/weather)";

#[wstd::http_server]
async fn main(req: Request<IncomingBody>, responder: Responder) -> Finished {
    let resp = match handle(req).await {
        Ok(body) => Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
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
    if req.method() != Method::GET {
        Err(anyhow!("unsupported method {}", req.method()).context(StatusCode::METHOD_NOT_ALLOWED))?
    }
    let location = get_location_name(&req).context("getting location name")?;

    let location_results = location_search(&location)
        .await
        .context("searching for location")?;

    use futures_concurrency::future::TryJoin;
    let results = location_results
        .iter()
        .map(|l| async move {
            let l = l.clone();
            let w = fetch_weather(&l)
                .await
                .with_context(|| format!("fetching weather for {}", l.qualified_name))?;
            // FIXME instead of using a pair here, use a struct so we encode
            // this json as a record.
            Ok::<_, anyhow::Error>((l, w))
        })
        .collect::<Vec<_>>()
        .try_join()
        .await?;

    serde_json::to_string(&results).context("serializing result to json")
}

fn get_location_name(req: &Request<IncomingBody>) -> Result<String> {
    let uri = req.uri();
    let query = match uri.query() {
        None => return Ok("Portland".to_string()),
        Some(q) => q,
    };
    #[derive(Deserialize)]
    struct QueryParams {
        city: String,
    }
    let params: QueryParams = serde_qs::from_str(query).map_err(|_| {
        anyhow!("missing required query param `city`").context(StatusCode::BAD_REQUEST)
    })?;

    Ok(params.city)
}

#[derive(Debug, Serialize, Clone)]
struct Location {
    name: String,
    qualified_name: String,
    population: Option<u32>,
    lat: f64,
    lon: f64,
}

async fn location_search(location_name: &str) -> Result<Vec<Location>> {
    #[derive(Serialize)]
    struct Query {
        name: String,
        count: usize,
        language: String,
        format: String,
    }
    let query = Query {
        name: location_name.to_string(),
        count: 10,
        language: "en".to_string(),
        format: "json".to_string(),
    };

    let uri = Uri::builder()
        .scheme("https")
        .authority("geocoding-api.open-meteo.com")
        .path_and_query(format!(
            "/v1/search?{}",
            serde_qs::to_string(&query).context("serialize query string")?
        ))
        .build()?;
    let request = Request::get(uri)
        .header("User-Agent", USER_AGENT)
        .body(wstd::io::empty())?;

    let resp = Client::new().send(request).await?;
    if resp.status() != StatusCode::OK {
        anyhow::bail!("geocoding-api returned status {:?}", resp.status());
    }

    #[derive(Deserialize)]
    struct Contents {
        results: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        name: String,
        latitude: f64,
        longitude: f64,
        population: Option<u32>,
        admin1: String,
        admin2: Option<String>,
        admin3: Option<String>,
        admin4: Option<String>,
    }

    let contents: Contents = resp.into_body().json().await?;
    let mut results = contents
        .results
        .into_iter()
        .map(|item| {
            let mut qualified_name = String::new();
            if let Some(name) = item.admin4 {
                qualified_name.push_str(&name);
                qualified_name.push_str(", ");
            }
            if let Some(name) = item.admin3 {
                qualified_name.push_str(&name);
                qualified_name.push_str(", ");
            }
            if let Some(name) = item.admin2 {
                qualified_name.push_str(&name);
                qualified_name.push_str(", ");
            }
            qualified_name.push_str(&item.admin1);
            Location {
                name: item.name,
                lat: item.latitude,
                lon: item.longitude,
                population: item.population,
                qualified_name,
            }
        })
        .collect::<Vec<_>>();
    // Sort by highest population first
    results.sort_by(|a, b| b.population.partial_cmp(&a.population).unwrap());
    Ok(results)
}

#[derive(Debug, Serialize)]
struct Weather {
    temp: f64,
    temp_unit: String,
    rain: f64,
    rain_unit: String,
}

async fn fetch_weather(location: &Location) -> Result<Weather> {
    #[derive(Serialize)]
    struct Query {
        latitude: f64,
        longitude: f64,
        current: String,
    }
    let query = Query {
        latitude: location.lat,
        longitude: location.lon,
        current: "temperature_2m,rain".to_string(),
    };
    let uri = Uri::builder()
        .scheme("https")
        .authority("api.open-meteo.com")
        .path_and_query(format!(
            "/v1/forecast?{}",
            serde_qs::to_string(&query).context("serialize query string")?
        ))
        .build()?;
    let request = Request::get(uri)
        .header("User-Agent", USER_AGENT)
        .body(wstd::io::empty())?;

    let resp = Client::new().send(request).await?;
    if resp.status() != StatusCode::OK {
        anyhow::bail!("forecast api returned status {:?}", resp.status());
    }

    #[derive(Deserialize)]
    struct Contents {
        current_units: Units,
        current: Data,
    }
    #[derive(Deserialize)]
    struct Units {
        temperature_2m: String,
        rain: String,
    }
    #[derive(Deserialize)]
    struct Data {
        temperature_2m: f64,
        rain: f64,
    }

    let contents: Contents = resp.into_body().json().await?;
    let weather = Weather {
        temp: contents.current.temperature_2m,
        temp_unit: contents.current_units.temperature_2m,
        rain: contents.current.rain,
        rain_unit: contents.current_units.rain,
    };
    Ok(weather)
}
