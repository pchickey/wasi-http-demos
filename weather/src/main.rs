//! This demo app shows a wasi-http server making an arbitrary number of
//! http requests as part of serving a single requests.
//!
//! Use the request query string to pass the parameters `city`, and optionally
//! `count` (defaults to 10). This app will tell you the current weather in
//! a set of `count` locations matching the `city` name. For example, when
//! searching for `city=portland&count=2`, it will return Portland, OR and
//! then Portland, ME - location results are sorted by population.
//!
//! This app first makes a request to `geocoding-api.open-meteo.com` to search
//! for a set of `count` locations for a given `city` name.
//!
//! Then, it makes `count` requests to `api.open-meteo.com`'s forecast api to
//! get the current temperature and rain accumulation in each of those
//! locations.
//!
//! The complete set of locations and weather reports are retuned as a json
//! array of records.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use wstd::http::{
    Client, IntoBody, Method, Request, Response, StatusCode, Uri,
    body::IncomingBody,
    server::{Finished, Responder},
};

/// Be polite: user-agent tells server where these results came from, so they
/// can easily block abuse
const USER_AGENT: &str = "Weather wasi-http demo (https://github.com/pchickey/wasi-http-demos)";

/// Handle all requests to this server, giving either the response body or an
/// error. Use a StatusCode in the context of the error to set error status
/// code.
async fn handle(req: Request<IncomingBody>) -> Result<String> {
    // reject anything besides GET
    if req.method() != Method::GET {
        Err(anyhow!("unsupported method {}", req.method()).context(StatusCode::METHOD_NOT_ALLOWED))?
    }
    // Parse the query out of the request
    let query = get_query(&req).context("getting location name")?;

    // Search for the locations in the query
    let location_results = fetch_locations(&query)
        .await
        .context("searching for location")?;

    // just used for serializing a json record in results
    #[derive(Serialize)]
    struct Item {
        location: Location,
        weather: Weather,
    }

    use futures_concurrency::future::TryJoin;
    let results = location_results
        .into_iter()
        // For each location found, constuct a future which fetches the
        // weather, then returns the record of location, weather
        .map(|location| async move {
            let weather = fetch_weather(&location)
                .await
                .with_context(|| format!("fetching weather for {}", location.qualified_name))?;
            Ok::<_, anyhow::Error>(Item { location, weather })
        })
        // Collect a vec of futures
        .collect::<Vec<_>>()
        // TryJoin::try_join takes a vec of futures which return a
        // result<item, error>, and gives a future which returns a
        // result<vec<item>, error>
        .try_join()
        // Get all of the successful items, or else the first error to
        // resolve.
        .await?;

    // Serialize vec<item> to json
    serde_json::to_string(&results).context("serializing result to json")
}

/// The query string given to this server contains a city, and optionally a
/// count.
#[derive(Deserialize)]
struct Query {
    city: String,
    #[serde(default = "default_count")]
    count: u32,
}
/// When the count is not given in the query string, it defaults to this number
const fn default_count() -> u32 {
    10
}
/// Default Query for when none is given. Portland is a good enough location
/// for me, so its good enough for the demo.
impl Default for Query {
    fn default() -> Self {
        Query {
            city: "Portland".to_string(),
            count: default_count(),
        }
    }
}

/// Parse the Query from the request uri.
fn get_query(req: &Request<IncomingBody>) -> Result<Query> {
    let uri = req.uri();
    // default if query is missing, because this is a demo and nobody wants a
    // BAD_REQUEST on an empty curl.
    if uri.query().is_none() {
        return Ok(Query::default());
    }
    // serde_qs (qs stands for query string) will extract the fields given in
    // Query's Deserialize impl. die with BAD_REQUEST if parsing fails.
    let query: Query = serde_qs::from_str(uri.query().unwrap()).context(StatusCode::BAD_REQUEST)?;

    // count of 0 is also a BAD_REQUEST.
    if query.count == 0 {
        Err(anyhow!("nonzero count required").context(StatusCode::BAD_REQUEST))?;
    }
    Ok(query)
}

/// Location struct contains the fields we care from the location search. We
/// massage the geolocation API response down to these fields because we dont
/// care about a bunch of its contents. The Serialize allows us to return this
/// value in our server response json.
#[derive(Debug, Serialize)]
struct Location {
    name: String,
    qualified_name: String,
    population: Option<u32>,
    latitude: f64,
    longitude: f64,
}

/// Fetch the locations corresponding to the query from the open-meteo
/// geocoding API.
async fn fetch_locations(query: &Query) -> Result<Vec<Location>> {
    // Utility struct describes the fields we use in the geocoding api's query
    // string
    #[derive(Serialize)]
    struct GeoQuery {
        name: String,
        count: u32,
        language: String,
        format: String,
    }
    // Value of the fields in the query string:
    let geo_query = GeoQuery {
        name: query.city.clone(),
        count: query.count,
        language: "en".to_string(),
        format: "json".to_string(),
    };

    // Construct the request uri using serde_qs to serialize GeoQuery into a query string.
    let uri = Uri::builder()
        .scheme("http")
        .authority("geocoding-api.open-meteo.com")
        .path_and_query(format!(
            "/v1/search?{}",
            serde_qs::to_string(&geo_query).context("serialize query string")?
        ))
        .build()?;
    // Request is a GET request with no body. User agent is polite to provide.
    let request = Request::get(uri)
        .header("User-Agent", USER_AGENT)
        .body(wstd::io::empty())?;

    // Make the request
    let resp = Client::new().send(request).await?;
    // Die with 503 if geocoding api fails for some reason
    if resp.status() != StatusCode::OK {
        anyhow::bail!("geocoding-api returned status {:?}", resp.status());
    }

    // Utility structs with Deserialize impls to extract the fields we care
    // about from the API's json response.
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
        // There are up to 4 admin region strings provided, only the first one
        // seems to be guaranteed to be delivered. If it was my API, I would
        // have made a single field `admin` which has a list of strings, but
        // its not my API!
        admin1: String,
        admin2: Option<String>,
        admin3: Option<String>,
        admin4: Option<String>,
    }
    impl Item {
        /// The API returns a set of "admin" names (for administrative
        /// regions), pretty-print them from most specific to least specific:
        fn qualified_name(&self) -> String {
            let mut n = String::new();
            if let Some(name) = &self.admin4 {
                n.push_str(name);
                n.push_str(", ");
            }
            if let Some(name) = &self.admin3 {
                n.push_str(name);
                n.push_str(", ");
            }
            if let Some(name) = &self.admin2 {
                n.push_str(name);
                n.push_str(", ");
            }
            n.push_str(&self.admin1);
            n
        }
    }

    // Collect the response body and parse the Contents field out of the json:
    let contents: Contents = resp
        .into_body()
        .json()
        .await
        .context("parsing geocoding-api response")?;

    // Massage the Contents into a Vec<Location>.
    let mut results = contents
        .results
        .into_iter()
        .map(|item| {
            let qualified_name = item.qualified_name();
            Location {
                name: item.name,
                latitude: item.latitude,
                longitude: item.longitude,
                population: item.population,
                qualified_name,
            }
        })
        .collect::<Vec<_>>();
    // Sort by highest population first.
    results.sort_by(|a, b| b.population.partial_cmp(&a.population).unwrap());
    Ok(results)
}

/// Weather struct contains the items in the weather report we care about: the
/// temperature, how much rain, and the units for each. The Serialize allows
/// us to return this value in our server response json.
#[derive(Debug, Serialize)]
struct Weather {
    temp: f64,
    temp_unit: String,
    rain: f64,
    rain_unit: String,
}

/// Fetch the weather for a given location from the open-meto forecast API.
async fn fetch_weather(location: &Location) -> Result<Weather> {
    // Utility struct for the query string expected by the forecast API
    #[derive(Serialize)]
    struct ForecastQuery {
        latitude: f64,
        longitude: f64,
        current: String,
    }
    // Value used for the forecast api query string
    let query = ForecastQuery {
        latitude: location.latitude,
        longitude: location.longitude,
        current: "temperature_2m,rain".to_string(),
    };
    // Construct the uri to the forecast api, serializing the query string
    // with serde_qs.
    let uri = Uri::builder()
        .scheme("http")
        .authority("api.open-meteo.com")
        .path_and_query(format!(
            "/v1/forecast?{}",
            serde_qs::to_string(&query).context("serialize query string")?
        ))
        .build()?;
    // Make the GET request, attaching user-agent, empty body.
    let request = Request::get(uri)
        .header("User-Agent", USER_AGENT)
        .body(wstd::io::empty())?;
    let resp = Client::new().send(request).await?;

    // Bubble up error if forecast api failed
    if resp.status() != StatusCode::OK {
        anyhow::bail!("forecast api returned status {:?}", resp.status());
    }

    // Utility structs for extracting fields from the forecast api's json
    // response.
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

    // Parse the contents of the json response
    let contents: Contents = resp.into_body().json().await?;
    // Massage those structs into a single Weather
    let weather = Weather {
        temp: contents.current.temperature_2m,
        temp_unit: contents.current_units.temperature_2m,
        rain: contents.current.rain,
        rain_unit: contents.current_units.rain,
    };
    Ok(weather)
}

/// The wstd http server runs `handle` and then packages the success or error into
/// the appropriate http response.
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
