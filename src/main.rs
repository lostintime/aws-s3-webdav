#[macro_use]
extern crate log;
extern crate env_logger;
extern crate clap;
#[macro_use]
extern crate serde_derive;
extern crate toml;
extern crate actix;
extern crate actix_web;

use actix_web::*;

// fn index(_req: HttpRequest) -> &'static str {
//     "Hello world!"
// }

fn main() {
    env_logger::init().unwrap();
    info!("starting up");

    let matches = clap::App::new("rust-aws-s3-proxy")
        .version("0.1.0")
        .about("AWS S3 Bucket proxy, adding authorization")
        .author("lostintime <lostintime.dev@gmail.com>")
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Set a custom config file, defaults to \"./config.toml\"")
                .takes_value(true)
                .required(false)
        )
        .get_matches();

    let config_path = matches.value_of("config").unwrap_or("config.toml");
    debug!("Value for config: {}", config_path);

    // match config::load_config(config_path.to_string()) {
    //     Ok(config) => {
    //         debug!("Config loaded successfully!");
    //         let bind_port = config.http.bind.unwrap_or("0.0.0.0:8080".to_string());
            
    //         info!("Start server on {}", bind_port);

    //         // Start http server
    //         HttpServer::new(move ||
    //                 Application::with_state(env::AppEnv {
    //                            db: addr.clone()
    //                     })
    //                     .resource("/", |r| r.f(routing::index))
    //                     .resource("/posts/{post_id}", |r| r.f(routing::fetch_post))
    //             )
    //             .bind(&bind_port)
    //             .expect(&format!("Cannot bind to {}", &bind_port))
    //             .run();
    //     },
    //     Err(e) => {
    //         error!("Failed to load config: {}", e)
    //     }
    // };
}
