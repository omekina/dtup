use std::{net::SocketAddr, path::PathBuf};

use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, Responder, get, rt, web};
use clap::Parser;

#[get("/")]
async fn index() -> impl Responder {
    include_str!("../target/index.html")
}

#[get("/script.js")]
async fn script() -> impl Responder {
    include_str!("../target/script.js")
}

#[get("/style.css")]
async fn style() -> impl Responder {
    include_str!("../target/style.css")
}

#[get("/api")]
async fn api(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;
    let mut stream = stream.aggregate_continuations().max_continuation_size(2usize.pow(20));
    rt::spawn(async move {});
    Ok(res)
}

#[derive(Parser)]
struct Args {
    entrypoint: PathBuf,
    #[clap(short, long)]
    bind_to: Option<SocketAddr>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let bind_to = match args.bind_to {
        Some(v) => v,
        None => "[::1]:8080".parse().unwrap(),
    };
    println!("listening on {:?}", bind_to);
    HttpServer::new(|| App::new().service(index))
        .bind(bind_to)?
        .run()
        .await
}
