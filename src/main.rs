extern crate actix;
extern crate actix_web;
extern crate aws_s3_webdav;
extern crate bytes;
extern crate clap;
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate log;
extern crate rusoto_core;
extern crate rusoto_credential;
extern crate rusoto_s3;
extern crate tokio_core;
extern crate toml;

mod routes;
mod env;

use actix_web::{http, server, App};
use rusoto_core::Region;
use std::sync::Arc;
use std::borrow::ToOwned;

fn main() {
    env_logger::init();
    info!("Starting up");

    let matches = clap::App::new("rust-aws-s3-proxy")
        .version("0.1.0")
        .about("AWS S3 Bucket proxy, adding authorization")
        .author("lostintime <lostintime.dev@gmail.com>")
        .arg(
            clap::Arg::with_name("bind")
                .short("b")
                .long("bind")
                .value_name("HOST")
                .env("HTTP_BIND")
                .help("Set HTTP server bind host or socket path")
                .takes_value(true)
                .default_value("0.0.0.0:8080")
                .required(false),
        )
        .arg(
            clap::Arg::with_name("aws_bucket")
                .long("aws-bucket")
                .value_name("BUCKET")
                .env("AWS_BUCKET")
                .help("AWS Bucket name")
                .takes_value(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("aws_key_prefix")
                .long("aws-key-prefix")
                .value_name("KEY_PREFIX")
                .env("AWS_KEY_PREFIX")
                .help("AWS Bucket key prefix")
                .takes_value(true)
                .required(false),
        )
        .arg(
            clap::Arg::with_name("aws_region")
                .long("aws-region")
                .value_name("REGION")
                .env("AWS_REGION")
                .help("AWS Region ID")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let bind_port = matches.value_of("bind").unwrap_or_default().to_owned();

    info!("Start server on {}", bind_port);

    // Start http server
    server::HttpServer::new(move || {
        info!("Building application");
        let args = matches.clone();

        let aws_region_name: String = args.value_of("aws_region")
            .expect("AWS Region argument required")
            .to_owned();

        let aws_region: Region = aws_region_name.parse().expect("Is a valid AWS region id");

        let state = env::AppState::new(env::AppConfig {
            aws: env::AwsConfig::new(&Region::Custom {
                name: aws_region.name().to_owned(),
                endpoint: format!("http://s3.{}.amazonaws.com", aws_region.name()).to_owned(),
            }),
            s3: env::S3Config::new(
                args.value_of("aws_bucket")
                    .expect("AWS Bucket name argument required"),
                args.value_of("aws_key_prefix").and_then(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                }),
            ),
        });

        App::with_state(Arc::new(state))
            .resource("/", |r| r.f(routes::index))
            .default_resource(move |r| {
                r.method(http::Method::GET).f(routes::get_object);
                r.method(http::Method::HEAD).f(routes::head_object);
                r.method(http::Method::PUT).f(routes::put_object);
                r.method(http::Method::DELETE).f(routes::delete_object);
                r.method(http::Method::from_bytes(b"COPY").unwrap())
                    .f(routes::copy_object);
                r.method(http::Method::from_bytes(b"MOVE").unwrap())
                    .f(routes::move_object);
            })
    }).bind(&bind_port)
        .expect(&format!("Cannot bind to {}", &bind_port))
        .run();
}
