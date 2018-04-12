use rusoto_core::Region;
use rusoto_s3::*;
use rusoto_credential;
use rusoto_core;

pub struct AwsConfig {
    pub profile_path: String,
    pub profile_name: String,
    pub region: Region,
}

impl AwsConfig {
    pub fn new(path: &String, name: &String, region: &Region) -> AwsConfig {
        AwsConfig {
            profile_path: path.to_owned(),
            profile_name: name.to_owned(),
            region: region.clone(),
        }
    }
}

pub struct AwsBucket(String);

impl <'a> From<&'a AwsBucket> for String {
    fn from(bucket: &AwsBucket) -> String {
        bucket.0.to_owned()
    }
}

pub struct AwsPrefix(String);

pub struct InvalidAwsPrefix(String);

impl AwsPrefix {
    fn parse(_s: String) -> Result<AwsPrefix, InvalidAwsPrefix> {
        return Ok(AwsPrefix(String::from("")))
    }
}

impl <'a> From<&'a AwsPrefix> for String {
    fn from(prefix: &AwsPrefix) -> String {
        prefix.0.to_owned()
    }
}

pub struct S3Config {
    pub bucket: String,
    pub prefix: Option<AwsPrefix>,
}

impl S3Config {
    pub fn new<B>(bucket: B, prefix: Option<B>) -> S3Config
    where
        B: Into<String>,
    {
        return S3Config {
            bucket: bucket.into(),
            prefix: prefix.map(|p| AwsPrefix(p.into())),
        };
    }
}

pub struct AppConfig {
    pub aws: AwsConfig,
    pub s3: S3Config,
}

type MyS3Client = S3Client<rusoto_credential::ProfileProvider, rusoto_core::reactor::RequestDispatcher>;

/// Application State (environment)
pub struct AppState {
    pub s3: Box<MyS3Client>,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(config: AppConfig) -> AppState {
        AppState {
            s3: Box::new(
                S3Client::new(
                    rusoto_core::reactor::RequestDispatcher::default(),
                    rusoto_credential::ProfileProvider::with_configuration(
                        config.aws.profile_path.clone(),
                        config.aws.profile_name.clone(),
                    ),
                    config.aws.region.clone(),
                    // Region::Custom {
                    //     name: config.s3.bucket.clone(),
                    //     endpoint: format!("http://s3.{}.amazonaws.com", config.s3.bucket).to_owned()
                    // }
                )
            ),
            config: config
        }
    }
}
