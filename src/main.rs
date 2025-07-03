use anyhow::{anyhow, Context, Result};
use jaq_core::Filter;
use jaq_json::Val;
use std::rc::Rc;
use std::sync::OnceLock;
use wstd::http::{
    body::IncomingBody,
    server::{Finished, Responder},
    IntoBody, Request, Response,
};

#[wstd::http_server]
async fn main(req: Request<IncomingBody>, responder: Responder) -> Finished {
    let resp = match handle(req).await {
        Ok(body) => Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(body.into_body())
            .unwrap(),
        Err(e) => Response::builder()
            .status(500)
            .body(format!("{e:?}").into_body())
            .unwrap(),
    };
    responder.respond(resp).await
}

type Filt = Filter<jaq_core::Native<Val>>;
static FILTER: OnceLock<Filt> = OnceLock::new();

async fn handle(req: Request<IncomingBody>) -> Result<String> {
    let filter = FILTER.get().expect("filter should be initialized");
    let inputs = jaq_core::RcIter::new(core::iter::empty());

    let body = req.into_body().bytes().await?;
    let mut lexer = hifijson::SliceLexer::new(&body);
    let body_val =
        hifijson::token::Lex::exactly_one(&mut lexer, Val::parse).context("parsing body json")?;

    let vals = filter
        .run((jaq_core::Ctx::new([], &inputs), body_val))
        .collect::<Result<Vec<Val>, jaq_json::Error>>()
        .map_err(|es| anyhow!("filter errors {es:?}"))?;
    let val = Val::Arr(Rc::new(vals));
    Ok(format!("{val}"))
}

#[component_init::init]
fn init() {
    let filt = create_filter().expect("creating filter");
    FILTER
        .set(filt)
        .ok()
        .expect("filter should be uninitialized")
}

fn create_filter() -> Result<Filt> {
    use jaq_core::load::{Arena, File, Loader};
    let file = File {
        code: ".[]",
        path: (),
    };
    let arena = Arena::default();
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let modules = loader
        .load(&arena, file)
        .map_err(|es| anyhow!("loader errors {es:?}"))?;
    let filter = jaq_core::Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|es| anyhow!("compiler errors {es:?}"))?;
    Ok(filter)
}
