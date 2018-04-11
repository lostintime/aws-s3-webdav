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
extern crate rocket_aws_s3_proxy;

mod routes;
mod env;

use actix_web::{App, http, server };
use rusoto_core::{Region};

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
            clap::Arg::with_name("aws_region")
                .long("aws-region")
                .value_name("REGION")
                .env("AWS_REGION")
                .help("AWS Region ID")
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
                .default_value("default")
                .required(false)
        )
        // TODO add argument for AWS key and secret
        .get_matches();

    let args = matches.clone();
    let bind_port = *(&args.value_of("bind").unwrap_or_default());

    info!("Start server on {}", bind_port);

    // Start http server
    server::HttpServer::new(move || {
        info!("Building application");
        let args = matches.clone();

        let aws_region_name = args.value_of("aws_region").expect("AWS Region argument required").to_owned();
        let config = env::AppConfig {
            aws: env::AwsConfig::new(
                &args.value_of("aws_config").expect("AWS Config path argument required").to_owned(),
                &args.value_of("aws_profile").expect("AWS Profile name argument required").to_owned(),
                &Region::Custom {
                    name: aws_region_name.to_owned(),
                    endpoint: format!("http://s3.{}.amazonaws.com", aws_region_name).to_owned()
                }
            ),
            s3: env::S3Config::new(
                args.value_of("aws_bucket")
                    .expect("AWS Bucket name argument required"),
            ),
        };

        App::with_state(env::AppState::new(config))
            .default_resource(|r| {
                info!("default_resource lambda");
                r.method(http::Method::GET).f(routes::get_object);
                r.method(http::Method::HEAD).f(routes::head_object);
                r.method(http::Method::PUT).f(routes::put_object);
                r.method(http::Method::DELETE).f(routes::delete_object);
                r.method(http::Method::from_bytes(b"COPY").unwrap()).f(routes::copy_object);
                r.method(http::Method::from_bytes(b"MOVE").unwrap()).f(routes::move_object);
            })
    }).bind(&bind_port)
        .expect(&format!("Cannot bind to {}", &bind_port))
        .run();
}
