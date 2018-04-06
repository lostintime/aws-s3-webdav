extern crate actix;
extern crate actix_web;
extern crate bytes;
extern crate clap;
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate log;
extern crate rusoto_core;
extern crate rusoto_credential;
extern crate rusoto_s3;
extern crate toml;

use actix::prelude::*;
use actix_web::{client, Application, AsyncResponder, Body, Error, HttpMessage, HttpRequest,
                HttpResponse, HttpServer, Method, StatusCode, error::ErrorInternalServerError,
                error::ErrorNotFound};
use rusoto_core::Region;
use rusoto_s3::*;
use futures::prelude::*;
use futures::{Future, Stream};
use bytes::Bytes;

struct S3Config {
    bucket: String,
}

impl S3Config {
    fn new<B>(bucket: B) -> S3Config
    where
        B: Into<String>,
    {
        return S3Config {
            bucket: bucket.into(),
        };
    }
}

struct AppConfig {
    s3: S3Config,
}

/// Application State (environment)
struct AppState {
    s3: S3Client<rusoto_credential::ProfileProvider, rusoto_core::reactor::RequestDispatcher>,
    config: AppConfig,
}

/// Get object from bucket
fn get_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
    let r1: GetObjectRequest = GetObjectRequest {
        bucket: String::from(req.state().config.s3.bucket.as_str()),
        key: String::from(req.path().trim_left_matches("/")),
        ..GetObjectRequest::default()
    };

    req.state()
        .s3
        .get_object(&r1)
        .map_err(|e| ErrorInternalServerError(format!("{:?}", e)))
        .map(|r| match r.body {
            Some(body) => HttpResponse::new(
                StatusCode::from_u16(200).expect("Failed to build status code"),
                Body::Streaming(Box::new(body.map_err(|e| {
                    ErrorInternalServerError("Something went wrong with body stream")
                }).map(Bytes::from))),
            ),
            None => HttpResponse::from_error(ErrorNotFound("Object not found")),
        })
        .responder()
}

fn main() {
    env_logger::init().unwrap();
    info!("starting up");

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
                .required(false)
        )
        .arg(
            clap::Arg::with_name("aws_bucket")
                .long("aws-bucket")
                .value_name("BUCKET_NAME")
                .env("AWS_BUCKET")
                .help("AWS Bucket name")
                .takes_value(true)
                .required(true)
        )
        .arg(
            clap::Arg::with_name("aws_config")
                .long("aws-config")
                .value_name("AWS_CONFIG")
                .env("AWS_CONFIG")
                .help("AWS Config file path")
                .takes_value(true)
                .required(true)
        )
        .arg(
            clap::Arg::with_name("aws_profile")
                .long("aws-profile")
                .value_name("AWS_PROFILE")
                .env("AWS_PROFILE")
                .help("AWS Profile name in AWS_CONFIG")
                .takes_value(true)
                .required(true)
        )
        // TODO add argument for AWS key and secret
        .get_matches();

    let args = matches.clone();
    let bind_port = *(&args.value_of("bind").unwrap_or_default());

    info!("Start server on {}", bind_port);

    // Start http server
    HttpServer::new(move || {
        let args = matches.clone();

        let state = AppState {
            s3: S3Client::new(
                rusoto_core::reactor::RequestDispatcher::default(),
                rusoto_credential::ProfileProvider::with_configuration(
                    args.value_of("aws_config").expect("aws_config argument required"),
                    args.value_of("aws_profile").expect("aws_profile argument required"),
                ),
                Region::EuCentral1,
            ),
            config: AppConfig {
                s3: S3Config::new(
                    args.value_of("aws_bucket")
                        .expect("AWS Bucket name argument required"),
                ), //                s3: S3Config {
                   //
                   //                    bucket: String::from(
                   //                        *(&)
                   //                    )
                   //                }
            },
        };

        Application::with_state(state).default_resource(|r| r.method(Method::GET).f(get_object))
    }).bind(&bind_port)
        .expect(&format!("Cannot bind to {}", &bind_port))
        .run();
}
