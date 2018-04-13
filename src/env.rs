use rusoto_core::Region;
use rusoto_s3::*;
use rusoto_core::{reactor::RequestDispatcher, reactor::CredentialsProvider};

pub struct AwsConfig {
    pub region: Region,
}

impl AwsConfig {
    pub fn new(region: &Region) -> AwsConfig {
        AwsConfig {
            region: region.clone(),
        }
    }
}

pub struct S3Config {
    pub bucket: String,
    pub prefix: Option<String>,
}

impl S3Config {
    pub fn new<B>(bucket: B, prefix: Option<B>) -> S3Config
    where
        B: Into<String>,
    {
        return S3Config {
            bucket: bucket.into(),
            prefix: prefix.map(|p| p.into()),
        };
    }
}

pub struct AppConfig {
    pub aws: AwsConfig,
    pub s3: S3Config,
}

/// Application State (environment)
pub struct AppState {
    pub s3: S3Client<CredentialsProvider, RequestDispatcher>,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(config: AppConfig) -> AppState {
        AppState {
            s3: S3Client::new(
                RequestDispatcher::default(),
                CredentialsProvider::default(),
                config.aws.region.to_owned(),
            ),
            config: config
        }
    }
}
