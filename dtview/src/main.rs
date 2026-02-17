use actix_web::{
    App, Error, HttpRequest, HttpResponse, HttpResponseBuilder, HttpServer, Responder, get,
    http::StatusCode, web,
};
use anyhow::Context;
use clap::Parser;
use dtparse::{
    BasicFileReader, DeviceTree, Node, ParseErrorReport, ParsingResult, ReportDisplay,
    SimpleFileSystemIncluder, parse,
};
use indexmap::IndexMap;
use notify::{Event, EventKind, Watcher};
use serde::Serialize;
use std::{
    io::{BufWriter, Stdout, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::RwLock, time::sleep};

#[derive(Parser)]
struct Args {
    entrypoint: PathBuf,
    #[clap(short, long)]
    bind_to: Option<SocketAddr>,
}

#[derive(Default, Debug)]
struct State {
    entrypoint: PathBuf,
    compiled: RwLock<Option<CompiledState>>,
    recompile_signal: tokio::sync::Notify,
}

#[derive(Default, Serialize, Debug)]
struct CompiledState {
    compiled_tree: Node,
    sources: IndexMap<PathBuf, String>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponseBuilder::new(StatusCode::OK)
        .content_type("text/html")
        .body(include_str!("../target/index.html"))
}

#[get("/script.js")]
async fn script() -> impl Responder {
    HttpResponseBuilder::new(StatusCode::OK)
        .content_type("application/javascript")
        .body(include_str!("../target/script.js"))
}

#[get("/style.css")]
async fn style() -> impl Responder {
    HttpResponseBuilder::new(StatusCode::OK)
        .content_type("style/css")
        .body(include_str!("../target/style.css"))
}

#[get("/api")]
async fn api(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<State>,
) -> Result<HttpResponse, Error> {
    let (res, mut session, _) = actix_ws::handle(&req, stream)?;
    tokio::spawn(async move {
        loop {
            let to_send = serde_json::to_string(&state.compiled.read().await.as_ref()).unwrap();
            if let Err(_) = session.text(to_send).await {
                break;
            };
            state.recompile_signal.notified().await;
        }
    });
    Ok(res)
}

fn write_report(report: ParseErrorReport, stdout: &mut BufWriter<Stdout>) {
    let mut reader = BasicFileReader::default();
    ReportDisplay::new(&*report)
        .write(&mut reader, stdout)
        .unwrap();
}

fn print_reports(reports: Vec<ParseErrorReport>) {
    let mut stdout = BufWriter::new(std::io::stdout());
    for report in reports {
        write_report(report, &mut stdout);
    }
    stdout.flush().unwrap();
}

fn compile_tree(entrypoint: &Path) -> anyhow::Result<(Result<DeviceTree, ()>, Vec<PathBuf>)> {
    let mut includer = SimpleFileSystemIncluder::new(".");
    let start = std::time::Instant::now();
    let compiled = match parse(entrypoint, &mut includer)
        .map_err(|e| e.error)
        .context("IO Error when compiling devicetree")?
    {
        ParsingResult::AllowCompilation(v, reports) => {
            print_reports(reports);
            println!("compiled in {:?}", start.elapsed());
            v
        }
        ParsingResult::AbortCompilation(_, reports) => {
            print_reports(reports);
            return Ok((Err(()), includer.included_files()));
        }
    };
    Ok((Ok(compiled), includer.included_files()))
}

async fn watch_for_changes(entrypoint: &Path, includes: &Vec<PathBuf>) -> anyhow::Result<()> {
    let notify = Arc::new(tokio::sync::Notify::new());
    let notify_watcher = notify.clone();
    let mut watcher = notify::recommended_watcher(move |event: Result<Event, notify::Error>| {
        match event.unwrap().kind {
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                notify_watcher.notify_waiters()
            }
            _ => {}
        }
    })
    .unwrap();
    watcher
        .watch(entrypoint, notify::RecursiveMode::NonRecursive)
        .context("Could not watch entrypoint")?;
    for include in includes {
        watcher
            .watch(&include, notify::RecursiveMode::NonRecursive)
            .context(format!("Could not watch include {:?}", include))?;
    }
    notify.notified().await;
    Ok(())
}

async fn compile_tennant(state: web::Data<State>) -> anyhow::Result<()> {
    loop {
        sleep(Duration::from_millis(50)).await;
        let (compiled, includes) =
            compile_tree(&state.entrypoint).context("Failed parsing devicetree")?;
        *state.compiled.write().await = match compiled {
            Ok(v) => Some(CompiledState {
                compiled_tree: v.root,
                sources: IndexMap::from_iter(
                    includes
                        .iter()
                        .chain(std::iter::once(&state.entrypoint))
                        .map(|v| (v.clone(), std::fs::read_to_string(v).unwrap())),
                ),
            }),
            Err(()) => None,
        };
        state.recompile_signal.notify_waiters();
        watch_for_changes(&state.entrypoint, &includes)
            .await
            .context("Failed watching for changes")?;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let bind_to = match args.bind_to {
        Some(v) => v,
        None => "[::1]:8080".parse().unwrap(),
    };
    let state = web::Data::new(State {
        entrypoint: args
            .entrypoint
            .canonicalize()
            .context("Could not canonicalize entrypoint path")?,
        ..Default::default()
    });
    let state_clone = state.clone();
    let tennant = compile_tennant(state_clone);
    println!("listening on {:?}", bind_to);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(api)
            .service(index)
            .service(script)
            .service(style)
    })
    .bind(bind_to)?
    .run();
    tokio::select! {
        r = server => {
            r.map_err(|e| e.into())
        }
        r = tennant => {
            r
        }
    }
}
